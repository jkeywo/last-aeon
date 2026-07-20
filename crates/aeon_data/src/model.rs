//! The authored-content model.
//!
//! Definitions grow richer with each milestone; this module owns their
//! validated Rust shapes. Everything is ordered by [`ContentKey`] so
//! iteration over content is deterministic everywhere.

use std::collections::BTreeMap;

use aeon_core::hash::StateHash;
use serde::{Deserialize, Serialize};

use crate::key::ContentKey;

/// A reference to a named function in an authored script file.
///
/// Function references are file-local: a definition may only name functions
/// defined in its own file. That keeps every content file a self-contained
/// unit and makes cross-file behaviour impossible to author by accident.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptFnRef {
    /// Content-relative path of the defining file.
    pub path: String,
    /// The function name inside that file.
    pub name: String,
}

/// How a job behaves when it fails, and how much attention it demands.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobCategory {
    /// Fails cheaply and automatically retries; only time is lost.
    Routine,
    /// Failure creates a setback, disaster creates a new problem.
    Consequential,
}

/// The four graded outcomes of a job.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobResultKind {
    /// Better than intended.
    CriticalSuccess,
    /// The job achieves its objective.
    Success,
    /// A setback (or a retry, for routine jobs).
    Failure,
    /// A severe failure creating a new problem.
    Disaster,
}

impl JobResultKind {
    /// All kinds, in canonical order.
    pub const ALL: [JobResultKind; 4] = [
        JobResultKind::CriticalSuccess,
        JobResultKind::Success,
        JobResultKind::Failure,
        JobResultKind::Disaster,
    ];
}

/// One choice offered by a result popup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupChoiceDef {
    /// Stable choice id within the result.
    pub id: ContentKey,
    /// Button label.
    pub label: String,
    /// Effect function applied when this choice is taken.
    pub effect_fn: Option<ScriptFnRef>,
}

/// One possible result of a job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResultDef {
    /// Relative weight of this outcome before modifiers.
    pub weight: u32,
    /// Whether this result opens a player-facing popup.
    pub popup: bool,
    /// Popup body text; templated with {leader}, {target}, {job}.
    pub popup_text: Option<String>,
    /// Choices offered by the popup; empty means a lone acknowledgement.
    pub choices: Vec<PopupChoiceDef>,
    /// Whether this result is flagged for the notable-result message log.
    pub log: bool,
    /// Log line; templated like popup_text. Falls back to a generic line.
    pub log_text: Option<String>,
    /// Effect function applied when this result occurs.
    pub effect_fn: Option<ScriptFnRef>,
}

/// The personal risks a job can expose its leader to.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RiskTag {
    /// Physical harm; the leader is laid up for a while.
    Injury,
    /// The leader is taken and held.
    Capture,
    /// Public disgrace; opinions of the leader suffer.
    Scandal,
    /// The leader cannot act for a while.
    Incapacity,
    /// The leader may die.
    Death,
}

/// What kind of target a job requires.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JobTargetKind {
    /// No target; the job acts on the owner organisation itself.
    #[default]
    None,
    /// Targets a character.
    Character,
    /// Targets an organisation.
    Organisation,
    /// Targets a province.
    Province,
    /// Targets one of the owner's armies.
    OwnArmy,
    /// Targets one of the owner's armies and a destination province.
    OwnArmyAndProvince,
    /// Targets one of the owner's ships and a destination province.
    OwnShipAndProvince,
}

/// An engine-owned military operation a job performs on success.
///
/// Content declares pacing, costs, and flavour; the simulation owns the
/// operational semantics (movement, engagements, conquest, loot,
/// blockade).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MilitaryOp {
    /// March the army to the target province.
    Move,
    /// Refill the army's supply train from the owner's stores.
    Resupply,
    /// Stand guard where the army is; deters and intercepts hostiles.
    Patrol,
    /// Besiege the target province; success takes its title.
    Besiege,
    /// Raid the target province for loot.
    Raid,
    /// Blockade the target province with the ship.
    Blockade,
}

/// The skill that governs a job's outcome.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GoverningSkill {
    /// Military leadership.
    Command,
    /// Negotiation and persuasion.
    Diplomacy,
    /// Schemes and subversion.
    Intrigue,
    /// Administration.
    Stewardship,
}

/// An authored job definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobDef {
    /// The job's stable content key.
    pub key: ContentKey,
    /// Short player-facing title.
    pub title: String,
    /// One-sentence player-facing summary.
    pub summary: String,
    /// Routine or consequential.
    pub category: JobCategory,
    /// Base duration in days.
    pub duration_days: u32,
    /// The skill that governs the outcome.
    pub skill: GoverningSkill,
    /// Difficulty on the same scale as skills (roughly 0..=20).
    pub difficulty: i32,
    /// What kind of target the job requires.
    pub target: JobTargetKind,
    /// Personal risks the leader is exposed to on failure or disaster.
    pub risks: Vec<RiskTag>,
    /// The engine-owned military operation this job performs, if any.
    pub military_op: Option<MilitaryOp>,
    /// Whether autonomous organisations may start this job.
    pub ai_available: bool,
    /// Which pressure an autonomous house starts this job to answer.
    pub ai_intent: AiIntent,
    /// Wealth deducted when the job starts.
    pub wealth_cost: i64,
    /// Manpower committed when the job starts.
    pub manpower_cost: i64,
    /// Supplies consumed when the job starts.
    pub supplies_cost: i64,
    /// Influence spent when the job starts.
    pub influence_cost: i64,
    /// Possible outcomes, keyed by kind. Success and failure are mandatory.
    pub results: BTreeMap<JobResultKind, JobResultDef>,
}

/// What kind of celestial body a map body is.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BodyKind {
    /// The system's world.
    Planet,
    /// A moon of a planet.
    Moon,
    /// An orbital starbase; a province in its own right.
    Starbase,
}

/// A celestial body in the local system.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyDef {
    /// The body's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// What kind of body this is.
    pub kind: BodyKind,
    /// Visual radius in kilometres (presentation scale, authored).
    pub radius_km: u32,
    /// Orbit radius around its parent in megametres.
    pub orbit_radius_mm: u32,
    /// Days for one full orbit around its parent; zero for the primary.
    pub orbit_days: u32,
    /// The body this one orbits, if any.
    pub parent: Option<ContentKey>,
}

/// A province on a body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvinceDef {
    /// The province's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// The body this province is on.
    pub body: ContentKey,
    /// Latitude of the province centre in millidegrees, -90000..=90000.
    pub latitude_mdeg: i32,
    /// Longitude of the province centre in millidegrees, -180000..180000.
    pub longitude_mdeg: i32,
    /// Monthly wealth output.
    pub wealth_output: i64,
    /// Monthly manpower output.
    pub manpower_output: i64,
    /// Monthly supplies output.
    pub supplies_output: i64,
}

/// Scenario metadata. Extended by the authored-scenario milestone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioDef {
    /// The scenario's stable content key.
    pub key: ContentKey,
    /// Player-facing scenario name.
    pub name: String,
    /// Campaign start year in the scenario's own numbering.
    pub start_year: i64,
    /// Campaign start month, 1..=12.
    pub start_month: u8,
    /// Campaign start day, 1..=30.
    pub start_day: u8,
    /// The house the player leads.
    pub player_house: Option<ContentKey>,
}

/// A loaded, validated content database.
///
/// Definitions are behaviour-free data; the retained per-file ASTs hold the
/// named script functions definitions may reference.
pub struct ContentSet {
    /// Job definitions by key.
    pub jobs: BTreeMap<ContentKey, JobDef>,
    /// Celestial bodies by key.
    pub bodies: BTreeMap<ContentKey, BodyDef>,
    /// Provinces by key.
    pub provinces: BTreeMap<ContentKey, ProvinceDef>,
    /// Trait definitions by key.
    pub traits: BTreeMap<ContentKey, TraitDef>,
    /// Name pools by key.
    pub name_pools: BTreeMap<ContentKey, NamePoolDef>,
    /// Characters by key.
    pub characters: BTreeMap<ContentKey, CharacterDef>,
    /// Organisations by key.
    pub organisations: BTreeMap<ContentKey, OrgDef>,
    /// Authored titles by key.
    pub titles: BTreeMap<ContentKey, TitleDef>,
    /// Offices by key.
    pub offices: BTreeMap<ContentKey, OfficeDef>,
    /// Ships by key.
    pub ships: BTreeMap<ContentKey, ShipDef>,
    /// Starting armies by key.
    pub armies: BTreeMap<ContentKey, ArmyDef>,
    /// Obligations standing at campaign start, by key.
    pub obligations: BTreeMap<ContentKey, ObligationDef>,
    /// Contextual events, by key.
    pub events: BTreeMap<ContentKey, EventDef>,
    /// The scenario, if this content set defines one.
    pub scenario: Option<ScenarioDef>,
    /// Compiled ASTs by content-relative path, for runtime function calls.
    pub asts: BTreeMap<String, rhai::AST>,
    /// Hash over all source files; binds snapshots to content.
    pub content_hash: StateHash,
}

impl ContentSet {
    /// Structural equality over the authored data (ASTs excluded; the
    /// content hash covers source identity).
    pub fn data_eq(&self, other: &ContentSet) -> bool {
        self.jobs == other.jobs
            && self.bodies == other.bodies
            && self.provinces == other.provinces
            && self.traits == other.traits
            && self.name_pools == other.name_pools
            && self.characters == other.characters
            && self.organisations == other.organisations
            && self.titles == other.titles
            && self.offices == other.offices
            && self.ships == other.ships
            && self.armies == other.armies
            && self.scenario == other.scenario
            && self.content_hash == other.content_hash
    }
}

/// A pool of given names for characters born during play.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamePoolDef {
    /// The pool's stable content key.
    pub key: ContentKey,
    /// Given names for male characters.
    pub male: Vec<String>,
    /// Given names for female characters.
    pub female: Vec<String>,
}

/// A character trait: personality facts that drive opinion and, later,
/// job effectiveness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraitDef {
    /// The trait's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// One-line description.
    pub summary: String,
    /// Opinion bonus between two characters sharing this trait.
    pub opinion_same: i32,
    /// Opinion penalty between holders of this trait and its opposites.
    pub opinion_opposed: i32,
    /// Keys of traits this one is opposed to.
    pub opposites: Vec<ContentKey>,
}

/// The four practical skills characters bring to jobs and rule.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillsDef {
    /// Military leadership.
    pub command: i32,
    /// Negotiation, courtship, persuasion.
    pub diplomacy: i32,
    /// Schemes, secrets, subversion.
    pub intrigue: i32,
    /// Administration and economics.
    pub stewardship: i32,
}

/// Biological sex recorded for lineage and procreation modelling.
///
/// Succession law in the setting is blind to it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Gender {
    /// Male.
    Male,
    /// Female.
    Female,
}

/// An authored character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterDef {
    /// The character's stable content key.
    pub key: ContentKey,
    /// Full player-facing name.
    pub name: String,
    /// Biological sex.
    pub gender: Gender,
    /// Birth date in scenario year numbering.
    pub birth_year: i64,
    /// Birth month, 1..=12.
    pub birth_month: u8,
    /// Birth day, 1..=30.
    pub birth_day: u8,
    /// Organisation this character belongs to, if any.
    pub organisation: Option<ContentKey>,
    /// Authored parents (0..=2), for lineage.
    pub parents: Vec<ContentKey>,
    /// Authored spouse.
    pub spouse: Option<ContentKey>,
    /// Trait keys.
    pub traits: Vec<ContentKey>,
    /// Base skills before trait or situational modifiers.
    pub skills: SkillsDef,
}

/// The organisation forms the MVP simulates.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrgKind {
    /// A hereditary dynastic house.
    DynasticHouse,
    /// The Tsar-appointed civilian government; rules-distinct.
    SanctoraImperim,
}

/// A dynastic house's standing in the local hierarchy.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HouseTier {
    /// One of the great houses contesting the planet.
    Great,
    /// Bound to a named great house.
    Vassal,
    /// Outside the great-house hierarchy.
    Independent,
}

/// An authored organisation (house or the Sanctora Imperim).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgDef {
    /// The organisation's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// What kind of organisation this is.
    pub kind: OrgKind,
    /// Hierarchy tier; houses only.
    pub tier: Option<HouseTier>,
    /// The liege house; vassal houses only.
    pub liege: Option<ContentKey>,
    /// Family surname, used to name children born during play.
    pub surname: Option<String>,
    /// Starting wealth.
    pub wealth: i64,
    /// Starting manpower.
    pub manpower: i64,
    /// Starting supplies.
    pub supplies: i64,
    /// Non-spendable political standing, 0..=100; caps and recharges
    /// influence.
    pub legitimacy: i32,
    /// The character who leads at campaign start.
    pub head: Option<ContentKey>,
    /// Provinces this organisation holds at campaign start.
    pub provinces: Vec<ContentKey>,
    /// Political map colour, 0..=255 each.
    pub color: (u8, u8, u8),
}

/// The authored higher dignities. Province titles are implicit — the
/// simulation creates one per province — so authored titles cover only
/// paramountcies and personal Imperial titles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleDef {
    /// The title's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// What the title is over.
    pub kind: TitleKindDef,
    /// The starting holder: an organisation key, character key, or vacant.
    pub holder: TitleHolderDef,
}

/// What an authored title covers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleKindDef {
    /// Paramount title over a body's provinces.
    Paramount {
        /// The body this paramountcy claims.
        body: ContentKey,
    },
    /// The Tsar-appointed Consul title; held personally, never inherited.
    Consul,
}

/// Starting holder of an authored title.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleHolderDef {
    /// Held by an organisation.
    Organisation(ContentKey),
    /// Held personally by a character.
    Character(ContentKey),
    /// Vacant or contested.
    Vacant,
}

/// A starship's broad class.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShipClass {
    /// A capital ship; its captain is a simulated character.
    Capital,
    /// A troop and cargo carrier.
    Transport,
    /// A light patrol vessel.
    Patrol,
}

/// An authored starship.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipDef {
    /// The ship's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// Broad class.
    pub class: ShipClass,
    /// Owning organisation.
    pub owner: ContentKey,
    /// Captain; required for capital ships.
    pub captain: Option<ContentKey>,
    /// Starting dock province.
    pub location: ContentKey,
}

/// An authored starting army, present in a province at campaign start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmyDef {
    /// The army's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// Owning organisation.
    pub owner: ContentKey,
    /// The general commanding it (a member of the owner).
    pub general: ContentKey,
    /// The province it stands in.
    pub province: ContentKey,
    /// Soldiers under arms.
    pub manpower: i64,
    /// Supplies in its train.
    pub supplies: i64,
}

/// The pressure an autonomous house would start a job to answer.
///
/// Authored on the job rather than hardcoded in the simulation, so the
/// AI's repertoire grows with the content instead of with the engine.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiIntent {
    /// Ordinary business, chosen when nothing is pressing.
    #[default]
    Routine,
    /// Restore order in a holding that is slipping.
    Order,
    /// Raise troops.
    Muster,
    /// Act on a favour, promise, or grievance.
    Obligation,
    /// Repair the treasury or stores.
    Resources,
    /// Shore up political standing.
    Standing,
    /// Press a claim that is actually viable.
    Claim,
}

/// The kind of situation an event arises from.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventFamily {
    /// Pressure in a province.
    #[default]
    Province,
    /// Politics between houses.
    Political,
    /// Something on the road.
    Travel,
    /// A complication in a job under way.
    Job,
}

/// Declarative conditions an event needs before it may fire.
///
/// Conditions are data rather than script so they can be validated at
/// load and evaluated identically on every replay.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRequires {
    /// Only fire for the player's own house.
    pub player_only: bool,
    /// Province order must be at or below this.
    pub max_order: Option<i32>,
    /// Province order must be at or above this.
    pub min_order: Option<i32>,
    /// A hostile army must be standing in the province.
    pub occupied: bool,
    /// The subject's house must be party to an open obligation.
    pub has_open_obligation: bool,
}

/// One answer the player may give to a weighty event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventChoiceDef {
    /// Stable choice id.
    pub id: ContentKey,
    /// Button label.
    pub label: String,
    /// Effect function applied when chosen.
    pub effect_fn: Option<ScriptFnRef>,
}

/// An authored contextual event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDef {
    /// The event's stable content key.
    pub key: ContentKey,
    /// Player-facing title.
    pub title: String,
    /// What kind of situation it arises from.
    pub family: EventFamily,
    /// Relative selection weight among eligible candidates.
    pub weight: u32,
    /// Days before this may fire against the same subject again.
    pub cooldown_days: u32,
    /// Whether this interrupts with a choice popup, or only writes to the
    /// log.
    pub weighty: bool,
    /// Situation text, templated with `{subject}`.
    pub text: String,
    /// Log line; falls back to the situation text.
    pub log_text: Option<String>,
    /// Conditions required before it may fire.
    pub requires: EventRequires,
    /// Choices offered by a weighty event.
    pub choices: Vec<EventChoiceDef>,
    /// Effect applied immediately by a minor event.
    pub effect_fn: Option<ScriptFnRef>,
}

/// What kind of political fact an obligation records.
///
/// This is the one vocabulary shared by authored starting obligations,
/// script effects, and the simulation's ledger: it is parsed exactly once,
/// at the content boundary, and typed everywhere after that.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObligationKind {
    /// The debtor owes the creditor a good turn.
    Favour,
    /// The debtor has undertaken something for the creditor, by a date.
    Promise,
    /// The creditor holds a grudge against the debtor.
    Grievance,
}

impl ObligationKind {
    /// Parses the authored spelling; anything else is a content error.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "favour" => Some(ObligationKind::Favour),
            "promise" => Some(ObligationKind::Promise),
            "grievance" => Some(ObligationKind::Grievance),
            _ => None,
        }
    }

    /// The stem of this kind's rows in the string table.
    fn text_stem(self) -> &'static str {
        match self {
            ObligationKind::Favour => "favour",
            ObligationKind::Promise => "promise",
            ObligationKind::Grievance => "grievance",
        }
    }

    /// The key of a short player-facing name.
    pub fn label_key(self) -> String {
        format!("ui.obligation.{}.label", self.text_stem())
    }

    /// The key of the phrase for an obligation this house owes out.
    ///
    /// A whole phrase rather than a noun and a preposition: which
    /// preposition a language wants, and whether it comes before or after,
    /// is not something a caller can decide by concatenation.
    pub fn owed_to_key(self) -> String {
        format!("ui.obligation.{}.owed-to", self.text_stem())
    }

    /// The key of the phrase for an obligation owed to this house.
    pub fn owed_from_key(self) -> String {
        format!("ui.obligation.{}.owed-from", self.text_stem())
    }

    /// Whether this kind counts in the debtor's favour or against it.
    pub fn is_positive(self) -> bool {
        matches!(self, ObligationKind::Favour | ObligationKind::Promise)
    }
}

/// An authored political obligation standing at campaign start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationDef {
    /// The obligation's stable content key.
    pub key: ContentKey,
    /// Favour, promise, or grievance.
    pub kind: ObligationKind,
    /// The organisation that owes, or is resented.
    pub debtor: ContentKey,
    /// The organisation that is owed, or resents.
    pub creditor: ContentKey,
    /// Where it came from, in plain words.
    pub origin: String,
    /// How much it weighs.
    pub weight: i32,
    /// Days until it lapses; `None` never lapses.
    pub days: Option<i64>,
}

/// An authored office: a revocable appointment held by a character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeDef {
    /// The office's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// The organisation whose authority the office carries.
    pub organisation: ContentKey,
    /// The province this office administers, if any.
    pub province: Option<ContentKey>,
    /// The starting holder.
    pub holder: Option<ContentKey>,
}
