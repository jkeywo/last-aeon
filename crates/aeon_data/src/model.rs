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

/// How a assignment behaves when it fails, and how much attention it demands.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssignmentCategory {
    /// Fails cheaply and automatically retries; only time is lost.
    Routine,
    /// Failure creates a setback, disaster creates a new problem.
    Consequential,
}

/// The four graded outcomes of a assignment.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutcomeKind {
    /// Better than intended.
    CriticalSuccess,
    /// The assignment achieves its objective.
    Success,
    /// A setback (or a retry, for routine assignments).
    Failure,
    /// A severe failure creating a new problem.
    Disaster,
}

impl OutcomeKind {
    /// All kinds, in canonical order.
    pub const ALL: [OutcomeKind; 4] = [
        OutcomeKind::CriticalSuccess,
        OutcomeKind::Success,
        OutcomeKind::Failure,
        OutcomeKind::Disaster,
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

/// One possible result of a assignment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeDef {
    /// Relative weight of this outcome before modifiers.
    pub weight: u32,
    /// Whether this result opens a player-facing popup.
    pub popup: bool,
    /// Popup body text; templated with {leader}, {target}, {assignment}.
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

/// The personal risks a assignment can expose its leader to.
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

impl RiskTag {
    /// Parses the authored spelling; anything else is a content error.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "injury" => Some(RiskTag::Injury),
            "capture" => Some(RiskTag::Capture),
            "scandal" => Some(RiskTag::Scandal),
            "incapacity" => Some(RiskTag::Incapacity),
            "death" => Some(RiskTag::Death),
            _ => None,
        }
    }

    /// The key of this risk's player-facing name.
    ///
    /// Risks used to reach the player through `{:?}`, which put the Rust
    /// variant name on screen — readable only because English happened to
    /// be the language the enum was written in.
    pub fn label_key(self) -> &'static str {
        match self {
            RiskTag::Injury => "ui.risk.injury",
            RiskTag::Capture => "ui.risk.capture",
            RiskTag::Scandal => "ui.risk.scandal",
            RiskTag::Incapacity => "ui.risk.incapacity",
            RiskTag::Death => "ui.risk.death",
        }
    }
}

/// What kind of target a assignment requires.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssignmentTargetKind {
    /// No target; the assignment acts on the owner organisation itself.
    #[default]
    None,
    /// Targets a character.
    Character,
    /// Targets an organisation.
    Organisation,
    /// Targets a province.
    Province,
    /// Targets one persistent formal war.
    War,
    /// Targets one exact side of one persistent formal war.
    WarSide,
    /// Targets one of the owner's armies.
    OwnArmy,
    /// Targets one of the owner's armies and a destination province.
    OwnArmyAndProvince,
    /// Targets one of the owner's ships and a destination province.
    OwnShipAndProvince,
}

/// An engine-owned military operation a assignment performs on success.
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

impl MilitaryOp {
    /// The key of this operation's player-facing name.
    pub fn label_key(self) -> &'static str {
        match self {
            MilitaryOp::Move => "ui.military-op.move",
            MilitaryOp::Resupply => "ui.military-op.resupply",
            MilitaryOp::Patrol => "ui.military-op.patrol",
            MilitaryOp::Besiege => "ui.military-op.besiege",
            MilitaryOp::Raid => "ui.military-op.raid",
            MilitaryOp::Blockade => "ui.military-op.blockade",
        }
    }
}

/// The skill that governs a assignment's outcome.
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

impl GoverningSkill {
    /// The key of this skill's player-facing name.
    pub fn label_key(self) -> &'static str {
        match self {
            GoverningSkill::Command => "ui.inspector.skill.command",
            GoverningSkill::Diplomacy => "ui.inspector.skill.diplomacy",
            GoverningSkill::Intrigue => "ui.inspector.skill.intrigue",
            GoverningSkill::Stewardship => "ui.inspector.skill.stewardship",
        }
    }
}

/// An authored assignment definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentDef {
    /// The assignment's stable content key.
    pub key: ContentKey,
    /// Short player-facing title.
    pub title: String,
    /// One-sentence player-facing summary.
    pub summary: String,
    /// Routine or consequential.
    pub category: AssignmentCategory,
    /// Whether this assignment deterministically resolves as Success.
    ///
    /// Guaranteed assignments still consume their authored time and costs;
    /// they simply do not make an outcome draw.
    pub guaranteed: bool,
    /// Base duration in days.
    pub duration_days: u32,
    /// The skill that governs the outcome.
    pub skill: GoverningSkill,
    /// Difficulty on the same scale as skills (roughly 0..=20).
    pub difficulty: i32,
    /// What kind of target the assignment requires.
    pub target: AssignmentTargetKind,
    /// Personal risks the leader is exposed to on failure or disaster.
    pub risks: Vec<RiskTag>,
    /// The engine-owned military operation this assignment performs, if any.
    pub military_op: Option<MilitaryOp>,
    /// Whether autonomous organisations may start this assignment.
    pub ai_available: bool,
    /// Which pressure an autonomous house starts this assignment to answer.
    pub ai_intent: AiIntent,
    /// Wealth deducted when the assignment starts.
    pub wealth_cost: i64,
    /// Manpower committed when the assignment starts.
    pub manpower_cost: i64,
    /// Supplies consumed when the assignment starts.
    pub supplies_cost: i64,
    /// Influence spent when the assignment starts.
    pub influence_cost: i64,
    /// An authored live-opinion effectiveness modifier, when any.
    ///
    /// The mechanism (read the opinion, scale, clamp, add to
    /// effectiveness) is simulation code; every number and both roles are
    /// authored here, per assignment.
    pub opinion_modifier: Option<OpinionModifierDef>,
    /// An authored live-Order effectiveness modifier, when any.
    ///
    /// Reads the target province's live Order the same way the opinion
    /// modifier reads a live relationship; only province-bearing target
    /// kinds may author one.
    pub order_modifier: Option<OrderModifierDef>,
    /// Whether this work is covert: the authored *kind* of the work, which
    /// never changes. Its lines are confided to the owning organisation and
    /// to every house that has already proved the owner behind it, while
    /// spectators and replay retain everything. Who has proved whom is a
    /// separate, durable, per-viewer-and-per-knower exposure record written
    /// by investigation; being found out does not make the work any less
    /// covert, so content keyed off this flag keeps recognising it.
    pub covert: bool,
    /// Possible outcomes, keyed by kind. Success and failure are mandatory.
    /// Who this may be aimed at. Checked in exactly one place, so the
    /// button, the forecast, the autonomous houses and any standing order
    /// all agree by construction.
    pub requires: AssignmentRequires,
    /// How loudly it asks to be done.
    pub urgency: Urgency,
    /// The phases it runs through, in order.
    ///
    /// Never empty once loaded: content that authors none gets a single
    /// interruptible phase covering `duration_days`, so the two can never
    /// disagree about how long the work takes.
    pub stages: Vec<StageDef>,
    pub results: BTreeMap<OutcomeKind, OutcomeDef>,
}

impl AssignmentDef {
    /// The phase covering `day`, counted from the assignment's start.
    ///
    /// Saturates at the last phase rather than running off the end, so a
    /// late tick cannot index past the work.
    pub fn stage_at(&self, day: i64) -> usize {
        let mut elapsed = 0i64;
        for (index, stage) in self.stages.iter().enumerate() {
            elapsed += i64::from(stage.days);
            if day < elapsed {
                return index;
            }
        }
        self.stages.len().saturating_sub(1)
    }

    /// The day, counted from the start, on which `stage` ends.
    pub fn stage_ends(&self, stage: usize) -> i64 {
        self.stages
            .iter()
            .take(stage + 1)
            .map(|s| i64::from(s.days))
            .sum()
    }

    /// The first day from which this can no longer be called off, if
    /// there is one.
    ///
    /// What the interface shows before the player commits: a deadline
    /// they cannot see is not a decision they get to make.
    pub fn point_of_no_return(&self) -> Option<i64> {
        let mut elapsed = 0i64;
        for stage in &self.stages {
            if !stage.interruptible {
                return Some(elapsed);
            }
            elapsed += i64::from(stage.days);
        }
        None
    }
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

/// A traded commodity: something provinces make and want.
///
/// A good is authored so a province's production and consumption are
/// typed, not free strings, and so a surplus has a worth. The set is
/// small by intent; the model is logistics, not a market.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodDef {
    /// The good's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// Wealth a surplus unit fetches when it sells.
    pub value: i64,
}

/// A building a province may raise to change what it makes.
///
/// Built over time at a cost, a building adds to a province's plain
/// wealth output and to its production and consumption of goods — the one
/// lever, beyond who holds the ground, over what a place is worth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildingDef {
    /// The building's stable content key.
    pub key: ContentKey,
    /// Player-facing name.
    pub name: String,
    /// One-sentence player-facing summary.
    pub summary: String,
    /// Wealth spent to begin construction.
    pub wealth_cost: i64,
    /// Supplies spent to begin construction.
    pub supplies_cost: i64,
    /// Days the construction takes.
    pub build_days: u32,
    /// Added to the province's monthly wealth output once built.
    pub adds_wealth: i64,
    /// Added monthly production of each good once built.
    pub produces: BTreeMap<ContentKey, i64>,
    /// Added monthly consumption of each good once built.
    pub consumes: BTreeMap<ContentKey, i64>,
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
    /// Whether ordinary starships may dock here.
    #[serde(default)]
    pub starport: bool,
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
    /// Monthly production of each good, by good key.
    pub produces: BTreeMap<ContentKey, i64>,
    /// Monthly consumption of each good, by good key.
    pub consumes: BTreeMap<ContentKey, i64>,
}

/// The medium an authored route crosses.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteKind {
    /// A border or road between provinces on one body.
    Surface,
    /// A lane between authored starports.
    Space,
}

/// One authored, undirected connection between two provinces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteDef {
    /// Stable content identity.
    pub key: ContentKey,
    /// The route medium.
    pub kind: RouteKind,
    /// First endpoint.
    pub a: ContentKey,
    /// Second endpoint.
    pub b: ContentKey,
    /// Base traversal time.
    pub travel_days: u32,
    /// Reserved route danger in permille; not resolved in the current slice.
    pub risk: u16,
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
    /// Reusable Situations this scenario enables.
    pub situations: Vec<ContentKey>,
}

/// A semantic subject kind exposed to authored Situation scripts.
///
/// These are deliberately game concepts rather than ECS component names:
/// the simulation may change how a character or war is stored without
/// changing the content-facing contract.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SituationSubjectKind {
    /// The authored scenario itself.
    Scenario,
    /// A celestial body.
    Body,
    /// A province.
    Province,
    /// A character.
    Character,
    /// A political organisation.
    Organisation,
    /// A legal title.
    Title,
    /// A revocable office.
    Office,
    /// An army.
    Army,
    /// A ship.
    Ship,
    /// A running assignment.
    Assignment,
    /// An entry in the political-obligation ledger.
    Obligation,
    /// A formal war.
    War,
}

/// Who may see a Situation in an ordinary player campaign.
///
/// Spectator visibility is presentation policy and deliberately does not
/// alter this authored audience.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SituationVisibilityDef {
    /// Every player may see the Situation.
    Public,
    /// Only the characters or organisations named by these bindings may see
    /// it. Binding names are validated when content loads.
    Bound(Vec<String>),
}

/// One authored stage a Situation projection may select.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationStageDef {
    /// Stable stage key within the Situation.
    pub key: ContentKey,
    /// Player-facing stage title, filled from the string table.
    pub title: String,
    /// Player-facing stage summary, filled from the string table.
    pub summary: String,
    /// Optional attention warning, filled from the string table when present.
    pub warning: Option<String>,
}

/// One assignment shortcut a Situation projection may offer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationActionDef {
    /// Stable action key within the Situation.
    pub key: ContentKey,
    /// The ordinary assignment flow this action opens.
    pub assignment: ContentKey,
    /// Player-facing action label, filled from the string table.
    pub label: String,
}

/// One pure political answer a Situation may record — a choice with no
/// assignment behind it, such as promising or refusing a demand.
///
/// The chosen response is durable authoritative state keyed by the exact
/// activation, so outcome predicates can read what the player said long
/// after the words were spoken.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationResponseDef {
    /// Stable response key within the Situation.
    pub key: ContentKey,
    /// Player-facing response label, filled from the string table.
    pub label: String,
}

/// One ordered way an ended Situation may resolve.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationOutcomeDef {
    /// Stable outcome key within the Situation.
    pub key: ContentKey,
    /// Pure predicate selecting this outcome. `None` is the mandatory
    /// fallback and must be the final outcome.
    pub predicate_fn: Option<ScriptFnRef>,
    /// Pure function returning typed effects applied once when this outcome
    /// resolves. Requires the definition to declare an `owner_binding` so
    /// the effects act for a concrete bound organisation.
    pub effects_fn: Option<ScriptFnRef>,
    /// Player-facing resolution text, filled from the string table.
    pub text: String,
}

/// An authored shell over live authoritative simulation facts.
///
/// Trigger functions return zero or more typed binding maps. Projection
/// functions select from the declared stages and actions. When an instance
/// disappears, outcome predicates are tried in declaration order before the
/// mandatory fallback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationDef {
    /// Stable Situation key.
    pub key: ContentKey,
    /// Player-facing title, filled from the string table.
    pub title: String,
    /// Player-facing summary, filled from the string table.
    pub summary: String,
    /// Kind of authored source allowed to enable this definition.
    pub source: SituationSubjectKind,
    /// Additional typed bindings returned by the trigger, by stable name.
    pub bindings: BTreeMap<String, SituationSubjectKind>,
    /// Declared organisation binding whose bound organisation stands behind
    /// outcome effects and the activation announcement, when any.
    pub owner_binding: Option<String>,
    /// Declared character binding standing behind the `target` effect role
    /// when outcome effects or announcements resolve, when any. Lets a
    /// consequence address the bound person — a requester, a petitioner —
    /// rather than only organisation-derived roles.
    pub subject_binding: Option<String>,
    /// Pure function returning zero or more active instance bindings.
    pub trigger_fn: ScriptFnRef,
    /// Pure function returning the fixed presentation blocks for an instance.
    pub projection_fn: ScriptFnRef,
    /// Authored ordering priority; larger values appear first.
    pub priority: i32,
    /// Whether activation writes a permanent tagged log entry.
    pub log_activation: bool,
    /// Optional activation announcement, filled from the string table.
    /// When present, activation raises a pausing player popup with this text.
    pub announcement: Option<String>,
    /// Optional guidance objective shown by the client when scenario
    /// guidance is enabled. Presentation-only; the simulation ignores it.
    pub guidance_objective: Option<String>,
    /// Optional "Show me how" guidance prose. Presentation-only.
    pub guidance_how: Option<String>,
    /// Optional "Why this matters" guidance prose. Presentation-only.
    pub guidance_why: Option<String>,
    /// Authored audience, public by default.
    pub visibility: SituationVisibilityDef,
    /// Stages the projection may select.
    pub stages: Vec<SituationStageDef>,
    /// Assignment shortcuts the projection may offer.
    pub actions: Vec<SituationActionDef>,
    /// Pure recorded answers the player may give, at most one per
    /// activation.
    pub responses: Vec<SituationResponseDef>,
    /// Ordered resolutions, ending in one mandatory fallback.
    pub outcomes: Vec<SituationOutcomeDef>,
}

/// A loaded, validated content database.
///
/// Definitions are behaviour-free data; the retained per-file ASTs hold the
/// named script functions definitions may reference.
pub struct ContentSet {
    /// Assignment definitions by key.
    pub assignments: BTreeMap<ContentKey, AssignmentDef>,
    /// Celestial bodies by key.
    pub bodies: BTreeMap<ContentKey, BodyDef>,
    /// Traded commodities by key.
    pub goods: BTreeMap<ContentKey, GoodDef>,
    /// Buildings a province may raise, by key.
    pub buildings: BTreeMap<ContentKey, BuildingDef>,
    /// Provinces by key.
    pub provinces: BTreeMap<ContentKey, ProvinceDef>,
    /// Authored surface and space routes by key.
    pub routes: BTreeMap<ContentKey, RouteDef>,
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
    /// Plans autonomous characters may pursue, by key.
    pub plans: BTreeMap<ContentKey, PlanDef>,
    /// Grand-strategy goals a house head may adopt, by key.
    pub goals: BTreeMap<ContentKey, GoalDef>,
    /// Authored Situation definitions by key.
    pub situations: BTreeMap<ContentKey, SituationDef>,
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
        self.assignments == other.assignments
            && self.bodies == other.bodies
            && self.goods == other.goods
            && self.buildings == other.buildings
            && self.provinces == other.provinces
            && self.traits == other.traits
            && self.name_pools == other.name_pools
            && self.characters == other.characters
            && self.organisations == other.organisations
            && self.titles == other.titles
            && self.offices == other.offices
            && self.ships == other.ships
            && self.armies == other.armies
            && self.obligations == other.obligations
            && self.events == other.events
            && self.plans == other.plans
            && self.goals == other.goals
            && self.situations == other.situations
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
/// assignment effectiveness.
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

/// The four practical skills characters bring to assignments and rule.
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
    /// Reusable Situations this title enables.
    pub situations: Vec<ContentKey>,
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
    /// Optional deputy command officer.
    pub first_officer: Option<ContentKey>,
    /// Maximum soldiers carried as one whole army.
    pub troop_capacity: i64,
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
    pub general: Option<ContentKey>,
    /// Optional deputy commander.
    pub lieutenant: Option<ContentKey>,
    /// The province it stands in.
    pub province: ContentKey,
    /// Soldiers under arms.
    pub manpower: i64,
    /// Supplies in its train.
    pub supplies: i64,
}

/// The pressure an autonomous house would start a assignment to answer.
///
/// Authored on the assignment rather than hardcoded in the simulation, so the
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
    /// Undermine a rival by indirect, deniable means.
    Subvert,
    /// Take ground from a rival by open, limited formal war: raise the
    /// host, declare, besiege one holding, and settle.
    Invade,
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
    /// A complication in a assignment under way.
    Assignment,
}

/// Declarative conditions an event needs before it may fire.
///
/// Conditions are data rather than script so they can be validated at
/// load and evaluated identically on every replay.
/// How loudly an assignment asks to be done.
///
/// Only two things read it: whether an idle character will pick an
/// assignment up unbidden, and whether a more pressing one may interrupt
/// a less pressing one. It is deliberately coarse — a finer scale would
/// invite tuning that no player could perceive.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Urgency {
    /// Worth doing when there is nothing better.
    #[default]
    Routine,
    /// Worth doing soon.
    Pressing,
    /// Worth dropping other work for.
    Urgent,
}

/// One phase of an assignment, and whether it can be called off.
///
/// An assignment that authors no stages is one stage long and can be
/// called off at any time, which is exactly how every assignment behaved
/// before stages existed. Authoring them is how content says that some
/// part of the work is a commitment: a march can be turned around, an
/// assault under way cannot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageDef {
    /// Stable id within its assignment, for naming it to the player.
    pub id: String,
    /// How long this phase lasts.
    pub days: u32,
    /// Whether the assignment can be called off during it.
    pub interruptible: bool,
    /// Applied if the assignment is called off during this phase.
    ///
    /// Being turned back on the road is not the same as abandoning a
    /// siege, so what it costs is authored per phase rather than once.
    pub on_interrupt: Option<ScriptFnRef>,
}

/// How a live opinion between two assignment-context roles shifts an
/// assignment's effectiveness.
///
/// Data, not behaviour: the simulation owns the arithmetic (multiply,
/// truncate, clamp, add inside the shared effectiveness calculation), and
/// this block owns every number. Roles reuse the closed [`EffectRole`]
/// vocabulary, restricted at load to the roles resolvable from the owner
/// and leader alone — a role that needs a target would silently read
/// nobody before one is chosen.
///
/// [`EffectRole`]: crate::effect::EffectRole
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpinionModifierDef {
    /// Whose live regard is read.
    pub from: crate::effect::EffectRole,
    /// Who that regard is toward.
    pub toward: crate::effect::EffectRole,
    /// Hundredths of an effectiveness point per point of opinion.
    pub per_point: i32,
    /// Lower clamp on the resulting shift, in effectiveness points.
    /// At most zero, so a neutral relationship never reads as a penalty.
    pub min: i32,
    /// Upper clamp on the resulting shift, in effectiveness points.
    /// At least zero, so a neutral relationship never reads as a bonus.
    pub max: i32,
}

/// How a target province's live Order shifts an assignment's effectiveness.
///
/// The second authored effectiveness modifier, shaped exactly like
/// [`OpinionModifierDef`]: data, not behaviour. The simulation owns the
/// arithmetic — the shortfall of the province's live Order below the
/// authored reference, times `per_hundred` hundredths of an effectiveness
/// point, truncated toward zero and clamped to the authored bounds, added
/// inside the shared effectiveness calculation. Content that authors
/// `max: 0` makes Order pure resistance: a well-ordered province blunts
/// the work and a disordered one never helps beyond neutral.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderModifierDef {
    /// The Order value at which the shift reads zero.
    pub reference: i32,
    /// Hundredths of an effectiveness point per point of Order below the
    /// reference. Order above the reference produces a negative shift.
    pub per_hundred: i32,
    /// Lower clamp on the resulting shift, in effectiveness points.
    /// At most zero.
    pub min: i32,
    /// Upper clamp on the resulting shift, in effectiveness points.
    /// At least zero.
    pub max: i32,
}

/// Who a target has to be, for an assignment to be offered against it.
///
/// The same shape as [`EventRequires`], and for the same reason: conditions
/// are data rather than script, so they can be validated at load, shown to
/// the player as the reason a button is refused, and evaluated identically
/// on every replay.
///
/// Every field defaults to "do not care", so an assignment that says
/// nothing is offered exactly as widely as it was before this existed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentRequires {
    /// Whose the target province must be.
    pub target_holder: HolderRelation,
    /// A force hostile to the owner must be standing in the target.
    pub target_occupied: bool,
    /// The ordered army must already be in the target province.
    pub army_present: bool,
    /// Whose house the target character must belong to.
    pub target_house: HolderRelation,
    /// The target must hold a title of this kind.
    pub target_holds_title: Option<TitleNeed>,
    /// The target organisation must owe the owner an open favour.
    pub target_owes_favour: bool,
    /// The owner must have a holding with a hostile force standing in it.
    ///
    /// Unlike the rest, this is about the owner rather than the target: it
    /// is what stops an assignment that answers an alarm being offered
    /// when no alarm is sounding.
    pub owner_threatened: bool,
    /// Somebody else's covert work must be running against the target
    /// province, and the owner must not yet have proved whose: a live
    /// covert province-aimed assignment owned by an organisation other
    /// than the owner, for which the owner holds no exposure record.
    ///
    /// About what is being done to the target rather than who holds it:
    /// it is what stops an enquiry into covert work being offered on
    /// ground nobody is working against, or against a hand already
    /// proved — either way, where it could prove nothing.
    pub target_under_covert_work: bool,
    /// The leader must hold a standing command over an army the owner
    /// fields — their own post, not any force the house happens to have.
    ///
    /// About the leader rather than the target: it is what stops an
    /// assignment that grows the leader's own force being accepted, paid
    /// for, and logged when they command nothing for it to land on.
    pub leader_commands_army: bool,
    /// The target province's order must be at or below this.
    pub max_order: Option<i32>,
    /// The target province's order must be at or above this.
    pub min_order: Option<i32>,
}

/// A kind of title a target must hold.
///
/// Deliberately without the body or province a real title names: the
/// question an assignment asks is "are they the Consul", not "are they
/// Consul of this particular place".
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleNeed {
    /// Holds a province's title.
    Province,
    /// Holds a body's paramountcy.
    Paramount,
    /// Holds the Consulship.
    Consul,
}

/// Whose something must be, relative to the organisation acting.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HolderRelation {
    /// Anyone's, including nobody's.
    #[default]
    Any,
    /// The acting organisation's own.
    Own,
    /// Somebody else's. Unheld ground counts as nobody's, and so is not
    /// somebody else's — you cannot raid an empty province.
    Other,
}

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
    /// Reusable Situations this authored obligation enables.
    pub situations: Vec<ContentKey>,
}

/// An authored plan: a goal a character may pursue over months, decomposed
/// into methods and steps.
///
/// A plan is the AI's counterpart to a campaign the player would run by
/// hand: where the reactive scorer answers a pressure with a single
/// assignment, a plan answers one with an ordered sequence of them. It is
/// data all the way down — the goal names a pressure from the existing
/// intent vocabulary, methods are gated by declarative conditions, and
/// every step names an ordinary assignment or a sub-plan — so a new plan
/// is authored content, not an engine change, and conditions validate at
/// load and replay identically.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDef {
    /// The plan's stable content key.
    pub key: ContentKey,
    /// Short player-facing title.
    pub title: String,
    /// One-sentence player-facing summary.
    pub summary: String,
    /// The pressure this plan answers, from the shared intent vocabulary.
    pub goal: AiIntent,
    /// Added to the scored pressure when weighing the plan against acting
    /// on the pressure directly: how much better a campaign is than a
    /// single act.
    pub score_bonus: i64,
    /// What the plan as a whole is aimed at. Restricted to `None`,
    /// `Organisation`, or `Province`; the finer target kinds belong to
    /// individual assignments.
    pub target: AssignmentTargetKind,
    /// Days after completion or abandonment before the same character may
    /// adopt this plan again.
    pub cooldown_days: u32,
    /// Abandon the plan outright if it is still running after this many
    /// days. The single safety valve against a plan waiting forever on a
    /// step that never becomes possible.
    pub max_days: u32,
    /// How many times one step may fail before the plan is abandoned.
    pub max_step_retries: u32,
    /// Whether this campaign is covert: the authored kind of the work.
    /// Its ordinary player surfaces (adoption and end logs, rumours, the
    /// inspector's pursuing line) are confided to the owning organisation
    /// and to every house that has proved the owner behind it — a durable,
    /// per-knower exposure record written by investigation — while
    /// spectators and replay retain everything.
    pub covert: bool,
    /// Conditions under which the campaign loses its grounds: while no
    /// step is committed, a day on which these hold abandons the plan, and
    /// a candidate on which they already hold is never adopted. Judged
    /// over the authority and the plan's target like a method gate. Work
    /// already accepted is never touched — it runs to its ordinary end —
    /// so only the uncommitted residue of a campaign can end this way.
    pub abandon_when: Option<PlanRequires>,
    /// Ways to pursue the goal, in authored preference order.
    pub methods: Vec<PlanMethodDef>,
}

/// One way to pursue a plan's goal: a gate and an ordered list of steps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanMethodDef {
    /// Stable id within its plan, for naming it in logs and tests.
    pub id: String,
    /// Conditions the actor must meet before this method may be chosen.
    pub requires: PlanRequires,
    /// The steps, in the order they are taken.
    pub steps: Vec<PlanStepDef>,
}

/// One step of a plan method.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStepDef {
    /// Stable id within its method; defaults to the action's key.
    pub id: String,
    /// What taking this step means.
    pub action: PlanStepAction,
    /// Skip the step when these conditions already hold — a treasury that
    /// is full does not need filling.
    pub skip_if: Option<PlanRequires>,
}

/// What a plan step does when its turn comes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStepAction {
    /// Start an ordinary assignment through the same gate the player uses.
    Assignment {
        /// The assignment to start.
        key: ContentKey,
        /// Where its target comes from.
        target: PlanTargetSelector,
    },
    /// Expand another plan's first eligible method in place, at adoption.
    SubPlan(ContentKey),
    /// Set one army's standing orders — the same editable list the player
    /// sets by hand, driving the same reactive behaviour. A plan never
    /// commands a battle; it points the army at the doctrine and the
    /// doctrine fights.
    Orders {
        /// Which army receives the orders.
        army: PlanArmySelector,
        /// The standing orders, in priority order.
        orders: Vec<ContentKey>,
    },
}

/// Which single army an orders step is for.
///
/// Per army, deliberately, never a broadcast to everything the house
/// fields: an order is given to a force, and which force is part of what
/// the plan means. The vocabulary grows by demonstrated need, like
/// [`PlanTargetSelector`]'s.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanArmySelector {
    /// The army the acting character generals; the lowest stable ID when
    /// they general several. No such army leaves the step waiting.
    #[default]
    Own,
    /// The largest army the acting character generals — most manpower,
    /// lowest stable ID on a tie. The host raised for a war rather than
    /// the household levy that happened to be indexed first. No such army
    /// leaves the step waiting.
    Strongest,
}

/// Where a plan step's assignment target comes from.
///
/// Deliberately tiny, and grown only by demonstrated need: a selector is
/// an integer choice over visible state, resolved at the moment its step
/// starts, so a plan aims at what is true then rather than what was true
/// at adoption.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlanTargetSelector {
    /// The assignment takes no target.
    #[default]
    None,
    /// The assignment is aimed at the plan's target.
    PlanTarget,
    /// The authority's most disordered holding — lowest order, lowest
    /// stable ID on a tie. Produces a province target; holding nothing
    /// leaves the step waiting.
    WorstHolding,
    /// The head of the plan's target organisation — a character. The knife
    /// a scheme aims at the person a rival house is led by. Needs the plan
    /// to have an organisation target; a headless house leaves the step
    /// waiting.
    TargetHead,
    /// The most disordered province held by the opposing side of the
    /// plan's exact formal-war target. Produces an army-and-province target
    /// using the acting character's own army; no such army or province
    /// leaves the step waiting.
    LowestEnemyProvinceInWar,
    /// The plan's target organisation's most disordered province sharing a
    /// surface route with a province the authority holds — lowest order,
    /// lowest stable ID on a tie. Produces a province target; needs the
    /// plan to have an organisation target, and no shared border leaves
    /// the step waiting.
    TargetBorderProvince,
    /// The most disordered province held by the opposing side of the
    /// plan's exact formal-war target that shares a surface route with a
    /// province the authority holds — lowest order, lowest stable ID on a
    /// tie. Produces an army-and-province target using the acting
    /// character's strongest own army (the host raised for the war, not
    /// the household levy); no such army or no enemy border province
    /// leaves the step waiting.
    LowestEnemyBorderProvinceInWar,
}

/// Declarative conditions gating a plan method, skipping a step,
/// abandoning an uncommitted plan, or setting a goal aside.
///
/// The same shape and reason as [`AssignmentRequires`]: conditions are
/// data rather than script, so they validate at load and evaluate
/// identically on every replay. Every field defaults to "do not care".
/// Integer facts about the actor's authority and the plan's (or goal's)
/// resolved target only — a condition the player could not check by
/// looking at the same screens does not belong here.
///
/// This shape is persisted: an active plan copies each step's `skip_if`
/// from the definition at adoption, so it rides in every campaign
/// snapshot. `serde(default)` keeps the derive tolerant: a missing field
/// reads as "do not care" (its default) instead of failing the whole parse,
/// so an older save reaches the version check and is refused cleanly. It
/// is a parse convenience only — a predicate added later still changes the
/// hashed serialised shape and still bumps `SNAPSHOT_FORMAT_VERSION` under
/// the snapshot rule, exactly as this shape did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanRequires {
    /// The authority's wealth must be at or above this.
    pub min_wealth: Option<i64>,
    /// The authority's manpower must be at or above this.
    pub min_manpower: Option<i64>,
    /// The authority's influence must be at or above this.
    pub min_influence: Option<i64>,
    /// The authority's effective legitimacy must be at or above this.
    pub min_legitimacy: Option<i32>,
    /// Whether the authority must have (or must not have) a standing army.
    pub has_army: Option<bool>,
    /// The plan's target organisation must owe the authority an open
    /// favour.
    pub target_owes_favour: bool,
    /// Whether the authority must be the dominant claimant of the crisis
    /// body. `None` does not care, while `Some(false)` lets authored plans
    /// describe what to do while still trailing.
    pub dominant_claimant: Option<bool>,
    /// Whether the acting head must currently hold a personal Paramount
    /// claim.
    pub has_paramount_claim: Option<bool>,
    /// The authority's complete prospective war branch must have at least
    /// this many permille of an organisation target's raised manpower.
    pub min_target_branch_manpower_permille: Option<i64>,
    /// The authority's complete prospective war branch must have at most
    /// this many permille of an organisation target's raised manpower.
    pub max_target_branch_manpower_permille: Option<i64>,
    /// Whether an exact formal-war target must still have at least one
    /// province held by the opposing frozen side.
    pub war_has_enemy_province: Option<bool>,
    /// The campaign day (days since the campaign start) must be at or
    /// after this. With `max_campaign_day` it authors a deterministic
    /// window in data rather than in engine code.
    pub min_campaign_day: Option<i64>,
    /// The campaign day must be at or before this.
    pub max_campaign_day: Option<i64>,
    /// The authority head's opinion of the target organisation's head must
    /// be at or below this — the authored hostility floor. A missing head
    /// on either side fails the condition.
    pub max_target_head_opinion: Option<i32>,
    /// The target organisation must owe the authority an open grievance —
    /// hostility's other authored ground, mirroring `target_owes_favour`.
    pub target_owes_grievance: bool,
    /// The authority head's opinion of the target organisation's head must
    /// be at or above this — the authored reconciliation line, the mirror
    /// of `max_target_head_opinion`. A missing head on either side fails
    /// the condition.
    pub min_target_head_opinion: Option<i32>,
    /// The target organisation must owe the authority no open grievance —
    /// the sibling of `target_owes_grievance`, for conditions that need
    /// the ledger clear rather than charged.
    pub target_owes_no_grievance: bool,
    /// Whether an active formal war must (or must not) place the authority
    /// and the target organisation on opposing sides — the bilateral
    /// reading, so a war already declared is a fact reconciliation cannot
    /// wish away.
    pub at_war_with_target: Option<bool>,
    /// Whether the authority must (or must not) lead a side of an active
    /// formal war — a war it declared or that was declared against it,
    /// its own war — the unilateral reading beside `at_war_with_target`,
    /// so a house already fighting somebody else can be told to wait
    /// before opening a second front. Being swept into a liege's war as a
    /// branch member is not a war of the house's own and does not count.
    pub at_war: Option<bool>,
    /// The authority's complete raised manpower — every army in its
    /// prospective war branch — must be at or above this: the absolute
    /// reading beside the permille comparison, for a campaign that arms
    /// to an authored strength rather than to a rival's.
    pub min_branch_manpower: Option<i64>,
    /// The authority's complete raised manpower must be at or below this.
    pub max_branch_manpower: Option<i64>,
    /// Whether an exact formal-war target must still have at least one
    /// province held by the opposing frozen side that shares a surface
    /// route with a province the authority holds — the geographic sibling
    /// of `war_has_enemy_province`, for campaigns that reach only across
    /// their own border.
    pub war_has_enemy_border_province: Option<bool>,
    /// Whether the acting character must (or must not) general an army the
    /// authority fields — their own standing command, not any force the
    /// house happens to have. Read against the plan's actor; an ambition,
    /// which has no actor of its own, reads its house's head. What lets a
    /// campaign muster a levy for a head who inherited a house whose army
    /// still answers to the dead, instead of skipping to a reinforcement
    /// nobody could receive.
    pub leader_commands_army: Option<bool>,
}

/// An authored grand-strategy goal: a house's standing ambition.
///
/// Where a [`PlanDef`] answers one pressure — a tactic — a goal is the
/// long horizon above several, chosen by a head and pursued across
/// reigns. A goal does not execute: while active it biases which
/// pressures the house reaches for, so the plans and assignments already
/// in place carry it out, and it presses advisory directives on the
/// house's vassals. Like a plan, it is data all the way down, so a new
/// ambition is authored content, not an engine change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalDef {
    /// The goal's stable content key.
    pub key: ContentKey,
    /// Short player-facing title.
    pub title: String,
    /// One-sentence player-facing summary.
    pub summary: String,
    /// When this ambition is attractive enough to adopt.
    pub trigger: GoalRequires,
    /// Relative authored priority among simultaneously eligible ambitions.
    /// Higher values are considered first; stable content key breaks ties.
    pub priority: i64,
    /// The pressures pursuing this goal favours; each is lifted by
    /// `favour_bonus` in the head's scoring while the goal is active.
    pub favours: Vec<AiIntent>,
    /// How much a favoured pressure is lifted while the goal is active.
    pub favour_bonus: i64,
    /// What the ambition as a whole is aimed at. Restricted to `None`,
    /// `Organisation`, or `Province`, like a plan's target.
    pub target: AssignmentTargetKind,
    /// How an organisation-aimed ambition resolves its concrete target.
    pub target_selector: GoalTargetSelector,
    /// Whether this ambition is covert: the authored kind of the work. Its
    /// adoption and end logs are confided to the owning organisation and to
    /// every house that has proved the owner behind it — a durable,
    /// per-knower exposure record written by investigation — while
    /// spectators and replay retain everything.
    pub covert: bool,
    /// The advisory directives pressed on the house's vassals while the
    /// goal is active.
    pub directives: Vec<DirectiveDef>,
    /// Conditions under which the ambition loses its grounds: on a monthly
    /// pulse on which these hold over the house and the goal's resolved
    /// target, the goal is set aside without starting its cooldown — the
    /// grounds may return, and a house that finds them returned may adopt
    /// the ambition afresh — and a candidate on which they already hold is
    /// never adopted. Judged with the plan vocabulary, so the relationship
    /// predicates a campaign gates on can end the ambition above it.
    pub set_aside_when: Option<PlanRequires>,
    /// Abandon the goal if it is still unmet after this many days.
    pub max_days: u32,
    /// Days after the goal ends before the house may adopt it again.
    pub cooldown_days: u32,
}

/// How an organisation-aimed goal picks its concrete target at adoption.
///
/// A selector is a deterministic integer choice over visible state, like a
/// plan step's target selector, and every threshold it weighs is authored
/// here rather than hardcoded. The vocabulary grows by demonstrated need.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalTargetSelector {
    /// The weakest rival great house — fewest holdings, lowest stable ID
    /// on a tie — outside the house's own chain of command.
    #[default]
    WeakestRival,
    /// A hostile organisation holding ground on the house's own border: it
    /// holds a province sharing a surface route with a held province, it
    /// is outside the house's chain of command, and the house's head
    /// regards its head at or below the authored opinion floor — or, when
    /// `with_grievance` is set, it owes the house an open grievance.
    /// Lowest stable organisation ID breaks ties; no such neighbour makes
    /// the goal unadoptable.
    HostileBorderNeighbour {
        /// The hostility floor: qualify when the acting head's opinion of
        /// the candidate's head is at or below this.
        max_head_opinion: i32,
        /// Whether an open grievance the candidate owes the house also
        /// qualifies it, independent of opinion.
        with_grievance: bool,
    },
}

/// One advisory directive a goal presses on each of the house's vassals.
///
/// A directive is a wish, not an order: it lifts the matching pressure in
/// the vassal head's own scoring, so the vassal is more likely to serve
/// its liege's aim without ever being compelled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectiveDef {
    /// The pressure the liege wants its vassals to feel more keenly.
    pub intent: AiIntent,
    /// Whether the directive carries the goal's target down to the vassal.
    pub target: DirectiveTarget,
}

/// Where a directive's target comes from.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DirectiveTarget {
    /// The directive names nothing; the vassal feels the pressure at large.
    #[default]
    None,
    /// The directive carries the goal's own target down to the vassal.
    GoalTarget,
}

/// Declarative trigger for adopting a goal.
///
/// The same shape and reason as [`PlanRequires`] — integer facts over
/// visible state, validated at load, evaluated identically on replay —
/// extended with the hierarchy facts a grand strategy weighs: whether the
/// house has vassals to command, and whether it is itself a vassal. Every
/// field defaults to "do not care".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalRequires {
    /// The house's wealth must be at or above this.
    pub min_wealth: Option<i64>,
    /// The house's manpower must be at or above this.
    pub min_manpower: Option<i64>,
    /// The house's effective legitimacy must be at or above this.
    pub min_legitimacy: Option<i32>,
    /// Whether the house must have (or must not have) a standing army.
    pub has_army: Option<bool>,
    /// The house must be the dominant claimant of the crisis body.
    pub dominant_claimant: bool,
    /// Whether the house must have (or must not have) vassals.
    pub has_vassals: Option<bool>,
    /// Whether the house must itself be (or not be) a vassal.
    pub is_vassal: Option<bool>,
    /// The current head must pass the authoritative eligibility rules for
    /// declaring a personal claim to the vacant Paramountcy.
    pub can_claim_paramountcy: bool,
    /// The campaign day (days since the campaign start) must be at or
    /// after this. With `max_campaign_day` it authors a deterministic
    /// adoption window in data rather than in engine code.
    pub min_campaign_day: Option<i64>,
    /// The campaign day must be at or before this.
    pub max_campaign_day: Option<i64>,
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
