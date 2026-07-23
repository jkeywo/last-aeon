//! The character-led assignment system: the universal unit of strategic action.
//!
//! Assignments are started by commands (player) or agency (AI organisations),
//! take calendar days, and resolve into graded results whose weights are
//! shifted by the leader's governing skill against the assignment's difficulty.
//! Results can log to the notable-result message log, open player popups
//! with choices, expose the leader to declared personal risks, and apply
//! authored script effects through the sandboxed effect boundary.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::model::{
    AssignmentCategory, AssignmentDef, AssignmentTargetKind, HolderRelation, OutcomeKind, RiskTag,
    TitleNeed,
};
use aeon_data::{ContentKey, EffectRole, ScriptEffect, ScriptHost};
use bevy::app::App;
use bevy::prelude::{Component, Entity, IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, MonthlyPulse, TickSet};
use crate::ids::{ArmyId, AssignmentId, CharacterId, OrgId, ProvinceId, ShipId};
use crate::politics::{CampaignOver, OpinionEntry, OpinionLedger, PlayerHouse, process_death};
use crate::state::{CampaignIds, ContentDb};
use crate::text::TextDb;

/// What a assignment acts on.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentTarget {
    /// The assignment acts on its owner organisation.
    None,
    /// A character.
    Character(CharacterId),
    /// An organisation.
    Org(OrgId),
    /// A province.
    Province(ProvinceId),
    /// One of the owner's armies.
    OwnArmy(ArmyId),
    /// One of the owner's armies marching on a province.
    ArmyToProvince(ArmyId, ProvinceId),
    /// One of the owner's ships sent against a province.
    ShipToProvince(ShipId, ProvinceId),
}

/// A running assignment.
#[derive(Component, Clone, Debug)]
pub struct ActiveAssignment {
    /// Stable ID.
    pub id: AssignmentId,
    /// The assignment definition's content key.
    pub def: ContentKey,
    /// The organisation this assignment serves.
    pub owner: OrgId,
    /// The character leading it.
    pub leader: CharacterId,
    /// What it acts on.
    pub target: AssignmentTarget,
    /// The day it (re)started.
    pub started: GameDate,
    /// The day it resolves.
    pub completes: GameDate,
    /// Whether the owner has asked for it to be called off.
    ///
    /// A request rather than an act, because it may land during a phase
    /// that cannot be interrupted — in which case it waits, and the
    /// player can see that it is waiting rather than wondering whether
    /// the click registered.
    pub cancel_requested: bool,
}

impl ActiveAssignment {
    /// Which phase it is in on `date`, given its definition.
    pub fn stage(&self, def: &AssignmentDef, date: GameDate) -> usize {
        def.stage_at(self.started.days_until(date))
    }

    /// Whether it can be called off on `date`.
    pub fn interruptible_on(&self, def: &AssignmentDef, date: GameDate) -> bool {
        def.stages
            .get(self.stage(def, date))
            .is_some_and(|stage| stage.interruptible)
    }
}

/// Temporary states that keep a character from leading new assignments.
#[derive(Component, Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterCondition {
    /// Hurt; recovering until this day.
    pub injured_until: Option<GameDate>,
    /// Held by an enemy until this day.
    pub captured_until: Option<GameDate>,
    /// Unable to act until this day.
    pub incapacitated_until: Option<GameDate>,
}

impl CharacterCondition {
    /// Whether the character can take on new work at `date`.
    pub fn can_lead(&self, date: GameDate) -> bool {
        let blocked = |until: Option<GameDate>| until.is_some_and(|u| u > date);
        !blocked(self.injured_until)
            && !blocked(self.captured_until)
            && !blocked(self.incapacitated_until)
    }
}

/// Lookup from assignment IDs to entities.
#[derive(Resource, Clone, Debug, Default)]
pub struct AssignmentsIndex {
    /// Assignments by stable ID.
    pub assignments: BTreeMap<AssignmentId, Entity>,
}

/// What a log entry is chiefly about, so a reader can navigate to it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogSubject {
    /// A character.
    Character(CharacterId),
    /// An organisation.
    Org(OrgId),
    /// A province.
    Province(ProvinceId),
}

/// Which stream an entry belongs to, so the log can be filtered.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogChannel {
    /// Assignment starts, results, and abandonment.
    #[default]
    Assignments,
    /// Titles, succession, offices, and standing.
    Politics,
    /// Operations, engagements, and conquest.
    Military,
    /// Resources, provincial output, and order.
    Economy,
    /// Contextual events and their consequences.
    Events,
}

impl LogChannel {
    /// Every channel, in a stable display order.
    pub const ALL: [LogChannel; 5] = [
        LogChannel::Assignments,
        LogChannel::Politics,
        LogChannel::Military,
        LogChannel::Economy,
        LogChannel::Events,
    ];

    /// The key of a short player-facing label.
    pub fn label_key(self) -> &'static str {
        match self {
            LogChannel::Assignments => "ui.log.channel.assignments",
            LogChannel::Politics => "ui.log.channel.politics",
            LogChannel::Military => "ui.log.channel.military",
            LogChannel::Economy => "ui.log.channel.economy",
            LogChannel::Events => "ui.log.channel.events",
        }
    }
}

/// One notable-result log entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// The day it happened.
    pub date: GameDate,
    /// Rendered text.
    pub text: String,
    /// The organisation involved, if any.
    pub org: Option<OrgId>,
    /// What the entry is about, for filtering and navigation.
    #[serde(default)]
    pub subject: Option<LogSubject>,
    /// The stream this entry belongs to.
    #[serde(default)]
    pub channel: LogChannel,
}

impl LogEntry {
    /// A new entry on a channel, with no subject or organisation yet.
    pub fn new(date: GameDate, text: impl Into<String>, channel: LogChannel) -> Self {
        Self {
            date,
            text: text.into(),
            org: None,
            subject: None,
            channel,
        }
    }

    /// A new entry without a date, for [`crate::access::log`] to stamp
    /// with the campaign date when it is appended.
    pub fn line(text: impl Into<String>, channel: LogChannel) -> Self {
        Self::new(GameDate::from_days(0), text, channel)
    }

    /// Attributes the entry to an organisation.
    pub fn by(mut self, org: Option<OrgId>) -> Self {
        self.org = org;
        self
    }

    /// Points the entry at the subject it is chiefly about.
    pub fn about(mut self, subject: LogSubject) -> Self {
        self.subject = Some(subject);
        self
    }
}

/// The notable-result message log: selective ongoing awareness of the
/// whole simulation, including other organisations' flagged results.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageLog {
    /// Entries in chronological order.
    pub entries: Vec<LogEntry>,
}

/// A player-facing popup waiting for an answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPopup {
    /// Monotonic popup id, for the answer command.
    pub id: u64,
    /// The day it opened.
    pub date: GameDate,
    /// The assignment definition involved.
    pub assignment: ContentKey,
    /// The result that opened it.
    pub result: OutcomeKind,
    /// Rendered situation text.
    pub text: String,
    /// Choice ids and labels; always at least one.
    pub choices: Vec<(ContentKey, String)>,
    /// Roles resolved at resolution time, for choice effects.
    pub roles: AssignmentRoles,
}

/// Popups awaiting player answers.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPopups {
    /// Next popup id to assign.
    pub next_id: u64,
    /// Open popups, oldest first.
    pub popups: Vec<PendingPopup>,
}

/// The sandboxed script host, recreated per process (never serialised).
#[derive(Resource)]
pub struct ScriptRuntime(pub ScriptHost);

/// The characters standing behind each script-effect role for one assignment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentRoles {
    /// `leader`.
    pub leader: Option<CharacterId>,
    /// `target` (a character target, or the target org's head).
    pub target: Option<CharacterId>,
    /// `target-head`.
    pub target_head: Option<CharacterId>,
    /// `owner-head`.
    pub owner_head: Option<CharacterId>,
    /// `liege-head`.
    pub liege_head: Option<CharacterId>,
    /// `consul`.
    pub consul: Option<CharacterId>,
    /// `sanctora`: every living Sanctora member.
    pub sanctora: Vec<CharacterId>,
    /// The province the assignment acted on, for province-scoped effects.
    #[serde(default)]
    pub province: Option<ProvinceId>,
}

/// What a role resolution starts from.
///
/// Whoever fires effects — assignment resolution, a popup answer, an event —
/// names the acting organisation and character, an explicit character
/// target, and where it happened. Everything else (owner-head,
/// liege-head, consul, sanctora, the leader fallback) is resolved the
/// same way for all of them by [`AssignmentRoles::resolve`], so the role
/// vocabulary cannot quietly mean different things in different systems.
#[derive(Clone, Copy, Debug, Default)]
pub struct RoleSeed {
    /// The organisation the action serves.
    pub owner: Option<OrgId>,
    /// The character leading it; falls back to the owner's head.
    pub leader: Option<CharacterId>,
    /// An explicit character target.
    pub target: Option<CharacterId>,
    /// The head of a targeted organisation.
    pub target_head: Option<CharacterId>,
    /// The province the action acted on.
    pub province: Option<ProvinceId>,
}

impl AssignmentRoles {
    /// The one resolver behind the effect-role vocabulary.
    pub fn resolve(world: &World, seed: RoleSeed) -> AssignmentRoles {
        let owner_record = seed.owner.and_then(|org| crate::access::org(world, org));
        let owner_head = owner_record.and_then(|record| record.head);
        let sanctora = match crate::access::sanctora_org(world) {
            Some(sanctora_org) => crate::access::living_character_ids(world)
                .into_iter()
                .filter(|id| crate::access::organisation_of(world, *id) == Some(sanctora_org))
                .collect(),
            None => Vec::new(),
        };
        AssignmentRoles {
            leader: seed.leader.or(owner_head),
            target: seed.target,
            target_head: seed.target_head,
            owner_head,
            liege_head: owner_record
                .and_then(|record| record.liege)
                .and_then(|liege| crate::access::org_head(world, liege)),
            consul: crate::access::consul(world),
            sanctora,
            province: seed.province,
        }
    }

    fn resolve_from(&self, role: EffectRole) -> Vec<CharacterId> {
        match role {
            EffectRole::Leader => self.leader.into_iter().collect(),
            EffectRole::Target => self.target.into_iter().collect(),
            EffectRole::TargetHead => self.target_head.into_iter().collect(),
            EffectRole::OwnerHead => self.owner_head.into_iter().collect(),
            EffectRole::LiegeHead => self.liege_head.into_iter().collect(),
            EffectRole::Consul => self.consul.into_iter().collect(),
            EffectRole::Sanctora => self.sanctora.clone(),
        }
    }

    fn resolve_toward(&self, role: EffectRole) -> Option<CharacterId> {
        self.resolve_from(role).first().copied()
    }
}

/// Why a assignment could not be started or answered.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AssignmentRejection {
    /// The campaign has ended.
    #[error("the campaign is over")]
    CampaignOver,
    /// No such assignment definition.
    #[error("unknown assignment definition '{0}'")]
    UnknownAssignment(ContentKey),
    /// The player has no house to act through.
    #[error("no player organisation")]
    NoPlayerOrg,
    /// The leader is not a living adult member of the organisation.
    #[error("that character cannot lead assignments for this organisation")]
    IneligibleLeader,
    /// The leader is already leading a assignment.
    #[error("that character is already leading a assignment")]
    LeaderBusy,
    /// The leader already holds a standing command over another force.
    #[error("that character already commands another force")]
    AlreadyAssigned,
    /// The leader is injured, captured, or incapacitated.
    #[error("that character is in no state to lead")]
    LeaderIndisposed,
    /// The target does not match the definition's target kind.
    #[error("the assignment's target is missing or of the wrong kind")]
    BadTarget,
    /// No such popup or choice.
    #[error("no such popup or choice")]
    BadPopupAnswer,
    /// No such active assignment, or it is not the player's to cancel.
    #[error("no such active assignment for your organisation")]
    BadAssignment,
    /// The organisation cannot pay the assignment's costs.
    #[error("the organisation cannot afford this assignment")]
    CannotAfford,
}

impl AssignmentRejection {
    /// The key of the player-facing sentence for this refusal.
    ///
    /// Separate from [`Display`], which stays the developer-facing
    /// message an error type owes its reader: the same refusal is both
    /// a diagnostic and something the player is shown at the slot.
    ///
    /// [`Display`]: core::fmt::Display
    pub fn label_key(&self) -> &'static str {
        match self {
            AssignmentRejection::CampaignOver => "sim.refusal.campaign-over",
            AssignmentRejection::UnknownAssignment(_) => "sim.refusal.unknown-assignment",
            AssignmentRejection::NoPlayerOrg => "sim.refusal.no-player-org",
            AssignmentRejection::IneligibleLeader => "sim.refusal.ineligible-leader",
            AssignmentRejection::LeaderBusy => "sim.refusal.leader-busy",
            AssignmentRejection::AlreadyAssigned => "sim.refusal.already-assigned",
            AssignmentRejection::LeaderIndisposed => "sim.refusal.leader-indisposed",
            AssignmentRejection::BadTarget => "sim.refusal.bad-target",
            AssignmentRejection::BadPopupAnswer => "sim.refusal.bad-popup-answer",
            AssignmentRejection::BadAssignment => "sim.refusal.bad-assignment",
            AssignmentRejection::CannotAfford => "sim.refusal.cannot-afford",
        }
    }
}

// ---------------------------------------------------------------------------
// Eligibility and start
// ---------------------------------------------------------------------------

/// A standing command a character already holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Post {
    /// They command an army.
    General {
        /// The army.
        army: ArmyId,
        /// Its name, for saying so.
        name: String,
    },
    /// They captain a ship.
    Captain {
        /// The ship.
        ship: ShipId,
        /// Its name, for saying so.
        name: String,
    },
}

impl Post {
    /// A short phrase naming the command, for the interface.
    pub fn describe(&self, strings: &TextDb) -> String {
        match self {
            Post::General { name, .. } => {
                strings.format("sim.assignment.general", &[("force", name)])
            }
            Post::Captain { name, .. } => {
                strings.format("sim.assignment.captain", &[("force", name)])
            }
        }
    }
}

/// What a character is doing, and whether it leaves them free to lead.
///
/// This is the single answer to "can they take this on", shared by the
/// simulation's own validation and by every part of the interface that
/// offers a choice of leader. Keeping one implementation is the point:
/// three separate ones drifted apart and offered the player characters
/// who could not act.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LeaderAvailability {
    /// Free to take on new work.
    Available,
    /// Already leading a assignment.
    Busy {
        /// The assignment they are leading.
        assignment: AssignmentId,
        /// Its definition, for naming it.
        def: ContentKey,
        /// When they will be free.
        completes: GameDate,
    },
    /// Injured, captured, or incapacitated.
    Indisposed {
        /// When they recover, if that is known.
        until: Option<GameDate>,
    },
    /// Holding a standing command. Not a blocker in itself — a general may
    /// still lead a diplomatic assignment — but it bars a *second* command.
    Posted(Post),
    /// Cannot lead for this organisation at all: dead, not a member, or
    /// not yet of age.
    Ineligible(AssignmentRejection),
}

impl LeaderAvailability {
    /// Whether the character is free of any commitment.
    pub fn is_available(&self) -> bool {
        matches!(self, LeaderAvailability::Available)
    }

    /// Why this character cannot lead *this* assignment, if they cannot.
    ///
    /// A standing command only bars taking on another one; it does not
    /// stop its holder doing something else entirely.
    pub fn blocks_assignment(&self, target: AssignmentTarget) -> Option<AssignmentRejection> {
        match self {
            LeaderAvailability::Available => None,
            LeaderAvailability::Busy { .. } => Some(AssignmentRejection::LeaderBusy),
            LeaderAvailability::Indisposed { .. } => Some(AssignmentRejection::LeaderIndisposed),
            LeaderAvailability::Ineligible(rejection) => Some(rejection.clone()),
            LeaderAvailability::Posted(assignment) => match (assignment, target) {
                // Ordering the force they already command is the point.
                (Post::General { army, .. }, AssignmentTarget::OwnArmy(ordered))
                | (Post::General { army, .. }, AssignmentTarget::ArmyToProvince(ordered, _))
                    if *army == ordered =>
                {
                    None
                }
                (Post::Captain { ship, .. }, AssignmentTarget::ShipToProvince(ordered, _))
                    if *ship == ordered =>
                {
                    None
                }
                // Commanding one force does not let you command another.
                (_, AssignmentTarget::OwnArmy(_))
                | (_, AssignmentTarget::ArmyToProvince(_, _))
                | (_, AssignmentTarget::ShipToProvince(_, _)) => {
                    Some(AssignmentRejection::AlreadyAssigned)
                }
                // Anything else is ordinary work they are free to do.
                _ => None,
            },
        }
    }

    /// A short player-facing phrase, for showing beside a name.
    pub fn describe(
        &self,
        strings: &TextDb,
        assignment_title: impl Fn(&ContentKey) -> String,
    ) -> String {
        match self {
            LeaderAvailability::Available => strings.text("sim.leader.available").to_owned(),
            LeaderAvailability::Busy { def, completes, .. } => strings.format(
                "sim.leader.busy",
                &[
                    ("assignment", &assignment_title(def)),
                    ("date", &completes.to_string()),
                ],
            ),
            LeaderAvailability::Indisposed { until: Some(until) } => strings.format(
                "sim.leader.indisposed-until",
                &[("date", &until.to_string())],
            ),
            LeaderAvailability::Indisposed { until: None } => {
                strings.text("sim.leader.indisposed").to_owned()
            }
            LeaderAvailability::Posted(assignment) => assignment.describe(strings),
            LeaderAvailability::Ineligible(rejection) => {
                strings.text(rejection.label_key()).to_owned()
            }
        }
    }
}

/// What a character is currently committed to, for `org`'s purposes.
///
/// Reports the most limiting commitment: ineligibility first, then
/// indisposition, then an active assignment, then a standing command.
pub fn leader_availability(
    world: &World,
    org: OrgId,
    leader: CharacterId,
    date: GameDate,
) -> LeaderAvailability {
    let Some(record) = crate::access::character(world, leader) else {
        return LeaderAvailability::Ineligible(AssignmentRejection::IneligibleLeader);
    };
    if !record.alive()
        || record.organisation != Some(org)
        || record.age_years(date) < crate::politics::ADULT_AGE
    {
        return LeaderAvailability::Ineligible(AssignmentRejection::IneligibleLeader);
    }

    let condition = crate::access::on_character::<CharacterCondition>(world, leader)
        .copied()
        .unwrap_or_default();
    if !condition.can_lead(date) {
        // Report the soonest day they are free of every condition.
        let until = [
            condition.injured_until,
            condition.captured_until,
            condition.incapacitated_until,
        ]
        .into_iter()
        .flatten()
        .filter(|day| *day > date)
        .max();
        return LeaderAvailability::Indisposed { until };
    }

    if let Some(assignments) = world.get_resource::<AssignmentsIndex>() {
        // Stable-ID order, so the reported assignment never depends on iteration.
        for entity in assignments.assignments.values() {
            if let Some(assignment) = world.get::<ActiveAssignment>(*entity)
                && assignment.leader == leader
            {
                return LeaderAvailability::Busy {
                    assignment: assignment.id,
                    def: assignment.def.clone(),
                    completes: assignment.completes,
                };
            }
        }
    }

    if let Some(forces) = world.get_resource::<crate::forces::ForcesIndex>() {
        for entity in forces.armies.values() {
            if let Some(army) = world.get::<crate::forces::ArmyRecord>(*entity)
                && army.general == leader
            {
                return LeaderAvailability::Posted(Post::General {
                    army: army.id,
                    name: army.name.clone(),
                });
            }
        }
        for entity in forces.ships.values() {
            if let Some(ship) = world.get::<crate::forces::ShipRecord>(*entity)
                && ship.captain == Some(leader)
            {
                return LeaderAvailability::Posted(Post::Captain {
                    ship: ship.id,
                    name: ship.name.clone(),
                });
            }
        }
    }

    LeaderAvailability::Available
}

fn leader_eligible(
    world: &World,
    org: OrgId,
    leader: CharacterId,
    date: GameDate,
    target: AssignmentTarget,
) -> Result<(), AssignmentRejection> {
    match leader_availability(world, org, leader, date).blocks_assignment(target) {
        Some(rejection) => Err(rejection),
        None => Ok(()),
    }
}

fn owned_army(world: &World, owner: OrgId, army: ArmyId) -> bool {
    crate::access::army(world, army).is_some_and(|record| record.owner == owner)
}

fn target_valid(
    world: &World,
    def: &AssignmentDef,
    owner: OrgId,
    target: AssignmentTarget,
) -> bool {
    let province_known = |id: ProvinceId| crate::access::province_entity(world, id).is_some();
    // Two questions, kept apart: is this the *shape* of target the
    // assignment takes, and is this particular target a legal one.
    let shaped = match (def.target, target) {
        (AssignmentTargetKind::None, AssignmentTarget::None) => true,
        (AssignmentTargetKind::Character, AssignmentTarget::Character(id)) => {
            crate::access::character(world, id).is_some_and(|r| r.alive())
        }
        (AssignmentTargetKind::Organisation, AssignmentTarget::Org(id)) => {
            id != owner && crate::access::org_entity(world, id).is_some()
        }
        (AssignmentTargetKind::Province, AssignmentTarget::Province(id)) => province_known(id),
        (AssignmentTargetKind::OwnArmy, AssignmentTarget::OwnArmy(army)) => {
            owned_army(world, owner, army)
        }
        (
            AssignmentTargetKind::OwnArmyAndProvince,
            AssignmentTarget::ArmyToProvince(army, province),
        ) => owned_army(world, owner, army) && province_known(province),
        (
            AssignmentTargetKind::OwnShipAndProvince,
            AssignmentTarget::ShipToProvince(ship, province),
        ) => {
            province_known(province)
                && crate::access::ship(world, ship).is_some_and(|record| record.owner == owner)
        }
        _ => false,
    };
    shaped && requirements_met(world, def, owner, target)
}

/// Asks for an assignment to be called off.
///
/// Whether that happens now depends on the phase it is in. During an
/// interruptible one it ends immediately, running whatever the phase says
/// being called off costs. During one that is not, the request is
/// recorded and honoured the moment the phase ends — an army mid-assault
/// does not turn round because a message arrived.
///
/// Recording rather than refusing matters: a click that appears to do
/// nothing is indistinguishable from a click that was not registered.
pub fn request_cancel(world: &mut World, assignment: AssignmentId) {
    let Some(entity) = crate::access::assignment_entity(world, assignment) else {
        return;
    };
    let date = world.resource::<CampaignClock>().date;
    let Some(active) = world.get::<ActiveAssignment>(entity).cloned() else {
        return;
    };
    let content = world.resource::<ContentDb>().0.clone();
    let Some(def) = content.assignments.get(&active.def) else {
        return;
    };

    if active.interruptible_on(def, date) {
        end_interrupted(world, entity, &active, def, date);
        return;
    }
    if let Some(mut record) = world.get_mut::<ActiveAssignment>(entity) {
        record.cancel_requested = true;
    }
}

/// Ends an assignment that has been called off, running the phase's own
/// account of what that costs.
fn end_interrupted(
    world: &mut World,
    entity: Entity,
    active: &ActiveAssignment,
    def: &AssignmentDef,
    date: GameDate,
) {
    let stage = active.stage(def, date);
    if let Some(on_interrupt) = def
        .stages
        .get(stage)
        .and_then(|stage| stage.on_interrupt.clone())
    {
        // The same path a result's effect takes, so being called off can
        // do anything an outcome can and is subject to the same rules.
        let roles = resolve_roles(world, active);
        let target_label = target_name(world, active.target);
        let content = world.resource::<ContentDb>().0.clone();
        let ctx = effect_context(
            world,
            &active.def,
            "interrupted",
            Some(active.leader),
            &target_label,
        );
        let effects = {
            let runtime = world.resource::<ScriptRuntime>();
            runtime.0.call_effect_fn(&content, &on_interrupt, ctx)
        };
        if let Ok(effects) = effects {
            apply_effects(world, &effects, &roles, Some(active.owner));
        }
    }
    log_interrupted(world, active, def, stage);
    world.despawn(entity);
    world
        .resource_mut::<AssignmentsIndex>()
        .assignments
        .remove(&active.id);
}

/// Says in the log that an assignment was called off, and from where.
fn log_interrupted(
    world: &mut World,
    active: &ActiveAssignment,
    def: &AssignmentDef,
    stage: usize,
) {
    let date = world.resource::<CampaignClock>().date;
    let strings = world.resource::<TextDb>().clone();
    let name = strings.text(&format!("assignment.{}.title", active.def));
    let phase = def
        .stages
        .get(stage)
        .map(|stage| stage.id.clone())
        .unwrap_or_default();
    let text = strings.format(
        "sim.assignment.called-off",
        &[("assignment", name), ("stage", &phase)],
    );
    let _ = date;
    crate::access::log(
        world,
        LogEntry {
            date: world.resource::<CampaignClock>().date,
            text,
            org: Some(active.owner),
            subject: Some(LogSubject::Character(active.leader)),
            channel: LogChannel::Assignments,
        },
    );
}

/// Whether this target is a legal one for this assignment.
///
/// Public so the interface can ask the question it is about to offer the
/// player, rather than deciding for itself and drifting. It takes no
/// leader: whether a target is legal has nothing to do with who would go.
pub fn target_allowed(
    world: &World,
    def_key: &ContentKey,
    owner: OrgId,
    target: AssignmentTarget,
) -> bool {
    let Some(content) = world.get_resource::<ContentDb>() else {
        return false;
    };
    let Some(def) = content.0.assignments.get(def_key) else {
        return false;
    };
    let def = def.clone();
    target_valid(world, &def, owner, target)
}

/// Whether the authored requirements hold between this owner and target.
///
/// The one place a relational condition is asked. The button, the
/// forecast, an autonomous house and a standing order all reach the
/// simulation through `validate_start`, which reaches here — so none of
/// them can offer something the others would refuse.
///
/// Before this existed, the only check was that the target *existed*: a
/// province was a legal raid target because it was a province, which is
/// how raiding your own holdings came to be on the menu.
fn requirements_met(
    world: &World,
    def: &AssignmentDef,
    owner: OrgId,
    target: AssignmentTarget,
) -> bool {
    let requires = &def.requires;

    // Whichever province this assignment ultimately acts on.
    let province = match target {
        AssignmentTarget::Province(id) => Some(id),
        AssignmentTarget::ArmyToProvince(_, id) => Some(id),
        AssignmentTarget::ShipToProvince(_, id) => Some(id),
        AssignmentTarget::OwnArmy(army) => {
            crate::access::army(world, army).map(|record| record.location)
        }
        _ => None,
    };

    if requires.target_holder != HolderRelation::Any {
        let Some(province) = province else {
            return false;
        };
        let holder = crate::warfare::province_holder(world, province);
        let ok = match requires.target_holder {
            // Unheld ground belongs to nobody, so it is not somebody
            // else's: there is nobody there to raid.
            HolderRelation::Other => holder.is_some_and(|held| held != owner),
            HolderRelation::Own => holder == Some(owner),
            HolderRelation::Any => true,
        };
        if !ok {
            return false;
        }
    }

    if requires.target_occupied {
        let Some(province) = province else {
            return false;
        };
        if !hostile_force_in(world, owner, province) {
            return false;
        }
    }

    if requires.army_present {
        let at = match target {
            AssignmentTarget::OwnArmy(army) | AssignmentTarget::ArmyToProvince(army, _) => {
                crate::access::army(world, army).map(|record| record.location)
            }
            _ => None,
        };
        if at.is_none() || at != province {
            return false;
        }
    }

    if requires.target_house != HolderRelation::Any {
        let AssignmentTarget::Character(id) = target else {
            return false;
        };
        let house = crate::access::character(world, id).and_then(|record| record.organisation);
        let ok = match requires.target_house {
            HolderRelation::Own => house == Some(owner),
            HolderRelation::Other => house.is_some_and(|house| house != owner),
            HolderRelation::Any => true,
        };
        if !ok {
            return false;
        }
    }

    if let Some(need) = requires.target_holds_title {
        let AssignmentTarget::Character(id) = target else {
            return false;
        };
        if !holds_title(world, id, need) {
            return false;
        }
    }

    if requires.target_owes_favour {
        let AssignmentTarget::Org(debtor) = target else {
            return false;
        };
        let owed = world
            .get_resource::<crate::obligations::Obligations>()
            .is_some_and(|ledger| ledger.owed(debtor, owner).next().is_some());
        if !owed {
            return false;
        }
    }

    // About the owner rather than the target: what stops an assignment
    // that answers an alarm being offered when none is sounding.
    if requires.owner_threatened && crate::warfare::threatened_holdings(world, owner).is_empty() {
        return false;
    }

    if requires.max_order.is_some() || requires.min_order.is_some() {
        let Some(province) = province else {
            return false;
        };
        let order = crate::order::province_order(world, province).order;
        if requires.max_order.is_some_and(|max| order > max) {
            return false;
        }
        if requires.min_order.is_some_and(|min| order < min) {
            return false;
        }
    }

    true
}

/// Whether a force belonging to anyone but `owner` stands in `province`.
fn hostile_force_in(world: &World, owner: OrgId, province: ProvinceId) -> bool {
    let Some(forces) = world.get_resource::<crate::forces::ForcesIndex>() else {
        return false;
    };
    forces.armies.values().any(|entity| {
        world
            .get::<crate::forces::ArmyRecord>(*entity)
            .is_some_and(|army| army.location == province && army.owner != owner)
    })
}

/// Whether a character holds a title of the required kind.
fn holds_title(world: &World, character: CharacterId, need: TitleNeed) -> bool {
    let Some(index) = world.get_resource::<crate::politics::PoliticsIndex>() else {
        return false;
    };
    index.titles.values().any(|entity| {
        world
            .get::<crate::politics::TitleRecord>(*entity)
            .is_some_and(|title| {
                title.holder == crate::politics::TitleHolder::Character(character)
                    && match need {
                        TitleNeed::Consul => {
                            matches!(title.kind, crate::politics::TitleKind::Consul)
                        }
                        TitleNeed::Paramount => {
                            matches!(title.kind, crate::politics::TitleKind::Paramount(_))
                        }
                        TitleNeed::Province => {
                            matches!(title.kind, crate::politics::TitleKind::Province(_))
                        }
                    }
            })
    })
}

/// Validates a start-assignment request for an organisation.
pub fn validate_start(
    world: &World,
    org: OrgId,
    def_key: &ContentKey,
    leader: CharacterId,
    target: AssignmentTarget,
) -> Result<(), AssignmentRejection> {
    if world.get_resource::<CampaignOver>().is_some() {
        return Err(AssignmentRejection::CampaignOver);
    }
    let content = world.resource::<ContentDb>().0.clone();
    let Some(def) = content.assignments.get(def_key) else {
        return Err(AssignmentRejection::UnknownAssignment(def_key.clone()));
    };
    let date = world.resource::<CampaignClock>().date;
    leader_eligible(world, org, leader, date, target)?;
    if !target_valid(world, def, org, target) {
        return Err(AssignmentRejection::BadTarget);
    }
    // A ship is ordered by its captain, nobody else — the same rule
    // armies have always had for their general.
    if let AssignmentTarget::ShipToProvince(ship, _) = target {
        let captain = crate::access::ship(world, ship).and_then(|record| record.captain);
        if captain != Some(leader) {
            return Err(AssignmentRejection::IneligibleLeader);
        }
    }
    // Army operations are led by the army's general, nobody else.
    if let AssignmentTarget::OwnArmy(army) | AssignmentTarget::ArmyToProvince(army, _) = target {
        let general = crate::access::army(world, army).map(|record| record.general);
        if general != Some(leader) {
            return Err(AssignmentRejection::IneligibleLeader);
        }
    }
    let affordable = crate::access::org_entity(world, org)
        .and_then(|e| world.get::<crate::economy::OrgResources>(e))
        .is_some_and(|r| {
            r.can_afford(
                def.wealth_cost,
                def.manpower_cost,
                def.supplies_cost,
                def.influence_cost,
            )
        });
    if !affordable {
        return Err(AssignmentRejection::CannotAfford);
    }
    Ok(())
}

/// Starts a assignment for an organisation. Callers must have validated.
pub fn start_assignment(
    world: &mut World,
    org: OrgId,
    def_key: &ContentKey,
    leader: CharacterId,
    target: AssignmentTarget,
) -> AssignmentId {
    let date = world.resource::<CampaignClock>().date;
    let (duration, costs) = {
        let content = world.resource::<ContentDb>().0.clone();
        let def = &content.assignments[def_key];
        (
            crate::forecast::assignment_duration_days(world, def, target),
            (
                def.wealth_cost,
                def.manpower_cost,
                def.supplies_cost,
                def.influence_cost,
            ),
        )
    };
    {
        let org_entity = crate::access::org_entity(world, org).expect("indexed");
        if let Some(mut resources) = world.get_mut::<crate::economy::OrgResources>(org_entity) {
            resources.spend(costs.0, costs.1, costs.2, costs.3);
        }
    }
    let id: AssignmentId = world.resource_mut::<CampaignIds>().0.allocate();
    let entity = world
        .spawn(ActiveAssignment {
            id,
            def: def_key.clone(),
            owner: org,
            leader,
            target,
            started: date,
            completes: date.add_days(duration),
            cancel_requested: false,
        })
        .id();
    world
        .resource_mut::<AssignmentsIndex>()
        .assignments
        .insert(id, entity);
    id
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

// Outcome weighting, sampling, duration, and risk chances live in
// `forecast`, so the numbers quoted to the player before a assignment are the
// numbers used to resolve it.
use crate::forecast::risk_permille;

/// The character standing behind each effect role for a assignment.
fn resolve_roles(world: &World, assignment: &ActiveAssignment) -> AssignmentRoles {
    let (target, target_head) = match assignment.target {
        AssignmentTarget::Character(id) => (Some(id), None),
        AssignmentTarget::Org(org) => {
            let head = crate::access::org_head(world, org);
            (head, head)
        }
        _ => (None, None),
    };

    // Where the assignment acted: an explicit province target, the province an
    // ordered force stands in, or failing both the leader's own location.
    let province = match assignment.target {
        AssignmentTarget::Province(province)
        | AssignmentTarget::ArmyToProvince(_, province)
        | AssignmentTarget::ShipToProvince(_, province) => Some(province),
        AssignmentTarget::OwnArmy(army) => {
            crate::access::army(world, army).map(|record| record.location)
        }
        _ => match crate::presence::character_location(world, assignment.leader) {
            Some(crate::presence::Location::Province(province)) => Some(province),
            _ => None,
        },
    };

    AssignmentRoles::resolve(
        world,
        RoleSeed {
            owner: Some(assignment.owner),
            leader: Some(assignment.leader),
            target,
            target_head,
            province,
        },
    )
}

/// The read-only context map every effect function receives.
///
/// One schema for every invocation — assignment results, popup choices, event
/// firings, and event answers — so an authored function can rely on the
/// same fields wherever it is called from:
/// - `source`: the assignment or event definition key
/// - `result`: the result kind or the chosen option, as text
/// - `leader`: the leading character's display name, possibly empty
/// - `target`: what the action acted on, as a display label, possibly empty
pub(crate) fn effect_context(
    world: &World,
    source: &ContentKey,
    result: &str,
    leader: Option<CharacterId>,
    target_label: &str,
) -> rhai::Map {
    let mut ctx = rhai::Map::new();
    ctx.insert("source".into(), source.as_str().into());
    ctx.insert("result".into(), result.into());
    ctx.insert(
        "leader".into(),
        leader
            .map(|id| crate::access::character_name(world, id))
            .unwrap_or_default()
            .into(),
    );
    ctx.insert("target".into(), target_label.into());
    ctx
}

fn target_name(world: &World, target: AssignmentTarget) -> String {
    match target {
        AssignmentTarget::None => String::new(),
        AssignmentTarget::Character(id) => crate::access::character_name(world, id),
        AssignmentTarget::Org(org) => crate::access::org_name(world, org),
        AssignmentTarget::Province(id) => crate::access::province_name(world, id),
        AssignmentTarget::OwnArmy(army) => crate::access::army(world, army)
            .map(|record| record.name.clone())
            .unwrap_or_default(),
        AssignmentTarget::ArmyToProvince(_, province)
        | AssignmentTarget::ShipToProvince(_, province) => {
            crate::access::province_name(world, province)
        }
    }
}

fn render_template(template: &str, leader: &str, target: &str, assignment_title: &str) -> String {
    template
        .replace("{leader}", leader)
        .replace("{target}", target)
        .replace("{assignment}", assignment_title)
}

/// Applies parsed script effects against resolved roles.
pub fn apply_effects(
    world: &mut World,
    effects: &[ScriptEffect],
    roles: &AssignmentRoles,
    owner: Option<OrgId>,
) {
    let date = world.resource::<CampaignClock>().date;
    for effect in effects {
        match effect {
            ScriptEffect::Log { message_key } => {
                // Templated like every other authored line, so a row can
                // name the people involved rather than the script having
                // to build the sentence by concatenation.
                let leader = roles
                    .leader
                    .map(|id| crate::access::character_name(world, id))
                    .unwrap_or_default();
                let target = roles
                    .target
                    .map(|id| crate::access::character_name(world, id))
                    .unwrap_or_default();
                let template = world.resource::<TextDb>().text(message_key).to_owned();
                let line = render_template(&template, &leader, &target, "");
                crate::access::log(
                    world,
                    LogEntry::line(line, LogChannel::Assignments).by(owner),
                );
            }
            ScriptEffect::FormArmy { manpower, supplies } => {
                let Some(owner) = owner else {
                    continue;
                };
                let Some(general) = roles.leader else {
                    continue;
                };
                // Validate against and deduct from the owner's pool;
                // clamp to what actually exists.
                let (manpower, supplies) = {
                    let org_entity = crate::access::org_entity(world, owner).expect("indexed");
                    let Some(mut resources) =
                        world.get_mut::<crate::economy::OrgResources>(org_entity)
                    else {
                        continue;
                    };
                    let manpower = (*manpower).clamp(0, resources.manpower);
                    let supplies = (*supplies).clamp(0, resources.supplies);
                    resources.manpower -= manpower;
                    resources.supplies -= supplies;
                    (manpower, supplies)
                };
                if manpower == 0 {
                    continue;
                }
                // The army musters where its general stands; a general in
                // transit musters at the organisation's first holding.
                let location = match crate::presence::character_location(world, general) {
                    Some(crate::presence::Location::Province(province)) => Some(province),
                    _ => world
                        .resource::<crate::map::MapIndex>()
                        .province_ids
                        .keys()
                        .next()
                        .copied(),
                };
                if let Some(location) = location {
                    crate::forces::form_army(world, owner, general, manpower, supplies, location);
                }
            }
            ScriptEffect::Opinion {
                from,
                toward,
                amount,
                days,
                reason,
            } => {
                let Some(toward_id) = roles.resolve_toward(*toward) else {
                    continue;
                };
                let expires = days.map(|d| date.add_days(d));
                for from_id in roles.resolve_from(*from) {
                    if from_id == toward_id {
                        continue;
                    }
                    let entity = crate::access::character_entity(world, from_id).expect("indexed");
                    if let Some(mut ledger) = world.get_mut::<OpinionLedger>(entity) {
                        ledger.set(OpinionEntry {
                            target: toward_id,
                            amount: *amount,
                            reason: reason.clone(),
                            expires,
                        });
                    }
                }
            }
            ScriptEffect::ClaimParamountcy => {
                if let Some(owner) = owner {
                    crate::crisis::claim_paramountcy(world, owner);
                }
            }
            ScriptEffect::CollectTithes => {
                if let Some(owner) = owner {
                    crate::crisis::collect_tithes(world, owner);
                }
            }
            ScriptEffect::Obligation {
                action,
                kind,
                debtor,
                creditor,
                weight,
                days,
                origin,
            } => {
                // Obligations bind houses, so each role resolves to the
                // organisation behind the character standing in it.
                let house_of = |role: EffectRole| -> Option<OrgId> {
                    crate::access::organisation_of(world, roles.resolve_toward(role)?)
                };
                if let (Some(debtor), Some(creditor)) = (house_of(*debtor), house_of(*creditor)) {
                    match action {
                        aeon_data::effect::ObligationAction::Create => {
                            crate::obligations::create(
                                world,
                                *kind,
                                debtor,
                                creditor,
                                origin.clone(),
                                *weight,
                                *days,
                            );
                        }
                        aeon_data::effect::ObligationAction::Fulfil => {
                            crate::obligations::settle(
                                world,
                                *kind,
                                debtor,
                                creditor,
                                crate::obligations::ObligationStatus::Fulfilled,
                            );
                        }
                        aeon_data::effect::ObligationAction::Break => {
                            crate::obligations::settle(
                                world,
                                *kind,
                                debtor,
                                creditor,
                                crate::obligations::ObligationStatus::Broken,
                            );
                        }
                    }
                }
            }
            ScriptEffect::Order { scope, amount } => match scope {
                aeon_data::effect::OrderScope::TargetProvince => {
                    if let Some(province) = roles.province {
                        crate::order::adjust_order(world, province, *amount);
                    }
                }
                aeon_data::effect::OrderScope::AllHeld => {
                    if let Some(owner) = owner {
                        for province in crate::order::held_provinces(world, owner) {
                            crate::order::adjust_order(world, province, *amount);
                        }
                    }
                }
            },
            ScriptEffect::Condition { target, tag } => {
                // The same harm a failed leader suffers, laid on the role
                // the content names — a knife that reaches its mark.
                for who in roles.resolve_from(*target) {
                    apply_risk(world, who, *tag, date);
                }
            }
            ScriptEffect::Wreck => {
                // Pull down what the target province most recently built.
                if let Some(province) = roles.province
                    && let Some(entity) = world
                        .resource::<crate::map::MapIndex>()
                        .provinces
                        .get(&province)
                        .copied()
                    && let Some(mut buildings) = world.get_mut::<crate::trade::Buildings>(entity)
                {
                    buildings.0.pop();
                }
            }
            ScriptEffect::Construct { building } => {
                // Raise the building on the province the work was aimed
                // at. Only what content defines can be built.
                if let Some(province) = roles.province
                    && let Ok(key) = ContentKey::new(building)
                    && world.resource::<ContentDb>().0.buildings.contains_key(&key)
                    && let Some(entity) = world
                        .resource::<crate::map::MapIndex>()
                        .provinces
                        .get(&province)
                        .copied()
                    && let Some(mut buildings) = world.get_mut::<crate::trade::Buildings>(entity)
                {
                    buildings.0.push(key);
                }
            }
        }
    }
}

/// Applies one personal-risk consequence to a character. Public so event
/// systems (and tests) can reuse the exact assignment-risk semantics.
pub fn apply_risk(world: &mut World, leader: CharacterId, tag: RiskTag, date: GameDate) {
    let entity = crate::access::character_entity(world, leader).expect("indexed");
    match tag {
        RiskTag::Injury => {
            let mut condition = world
                .get_mut::<CharacterCondition>(entity)
                .expect("characters carry conditions");
            condition.injured_until = Some(date.add_days(90));
        }
        RiskTag::Capture => {
            let mut condition = world
                .get_mut::<CharacterCondition>(entity)
                .expect("characters carry conditions");
            condition.captured_until = Some(date.add_days(360));
        }
        RiskTag::Incapacity => {
            let mut condition = world
                .get_mut::<CharacterCondition>(entity)
                .expect("characters carry conditions");
            condition.incapacitated_until = Some(date.add_days(180));
        }
        RiskTag::Scandal => {
            // Every living organisation head thinks less of the leader.
            let heads: Vec<CharacterId> = crate::access::org_ids(world)
                .into_iter()
                .filter_map(|org| crate::access::org_head(world, org))
                .filter(|h| *h != leader)
                .collect();
            for head in heads {
                let head_entity = crate::access::character_entity(world, head).expect("indexed");
                if let Some(mut ledger) = world.get_mut::<OpinionLedger>(head_entity) {
                    ledger.set(OpinionEntry {
                        target: leader,
                        amount: -15,
                        reason: "scandal".to_owned(),
                        expires: Some(date.add_days(720)),
                    });
                }
            }
        }
        RiskTag::Death => {
            process_death(world, leader, date);
        }
    }
}

/// Ends assignments whose cancellation was asked for during a phase that
/// could not be interrupted, and which now can be.
///
/// Runs before the day's resolutions, so an assignment called off on the
/// first day it becomes interruptible does not also resolve.
fn honour_deferred_cancels(world: &mut World, date: GameDate) {
    let waiting: Vec<Entity> = world
        .resource::<AssignmentsIndex>()
        .assignments
        .values()
        .filter(|entity| {
            world
                .get::<ActiveAssignment>(**entity)
                .is_some_and(|active| active.cancel_requested)
        })
        .copied()
        .collect();

    let content = world.resource::<ContentDb>().0.clone();
    for entity in waiting {
        let Some(active) = world.get::<ActiveAssignment>(entity).cloned() else {
            continue;
        };
        let Some(def) = content.assignments.get(&active.def) else {
            continue;
        };
        if active.interruptible_on(def, date) {
            end_interrupted(world, entity, &active, def, date);
        }
    }
}

/// Daily: resolves every assignment due today, in stable-ID order.
pub fn resolve_due_assignments(world: &mut World) {
    if world.get_resource::<AssignmentsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    honour_deferred_cancels(world, date);
    let player = world.get_resource::<PlayerHouse>().and_then(|p| p.0);

    let due: Vec<(AssignmentId, Entity)> = world
        .resource::<AssignmentsIndex>()
        .assignments
        .iter()
        .filter(|(_, entity)| {
            world
                .get::<ActiveAssignment>(**entity)
                .is_some_and(|assignment| assignment.completes <= date)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect();

    for (assignment_id, entity) in due {
        let assignment = world
            .get::<ActiveAssignment>(entity)
            .expect("indexed")
            .clone();
        let content = world.resource::<ContentDb>().0.clone();
        let def = content.assignments[&assignment.def].clone();

        // A dead or removed leader abandons the assignment.
        let leader_alive =
            crate::access::character(world, assignment.leader).is_some_and(|r| r.alive());
        if !leader_alive {
            crate::access::log(
                world,
                LogEntry::line(
                    format!("'{}' was abandoned; its leader is gone.", def.title),
                    LogChannel::Assignments,
                )
                .by(Some(assignment.owner))
                .about(LogSubject::Character(assignment.leader)),
            );
            world.despawn(entity);
            world
                .resource_mut::<AssignmentsIndex>()
                .assignments
                .remove(&assignment_id);
            // A plan waiting on this assignment stops waiting; its own
            // abandon checks answer for the missing leader.
            crate::plans::note_dropped(world, assignment_id);
            continue;
        }

        // Outcome, drawn by the same sampler the forecast describes.
        let effectiveness = crate::forecast::effectiveness(world, assignment.leader, &def);
        // The purpose label is a stream identity, not a name. It is
        // hashed into the seed, so changing it re-rolls every outcome in
        // every campaign ever played. It stays spelled the way it was
        // first written, and survives the rename deliberately.
        let mut rng = crate::access::derived_rng(
            world,
            "job-resolution",
            &[assignment_id.raw(), date.days_since_epoch() as u64],
        );
        let mut outcome = crate::forecast::resolve_outcome(&def, effectiveness, &mut rng);
        if matches!(outcome, OutcomeKind::Success | OutcomeKind::CriticalSuccess)
            && let Some(op) = def.military_op
            && !crate::warfare::apply_military_op(world, op, &assignment)
        {
            // The operation was defeated in the field.
            outcome = OutcomeKind::Failure;
        }
        let result = def
            .results
            .get(&outcome)
            .cloned()
            .unwrap_or_else(|| def.results[&OutcomeKind::Failure].clone());

        // Personal risks on bad outcomes.
        if matches!(outcome, OutcomeKind::Failure | OutcomeKind::Disaster) {
            let disaster = outcome == OutcomeKind::Disaster;
            for (index, tag) in def.risks.iter().enumerate() {
                // Frozen for the same reason as the resolution stream.
                let mut risk_rng = crate::access::derived_rng(
                    world,
                    "job-risk",
                    &[
                        assignment_id.raw(),
                        date.days_since_epoch() as u64,
                        index as u64,
                    ],
                );
                if risk_rng.check_permille(risk_permille(*tag, disaster)) {
                    apply_risk(world, assignment.leader, *tag, date);
                }
            }
        }

        let roles = resolve_roles(world, &assignment);
        let leader_name = crate::access::character_name(world, assignment.leader);
        let target_label = target_name(world, assignment.target);

        // Authored result effects.
        if let Some(fn_ref) = &result.effect_fn {
            let ctx = effect_context(
                world,
                &assignment.def,
                &format!("{outcome:?}"),
                Some(assignment.leader),
                &target_label,
            );
            let effects = {
                let runtime = world.resource::<ScriptRuntime>();
                runtime.0.call_effect_fn(&content, fn_ref, ctx)
            };
            match effects {
                Ok(effects) => apply_effects(world, &effects, &roles, Some(assignment.owner)),
                Err(err) => {
                    crate::access::log(
                        world,
                        LogEntry::line(
                            format!("script error resolving '{}': {err}", def.title),
                            LogChannel::Assignments,
                        )
                        .by(Some(assignment.owner)),
                    );
                }
            }
        }

        // Notable-result log (all organisations).
        if result.log {
            let text = result
                .log_text
                .as_deref()
                .map(|t| render_template(t, &leader_name, &target_label, &def.title))
                .unwrap_or_else(|| {
                    format!("{leader_name} finished '{}' ({outcome:?}).", def.title)
                });
            crate::access::log(
                world,
                LogEntry::line(text, LogChannel::Assignments)
                    .by(Some(assignment.owner))
                    .about(LogSubject::Character(assignment.leader)),
            );
        }

        // Player popups.
        if result.popup && player == Some(assignment.owner) {
            let text = result
                .popup_text
                .as_deref()
                .map(|t| render_template(t, &leader_name, &target_label, &def.title))
                .unwrap_or_else(|| format!("'{}' resolved: {outcome:?}.", def.title));
            let choices = if result.choices.is_empty() {
                vec![(
                    ContentKey::new("continue").expect("static key"),
                    "Continue".to_owned(),
                )]
            } else {
                result
                    .choices
                    .iter()
                    .map(|c| (c.id.clone(), c.label.clone()))
                    .collect()
            };
            let mut popups = world.resource_mut::<PendingPopups>();
            let id = popups.next_id;
            popups.next_id += 1;
            popups.popups.push(PendingPopup {
                id,
                date,
                assignment: assignment.def.clone(),
                result: outcome,
                text,
                choices,
                roles: roles.clone(),
            });
        }

        // Routine failures restart; everything else completes.
        let restart = def.category == AssignmentCategory::Routine
            && outcome == OutcomeKind::Failure
            && world.get_resource::<CampaignOver>().is_none();
        if restart {
            let duration = i64::from(def.duration_days);
            let mut active = world.get_mut::<ActiveAssignment>(entity).expect("indexed");
            active.started = date;
            active.completes = date.add_days(duration);
        } else {
            world.despawn(entity);
            world
                .resource_mut::<AssignmentsIndex>()
                .assignments
                .remove(&assignment_id);
            // A plan waiting on this assignment learns how it went.
            crate::plans::note_resolution(world, assignment_id, outcome);
        }
    }
}

/// Answers a pending popup, applying the chosen effect.
pub fn answer_popup(
    world: &mut World,
    popup_id: u64,
    choice: &ContentKey,
) -> Result<(), AssignmentRejection> {
    let popup = {
        let popups = world.resource::<PendingPopups>();
        popups
            .popups
            .iter()
            .find(|p| p.id == popup_id)
            .cloned()
            .ok_or(AssignmentRejection::BadPopupAnswer)?
    };
    if !popup.choices.iter().any(|(id, _)| id == choice) {
        return Err(AssignmentRejection::BadPopupAnswer);
    }

    let content = world.resource::<ContentDb>().0.clone();
    // A popup raised by a contextual event carries the event's key where a
    // assignment popup carries the assignment's; the event runtime settles those.
    if content.events.contains_key(&popup.assignment) {
        crate::events::answer_event(world, &popup.assignment, choice, &popup.roles);
        world
            .resource_mut::<PendingPopups>()
            .popups
            .retain(|p| p.id != popup_id);
        return Ok(());
    }
    let effect_fn = content
        .assignments
        .get(&popup.assignment)
        .and_then(|def| def.results.get(&popup.result))
        .and_then(|result| {
            result
                .choices
                .iter()
                .find(|c| &c.id == choice)
                .and_then(|c| c.effect_fn.clone())
        });
    if let Some(fn_ref) = effect_fn {
        let target_label = popup
            .roles
            .target
            .map(|id| crate::access::character_name(world, id))
            .unwrap_or_default();
        let ctx = effect_context(
            world,
            &popup.assignment,
            choice.as_str(),
            popup.roles.leader,
            &target_label,
        );
        let effects = {
            let runtime = world.resource::<ScriptRuntime>();
            runtime.0.call_effect_fn(&content, &fn_ref, ctx)
        };
        if let Ok(effects) = effects {
            let player = world.get_resource::<PlayerHouse>().and_then(|p| p.0);
            apply_effects(world, &effects, &popup.roles, player);
        }
    }

    world
        .resource_mut::<PendingPopups>()
        .popups
        .retain(|p| p.id != popup_id);
    Ok(())
}

// ---------------------------------------------------------------------------
// AI agency
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Snapshot state
// ---------------------------------------------------------------------------

/// Serialised active assignment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentState {
    /// Stable ID.
    pub id: AssignmentId,
    /// Definition key.
    pub def: ContentKey,
    /// Owning organisation.
    pub owner: OrgId,
    /// Leader.
    pub leader: CharacterId,
    /// Target.
    pub target: AssignmentTarget,
    /// Start day.
    pub started: GameDate,
    /// Resolution day.
    pub completes: GameDate,
}

/// The complete serialised assignment world.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentsState {
    /// Active assignments in ID order.
    pub assignments: Vec<AssignmentState>,
    /// The notable-result message log.
    pub message_log: MessageLog,
    /// Popups awaiting answers.
    pub popups: PendingPopups,
}

/// Captures the assignment world for a snapshot.
pub fn capture_assignments(world: &World) -> AssignmentsState {
    let Some(index) = world.get_resource::<AssignmentsIndex>() else {
        return AssignmentsState::default();
    };
    AssignmentsState {
        assignments: index
            .assignments
            .values()
            .map(|entity| {
                let assignment = world.get::<ActiveAssignment>(*entity).expect("indexed");
                AssignmentState {
                    id: assignment.id,
                    def: assignment.def.clone(),
                    owner: assignment.owner,
                    leader: assignment.leader,
                    target: assignment.target,
                    started: assignment.started,
                    completes: assignment.completes,
                }
            })
            .collect(),
        message_log: world
            .get_resource::<MessageLog>()
            .cloned()
            .unwrap_or_default(),
        popups: world
            .get_resource::<PendingPopups>()
            .cloned()
            .unwrap_or_default(),
    }
}

/// Respawns the assignment world from a snapshot.
pub fn restore_assignments(world: &mut World, state: &AssignmentsState) {
    let mut index = AssignmentsIndex::default();
    for assignment in &state.assignments {
        let entity = world
            .spawn(ActiveAssignment {
                id: assignment.id,
                def: assignment.def.clone(),
                owner: assignment.owner,
                leader: assignment.leader,
                target: assignment.target,
                started: assignment.started,
                completes: assignment.completes,
                cancel_requested: false,
            })
            .id();
        index.assignments.insert(assignment.id, entity);
    }
    world.insert_resource(index);
    world.insert_resource(state.message_log.clone());
    world.insert_resource(state.popups.clone());
}

/// Installs assignment resources for a fresh campaign.
pub fn init_assignments(world: &mut World) {
    world.insert_resource(AssignmentsIndex::default());
    world.insert_resource(MessageLog::default());
    world.insert_resource(PendingPopups::default());
    world.insert_resource(ScriptRuntime(ScriptHost::new()));
    world.insert_resource(crate::plans::Plans::default());
    world.insert_resource(crate::goals::Goals::default());
    world.insert_resource(crate::goals::IssuedDirectives::default());
}

pub(crate) fn install(app: &mut App) {
    // Explicit cross-module ordering: assignment resolutions land before the
    // day's appointment reactions, and monthly AI planning follows the
    // opinion cleanup. Insertion order would give the same result today;
    // stating it keeps determinism independent of plugin build order.
    app.add_systems(
        DailyTick,
        resolve_due_assignments
            .in_set(TickSet::Simulation)
            .before(crate::politics::daily_appointments),
    );
    app.add_systems(
        MonthlyPulse,
        (
            crate::agency::characters_act.after(crate::politics::expire_opinion_modifiers),
            // After heads have acted with their houses' authority, so a
            // head who has just taken up work is not also counted as idle.
            crate::agency::household_acts.after(crate::agency::characters_act),
        ),
    );
}
