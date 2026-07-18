//! The contextual event runtime.
//!
//! Events are authored data selected from the current campaign state, not
//! unconditioned flavour. Each definition declares the family of situation
//! it belongs to, the conditions under which it may fire, how often it may
//! recur against the same subject, and what its choices do.
//!
//! Selection is wholly deterministic: candidates are gathered in stable ID
//! order, and the draw uses the campaign's seeded RNG on the day and the
//! subject, so the same campaign always produces the same events.
//!
//! Weighty events raise a choice popup and pause the campaign; minor ones
//! post to the message log without interrupting, which keeps interruption
//! proportionate to consequence.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_core::rng::DeterministicRng;
use aeon_data::model::{EventDef, EventFamily};
use aeon_data::{ContentKey, ContentSet};
use bevy::app::App;
use bevy::prelude::{IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::ids::{CharacterId, JobId, OrgId, ProvinceId};
use crate::jobs::{
    JobRoles, LogChannel, LogEntry, LogSubject, MessageLog, PendingPopup, PendingPopups,
    ScriptRuntime,
};
use crate::obligations::Obligations;
use crate::order::province_order;
use crate::politics::{CharacterRecord, PlayerHouse, PoliticsIndex};
use crate::state::{CampaignSeed, ContentDb};

/// Chance per day that a weighty event fires anywhere, in permille.
///
/// Deliberately low: a weighty event interrupts the campaign, so it should
/// be worth the interruption. Roughly three or four a year.
pub const WEIGHTY_CHANCE_PERMILLE: u32 = 10;
/// Chance per day that a minor event fires anywhere, in permille.
///
/// Minor events only write to the log, so they can be far more frequent
/// without costing the player anything but colour.
pub const MINOR_CHANCE_PERMILLE: u32 = 70;

/// What an event happened to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventSubject {
    /// A province under pressure.
    Province(ProvinceId),
    /// An organisation and its politics.
    Org(OrgId),
    /// A character, typically travelling.
    Character(CharacterId),
    /// A job in progress.
    Job(JobId),
}

impl EventSubject {
    /// A stable key for cooldown bookkeeping.
    fn raw(self) -> u64 {
        match self {
            EventSubject::Province(id) => id.raw(),
            EventSubject::Org(id) => id.raw(),
            EventSubject::Character(id) => id.raw(),
            EventSubject::Job(id) => id.raw(),
        }
    }
}

/// One event that has fired, kept so the history stays inspectable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventOccurrence {
    /// The definition that fired.
    pub event: ContentKey,
    /// What it happened to.
    pub subject: EventSubject,
    /// When.
    pub date: GameDate,
    /// The choice taken, once answered.
    pub choice: Option<ContentKey>,
}

/// Event bookkeeping: what has fired, and when each may fire again.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventState {
    /// The day each (event, subject) pair last fired.
    pub last_fired: BTreeMap<(ContentKey, u64), GameDate>,
    /// Everything that has happened, oldest first.
    pub history: Vec<EventOccurrence>,
}

impl EventState {
    /// Whether an event may fire against a subject on `date`.
    fn off_cooldown(&self, event: &EventDef, subject: EventSubject, date: GameDate) -> bool {
        match self.last_fired.get(&(event.key.clone(), subject.raw())) {
            None => true,
            Some(last) => last.days_until(date) >= i64::from(event.cooldown_days),
        }
    }
}

/// The conditions an event needs, evaluated against one subject.
///
/// Eligibility is declarative rather than scripted so it can be validated
/// at load, reasoned about, and replayed identically.
fn eligible(world: &World, def: &EventDef, subject: EventSubject) -> bool {
    let player = world.get_resource::<PlayerHouse>().and_then(|p| p.0);
    let requires = &def.requires;

    // Which organisation this subject belongs to, where that makes sense.
    let owner = match subject {
        EventSubject::Province(province) => crate::warfare::province_holder(world, province),
        EventSubject::Org(org) => Some(org),
        EventSubject::Character(character) => world
            .get_resource::<PoliticsIndex>()
            .and_then(|index| index.characters.get(&character).copied())
            .and_then(|entity| world.get::<CharacterRecord>(entity))
            .and_then(|record| record.organisation),
        EventSubject::Job(job) => world
            .get_resource::<crate::jobs::JobsIndex>()
            .and_then(|index| index.jobs.get(&job).copied())
            .and_then(|entity| world.get::<crate::jobs::ActiveJob>(entity))
            .map(|record| record.owner),
    };

    if requires.player_only && owner != player {
        return false;
    }

    if let EventSubject::Province(province) = subject {
        let state = province_order(world, province);
        if let Some(max) = requires.max_order
            && state.order > max
        {
            return false;
        }
        if let Some(min) = requires.min_order
            && state.order < min
        {
            return false;
        }
        if requires.occupied {
            let pressures = crate::order::pressures(world, province);
            if !pressures.occupied {
                return false;
            }
        }
    } else if requires.max_order.is_some() || requires.min_order.is_some() || requires.occupied {
        // Order conditions only mean anything about a province.
        return false;
    }

    if requires.has_open_obligation {
        let Some(owner) = owner else {
            return false;
        };
        let bound = world
            .get_resource::<Obligations>()
            .is_some_and(|ledger| ledger.involving(owner).next().is_some());
        if !bound {
            return false;
        }
    }

    true
}

/// Every subject an event of this family could apply to, in stable order.
fn subjects_for(world: &World, family: EventFamily) -> Vec<EventSubject> {
    match family {
        EventFamily::Province => world
            .get_resource::<crate::map::MapIndex>()
            .map(|index| {
                index
                    .provinces
                    .keys()
                    .map(|id| EventSubject::Province(*id))
                    .collect()
            })
            .unwrap_or_default(),
        EventFamily::Political => world
            .get_resource::<PoliticsIndex>()
            .map(|index| index.orgs.keys().map(|id| EventSubject::Org(*id)).collect())
            .unwrap_or_default(),
        EventFamily::Travel => world
            .get_resource::<PoliticsIndex>()
            .map(|index| {
                index
                    .characters
                    .iter()
                    .filter(|(_, entity)| {
                        // Only characters actually under way.
                        world
                            .get::<crate::presence::CharacterLocation>(**entity)
                            .is_some_and(|location| {
                                matches!(location.0, crate::presence::Location::Transit { .. })
                            })
                    })
                    .map(|(id, _)| EventSubject::Character(*id))
                    .collect()
            })
            .unwrap_or_default(),
        EventFamily::Job => world
            .get_resource::<crate::jobs::JobsIndex>()
            .map(|index| index.jobs.keys().map(|id| EventSubject::Job(*id)).collect())
            .unwrap_or_default(),
    }
}

/// Gathers every (event, subject) pair that could fire today.
fn candidates(
    world: &World,
    content: &ContentSet,
    weighty: bool,
) -> Vec<(ContentKey, EventSubject, u32)> {
    let Some(state) = world.get_resource::<EventState>() else {
        return Vec::new();
    };
    let date = world.resource::<CampaignClock>().date;
    let mut found = Vec::new();
    // Events in content-key order, subjects in stable ID order.
    for def in content.events.values() {
        if def.weighty != weighty {
            continue;
        }
        for subject in subjects_for(world, def.family) {
            if state.off_cooldown(def, subject, date) && eligible(world, def, subject) {
                found.push((def.key.clone(), subject, def.weight.max(1)));
            }
        }
    }
    found
}

/// Picks one candidate by weight, deterministically.
fn pick(
    rng: &mut DeterministicRng,
    candidates: &[(ContentKey, EventSubject, u32)],
) -> Option<(ContentKey, EventSubject)> {
    let total: u64 = candidates
        .iter()
        .map(|(_, _, weight)| u64::from(*weight))
        .sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.roll(total);
    for (key, subject, weight) in candidates {
        let weight = u64::from(*weight);
        if roll < weight {
            return Some((key.clone(), *subject));
        }
        roll -= weight;
    }
    candidates
        .last()
        .map(|(key, subject, _)| (key.clone(), *subject))
}

/// The characters standing behind an event's roles.
fn roles_for(world: &World, subject: EventSubject) -> JobRoles {
    let head_of = |org: OrgId| -> Option<CharacterId> {
        world
            .get_resource::<PoliticsIndex>()
            .and_then(|index| index.orgs.get(&org).copied())
            .and_then(|entity| world.get::<crate::politics::OrgRecord>(entity))
            .and_then(|record| record.head)
    };
    let (owner, leader, province) = match subject {
        EventSubject::Province(province) => (
            crate::warfare::province_holder(world, province),
            None,
            Some(province),
        ),
        EventSubject::Org(org) => (Some(org), None, None),
        EventSubject::Character(character) => {
            let org = world
                .get_resource::<PoliticsIndex>()
                .and_then(|index| index.characters.get(&character).copied())
                .and_then(|entity| world.get::<CharacterRecord>(entity))
                .and_then(|record| record.organisation);
            (org, Some(character), None)
        }
        EventSubject::Job(job) => {
            let record = world
                .get_resource::<crate::jobs::JobsIndex>()
                .and_then(|index| index.jobs.get(&job).copied())
                .and_then(|entity| world.get::<crate::jobs::ActiveJob>(entity).cloned());
            match record {
                Some(record) => (Some(record.owner), Some(record.leader), None),
                None => (None, None, None),
            }
        }
    };
    JobRoles {
        leader: leader.or_else(|| owner.and_then(head_of)),
        target: None,
        target_head: None,
        owner_head: owner.and_then(head_of),
        liege_head: None,
        consul: None,
        sanctora: Vec::new(),
        province,
    }
}

/// A short label naming what an event happened to.
fn subject_name(world: &World, subject: EventSubject) -> String {
    match subject {
        EventSubject::Province(province) => world
            .get_resource::<crate::map::MapIndex>()
            .and_then(|index| index.provinces.get(&province).copied())
            .and_then(|entity| world.get::<crate::map::DisplayName>(entity))
            .map(|name| name.0.clone())
            .unwrap_or_default(),
        EventSubject::Org(org) => crate::crisis::org_display_name(world, org),
        EventSubject::Character(character) => world
            .get_resource::<PoliticsIndex>()
            .and_then(|index| index.characters.get(&character).copied())
            .and_then(|entity| world.get::<CharacterRecord>(entity))
            .map(|record| record.name.clone())
            .unwrap_or_default(),
        EventSubject::Job(_) => String::new(),
    }
}

/// Fires one event against one subject.
fn fire(world: &mut World, key: &ContentKey, subject: EventSubject) {
    let date = world.resource::<CampaignClock>().date;
    let content = world.resource::<ContentDb>().0.clone();
    let Some(def) = content.events.get(key).cloned() else {
        return;
    };
    let roles = roles_for(world, subject);
    let name = subject_name(world, subject);
    let text = def.text.replace("{subject}", &name);
    let owner = match subject {
        EventSubject::Province(province) => crate::warfare::province_holder(world, province),
        EventSubject::Org(org) => Some(org),
        _ => roles
            .owner_head
            .and_then(|head| {
                world
                    .get_resource::<PoliticsIndex>()
                    .and_then(|index| index.characters.get(&head).copied())
            })
            .and_then(|entity| world.get::<CharacterRecord>(entity))
            .and_then(|record| record.organisation),
    };

    // Record the firing before anything else, so cooldowns and history
    // hold even if an effect fails.
    {
        let mut state = world.resource_mut::<EventState>();
        state.last_fired.insert((key.clone(), subject.raw()), date);
        state.history.push(EventOccurrence {
            event: key.clone(),
            subject,
            date,
            choice: None,
        });
    }

    let log_subject = match subject {
        EventSubject::Province(province) => Some(LogSubject::Province(province)),
        EventSubject::Org(org) => Some(LogSubject::Org(org)),
        EventSubject::Character(character) => Some(LogSubject::Character(character)),
        EventSubject::Job(_) => roles.leader.map(LogSubject::Character),
    };

    if def.weighty && !def.choices.is_empty() {
        // A weighty event asks the player, and the client pauses for it.
        let mut popups = world.resource_mut::<PendingPopups>();
        let id = popups.next_id;
        popups.next_id += 1;
        popups.popups.push(PendingPopup {
            id,
            date,
            job: key.clone(),
            result: aeon_data::model::JobResultKind::Success,
            text: text.clone(),
            choices: def
                .choices
                .iter()
                .map(|choice| (choice.id.clone(), choice.label.clone()))
                .collect(),
            roles: roles.clone(),
        });
    }

    // Every event writes a line, weighty or not.
    let line = def.log_text.clone().unwrap_or(text);
    let mut entry =
        LogEntry::new(date, line.replace("{subject}", &name), LogChannel::Events).by(owner);
    if let Some(log_subject) = log_subject {
        entry = entry.about(log_subject);
    }
    world.resource_mut::<MessageLog>().entries.push(entry);

    // A minor event applies its own effect immediately; a weighty one
    // waits for the player's answer.
    if !def.weighty
        && let Some(effect_fn) = &def.effect_fn
    {
        let effects = {
            let runtime = world.resource::<ScriptRuntime>();
            runtime
                .0
                .call_effect_fn(&content, effect_fn, rhai::Map::new())
        };
        if let Ok(effects) = effects {
            crate::jobs::apply_effects(world, &effects, &roles, owner);
        }
    }
}

/// Draws the day's events.
pub fn tick_events(world: &mut World) {
    if world.get_resource::<ContentDb>().is_none() {
        return;
    }
    if world.get_resource::<EventState>().is_none() {
        world.insert_resource(EventState::default());
    }
    let content = world.resource::<ContentDb>().0.clone();
    if content.events.is_empty() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let seed = world.resource::<CampaignSeed>().0;
    let day = date.days_since_epoch() as u64;

    // Two independent draws, so a rare weighty event never crowds out the
    // ordinary texture of the log, and vice versa.
    for (weighty, chance, purpose) in [
        (true, WEIGHTY_CHANCE_PERMILLE, "event-weighty"),
        (false, MINOR_CHANCE_PERMILLE, "event-minor"),
    ] {
        let mut gate = DeterministicRng::derive(seed, purpose, &[day]);
        if !gate.check_permille(chance) {
            continue;
        }
        let found = candidates(world, &content, weighty);
        if found.is_empty() {
            continue;
        }
        let mut rng = DeterministicRng::derive(seed, "event-pick", &[day, u64::from(weighty)]);
        if let Some((key, subject)) = pick(&mut rng, &found) {
            fire(world, &key, subject);
        }
    }
}

/// Applies the chosen answer to a weighty event popup.
///
/// Returns whether the popup belonged to an event; job popups are handled
/// by the job system.
pub fn answer_event(
    world: &mut World,
    event: &ContentKey,
    choice: &ContentKey,
    roles: &JobRoles,
) -> bool {
    let content = world.resource::<ContentDb>().0.clone();
    let Some(def) = content.events.get(event) else {
        return false;
    };
    let Some(chosen) = def.choices.iter().find(|option| &option.id == choice) else {
        return false;
    };

    // Record the answer against the most recent firing of this event.
    if let Some(mut state) = world.get_resource_mut::<EventState>()
        && let Some(occurrence) = state
            .history
            .iter_mut()
            .rev()
            .find(|occurrence| &occurrence.event == event && occurrence.choice.is_none())
    {
        occurrence.choice = Some(choice.clone());
    }

    let owner = roles
        .owner_head
        .and_then(|head| {
            world
                .get_resource::<PoliticsIndex>()
                .and_then(|index| index.characters.get(&head).copied())
        })
        .and_then(|entity| world.get::<CharacterRecord>(entity))
        .and_then(|record| record.organisation);

    if let Some(effect_fn) = &chosen.effect_fn {
        let effects = {
            let runtime = world.resource::<ScriptRuntime>();
            runtime
                .0
                .call_effect_fn(&content, effect_fn, rhai::Map::new())
        };
        if let Ok(effects) = effects {
            crate::jobs::apply_effects(world, &effects, roles, owner);
        }
    }
    true
}

/// Captures event bookkeeping for a snapshot.
pub fn capture(world: &World) -> EventState {
    world
        .get_resource::<EventState>()
        .cloned()
        .unwrap_or_default()
}

/// Restores event bookkeeping from a snapshot.
pub fn restore(world: &mut World, state: &EventState) {
    world.insert_resource(state.clone());
}

/// Installs the daily event draw.
pub fn install(app: &mut App) {
    app.add_systems(DailyTick, tick_events.in_set(TickSet::Events));
}
