//! The character-led job system: the universal unit of strategic action.
//!
//! Jobs are started by commands (player) or agency (AI organisations),
//! take calendar days, and resolve into graded results whose weights are
//! shifted by the leader's governing skill against the job's difficulty.
//! Results can log to the notable-result message log, open player popups
//! with choices, expose the leader to declared personal risks, and apply
//! authored script effects through the sandboxed effect boundary.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_core::rng::DeterministicRng;
use aeon_data::model::{JobCategory, JobDef, JobResultKind, JobTargetKind, RiskTag};
use aeon_data::{ContentKey, ScriptEffect, ScriptHost};
use bevy::app::App;
use bevy::prelude::{Component, Entity, IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, MonthlyPulse, TickSet};
use crate::ids::{ArmyId, CharacterId, JobId, OrgId, ProvinceId, ShipId};
use crate::politics::{
    CampaignOver, CharacterRecord, OpinionEntry, OpinionLedger, OrgRecord, PlayerHouse,
    PoliticsIndex, process_death,
};
use crate::state::{CampaignIds, CampaignSeed, ContentDb};

/// What a job acts on.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobTarget {
    /// The job acts on its owner organisation.
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

/// A running job.
#[derive(Component, Clone, Debug)]
pub struct ActiveJob {
    /// Stable ID.
    pub id: JobId,
    /// The job definition's content key.
    pub def: ContentKey,
    /// The organisation this job serves.
    pub owner: OrgId,
    /// The character leading it.
    pub leader: CharacterId,
    /// What it acts on.
    pub target: JobTarget,
    /// The day it (re)started.
    pub started: GameDate,
    /// The day it resolves.
    pub completes: GameDate,
}

/// Temporary states that keep a character from leading new jobs.
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

/// Lookup from job IDs to entities.
#[derive(Resource, Clone, Debug, Default)]
pub struct JobsIndex {
    /// Jobs by stable ID.
    pub jobs: BTreeMap<JobId, Entity>,
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
    /// Job starts, results, and abandonment.
    #[default]
    Jobs,
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
        LogChannel::Jobs,
        LogChannel::Politics,
        LogChannel::Military,
        LogChannel::Economy,
        LogChannel::Events,
    ];

    /// A short player-facing label.
    pub fn label(self) -> &'static str {
        match self {
            LogChannel::Jobs => "Jobs",
            LogChannel::Politics => "Politics",
            LogChannel::Military => "Military",
            LogChannel::Economy => "Economy",
            LogChannel::Events => "Events",
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
    /// The job definition involved.
    pub job: ContentKey,
    /// The result that opened it.
    pub result: JobResultKind,
    /// Rendered situation text.
    pub text: String,
    /// Choice ids and labels; always at least one.
    pub choices: Vec<(ContentKey, String)>,
    /// Roles resolved at resolution time, for choice effects.
    pub roles: JobRoles,
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

/// The characters standing behind each script-effect role for one job.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRoles {
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
    /// The province the job acted on, for province-scoped effects.
    #[serde(default)]
    pub province: Option<ProvinceId>,
}

impl JobRoles {
    fn resolve_from(&self, role: &str) -> Vec<CharacterId> {
        match role {
            "leader" => self.leader.into_iter().collect(),
            "target" => self.target.into_iter().collect(),
            "target-head" => self.target_head.into_iter().collect(),
            "owner-head" => self.owner_head.into_iter().collect(),
            "liege-head" => self.liege_head.into_iter().collect(),
            "consul" => self.consul.into_iter().collect(),
            "sanctora" => self.sanctora.clone(),
            _ => Vec::new(),
        }
    }

    fn resolve_toward(&self, role: &str) -> Option<CharacterId> {
        self.resolve_from(role).first().copied()
    }
}

/// Why a job could not be started or answered.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JobRejection {
    /// The campaign has ended.
    #[error("the campaign is over")]
    CampaignOver,
    /// No such job definition.
    #[error("unknown job definition '{0}'")]
    UnknownJob(ContentKey),
    /// The player has no house to act through.
    #[error("no player organisation")]
    NoPlayerOrg,
    /// The leader is not a living adult member of the organisation.
    #[error("that character cannot lead jobs for this organisation")]
    IneligibleLeader,
    /// The leader is already leading a job.
    #[error("that character is already leading a job")]
    LeaderBusy,
    /// The leader already holds a standing command over another force.
    #[error("that character already commands another force")]
    AlreadyAssigned,
    /// The leader is injured, captured, or incapacitated.
    #[error("that character is in no state to lead")]
    LeaderIndisposed,
    /// The target does not match the definition's target kind.
    #[error("the job's target is missing or of the wrong kind")]
    BadTarget,
    /// No such popup or choice.
    #[error("no such popup or choice")]
    BadPopupAnswer,
    /// No such active job, or it is not the player's to cancel.
    #[error("no such active job for your organisation")]
    BadJob,
    /// The organisation cannot pay the job's costs.
    #[error("the organisation cannot afford this job")]
    CannotAfford,
}

// ---------------------------------------------------------------------------
// Eligibility and start
// ---------------------------------------------------------------------------

/// A standing command a character already holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Assignment {
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

impl Assignment {
    /// A short phrase naming the command, for the interface.
    pub fn describe(&self) -> String {
        match self {
            Assignment::General { name, .. } => format!("General, {name}"),
            Assignment::Captain { name, .. } => format!("Captain, {name}"),
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
    /// Already leading a job.
    Busy {
        /// The job they are leading.
        job: JobId,
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
    /// still lead a diplomatic job — but it bars a *second* command.
    Assigned(Assignment),
    /// Cannot lead for this organisation at all: dead, not a member, or
    /// not yet of age.
    Ineligible(JobRejection),
}

impl LeaderAvailability {
    /// Whether the character is free of any commitment.
    pub fn is_available(&self) -> bool {
        matches!(self, LeaderAvailability::Available)
    }

    /// Why this character cannot lead *this* job, if they cannot.
    ///
    /// A standing command only bars taking on another one; it does not
    /// stop its holder doing something else entirely.
    pub fn blocks_job(&self, target: JobTarget) -> Option<JobRejection> {
        match self {
            LeaderAvailability::Available => None,
            LeaderAvailability::Busy { .. } => Some(JobRejection::LeaderBusy),
            LeaderAvailability::Indisposed { .. } => Some(JobRejection::LeaderIndisposed),
            LeaderAvailability::Ineligible(rejection) => Some(rejection.clone()),
            LeaderAvailability::Assigned(assignment) => match (assignment, target) {
                // Ordering the force they already command is the point.
                (Assignment::General { army, .. }, JobTarget::OwnArmy(ordered))
                | (Assignment::General { army, .. }, JobTarget::ArmyToProvince(ordered, _))
                    if *army == ordered =>
                {
                    None
                }
                (Assignment::Captain { ship, .. }, JobTarget::ShipToProvince(ordered, _))
                    if *ship == ordered =>
                {
                    None
                }
                // Commanding one force does not let you command another.
                (_, JobTarget::OwnArmy(_))
                | (_, JobTarget::ArmyToProvince(_, _))
                | (_, JobTarget::ShipToProvince(_, _)) => Some(JobRejection::AlreadyAssigned),
                // Anything else is ordinary work they are free to do.
                _ => None,
            },
        }
    }

    /// A short player-facing phrase, for showing beside a name.
    pub fn describe(&self, job_title: impl Fn(&ContentKey) -> String) -> String {
        match self {
            LeaderAvailability::Available => "available".to_owned(),
            LeaderAvailability::Busy { def, completes, .. } => {
                format!("leading {} until {completes}", job_title(def))
            }
            LeaderAvailability::Indisposed { until: Some(until) } => {
                format!("indisposed until {until}")
            }
            LeaderAvailability::Indisposed { until: None } => "indisposed".to_owned(),
            LeaderAvailability::Assigned(assignment) => assignment.describe(),
            LeaderAvailability::Ineligible(JobRejection::IneligibleLeader) => {
                "not able to lead for this house".to_owned()
            }
            LeaderAvailability::Ineligible(rejection) => rejection.to_string(),
        }
    }
}

/// What a character is currently committed to, for `org`'s purposes.
///
/// Reports the most limiting commitment: ineligibility first, then
/// indisposition, then an active job, then a standing command.
pub fn leader_availability(
    world: &World,
    org: OrgId,
    leader: CharacterId,
    date: GameDate,
) -> LeaderAvailability {
    let Some(index) = world.get_resource::<PoliticsIndex>() else {
        return LeaderAvailability::Ineligible(JobRejection::IneligibleLeader);
    };
    let Some(entity) = index.characters.get(&leader) else {
        return LeaderAvailability::Ineligible(JobRejection::IneligibleLeader);
    };
    let Some(record) = world.get::<CharacterRecord>(*entity) else {
        return LeaderAvailability::Ineligible(JobRejection::IneligibleLeader);
    };
    if !record.alive()
        || record.organisation != Some(org)
        || record.age_years(date) < crate::politics::ADULT_AGE
    {
        return LeaderAvailability::Ineligible(JobRejection::IneligibleLeader);
    }

    let condition = world
        .get::<CharacterCondition>(*entity)
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

    if let Some(jobs) = world.get_resource::<JobsIndex>() {
        // Stable-ID order, so the reported job never depends on iteration.
        for entity in jobs.jobs.values() {
            if let Some(job) = world.get::<ActiveJob>(*entity)
                && job.leader == leader
            {
                return LeaderAvailability::Busy {
                    job: job.id,
                    def: job.def.clone(),
                    completes: job.completes,
                };
            }
        }
    }

    if let Some(forces) = world.get_resource::<crate::forces::ForcesIndex>() {
        for entity in forces.armies.values() {
            if let Some(army) = world.get::<crate::forces::ArmyRecord>(*entity)
                && army.general == leader
            {
                return LeaderAvailability::Assigned(Assignment::General {
                    army: army.id,
                    name: army.name.clone(),
                });
            }
        }
        for entity in forces.ships.values() {
            if let Some(ship) = world.get::<crate::forces::ShipRecord>(*entity)
                && ship.captain == Some(leader)
            {
                return LeaderAvailability::Assigned(Assignment::Captain {
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
    target: JobTarget,
) -> Result<(), JobRejection> {
    match leader_availability(world, org, leader, date).blocks_job(target) {
        Some(rejection) => Err(rejection),
        None => Ok(()),
    }
}

fn owned_army(world: &World, owner: OrgId, army: ArmyId) -> bool {
    world
        .get_resource::<crate::forces::ForcesIndex>()
        .and_then(|forces| forces.armies.get(&army).copied())
        .and_then(|entity| world.get::<crate::forces::ArmyRecord>(entity))
        .is_some_and(|record| record.owner == owner)
}

fn target_valid(world: &World, def: &JobDef, owner: OrgId, target: JobTarget) -> bool {
    let index = world.resource::<PoliticsIndex>();
    let province_known = |id: ProvinceId| {
        world
            .resource::<crate::map::MapIndex>()
            .provinces
            .contains_key(&id)
    };
    match (def.target, target) {
        (JobTargetKind::None, JobTarget::None) => true,
        (JobTargetKind::Character, JobTarget::Character(id)) => index
            .characters
            .get(&id)
            .and_then(|e| world.get::<CharacterRecord>(*e))
            .is_some_and(|r| r.alive()),
        (JobTargetKind::Organisation, JobTarget::Org(id)) => {
            id != owner && index.orgs.contains_key(&id)
        }
        (JobTargetKind::Province, JobTarget::Province(id)) => province_known(id),
        (JobTargetKind::OwnArmy, JobTarget::OwnArmy(army)) => owned_army(world, owner, army),
        (JobTargetKind::OwnArmyAndProvince, JobTarget::ArmyToProvince(army, province)) => {
            owned_army(world, owner, army) && province_known(province)
        }
        (JobTargetKind::OwnShipAndProvince, JobTarget::ShipToProvince(ship, province)) => {
            province_known(province)
                && world
                    .get_resource::<crate::forces::ForcesIndex>()
                    .and_then(|forces| forces.ships.get(&ship).copied())
                    .and_then(|entity| world.get::<crate::forces::ShipRecord>(entity))
                    .is_some_and(|record| record.owner == owner)
        }
        _ => false,
    }
}

/// Validates a start-job request for an organisation.
pub fn validate_start(
    world: &World,
    org: OrgId,
    def_key: &ContentKey,
    leader: CharacterId,
    target: JobTarget,
) -> Result<(), JobRejection> {
    if world.get_resource::<CampaignOver>().is_some() {
        return Err(JobRejection::CampaignOver);
    }
    let content = world.resource::<ContentDb>().0.clone();
    let Some(def) = content.jobs.get(def_key) else {
        return Err(JobRejection::UnknownJob(def_key.clone()));
    };
    let date = world.resource::<CampaignClock>().date;
    leader_eligible(world, org, leader, date, target)?;
    if !target_valid(world, def, org, target) {
        return Err(JobRejection::BadTarget);
    }
    // Army operations are led by the army's general, nobody else.
    if let JobTarget::OwnArmy(army) | JobTarget::ArmyToProvince(army, _) = target {
        let general = world
            .get_resource::<crate::forces::ForcesIndex>()
            .and_then(|forces| forces.armies.get(&army).copied())
            .and_then(|entity| world.get::<crate::forces::ArmyRecord>(entity))
            .map(|record| record.general);
        if general != Some(leader) {
            return Err(JobRejection::IneligibleLeader);
        }
    }
    let affordable = {
        let index = world.resource::<PoliticsIndex>();
        index
            .orgs
            .get(&org)
            .and_then(|e| world.get::<crate::economy::OrgResources>(*e))
            .is_some_and(|r| {
                r.can_afford(
                    def.wealth_cost,
                    def.manpower_cost,
                    def.supplies_cost,
                    def.influence_cost,
                )
            })
    };
    if !affordable {
        return Err(JobRejection::CannotAfford);
    }
    Ok(())
}

/// Starts a job for an organisation. Callers must have validated.
pub fn start_job(
    world: &mut World,
    org: OrgId,
    def_key: &ContentKey,
    leader: CharacterId,
    target: JobTarget,
) -> JobId {
    let date = world.resource::<CampaignClock>().date;
    let (duration, costs) = {
        let content = world.resource::<ContentDb>().0.clone();
        let def = &content.jobs[def_key];
        (
            crate::forecast::job_duration_days(world, def, target),
            (
                def.wealth_cost,
                def.manpower_cost,
                def.supplies_cost,
                def.influence_cost,
            ),
        )
    };
    {
        let org_entity = world.resource::<PoliticsIndex>().orgs[&org];
        if let Some(mut resources) = world.get_mut::<crate::economy::OrgResources>(org_entity) {
            resources.spend(costs.0, costs.1, costs.2, costs.3);
        }
    }
    let id: JobId = world.resource_mut::<CampaignIds>().0.allocate();
    let entity = world
        .spawn(ActiveJob {
            id,
            def: def_key.clone(),
            owner: org,
            leader,
            target,
            started: date,
            completes: date.add_days(duration),
        })
        .id();
    world.resource_mut::<JobsIndex>().jobs.insert(id, entity);
    id
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

// Outcome weighting, duration, and risk chances live in `forecast`, so the
// numbers quoted to the player before a job are the numbers used to resolve
// it. Re-exported here for the resolution code below.
use crate::forecast::{result_weights, risk_permille};

/// The character standing behind each effect role for a job.
fn resolve_roles(world: &World, job: &ActiveJob) -> JobRoles {
    let index = world.resource::<PoliticsIndex>();
    let org_head = |org: OrgId| -> Option<CharacterId> {
        index
            .orgs
            .get(&org)
            .and_then(|e| world.get::<OrgRecord>(*e))
            .and_then(|r| r.head)
    };
    let owner_record = index
        .orgs
        .get(&job.owner)
        .and_then(|e| world.get::<OrgRecord>(*e));

    let (target, target_head) = match job.target {
        JobTarget::Character(id) => (Some(id), None),
        JobTarget::Org(org) => (org_head(org), org_head(org)),
        _ => (None, None),
    };

    let consul = index.titles.values().find_map(|entity| {
        let title = world.get::<crate::politics::TitleRecord>(*entity)?;
        match (title.kind, title.holder) {
            (
                crate::politics::TitleKind::Consul,
                crate::politics::TitleHolder::Character(holder),
            ) => Some(holder),
            _ => None,
        }
    });

    let sanctora_org = index.orgs.iter().find_map(|(org_id, entity)| {
        let record = world.get::<OrgRecord>(*entity)?;
        (record.kind == aeon_data::model::OrgKind::SanctoraImperim).then_some(*org_id)
    });
    let sanctora = index
        .characters
        .iter()
        .filter(|(_, e)| {
            world.get::<CharacterRecord>(**e).is_some_and(|r| {
                r.alive() && sanctora_org.is_some() && r.organisation == sanctora_org
            })
        })
        .map(|(id, _)| *id)
        .collect();

    // Where the job acted: an explicit province target, the province an
    // ordered force stands in, or failing both the leader's own location.
    let province = match job.target {
        JobTarget::Province(province)
        | JobTarget::ArmyToProvince(_, province)
        | JobTarget::ShipToProvince(_, province) => Some(province),
        JobTarget::OwnArmy(army) => world
            .get_resource::<crate::forces::ForcesIndex>()
            .and_then(|forces| forces.armies.get(&army).copied())
            .and_then(|entity| world.get::<crate::forces::ArmyRecord>(entity))
            .map(|record| record.location),
        _ => match crate::presence::character_location(world, job.leader) {
            Some(crate::presence::Location::Province(province)) => Some(province),
            _ => None,
        },
    };

    JobRoles {
        leader: Some(job.leader),
        target,
        target_head,
        owner_head: owner_record.and_then(|r| r.head),
        liege_head: owner_record.and_then(|r| r.liege).and_then(org_head),
        consul,
        sanctora,
        province,
    }
}

fn display_name(world: &World, id: CharacterId) -> String {
    let index = world.resource::<PoliticsIndex>();
    index
        .characters
        .get(&id)
        .and_then(|e| world.get::<CharacterRecord>(*e))
        .map(|r| r.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn target_name(world: &World, target: JobTarget) -> String {
    match target {
        JobTarget::None => String::new(),
        JobTarget::Character(id) => display_name(world, id),
        JobTarget::Org(org) => {
            let index = world.resource::<PoliticsIndex>();
            index
                .orgs
                .get(&org)
                .and_then(|e| world.get::<OrgRecord>(*e))
                .and_then(|r| {
                    world
                        .resource::<ContentDb>()
                        .0
                        .organisations
                        .get(&r.key)
                        .map(|d| d.name.clone())
                })
                .unwrap_or_else(|| org.to_string())
        }
        JobTarget::Province(id) => {
            let map = world.resource::<crate::map::MapIndex>();
            map.provinces
                .get(&id)
                .and_then(|e| world.get::<crate::map::DisplayName>(*e))
                .map(|n| n.0.clone())
                .unwrap_or_else(|| id.to_string())
        }
        JobTarget::OwnArmy(army) => world
            .get_resource::<crate::forces::ForcesIndex>()
            .and_then(|forces| forces.armies.get(&army).copied())
            .and_then(|entity| world.get::<crate::forces::ArmyRecord>(entity))
            .map(|record| record.name.clone())
            .unwrap_or_default(),
        JobTarget::ArmyToProvince(_, province) | JobTarget::ShipToProvince(_, province) => {
            target_name(world, JobTarget::Province(province))
        }
    }
}

fn render_template(template: &str, leader: &str, target: &str, job_title: &str) -> String {
    template
        .replace("{leader}", leader)
        .replace("{target}", target)
        .replace("{job}", job_title)
}

/// Applies parsed script effects against resolved roles.
pub fn apply_effects(
    world: &mut World,
    effects: &[ScriptEffect],
    roles: &JobRoles,
    owner: Option<OrgId>,
) {
    let date = world.resource::<CampaignClock>().date;
    for effect in effects {
        match effect {
            ScriptEffect::Log { message } => {
                world
                    .resource_mut::<MessageLog>()
                    .entries
                    .push(LogEntry::new(date, message.clone(), LogChannel::Jobs).by(owner));
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
                    let org_entity = world.resource::<PoliticsIndex>().orgs[&owner];
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
                    _ => {
                        let index = world.resource::<PoliticsIndex>();
                        let map = world.resource::<crate::map::MapIndex>();
                        let _ = &index;
                        map.province_ids.keys().next().copied()
                    }
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
                let Some(toward_id) = roles.resolve_toward(toward) else {
                    continue;
                };
                let expires = days.map(|d| date.add_days(d));
                for from_id in roles.resolve_from(from) {
                    if from_id == toward_id {
                        continue;
                    }
                    let entity = world.resource::<PoliticsIndex>().characters[&from_id];
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
                let house_of = |role: &str| -> Option<OrgId> {
                    let character = roles.resolve_toward(role)?;
                    let index = world.resource::<PoliticsIndex>();
                    index
                        .characters
                        .get(&character)
                        .and_then(|entity| world.get::<CharacterRecord>(*entity))
                        .and_then(|record| record.organisation)
                };
                let obligation_kind = match kind.as_str() {
                    "favour" => crate::obligations::ObligationKind::Favour,
                    "promise" => crate::obligations::ObligationKind::Promise,
                    _ => crate::obligations::ObligationKind::Grievance,
                };
                if let (Some(debtor), Some(creditor)) = (house_of(debtor), house_of(creditor)) {
                    match action {
                        aeon_data::effect::ObligationAction::Create => {
                            crate::obligations::create(
                                world,
                                obligation_kind,
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
                                obligation_kind,
                                debtor,
                                creditor,
                                crate::obligations::ObligationStatus::Fulfilled,
                            );
                        }
                        aeon_data::effect::ObligationAction::Break => {
                            crate::obligations::settle(
                                world,
                                obligation_kind,
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
        }
    }
}

/// Applies one personal-risk consequence to a character. Public so event
/// systems (and tests) can reuse the exact job-risk semantics.
pub fn apply_risk(world: &mut World, leader: CharacterId, tag: RiskTag, date: GameDate) {
    let entity = world.resource::<PoliticsIndex>().characters[&leader];
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
            let heads: Vec<CharacterId> = {
                let index = world.resource::<PoliticsIndex>();
                index
                    .orgs
                    .values()
                    .filter_map(|e| world.get::<OrgRecord>(*e).and_then(|r| r.head))
                    .filter(|h| *h != leader)
                    .collect()
            };
            for head in heads {
                let head_entity = world.resource::<PoliticsIndex>().characters[&head];
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

/// Daily: resolves every job due today, in stable-ID order.
pub fn resolve_due_jobs(world: &mut World) {
    if world.get_resource::<JobsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let seed = world.resource::<CampaignSeed>().0;
    let player = world.get_resource::<PlayerHouse>().and_then(|p| p.0);

    let due: Vec<(JobId, Entity)> = world
        .resource::<JobsIndex>()
        .jobs
        .iter()
        .filter(|(_, entity)| {
            world
                .get::<ActiveJob>(**entity)
                .is_some_and(|job| job.completes <= date)
        })
        .map(|(id, entity)| (*id, *entity))
        .collect();

    for (job_id, entity) in due {
        let job = world.get::<ActiveJob>(entity).expect("indexed").clone();
        let content = world.resource::<ContentDb>().0.clone();
        let def = content.jobs[&job.def].clone();

        // A dead or removed leader abandons the job.
        let leader_alive = {
            let index = world.resource::<PoliticsIndex>();
            index
                .characters
                .get(&job.leader)
                .and_then(|e| world.get::<CharacterRecord>(*e))
                .is_some_and(|r| r.alive())
        };
        if !leader_alive {
            world.resource_mut::<MessageLog>().entries.push(
                LogEntry::new(
                    date,
                    format!("'{}' was abandoned; its leader is gone.", def.title),
                    LogChannel::Jobs,
                )
                .by(Some(job.owner))
                .about(LogSubject::Character(job.leader)),
            );
            world.despawn(entity);
            world.resource_mut::<JobsIndex>().jobs.remove(&job_id);
            continue;
        }

        // Outcome, rolled against exactly the table the forecast reports.
        let effectiveness = crate::forecast::effectiveness(world, job.leader, &def);
        let mut rng = DeterministicRng::derive(
            seed,
            "job-resolution",
            &[job_id.raw(), date.days_since_epoch() as u64],
        );
        let weights: Vec<(JobResultKind, u64)> = result_weights(&def, effectiveness);
        let total: u64 = weights.iter().map(|(_, w)| w).sum();
        let mut roll = rng.roll(total.max(1));
        let mut outcome = weights
            .last()
            .map(|(k, _)| *k)
            .unwrap_or(JobResultKind::Failure);
        for (kind, weight) in &weights {
            if roll < *weight {
                outcome = *kind;
                break;
            }
            roll -= *weight;
        }
        if matches!(
            outcome,
            JobResultKind::Success | JobResultKind::CriticalSuccess
        ) && let Some(op) = def.military_op
            && !crate::warfare::apply_military_op(world, op, &job)
        {
            // The operation was defeated in the field.
            outcome = JobResultKind::Failure;
        }
        let result = def
            .results
            .get(&outcome)
            .cloned()
            .unwrap_or_else(|| def.results[&JobResultKind::Failure].clone());

        // Personal risks on bad outcomes.
        if matches!(outcome, JobResultKind::Failure | JobResultKind::Disaster) {
            let disaster = outcome == JobResultKind::Disaster;
            for (index, tag) in def.risks.iter().enumerate() {
                let mut risk_rng = DeterministicRng::derive(
                    seed,
                    "job-risk",
                    &[job_id.raw(), date.days_since_epoch() as u64, index as u64],
                );
                if risk_rng.check_permille(risk_permille(*tag, disaster)) {
                    apply_risk(world, job.leader, *tag, date);
                }
            }
        }

        let roles = resolve_roles(world, &job);
        let leader_name = display_name(world, job.leader);
        let target_label = target_name(world, job.target);

        // Authored result effects.
        if let Some(fn_ref) = &result.effect_fn {
            let mut ctx = rhai::Map::new();
            ctx.insert("job".into(), job.def.as_str().into());
            ctx.insert("result".into(), format!("{outcome:?}").into());
            ctx.insert("leader".into(), leader_name.clone().into());
            ctx.insert("target".into(), target_label.clone().into());
            let effects = {
                let runtime = world.resource::<ScriptRuntime>();
                runtime.0.call_effect_fn(&content, fn_ref, ctx)
            };
            match effects {
                Ok(effects) => apply_effects(world, &effects, &roles, Some(job.owner)),
                Err(err) => {
                    world.resource_mut::<MessageLog>().entries.push(
                        LogEntry::new(
                            date,
                            format!("script error resolving '{}': {err}", def.title),
                            LogChannel::Jobs,
                        )
                        .by(Some(job.owner)),
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
            world.resource_mut::<MessageLog>().entries.push(
                LogEntry::new(date, text, LogChannel::Jobs)
                    .by(Some(job.owner))
                    .about(LogSubject::Character(job.leader)),
            );
        }

        // Player popups.
        if result.popup && player == Some(job.owner) {
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
                job: job.def.clone(),
                result: outcome,
                text,
                choices,
                roles: roles.clone(),
            });
        }

        // Routine failures restart; everything else completes.
        let restart = def.category == JobCategory::Routine
            && outcome == JobResultKind::Failure
            && world.get_resource::<CampaignOver>().is_none();
        if restart {
            let duration = i64::from(def.duration_days);
            let mut active = world.get_mut::<ActiveJob>(entity).expect("indexed");
            active.started = date;
            active.completes = date.add_days(duration);
        } else {
            world.despawn(entity);
            world.resource_mut::<JobsIndex>().jobs.remove(&job_id);
        }
    }
}

/// Answers a pending popup, applying the chosen effect.
pub fn answer_popup(
    world: &mut World,
    popup_id: u64,
    choice: &ContentKey,
) -> Result<(), JobRejection> {
    let popup = {
        let popups = world.resource::<PendingPopups>();
        popups
            .popups
            .iter()
            .find(|p| p.id == popup_id)
            .cloned()
            .ok_or(JobRejection::BadPopupAnswer)?
    };
    if !popup.choices.iter().any(|(id, _)| id == choice) {
        return Err(JobRejection::BadPopupAnswer);
    }

    let content = world.resource::<ContentDb>().0.clone();
    // A popup raised by a contextual event carries the event's key where a
    // job popup carries the job's; the event runtime settles those.
    if content.events.contains_key(&popup.job) {
        crate::events::answer_event(world, &popup.job, choice, &popup.roles);
        world
            .resource_mut::<PendingPopups>()
            .popups
            .retain(|p| p.id != popup_id);
        return Ok(());
    }
    let effect_fn = content
        .jobs
        .get(&popup.job)
        .and_then(|def| def.results.get(&popup.result))
        .and_then(|result| {
            result
                .choices
                .iter()
                .find(|c| &c.id == choice)
                .and_then(|c| c.effect_fn.clone())
        });
    if let Some(fn_ref) = effect_fn {
        let mut ctx = rhai::Map::new();
        ctx.insert("job".into(), popup.job.as_str().into());
        ctx.insert("choice".into(), choice.as_str().into());
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

/// Serialised active job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobState {
    /// Stable ID.
    pub id: JobId,
    /// Definition key.
    pub def: ContentKey,
    /// Owning organisation.
    pub owner: OrgId,
    /// Leader.
    pub leader: CharacterId,
    /// Target.
    pub target: JobTarget,
    /// Start day.
    pub started: GameDate,
    /// Resolution day.
    pub completes: GameDate,
}

/// The complete serialised job world.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobsState {
    /// Active jobs in ID order.
    pub jobs: Vec<JobState>,
    /// The notable-result message log.
    pub message_log: MessageLog,
    /// Popups awaiting answers.
    pub popups: PendingPopups,
}

/// Captures the job world for a snapshot.
pub fn capture_jobs(world: &World) -> JobsState {
    let Some(index) = world.get_resource::<JobsIndex>() else {
        return JobsState::default();
    };
    JobsState {
        jobs: index
            .jobs
            .values()
            .map(|entity| {
                let job = world.get::<ActiveJob>(*entity).expect("indexed");
                JobState {
                    id: job.id,
                    def: job.def.clone(),
                    owner: job.owner,
                    leader: job.leader,
                    target: job.target,
                    started: job.started,
                    completes: job.completes,
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

/// Respawns the job world from a snapshot.
pub fn restore_jobs(world: &mut World, state: &JobsState) {
    let mut index = JobsIndex::default();
    for job in &state.jobs {
        let entity = world
            .spawn(ActiveJob {
                id: job.id,
                def: job.def.clone(),
                owner: job.owner,
                leader: job.leader,
                target: job.target,
                started: job.started,
                completes: job.completes,
            })
            .id();
        index.jobs.insert(job.id, entity);
    }
    world.insert_resource(index);
    world.insert_resource(state.message_log.clone());
    world.insert_resource(state.popups.clone());
}

/// Installs job resources for a fresh campaign.
pub fn init_jobs(world: &mut World) {
    world.insert_resource(JobsIndex::default());
    world.insert_resource(MessageLog::default());
    world.insert_resource(PendingPopups::default());
    world.insert_resource(ScriptRuntime(ScriptHost::new()));
}

pub(crate) fn install(app: &mut App) {
    // Explicit cross-module ordering: job resolutions land before the
    // day's appointment reactions, and monthly AI planning follows the
    // opinion cleanup. Insertion order would give the same result today;
    // stating it keeps determinism independent of plugin build order.
    app.add_systems(
        DailyTick,
        resolve_due_jobs
            .in_set(TickSet::Simulation)
            .before(crate::politics::daily_appointments),
    );
    app.add_systems(
        MonthlyPulse,
        crate::agency::ai_start_jobs.after(crate::politics::expire_opinion_modifiers),
    );
}
