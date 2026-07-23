//! Grand-strategy goals: a house's standing ambition, above its plans.
//!
//! Where a plan answers one pressure — a tactic — a goal is the long
//! horizon above several: an ambition a head adopts and the house pursues
//! across reigns. A goal does not execute. While it is active it biases
//! which pressures the head reaches for, so the plans and assignments
//! already in place carry it out and no second executor is needed. It is
//! the house's, keyed by the organisation rather than the person, so the
//! plan dies with its leader but the ambition does not.
//!
//! Determinism: goal state is one resource of `BTreeMap`s serialised into
//! the snapshot like every other section; adoption is head-only, monthly,
//! and consumes one roll on a frozen stream; the target it resolves and
//! every trigger it weighs are integer facts over visible state, walked
//! in stable ID order.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use aeon_data::model::{
    AiIntent, AssignmentTargetKind, DirectiveTarget, GoalRequires, HouseTier, OrgKind,
};
use bevy::prelude::{Resource, World};
use serde::{Deserialize, Serialize};

use crate::assignments::{AssignmentTarget, LogChannel, LogEntry, LogSubject};
use crate::clock::CampaignClock;
use crate::ids::{CharacterId, OrgId};
use crate::politics::CampaignOver;
use crate::state::ContentDb;
use crate::text::TextDb;

/// A goal a house is pursuing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveGoal {
    /// The authored goal.
    pub def: ContentKey,
    /// The head who set the house on it. Informational: the ambition
    /// outlives the reign, so this is who began it, not who must finish.
    pub adopted_by: CharacterId,
    /// What the ambition is aimed at, resolved once at adoption.
    pub target: AssignmentTarget,
    /// The day the house set its mind to it.
    pub started: GameDate,
}

/// Every house's active ambition and its cooldowns, one resource.
///
/// Keyed by organisation, not character: a goal is the house's, and
/// survives the succession that ends a plan. `BTreeMap`s give the stable
/// iteration order determinism requires for free.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goals {
    /// Active ambitions by the house pursuing them.
    pub active: BTreeMap<OrgId, ActiveGoal>,
    /// Days before which a house may not adopt a goal again.
    pub cooldowns: BTreeMap<(OrgId, ContentKey), GameDate>,
}

/// The bonus a house's active goal lends a pressure.
///
/// Zero when the house has no goal, or the goal does not favour this
/// pressure. This is the whole way a goal steers the house: the head
/// feels the pressures that serve its ambition more keenly, and the
/// existing scorer, plans and assignments do the rest.
pub fn favour_bonus(world: &World, authority: OrgId, intent: AiIntent) -> i64 {
    let Some(goals) = world.get_resource::<Goals>() else {
        return 0;
    };
    let Some(active) = goals.active.get(&authority) else {
        return 0;
    };
    let content = world.resource::<ContentDb>().0.clone();
    let Some(def) = content.goals.get(&active.def) else {
        return 0;
    };
    if def.favours.contains(&intent) {
        def.favour_bonus
    } else {
        0
    }
}

/// The target of a house's active goal, if it has one.
///
/// What a liege's directives carry down to its vassals.
pub fn active_target(world: &World, authority: OrgId) -> Option<AssignmentTarget> {
    world
        .get_resource::<Goals>()
        .and_then(|goals| goals.active.get(&authority))
        .map(|active| active.target)
}

/// How strongly an advisory directive lifts the pressure it names in a
/// vassal head's scoring. Fixed and modest: a directive is a wish, felt
/// less keenly than a house's own ambition, and it never compels.
pub const DIRECTIVE_BONUS: i64 = 25;

/// A directive issued to a vassal by hand rather than derived from a
/// goal — how the player presses a wish on a house that answers to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedDirective {
    /// The house that issued it.
    pub from: OrgId,
    /// The pressure the issuer wants felt more keenly.
    pub intent: AiIntent,
    /// What it is aimed at, if anything.
    pub target: Option<AssignmentTarget>,
}

/// Directives standing by hand, one per vassal, keyed by the vassal they
/// are pressed on.
///
/// A goal's directives are derived from goal state and need no storage;
/// a hand-issued directive has no goal behind it, so it is kept here. One
/// per vassal: issuing a new one replaces the last.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedDirectives {
    /// The standing directive on each vassal, by vassal.
    pub by_vassal: BTreeMap<OrgId, IssuedDirective>,
}

/// The directives a vassal currently receives from its liege.
///
/// Each is an intent the liege wants felt more keenly, and — when the
/// directive carries the goal's own target — what it is aimed at. Two
/// sources merge here: the directives a liege's active goal presses,
/// derived and lapsing for free when the goal ends, and any standing
/// directive issued by hand. A house with no liege, or a liege pressing
/// nothing, receives nothing.
pub fn directives_on(world: &World, vassal: OrgId) -> Vec<(AiIntent, Option<AssignmentTarget>)> {
    let Some(liege) = crate::access::org(world, vassal).and_then(|r| r.liege) else {
        return Vec::new();
    };
    let mut pressed: Vec<(AiIntent, Option<AssignmentTarget>)> = Vec::new();

    // Derived from the liege's active goal.
    if let Some(goals) = world.get_resource::<Goals>()
        && let Some(active) = goals.active.get(&liege)
    {
        let content = world.resource::<ContentDb>().0.clone();
        if let Some(def) = content.goals.get(&active.def) {
            for directive in &def.directives {
                let target = match directive.target {
                    DirectiveTarget::None => None,
                    DirectiveTarget::GoalTarget => Some(active.target),
                };
                pressed.push((directive.intent, target));
            }
        }
    }

    // Pressed by hand, and only from this vassal's own liege.
    if let Some(issued) = world.get_resource::<IssuedDirectives>()
        && let Some(directive) = issued.by_vassal.get(&vassal)
        && directive.from == liege
    {
        pressed.push((directive.intent, directive.target));
    }
    pressed
}

/// The bonus a vassal feels on a pressure because its liege directs it
/// there. Zero unless the liege's active goal presses a matching
/// directive.
pub fn directive_bonus(world: &World, vassal: OrgId, intent: AiIntent) -> i64 {
    if directives_on(world, vassal)
        .iter()
        .any(|(pressed, _)| *pressed == intent)
    {
        DIRECTIVE_BONUS
    } else {
        0
    }
}

/// Whether a goal's trigger holds for a house.
///
/// Integer facts over visible state, mirroring what the player could
/// check on the same screens, extended with the two hierarchy facts a
/// grand strategy weighs.
fn trigger_met(world: &World, authority: OrgId, req: &GoalRequires) -> bool {
    let resources = crate::access::org_entity(world, authority)
        .and_then(|e| world.get::<crate::economy::OrgResources>(e))
        .copied()
        .unwrap_or_default();
    if let Some(need) = req.min_wealth
        && resources.wealth < need
    {
        return false;
    }
    if let Some(need) = req.min_manpower
        && resources.manpower < need
    {
        return false;
    }
    if let Some(need) = req.min_legitimacy
        && crate::economy::effective_legitimacy(world, authority) < need
    {
        return false;
    }
    if let Some(wanted) = req.has_army {
        let has = world
            .get_resource::<crate::forces::ForcesIndex>()
            .is_some_and(|index| {
                index.armies.values().any(|entity| {
                    world
                        .get::<crate::forces::ArmyRecord>(*entity)
                        .is_some_and(|army| army.owner == authority)
                })
            });
        if has != wanted {
            return false;
        }
    }
    if req.dominant_claimant {
        let dominant = world
            .get_resource::<crate::map::MapIndex>()
            .and_then(|index| index.provinces.values().next().copied())
            .and_then(|entity| world.get::<crate::map::ProvinceRecord>(entity))
            .map(|record| record.body)
            .and_then(|body| crate::crisis::dominant_claimant(world, body));
        if dominant != Some(authority) {
            return false;
        }
    }
    if let Some(wanted) = req.has_vassals
        && has_vassals(world, authority) != wanted
    {
        return false;
    }
    if let Some(wanted) = req.is_vassal {
        let is_vassal =
            crate::access::org(world, authority).is_some_and(|r| r.tier == Some(HouseTier::Vassal));
        if is_vassal != wanted {
            return false;
        }
    }
    true
}

/// Whether any house declares `liege` as its liege.
pub fn has_vassals(world: &World, liege: OrgId) -> bool {
    crate::access::org_ids(world).into_iter().any(|org| {
        crate::access::org(world, org).is_some_and(|r| r.liege == Some(liege) && !r.defunct)
    })
}

/// Resolves a goal's concrete target from the kind it declares.
///
/// `None` needs nothing. An organisation-aimed goal takes the weakest
/// rival great house — fewest holdings, lowest stable ID on a tie — that
/// is not the house's own liege; no such rival makes the goal
/// unadoptable, like a plan candidate that cannot resolve its target.
fn resolve_target(
    world: &World,
    authority: OrgId,
    kind: AssignmentTargetKind,
) -> Option<AssignmentTarget> {
    match kind {
        AssignmentTargetKind::None => Some(AssignmentTarget::None),
        AssignmentTargetKind::Organisation => {
            weakest_rival(world, authority).map(AssignmentTarget::Org)
        }
        // Province-aimed goals wait on a selector a real goal needs; none
        // does yet, so the vocabulary has not grown one.
        _ => None,
    }
}

/// The weakest rival great house to `self_org`: a dynastic great house,
/// still standing, outside the house's own chain of command, holding the
/// fewest provinces.
fn weakest_rival(world: &World, self_org: OrgId) -> Option<OrgId> {
    crate::access::org_ids(world)
        .into_iter()
        .filter(|org| *org != self_org)
        .filter(|org| {
            crate::access::org(world, *org).is_some_and(|r| {
                !r.defunct && r.kind == OrgKind::DynasticHouse && r.tier == Some(HouseTier::Great)
            })
        })
        // Not the house's own liege chain: you do not conquer your liege.
        .filter(|org| crate::politics::answers_to(world, self_org, *org).is_none())
        .min_by_key(|org| (crate::order::held_provinces(world, *org).len(), *org))
}

/// Monthly, head-only: a house with no ambition may form one.
///
/// Called from the agency pass for the head of each standing non-player
/// house. Adoption consumes one roll on the frozen `"grand-goal"` stream,
/// subjects `[org, month]`, so a house does not seize on an ambition the
/// first month it could; then it takes the first goal, in content-key
/// order, whose trigger holds, is off cooldown, and can resolve a target.
pub fn maybe_adopt_goal(world: &mut World, head: CharacterId, authority: OrgId) {
    if world
        .get_resource::<Goals>()
        .is_some_and(|goals| goals.active.contains_key(&authority))
    {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let month = date.days_since_epoch() as u64 / 30;
    // A house does not set its mind to a grand ambition every month it
    // could. The label is an identity, frozen forever.
    let mut rng = crate::access::derived_rng(world, "grand-goal", &[authority.raw(), month]);
    if !rng.check_permille(300) {
        return;
    }

    let content = world.resource::<ContentDb>().0.clone();
    let chosen = content.goals.values().find_map(|def| {
        let off_cooldown = world
            .resource::<Goals>()
            .cooldowns
            .get(&(authority, def.key.clone()))
            .is_none_or(|until| date >= *until);
        if !off_cooldown || !trigger_met(world, authority, &def.trigger) {
            return None;
        }
        resolve_target(world, authority, def.target).map(|target| (def.key.clone(), target))
    });
    let Some((key, target)) = chosen else {
        return;
    };

    let title = content
        .goals
        .get(&key)
        .map(|def| def.title.clone())
        .unwrap_or_default();
    world.resource_mut::<Goals>().active.insert(
        authority,
        ActiveGoal {
            def: key,
            adopted_by: head,
            target,
            started: date,
        },
    );
    let text = world.resource::<TextDb>().format(
        "sim.goal.adopted",
        &[
            ("house", &crate::access::org_name(world, authority)),
            ("goal", &title),
        ],
    );
    let mut entry = LogEntry::line(text, LogChannel::Politics).by(Some(authority));
    if let AssignmentTarget::Org(rival) = target {
        entry = entry.about(LogSubject::Org(rival));
    } else {
        entry = entry.about(LogSubject::Org(authority));
    }
    crate::access::log(world, entry);
}

/// Monthly: an ambition ends when it is out of time, its house has
/// fallen, or the rival it was aimed at is gone.
///
/// Walked in stable house-ID order. Completion by a fallen target and
/// abandonment by exhaustion both start the cooldown and say so, so the
/// player can follow what a rival house is about.
pub fn advance_goals(world: &mut World) {
    // A content-free world (a snapshot restored without its content) holds
    // no goals worth advancing and no content to read them from.
    if world.get_resource::<Goals>().is_none()
        || world.get_resource::<ContentDb>().is_none()
        || world.get_resource::<CampaignOver>().is_some()
    {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let content = world.resource::<ContentDb>().0.clone();

    let houses: Vec<OrgId> = world.resource::<Goals>().active.keys().copied().collect();
    for authority in houses {
        let active = world.resource::<Goals>().active[&authority].clone();
        let Some(def) = content.goals.get(&active.def) else {
            world.resource_mut::<Goals>().active.remove(&authority);
            continue;
        };

        let house_stands = crate::access::org(world, authority).is_some_and(|r| !r.defunct);
        let target_fallen = match active.target {
            AssignmentTarget::Org(rival) => {
                crate::access::org(world, rival).is_none_or(|r| r.defunct)
            }
            _ => false,
        };
        let out_of_time =
            date.days_since_epoch() - active.started.days_since_epoch() > i64::from(def.max_days);

        if !house_stands {
            world.resource_mut::<Goals>().active.remove(&authority);
            continue;
        }
        if !target_fallen && !out_of_time {
            continue;
        }

        // The ambition ends. A fallen rival is an ambition achieved; time
        // run out is one set aside. Either way, the cooldown holds the
        // house off the same ambition for a while.
        world.resource_mut::<Goals>().active.remove(&authority);
        if def.cooldown_days > 0 {
            world.resource_mut::<Goals>().cooldowns.insert(
                (authority, active.def.clone()),
                date.add_days(i64::from(def.cooldown_days)),
            );
        }
        let key = if target_fallen {
            "sim.goal.achieved"
        } else {
            "sim.goal.set-aside"
        };
        let text = world.resource::<TextDb>().format(
            key,
            &[
                ("house", &crate::access::org_name(world, authority)),
                ("goal", &def.title),
            ],
        );
        crate::access::log(
            world,
            LogEntry::line(text, LogChannel::Politics).by(Some(authority)),
        );
    }
}

/// Captures goal state for a snapshot.
pub fn capture(world: &World) -> Goals {
    world.get_resource::<Goals>().cloned().unwrap_or_default()
}

/// Restores goal state from a snapshot.
pub fn restore(world: &mut World, state: &Goals) {
    world.insert_resource(state.clone());
}

/// Captures hand-issued directives for a snapshot.
pub fn capture_issued(world: &World) -> IssuedDirectives {
    world
        .get_resource::<IssuedDirectives>()
        .cloned()
        .unwrap_or_default()
}

/// Restores hand-issued directives from a snapshot.
pub fn restore_issued(world: &mut World, state: &IssuedDirectives) {
    world.insert_resource(state.clone());
}

/// Installs the monthly goal expiry.
pub(crate) fn install(app: &mut bevy::prelude::App) {
    use crate::clock::MonthlyPulse;
    use bevy::prelude::IntoScheduleConfigs;
    // Before the agency pass adopts new ones, so a goal that ended this
    // month frees its house to form another.
    app.add_systems(
        MonthlyPulse,
        advance_goals.before(crate::agency::characters_act),
    );
}
