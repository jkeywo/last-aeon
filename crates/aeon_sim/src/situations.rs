//! Authored Situation lifecycles over live authoritative simulation facts.
//!
//! Rhai decides which structurally identified instances are active and how
//! they read. Rust owns validation, persistence, ordering, command launches,
//! resolution notices, diagnostics, and the fixed presentation vocabulary.

use std::collections::{BTreeMap, BTreeSet};

use aeon_core::calendar::GameDate;
use aeon_data::model::{SituationDef, SituationSubjectKind, SituationVisibilityDef};
use aeon_data::{ContentKey, ContentSet};
use bevy::app::App;
use bevy::prelude::{Resource, World};
use rhai::{Array, Dynamic, Map};
use serde::{Deserialize, Serialize};

use crate::assignments::{AssignmentTarget, LogAudience, LogChannel, LogEntry, ScriptRuntime};
use crate::clock::{CampaignClock, SettledDay};
use crate::ids::{
    ArmyId, AssignmentId, BodyId, CharacterId, OfficeId, OrgId, ProvinceId, ShipId, TitleId, WarId,
};
use crate::state::ContentDb;

/// One semantic subject bound into a Situation instance.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SituationSubject {
    /// The authored scenario.
    Scenario(ContentKey),
    /// A celestial body.
    Body(BodyId),
    /// A province.
    Province(ProvinceId),
    /// A character.
    Character(CharacterId),
    /// A political organisation.
    Organisation(OrgId),
    /// A legal title.
    Title(TitleId),
    /// A revocable office.
    Office(OfficeId),
    /// An army.
    Army(ArmyId),
    /// A ship.
    Ship(ShipId),
    /// A running assignment.
    Assignment(AssignmentId),
    /// A political obligation.
    Obligation(u64),
    /// A formal war occurrence.
    War(WarId),
}

impl SituationSubject {
    /// The content vocabulary kind of this subject.
    pub fn kind(&self) -> SituationSubjectKind {
        match self {
            Self::Scenario(_) => SituationSubjectKind::Scenario,
            Self::Body(_) => SituationSubjectKind::Body,
            Self::Province(_) => SituationSubjectKind::Province,
            Self::Character(_) => SituationSubjectKind::Character,
            Self::Organisation(_) => SituationSubjectKind::Organisation,
            Self::Title(_) => SituationSubjectKind::Title,
            Self::Office(_) => SituationSubjectKind::Office,
            Self::Army(_) => SituationSubjectKind::Army,
            Self::Ship(_) => SituationSubjectKind::Ship,
            Self::Assignment(_) => SituationSubjectKind::Assignment,
            Self::Obligation(_) => SituationSubjectKind::Obligation,
            Self::War(_) => SituationSubjectKind::War,
        }
    }

    /// Raw occurrence identity, absent only for the scenario source.
    pub fn raw(&self) -> Option<u64> {
        match self {
            Self::Scenario(_) => None,
            Self::Body(id) => Some(id.raw()),
            Self::Province(id) => Some(id.raw()),
            Self::Character(id) => Some(id.raw()),
            Self::Organisation(id) => Some(id.raw()),
            Self::Title(id) => Some(id.raw()),
            Self::Office(id) => Some(id.raw()),
            Self::Army(id) => Some(id.raw()),
            Self::Ship(id) => Some(id.raw()),
            Self::Assignment(id) => Some(id.raw()),
            Self::Obligation(id) => Some(*id),
            Self::War(id) => Some(id.raw()),
        }
    }
}

/// The authored definition which enabled an instance, and its concrete source.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SituationSource {
    /// Source kind declared by the Situation definition.
    pub kind: SituationSubjectKind,
    /// Authored scenario, title, or obligation key.
    pub key: ContentKey,
    /// Runtime title or obligation ID. Scenarios have none.
    pub id: Option<u64>,
}

impl SituationSource {
    fn subject(&self) -> Result<SituationSubject, SituationError> {
        match (self.kind, self.id) {
            (SituationSubjectKind::Scenario, None) => {
                Ok(SituationSubject::Scenario(self.key.clone()))
            }
            (SituationSubjectKind::Title, Some(raw)) => TitleId::from_raw(raw)
                .map(SituationSubject::Title)
                .ok_or_else(|| SituationError::BadSubject("invalid title source ID".to_owned())),
            (SituationSubjectKind::Obligation, Some(raw)) if raw > 0 => {
                Ok(SituationSubject::Obligation(raw))
            }
            _ => Err(SituationError::BadSubject(format!(
                "unsupported {:?} source",
                self.kind
            ))),
        }
    }
}

/// Structural identity: definition, authored source, and ordered bindings.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SituationInstanceKey {
    /// Reusable Situation definition.
    pub definition: ContentKey,
    /// Concrete source attachment.
    pub source: SituationSource,
    /// Additional declared bindings, in name order.
    pub bindings: BTreeMap<String, SituationSubject>,
}

/// One activation of a structural Situation key.
///
/// Structural keys are deliberately reusable (for example, every vacancy of
/// the same Consular title has the same key). The activation date distinguishes
/// lifecycles because reactivation can only occur on a later settled day.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SituationOccurrence {
    /// Reusable structural Situation identity.
    pub situation: SituationInstanceKey,
    /// First settled date of this activation.
    pub activated: GameDate,
}

/// Minimal persisted state for one active lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSituation {
    /// Structural identity and bound subjects.
    pub key: SituationInstanceKey,
    /// First settled date this lifecycle was returned by its trigger.
    pub activated: GameDate,
}

impl ActiveSituation {
    /// Exact provenance tag for this activation.
    pub fn occurrence(&self) -> SituationOccurrence {
        SituationOccurrence {
            situation: self.key.clone(),
            activated: self.activated,
        }
    }
}

/// One fixed metric row returned by a projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SituationMetric {
    /// String-table key naming the value.
    pub label_key: String,
    /// Integer or textual display value.
    pub value: SituationMetricValue,
}

/// The bounded metric value vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SituationMetricValue {
    /// Signed integer value.
    Integer(i64),
    /// Authored or resolved text.
    Text(String),
}

/// A navigation link or displayed participant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationLink {
    /// Semantic subject kind.
    pub kind: SituationSubjectKind,
    /// Stable subject ID.
    pub id: u64,
    /// Optional string-table key overriding its ordinary display name.
    pub label_key: Option<String>,
}

/// One authored heading with a fixed set of semantically linked participants.
///
/// Groups let content explain roles within a Situation without teaching the
/// client what an attacker, candidate, creditor, or any future role means.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationParticipantGroup {
    /// String-table key naming the role shared by the participants.
    pub label_key: String,
    /// Participants in deterministic authored order.
    pub participants: Vec<SituationLink>,
}

/// One concrete assignment shortcut emitted by a projection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SituationAction {
    /// Authored action ID within the definition.
    pub id: ContentKey,
    /// Leader preselected by content, when fixed.
    pub leader: Option<CharacterId>,
    /// Concrete assignment target.
    pub target: AssignmentTarget,
    /// Semantic subjects that distinguish repeated instances of this action.
    pub context: Vec<SituationLink>,
}

/// Fixed presentation model derived for one active instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SituationProjection {
    /// Authored stage selected by the script.
    pub stage: ContentKey,
    /// Optional authoritative deadline.
    pub deadline: Option<GameDate>,
    /// Whether this stage raises attention.
    pub warning: bool,
    /// Fixed metric rows.
    pub metrics: Vec<SituationMetric>,
    /// Displayed participants.
    pub participants: Vec<SituationLink>,
    /// Displayed participants separated into authored semantic roles.
    pub participant_groups: Vec<SituationParticipantGroup>,
    /// Other navigable subjects.
    pub links: Vec<SituationLink>,
    /// Assignment actions available to the current viewer.
    pub actions: Vec<SituationAction>,
    /// Safe values used when freezing a resolution sentence.
    pub resolution_values: BTreeMap<String, String>,
}

/// A dismissible record of one completed Situation lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationResolution {
    /// Monotonic notice identity used by the dismiss command.
    pub id: u64,
    /// Ended lifecycle.
    pub situation: SituationInstanceKey,
    /// First settled date of the activation which ended.
    pub activated: GameDate,
    /// Date it ended.
    pub resolved: GameDate,
    /// Authored outcome selected in declaration order.
    pub outcome: ContentKey,
    /// Frozen rendered resolution text.
    pub text: String,
    /// Participants frozen at resolution time.
    pub participants: Vec<SituationLink>,
    /// Participant roles frozen at resolution time.
    #[serde(default)]
    pub participant_groups: Vec<SituationParticipantGroup>,
    /// Links frozen at resolution time.
    pub links: Vec<SituationLink>,
}

impl SituationResolution {
    /// Exact provenance tag for the activation represented by this notice.
    pub fn occurrence(&self) -> SituationOccurrence {
        SituationOccurrence {
            situation: self.situation.clone(),
            activated: self.activated,
        }
    }
}

/// Snapshotted lifecycle, notice, and deterministic diagnostic bookkeeping.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituationState {
    /// Active lifecycles in structural-key order.
    pub active: BTreeMap<SituationInstanceKey, ActiveSituation>,
    /// Undismissed resolution notices in creation order.
    pub resolutions: Vec<SituationResolution>,
    /// Next notice ID.
    pub next_resolution_id: u64,
    /// Current runtime error per unavailable instance.
    #[serde(default)]
    pub runtime_errors: BTreeMap<SituationInstanceKey, String>,
    /// Diagnostic fingerprints already written to the permanent log.
    #[serde(default)]
    pub logged_diagnostics: BTreeSet<String>,
}

/// A derived card ready for fixed client layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SituationCard {
    /// Active lifecycle.
    pub active: ActiveSituation,
    /// Definition title.
    pub title: String,
    /// Definition summary.
    pub summary: String,
    /// Authored priority.
    pub priority: i32,
    /// Projection, absent when the script is unavailable.
    pub projection: Option<SituationProjection>,
    /// Deterministic runtime error when unavailable.
    pub unavailable: Option<String>,
}

/// One deterministic trigger, projection, or outcome failure found while
/// validating the currently attached Situation deck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SituationRuntimeIssue {
    /// Structural instance or source placeholder that could not be evaluated.
    pub situation: SituationInstanceKey,
    /// Stable diagnostic text returned by the sandbox boundary or shape parser.
    pub error: String,
}

/// Why an authored Situation value could not be used.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SituationError {
    /// A script function failed.
    #[error("script call failed: {0}")]
    Script(String),
    /// A returned value had the wrong outer type.
    #[error("{0}")]
    BadShape(String),
    /// A binding or link named an invalid subject.
    #[error("{0}")]
    BadSubject(String),
    /// A projection selected undeclared content.
    #[error("{0}")]
    Undeclared(String),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Attachment {
    definition: ContentKey,
    source: SituationSource,
}

fn kind_name(kind: SituationSubjectKind) -> &'static str {
    match kind {
        SituationSubjectKind::Scenario => "scenario",
        SituationSubjectKind::Body => "body",
        SituationSubjectKind::Province => "province",
        SituationSubjectKind::Character => "character",
        SituationSubjectKind::Organisation => "organisation",
        SituationSubjectKind::Title => "title",
        SituationSubjectKind::Office => "office",
        SituationSubjectKind::Army => "army",
        SituationSubjectKind::Ship => "ship",
        SituationSubjectKind::Assignment => "assignment",
        SituationSubjectKind::Obligation => "obligation",
        SituationSubjectKind::War => "war",
    }
}

fn parse_kind(text: &str) -> Option<SituationSubjectKind> {
    Some(match text {
        "scenario" => SituationSubjectKind::Scenario,
        "body" => SituationSubjectKind::Body,
        "province" => SituationSubjectKind::Province,
        "character" => SituationSubjectKind::Character,
        "organisation" => SituationSubjectKind::Organisation,
        "title" => SituationSubjectKind::Title,
        "office" => SituationSubjectKind::Office,
        "army" => SituationSubjectKind::Army,
        "ship" => SituationSubjectKind::Ship,
        "assignment" => SituationSubjectKind::Assignment,
        "obligation" => SituationSubjectKind::Obligation,
        "war" => SituationSubjectKind::War,
        _ => return None,
    })
}

fn source_map(source: &SituationSource) -> Map {
    let mut map = Map::new();
    map.insert("kind".into(), kind_name(source.kind).into());
    map.insert("key".into(), source.key.as_str().into());
    map.insert(
        "id".into(),
        source
            .id
            .map(|id| Dynamic::from(i64::try_from(id).expect("IDs fit Rhai integers")))
            .unwrap_or(Dynamic::UNIT),
    );
    map
}

fn bindings_map(key: &SituationInstanceKey) -> Map {
    let mut bindings = Map::new();
    bindings.insert("source".into(), source_map(&key.source).into());
    for (name, subject) in &key.bindings {
        if let Some(raw) = subject.raw() {
            bindings.insert(
                name.as_str().into(),
                i64::try_from(raw).expect("IDs fit Rhai integers").into(),
            );
        }
    }
    bindings
}

fn call_context_with_world(
    key: &SituationInstanceKey,
    activated: Option<GameDate>,
    world_view: &Map,
) -> Map {
    let mut context = Map::new();
    context.insert("source".into(), source_map(&key.source).into());
    context.insert("bindings".into(), bindings_map(key).into());
    // Projection and outcome calls receive the exact instance's activation
    // date. A trigger receives the oldest live activation for its
    // definition and source — exact for single-instance Situations — and
    // the unit value when nothing is currently active.
    context.insert(
        "activated".into(),
        activated
            .map(|date| Dynamic::from(date.days_since_epoch()))
            .unwrap_or(Dynamic::UNIT),
    );
    context.insert("world".into(), world_view.clone().into());
    context
}

fn attachments(world: &World, content: &ContentSet) -> Vec<Attachment> {
    let mut found = BTreeSet::new();
    if let Some(scenario) = &content.scenario {
        for definition in &scenario.situations {
            found.insert(Attachment {
                definition: definition.clone(),
                source: SituationSource {
                    kind: SituationSubjectKind::Scenario,
                    key: scenario.key.clone(),
                    id: None,
                },
            });
        }
    }
    if let Some(index) = world.get_resource::<crate::politics::PoliticsIndex>() {
        for (key, def) in &content.titles {
            let Some(id) = index.title_keys.get(key) else {
                continue;
            };
            for definition in &def.situations {
                found.insert(Attachment {
                    definition: definition.clone(),
                    source: SituationSource {
                        kind: SituationSubjectKind::Title,
                        key: key.clone(),
                        id: Some(id.raw()),
                    },
                });
            }
        }
    }
    if let Some(ledger) = world.get_resource::<crate::obligations::Obligations>() {
        for entry in &ledger.entries {
            let Some(source) = &entry.source else {
                continue;
            };
            let Some(def) = content.obligations.get(source) else {
                continue;
            };
            for definition in &def.situations {
                found.insert(Attachment {
                    definition: definition.clone(),
                    source: SituationSource {
                        kind: SituationSubjectKind::Obligation,
                        key: source.clone(),
                        id: Some(entry.id),
                    },
                });
            }
        }
    }
    found.into_iter().collect()
}

fn raw_id(value: &Dynamic, field: &str) -> Result<u64, SituationError> {
    let raw = value
        .as_int()
        .map_err(|_| SituationError::BadShape(format!("{field} must be an integer ID")))?;
    u64::try_from(raw)
        .ok()
        .filter(|raw| *raw > 0)
        .ok_or_else(|| SituationError::BadSubject(format!("{field} must be a positive ID")))
}

fn subject_from_raw(
    world: &World,
    kind: SituationSubjectKind,
    raw: u64,
) -> Result<SituationSubject, SituationError> {
    let invalid = || SituationError::BadSubject(format!("unknown {} ID {raw}", kind_name(kind)));
    match kind {
        SituationSubjectKind::Scenario => Err(SituationError::BadSubject(
            "scenario cannot be an additional numeric binding".to_owned(),
        )),
        SituationSubjectKind::Body => BodyId::from_raw(raw)
            .filter(|id| crate::access::body_entity(world, *id).is_some())
            .map(SituationSubject::Body)
            .ok_or_else(invalid),
        SituationSubjectKind::Province => ProvinceId::from_raw(raw)
            .filter(|id| crate::access::province_entity(world, *id).is_some())
            .map(SituationSubject::Province)
            .ok_or_else(invalid),
        SituationSubjectKind::Character => CharacterId::from_raw(raw)
            .filter(|id| crate::access::character_entity(world, *id).is_some())
            .map(SituationSubject::Character)
            .ok_or_else(invalid),
        SituationSubjectKind::Organisation => OrgId::from_raw(raw)
            .filter(|id| crate::access::org_entity(world, *id).is_some())
            .map(SituationSubject::Organisation)
            .ok_or_else(invalid),
        SituationSubjectKind::Title => TitleId::from_raw(raw)
            .filter(|id| crate::access::title_entity(world, *id).is_some())
            .map(SituationSubject::Title)
            .ok_or_else(invalid),
        SituationSubjectKind::Office => OfficeId::from_raw(raw)
            .filter(|id| crate::access::office_entity(world, *id).is_some())
            .map(SituationSubject::Office)
            .ok_or_else(invalid),
        SituationSubjectKind::Army => ArmyId::from_raw(raw)
            .filter(|id| crate::access::army_entity(world, *id).is_some())
            .map(SituationSubject::Army)
            .ok_or_else(invalid),
        SituationSubjectKind::Ship => ShipId::from_raw(raw)
            .filter(|id| crate::access::ship_entity(world, *id).is_some())
            .map(SituationSubject::Ship)
            .ok_or_else(invalid),
        SituationSubjectKind::Assignment => AssignmentId::from_raw(raw)
            .filter(|id| crate::access::assignment_entity(world, *id).is_some())
            .map(SituationSubject::Assignment)
            .ok_or_else(invalid),
        SituationSubjectKind::Obligation => world
            .get_resource::<crate::obligations::Obligations>()
            .is_some_and(|ledger| ledger.entries.iter().any(|entry| entry.id == raw))
            .then_some(SituationSubject::Obligation(raw))
            .ok_or_else(invalid),
        // The war module validates this occurrence when it projects/actions;
        // accepting a positive ID here keeps this generic binding seam free of
        // storage knowledge.
        SituationSubjectKind::War => WarId::from_raw(raw)
            .filter(|id| crate::wars::war(world, *id).is_some())
            .map(SituationSubject::War)
            .ok_or_else(invalid),
    }
}

fn parse_trigger(
    world: &World,
    def: &SituationDef,
    source: &SituationSource,
    dynamic: Dynamic,
) -> Result<Vec<SituationInstanceKey>, SituationError> {
    let returned = dynamic.try_cast::<Array>().ok_or_else(|| {
        SituationError::BadShape("trigger must return an array of binding maps".to_owned())
    })?;
    let declared: BTreeSet<&str> = def.bindings.keys().map(String::as_str).collect();
    let mut keys = BTreeSet::new();
    for (index, dynamic) in returned.into_iter().enumerate() {
        let map = dynamic.try_cast::<Map>().ok_or_else(|| {
            SituationError::BadShape(format!("trigger item {index} must be a map"))
        })?;
        let returned_names: BTreeSet<&str> = map.keys().map(AsRef::as_ref).collect();
        if returned_names != declared {
            return Err(SituationError::BadShape(format!(
                "trigger item {index} bindings differ from the declared schema"
            )));
        }
        let mut bindings = BTreeMap::new();
        for (name, kind) in &def.bindings {
            let value = &map[name.as_str()];
            let raw = raw_id(value, name)?;
            bindings.insert(name.clone(), subject_from_raw(world, *kind, raw)?);
        }
        keys.insert(SituationInstanceKey {
            definition: def.key.clone(),
            source: source.clone(),
            bindings,
        });
    }
    Ok(keys.into_iter().collect())
}

fn string_field(map: &Map, field: &str) -> Result<String, SituationError> {
    map.get(field)
        .and_then(|value| value.clone().into_string().ok())
        .ok_or_else(|| SituationError::BadShape(format!("{field} must be a string")))
}

fn optional_integer(map: &Map, field: &str) -> Result<Option<i64>, SituationError> {
    match map.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_int()
            .map(Some)
            .map_err(|_| SituationError::BadShape(format!("{field} must be an integer"))),
    }
}

fn optional_bool(map: &Map, field: &str) -> Result<bool, SituationError> {
    match map.get(field) {
        None => Ok(false),
        Some(value) => value
            .as_bool()
            .map_err(|_| SituationError::BadShape(format!("{field} must be a bool"))),
    }
}

fn optional_array(map: &Map, field: &str) -> Result<Array, SituationError> {
    match map.get(field) {
        None => Ok(Array::new()),
        Some(value) => value
            .clone()
            .try_cast::<Array>()
            .ok_or_else(|| SituationError::BadShape(format!("{field} must be an array"))),
    }
}

fn item_map(dynamic: Dynamic, block: &str, index: usize) -> Result<Map, SituationError> {
    dynamic
        .try_cast::<Map>()
        .ok_or_else(|| SituationError::BadShape(format!("{block} item {index} must be a map")))
}

fn reject_unknown(map: &Map, allowed: &[&str], context: &str) -> Result<(), SituationError> {
    if let Some(key) = map
        .keys()
        .map(AsRef::as_ref)
        .find(|key| !allowed.contains(key))
    {
        Err(SituationError::BadShape(format!(
            "unknown {context} field '{key}'"
        )))
    } else {
        Ok(())
    }
}

fn parse_links(map: &Map, field: &str) -> Result<Vec<SituationLink>, SituationError> {
    optional_array(map, field)?
        .into_iter()
        .enumerate()
        .map(|(index, dynamic)| {
            let item = item_map(dynamic, field, index)?;
            reject_unknown(&item, &["kind", "id", "label_key"], field)?;
            let kind_text = string_field(&item, "kind")?;
            let kind = parse_kind(&kind_text).ok_or_else(|| {
                SituationError::BadSubject(format!("unknown link kind '{kind_text}'"))
            })?;
            if kind == SituationSubjectKind::Scenario {
                return Err(SituationError::BadSubject(
                    "scenario links require no numeric ID and are unsupported".to_owned(),
                ));
            }
            let id = raw_id(
                item.get("id").ok_or_else(|| {
                    SituationError::BadShape(format!("{field} item {index} needs id"))
                })?,
                "id",
            )?;
            let label_key = item
                .get("label_key")
                .map(|value| {
                    value.clone().into_string().map_err(|_| {
                        SituationError::BadShape("label_key must be a string".to_owned())
                    })
                })
                .transpose()?;
            Ok(SituationLink {
                kind,
                id,
                label_key,
            })
        })
        .collect()
}

fn parse_participant_groups(map: &Map) -> Result<Vec<SituationParticipantGroup>, SituationError> {
    optional_array(map, "participant_groups")?
        .into_iter()
        .enumerate()
        .map(|(index, dynamic)| {
            let item = item_map(dynamic, "participant_groups", index)?;
            reject_unknown(&item, &["label_key", "participants"], "participant group")?;
            Ok(SituationParticipantGroup {
                label_key: string_field(&item, "label_key")?,
                participants: parse_links(&item, "participants")?,
            })
        })
        .collect()
}

fn parse_target(map: &Map) -> Result<AssignmentTarget, SituationError> {
    let Some(kind) = map.get("target_kind") else {
        return Ok(AssignmentTarget::None);
    };
    let kind = kind
        .clone()
        .into_string()
        .map_err(|_| SituationError::BadShape("target_kind must be a string".to_owned()))?;
    let a = || {
        map.get("target_a")
            .ok_or_else(|| SituationError::BadShape(format!("{kind} target needs target_a")))
            .and_then(|value| raw_id(value, "target_a"))
    };
    let b = || {
        map.get("target_b")
            .ok_or_else(|| SituationError::BadShape(format!("{kind} target needs target_b")))
            .and_then(|value| raw_id(value, "target_b"))
    };
    match kind.as_str() {
        "none" => Ok(AssignmentTarget::None),
        "character" => CharacterId::from_raw(a()?)
            .map(AssignmentTarget::Character)
            .ok_or_else(|| SituationError::BadSubject("invalid character target".to_owned())),
        "organisation" => OrgId::from_raw(a()?)
            .map(AssignmentTarget::Org)
            .ok_or_else(|| SituationError::BadSubject("invalid organisation target".to_owned())),
        "province" => ProvinceId::from_raw(a()?)
            .map(AssignmentTarget::Province)
            .ok_or_else(|| SituationError::BadSubject("invalid province target".to_owned())),
        "own-army" => ArmyId::from_raw(a()?)
            .map(AssignmentTarget::OwnArmy)
            .ok_or_else(|| SituationError::BadSubject("invalid army target".to_owned())),
        "army-to-province" => match (ArmyId::from_raw(a()?), ProvinceId::from_raw(b()?)) {
            (Some(army), Some(province)) => Ok(AssignmentTarget::ArmyToProvince(army, province)),
            _ => Err(SituationError::BadSubject(
                "invalid army-to-province target".to_owned(),
            )),
        },
        "ship-to-province" => match (ShipId::from_raw(a()?), ProvinceId::from_raw(b()?)) {
            (Some(ship), Some(province)) => Ok(AssignmentTarget::ShipToProvince(ship, province)),
            _ => Err(SituationError::BadSubject(
                "invalid ship-to-province target".to_owned(),
            )),
        },
        "war" => WarId::from_raw(a()?)
            .map(AssignmentTarget::War)
            .ok_or_else(|| SituationError::BadSubject("invalid war target".to_owned())),
        "war-side" => {
            let war = WarId::from_raw(a()?)
                .ok_or_else(|| SituationError::BadSubject("invalid war target".to_owned()))?;
            let side = map
                .get("target_side")
                .ok_or_else(|| {
                    SituationError::BadShape("war-side target needs target_side".to_owned())
                })?
                .clone()
                .into_string()
                .map_err(|_| SituationError::BadShape("target_side must be a string".to_owned()))?;
            let side = match side.as_str() {
                "attacker" => crate::wars::WarSideId::Attacker,
                "defender" => crate::wars::WarSideId::Defender,
                _ => {
                    return Err(SituationError::BadSubject(format!(
                        "unsupported war side '{side}'"
                    )));
                }
            };
            Ok(AssignmentTarget::WarSide(war, side))
        }
        other => Err(SituationError::BadSubject(format!(
            "unsupported assignment target kind '{other}'"
        ))),
    }
}

fn parse_projection(
    def: &SituationDef,
    dynamic: Dynamic,
) -> Result<SituationProjection, SituationError> {
    let map = dynamic
        .try_cast::<Map>()
        .ok_or_else(|| SituationError::BadShape("projection must return a map".to_owned()))?;
    reject_unknown(
        &map,
        &[
            "stage",
            "deadline",
            "warning",
            "metrics",
            "participants",
            "participant_groups",
            "links",
            "actions",
            "resolution_values",
        ],
        "projection",
    )?;
    let stage_text = string_field(&map, "stage")?;
    let stage = ContentKey::new(&stage_text)
        .map_err(|_| SituationError::BadShape(format!("invalid stage key '{stage_text}'")))?;
    let stage_def = def
        .stages
        .iter()
        .find(|candidate| candidate.key == stage)
        .ok_or_else(|| SituationError::Undeclared(format!("undeclared stage '{stage}'")))?;
    let warning = optional_bool(&map, "warning")?;
    if warning && stage_def.warning.is_none() {
        return Err(SituationError::Undeclared(format!(
            "stage '{stage}' has no authored warning"
        )));
    }

    let metrics = optional_array(&map, "metrics")?
        .into_iter()
        .enumerate()
        .map(|(index, dynamic)| {
            let item = item_map(dynamic, "metrics", index)?;
            reject_unknown(&item, &["label_key", "value"], "metric")?;
            let label_key = string_field(&item, "label_key")?;
            let value = item.get("value").ok_or_else(|| {
                SituationError::BadShape(format!("metric item {index} needs value"))
            })?;
            let value = if let Ok(integer) = value.as_int() {
                SituationMetricValue::Integer(integer)
            } else if let Ok(text) = value.clone().into_string() {
                SituationMetricValue::Text(text)
            } else {
                return Err(SituationError::BadShape(
                    "metric value must be an integer or string".to_owned(),
                ));
            };
            Ok(SituationMetric { label_key, value })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let actions = optional_array(&map, "actions")?
        .into_iter()
        .enumerate()
        .map(|(index, dynamic)| {
            let item = item_map(dynamic, "actions", index)?;
            reject_unknown(
                &item,
                &[
                    "id",
                    "leader",
                    "target_kind",
                    "target_a",
                    "target_b",
                    "target_side",
                    "context",
                ],
                "action",
            )?;
            let id_text = string_field(&item, "id")?;
            let id = ContentKey::new(&id_text)
                .map_err(|_| SituationError::BadShape(format!("invalid action key '{id_text}'")))?;
            if !def.actions.iter().any(|action| action.key == id) {
                return Err(SituationError::Undeclared(format!(
                    "undeclared action '{id}'"
                )));
            }
            let leader = item
                .get("leader")
                .map(|value| raw_id(value, "leader"))
                .transpose()?
                .map(|raw| {
                    CharacterId::from_raw(raw).ok_or_else(|| {
                        SituationError::BadSubject("invalid action leader".to_owned())
                    })
                })
                .transpose()?;
            Ok(SituationAction {
                id,
                leader,
                target: parse_target(&item)?,
                context: parse_links(&item, "context")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let resolution_values = match map.get("resolution_values") {
        None => BTreeMap::new(),
        Some(value) => value
            .clone()
            .try_cast::<Map>()
            .ok_or_else(|| SituationError::BadShape("resolution_values must be a map".to_owned()))?
            .into_iter()
            .map(|(key, value)| {
                value
                    .into_string()
                    .map(|value| (key.to_string(), value))
                    .map_err(|_| {
                        SituationError::BadShape(
                            "resolution_values values must be strings".to_owned(),
                        )
                    })
            })
            .collect::<Result<_, _>>()?,
    };

    Ok(SituationProjection {
        stage,
        deadline: optional_integer(&map, "deadline")?.map(GameDate::from_days),
        warning,
        metrics,
        participants: parse_links(&map, "participants")?,
        participant_groups: parse_participant_groups(&map)?,
        links: parse_links(&map, "links")?,
        actions,
        resolution_values,
    })
}

fn call_dynamic(
    world: &World,
    content: &ContentSet,
    function: &aeon_data::model::ScriptFnRef,
    context: Map,
) -> Result<Dynamic, SituationError> {
    world
        .resource::<ScriptRuntime>()
        .0
        .call_dynamic_fn(content, function, context)
        .map_err(|error| SituationError::Script(error.to_string()))
}

fn project(
    world: &World,
    content: &ContentSet,
    key: &SituationInstanceKey,
    activated: GameDate,
) -> Result<SituationProjection, SituationError> {
    let world_view = crate::script_world::context_value(world);
    project_with_world(world, content, key, activated, &world_view)
}

fn project_with_world(
    world: &World,
    content: &ContentSet,
    key: &SituationInstanceKey,
    activated: GameDate,
    world_view: &Map,
) -> Result<SituationProjection, SituationError> {
    let def = content.situations.get(&key.definition).ok_or_else(|| {
        SituationError::Undeclared(format!(
            "missing Situation '{}'; content changed",
            key.definition
        ))
    })?;
    let dynamic = call_dynamic(
        world,
        content,
        &def.projection_fn,
        call_context_with_world(key, Some(activated), world_view),
    )?;
    parse_projection(def, dynamic)
}

fn render_resolution(template: &str, values: &BTreeMap<String, String>) -> String {
    let mut rendered = template.to_owned();
    for (key, value) in values {
        rendered = rendered.replace(&format!("{{{key}}}"), value);
    }
    rendered
}

fn resolve_with_world(
    world: &World,
    content: &ContentSet,
    active: &ActiveSituation,
    id: u64,
    date: GameDate,
    world_view: &Map,
) -> Result<SituationResolution, SituationError> {
    let def = content
        .situations
        .get(&active.key.definition)
        .ok_or_else(|| SituationError::Undeclared("missing Situation definition".to_owned()))?;
    let context = call_context_with_world(&active.key, Some(active.activated), world_view);
    let mut selected = None;
    for outcome in &def.outcomes {
        let Some(predicate) = &outcome.predicate_fn else {
            selected = Some(outcome);
            break;
        };
        let dynamic = call_dynamic(world, content, predicate, context.clone())?;
        let matches = dynamic.as_bool().map_err(|_| {
            SituationError::BadShape(format!(
                "outcome '{}' predicate must return a bool",
                outcome.key
            ))
        })?;
        if matches {
            selected = Some(outcome);
            break;
        }
    }
    let outcome = selected.ok_or_else(|| {
        SituationError::Undeclared("Situation has no fallback outcome".to_owned())
    })?;
    let projection = project_with_world(world, content, &active.key, active.activated, world_view)?;
    Ok(SituationResolution {
        id,
        situation: active.key.clone(),
        activated: active.activated,
        resolved: date,
        outcome: outcome.key.clone(),
        text: render_resolution(&outcome.text, &projection.resolution_values),
        participants: projection.participants,
        participant_groups: projection.participant_groups,
        links: projection.links,
    })
}

fn fingerprint(occurrence: &SituationOccurrence, phase: &str, error: &str) -> String {
    let key = &occurrence.situation;
    let mut stable = format!(
        "{}|{}|{}:{}:{}",
        occurrence.activated.days_since_epoch(),
        key.definition,
        kind_name(key.source.kind),
        key.source.key,
        key.source
            .id
            .map_or_else(|| "-".to_owned(), |id| id.to_string())
    );
    for (name, subject) in &key.bindings {
        stable.push('|');
        stable.push_str(name);
        stable.push(':');
        stable.push_str(kind_name(subject.kind()));
        stable.push(':');
        match subject {
            SituationSubject::Scenario(scenario) => stable.push_str(scenario.as_str()),
            _ => stable.push_str(&subject.raw().expect("non-scenario subject").to_string()),
        }
    }
    stable.push('|');
    stable.push_str(phase);
    stable.push('|');
    stable.push_str(error);
    stable
}

fn log_diagnostic(world: &mut World, occurrence: &SituationOccurrence, error: &str) {
    let key = &occurrence.situation;
    let situation = world
        .get_resource::<ContentDb>()
        .and_then(|content| content.0.situations.get(&key.definition))
        .map(|definition| definition.title.as_str())
        .unwrap_or(key.definition.as_str())
        .to_owned();
    let text = world.resource::<crate::text::TextDb>().format(
        "sim.situation.unavailable",
        &[("situation", &situation), ("error", error)],
    );
    crate::access::log(
        world,
        situation_log_entry(world, occurrence, LogEntry::line(text, LogChannel::Events)),
    );
}

fn situation_log_entry(
    world: &World,
    occurrence: &SituationOccurrence,
    entry: LogEntry,
) -> LogEntry {
    let mut entry = entry
        .for_situation(occurrence.clone())
        .for_audience(log_audience(world, &occurrence.situation));
    if let Some(war) = situation_war(&occurrence.situation) {
        entry = entry.for_war(war);
    }
    entry
}

/// Evaluates every attachment against the fully settled authoritative day.
pub fn evaluate(world: &mut World) {
    let Some(content) = world.get_resource::<ContentDb>().map(|db| db.0.clone()) else {
        return;
    };
    if world.get_resource::<ScriptRuntime>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let world_view = crate::script_world::context_value(world);
    let old = world
        .get_resource::<SituationState>()
        .cloned()
        .unwrap_or_default();

    let mut discovered = BTreeSet::new();
    let mut failed_sources: BTreeMap<(ContentKey, SituationSource), String> = BTreeMap::new();
    for attachment in attachments(world, &content) {
        let Some(def) = content.situations.get(&attachment.definition) else {
            continue;
        };
        let synthetic = SituationInstanceKey {
            definition: def.key.clone(),
            source: attachment.source.clone(),
            bindings: BTreeMap::new(),
        };
        // The oldest live activation for this definition and source; exact
        // for single-instance Situations, and the unit value when no
        // lifecycle is currently active.
        let earliest_activation = old
            .active
            .values()
            .filter(|lifecycle| {
                lifecycle.key.definition == def.key && lifecycle.key.source == attachment.source
            })
            .map(|lifecycle| lifecycle.activated)
            .min();
        let result = call_dynamic(
            world,
            &content,
            &def.trigger_fn,
            call_context_with_world(&synthetic, earliest_activation, &world_view),
        )
        .and_then(|dynamic| parse_trigger(world, def, &attachment.source, dynamic));
        match result {
            Ok(keys) => discovered.extend(keys),
            Err(error) => {
                failed_sources.insert(
                    (def.key.clone(), attachment.source.clone()),
                    error.to_string(),
                );
            }
        }
    }

    let mut active = BTreeMap::new();
    let mut errors = BTreeMap::new();
    let mut activations = Vec::new();
    for key in discovered {
        let lifecycle = old.active.get(&key).cloned().unwrap_or_else(|| {
            let lifecycle = ActiveSituation {
                key: key.clone(),
                activated: date,
            };
            activations.push(lifecycle.occurrence());
            lifecycle
        });
        active.insert(key, lifecycle);
    }

    for ((definition, source), error) in &failed_sources {
        let mut preserved = false;
        for (key, lifecycle) in &old.active {
            if &key.definition == definition && &key.source == source {
                active.insert(key.clone(), lifecycle.clone());
                errors.insert(key.clone(), error.clone());
                preserved = true;
            }
        }
        if !preserved {
            let key = SituationInstanceKey {
                definition: definition.clone(),
                source: source.clone(),
                bindings: BTreeMap::new(),
            };
            active.insert(
                key.clone(),
                ActiveSituation {
                    key: key.clone(),
                    activated: date,
                },
            );
            errors.insert(key, error.clone());
        }
    }

    let mut resolutions = old.resolutions.clone();
    let mut next_resolution_id = old.next_resolution_id;
    let ended: Vec<_> = old
        .active
        .iter()
        .filter(|(key, _)| {
            !active.contains_key(*key)
                && !failed_sources.contains_key(&(key.definition.clone(), key.source.clone()))
        })
        .map(|(_, active)| active.clone())
        .collect();
    let mut resolution_logs = Vec::new();
    let mut resolution_effects = Vec::new();
    for ended in ended {
        next_resolution_id += 1;
        match resolve_with_world(
            world,
            &content,
            &ended,
            next_resolution_id,
            date,
            &world_view,
        ) {
            Ok(notice) => {
                resolution_logs.push((notice.occurrence(), notice.text.clone()));
                // A selected outcome may carry authored effects. They apply
                // once, after the new lifecycle state is installed, against
                // the organisation named by the declared owner binding.
                if let Some(effects_fn) = content
                    .situations
                    .get(&notice.situation.definition)
                    .and_then(|def| {
                        def.outcomes
                            .iter()
                            .find(|outcome| outcome.key == notice.outcome)
                    })
                    .and_then(|outcome| outcome.effects_fn.clone())
                {
                    resolution_effects.push((
                        notice.occurrence(),
                        effects_fn,
                        call_context_with_world(&ended.key, Some(ended.activated), &world_view),
                    ));
                }
                resolutions.push(notice);
            }
            Err(error) => {
                next_resolution_id -= 1;
                active.insert(ended.key.clone(), ended.clone());
                errors.insert(ended.key, error.to_string());
            }
        }
    }

    // Parse every live projection now so runtime errors are deterministic,
    // log-once, and snapshotted even if no client happens to open the panel.
    for (key, lifecycle) in &active {
        if errors.contains_key(key) {
            continue;
        }
        if let Err(error) = project_with_world(
            world,
            &content,
            &lifecycle.key,
            lifecycle.activated,
            &world_view,
        ) {
            errors.insert(key.clone(), error.to_string());
        }
    }

    let mut logged_diagnostics = old.logged_diagnostics.clone();
    let new_diagnostics: Vec<_> = errors
        .iter()
        .filter_map(|(key, error)| {
            let occurrence = active.get(key)?.occurrence();
            let mark = fingerprint(&occurrence, "runtime", error);
            logged_diagnostics
                .insert(mark)
                .then_some((occurrence, error.clone()))
        })
        .collect();

    world.insert_resource(SituationState {
        active,
        resolutions,
        next_resolution_id,
        runtime_errors: errors,
        logged_diagnostics,
    });

    for occurrence in activations {
        let Some(def) = content.situations.get(&occurrence.situation.definition) else {
            continue;
        };
        if def.log_activation {
            let text = world
                .resource::<crate::text::TextDb>()
                .format("sim.situation.activated", &[("situation", &def.title)]);
            crate::access::log(
                world,
                situation_log_entry(world, &occurrence, LogEntry::line(text, LogChannel::Events)),
            );
        }
        // An authored announcement gives the activation a popup-bearing
        // path: the popup enters ordinary authoritative popup state, so the
        // existing popup auto-pause covers it without touching the
        // deliberately non-pausing warning attention model. Both the check
        // and the content are pure functions of authoritative state.
        if let Some(announcement) = &def.announcement
            && visible_to_player(world, &occurrence.situation)
        {
            let owner = outcome_owner(def, &occurrence.situation);
            let roles = crate::assignments::AssignmentRoles::resolve(
                world,
                crate::assignments::RoleSeed {
                    owner,
                    ..Default::default()
                },
            );
            let acknowledge = world
                .resource::<crate::text::TextDb>()
                .text("sim.situation.acknowledge")
                .to_owned();
            if let Some(mut popups) = world.get_resource_mut::<crate::assignments::PendingPopups>()
            {
                let id = popups.next_id;
                popups.next_id += 1;
                popups.popups.push(crate::assignments::PendingPopup {
                    id,
                    date,
                    assignment: occurrence.situation.definition.clone(),
                    result: aeon_data::model::OutcomeKind::Success,
                    text: announcement.clone(),
                    choices: vec![(
                        ContentKey::new("acknowledged").expect("static key"),
                        acknowledge,
                    )],
                    roles,
                    origin_situation: Some(occurrence.clone()),
                });
            }
        }
    }
    for (occurrence, text) in resolution_logs {
        crate::access::log(
            world,
            situation_log_entry(world, &occurrence, LogEntry::line(text, LogChannel::Events)),
        );
    }
    // Outcome effects run last, after lifecycle state and resolution history
    // are in place, in deterministic resolution order.
    for (occurrence, effects_fn, context) in resolution_effects {
        let Some(def) = content.situations.get(&occurrence.situation.definition) else {
            continue;
        };
        let owner = outcome_owner(def, &occurrence.situation);
        let effects = {
            let runtime = world.resource::<ScriptRuntime>();
            runtime.0.call_effect_fn(&content, &effects_fn, context)
        };
        match effects {
            Ok(effects) => {
                let roles = crate::assignments::AssignmentRoles::resolve(
                    world,
                    crate::assignments::RoleSeed {
                        owner,
                        ..Default::default()
                    },
                );
                crate::assignments::apply_effects_with_origin(
                    world,
                    &effects,
                    &roles,
                    owner,
                    Some(&occurrence),
                );
            }
            Err(error) => log_diagnostic(world, &occurrence, &error.to_string()),
        }
    }
    for (occurrence, error) in new_diagnostics {
        log_diagnostic(world, &occurrence, &error);
    }
}

/// The organisation bound by a definition's declared owner binding, if any.
fn outcome_owner(def: &SituationDef, key: &SituationInstanceKey) -> Option<crate::ids::OrgId> {
    let binding = def.owner_binding.as_ref()?;
    match key.bindings.get(binding) {
        Some(SituationSubject::Organisation(org)) => Some(*org),
        _ => None,
    }
}

/// Derives every active card in deterministic key order.
pub fn active_cards(world: &World) -> Vec<SituationCard> {
    let Some(content) = world.get_resource::<ContentDb>() else {
        return Vec::new();
    };
    let Some(state) = world.get_resource::<SituationState>() else {
        return Vec::new();
    };
    let mut cards: Vec<_> = state
        .active
        .values()
        .filter_map(|active| {
            let def = content.0.situations.get(&active.key.definition)?;
            let mut unavailable = state.runtime_errors.get(&active.key).cloned();
            let projection = if unavailable.is_none() {
                match project(world, &content.0, &active.key, active.activated) {
                    Ok(projection) => Some(projection),
                    Err(error) => {
                        unavailable = Some(error.to_string());
                        None
                    }
                }
            } else {
                None
            };
            Some(SituationCard {
                active: active.clone(),
                title: def.title.clone(),
                summary: def.summary.clone(),
                priority: def.priority,
                projection,
                unavailable,
            })
        })
        .collect();
    cards.sort_by(|left, right| {
        let left_warning = left
            .projection
            .as_ref()
            .is_some_and(|projection| projection.warning);
        let right_warning = right
            .projection
            .as_ref()
            .is_some_and(|projection| projection.warning);
        right_warning
            .cmp(&left_warning)
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| left.active.key.cmp(&right.active.key))
    });
    cards
}

/// Evaluates the opening/attached deck and returns every deterministic runtime
/// issue. Content validation uses this on a disposable campaign world.
pub fn validate_opening(world: &mut World) -> Vec<SituationRuntimeIssue> {
    evaluate(world);
    world
        .get_resource::<SituationState>()
        .map(|state| {
            state
                .runtime_errors
                .iter()
                .map(|(situation, error)| SituationRuntimeIssue {
                    situation: situation.clone(),
                    error: error.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the current player may see an instance; spectators see all.
pub fn visible_to_player(world: &World, key: &SituationInstanceKey) -> bool {
    let Some(player) = world
        .get_resource::<crate::politics::PlayerHouse>()
        .map(|player| player.0)
    else {
        return false;
    };
    let Some(player) = player else {
        return true;
    };
    log_audience(world, key).visible_to(Some(player))
}

/// Captures the concrete organisation audience authored for one Situation.
///
/// The returned IDs are suitable for persisting on log entries, so a private
/// line does not become public merely because its Situation card has ended.
pub fn log_audience(world: &World, key: &SituationInstanceKey) -> LogAudience {
    let Some(def) = world
        .get_resource::<ContentDb>()
        .and_then(|content| content.0.situations.get(&key.definition))
    else {
        return LogAudience::organisations([]);
    };
    match &def.visibility {
        SituationVisibilityDef::Public => LogAudience::Public,
        SituationVisibilityDef::Bound(names) => {
            LogAudience::organisations(names.iter().filter_map(|name| {
                let subject = if name == "source" {
                    key.source.subject().ok()
                } else {
                    key.bindings.get(name).cloned()
                };
                match subject {
                    Some(SituationSubject::Organisation(org)) => Some(org),
                    Some(SituationSubject::Character(character)) => {
                        crate::access::organisation_of(world, character)
                    }
                    _ => None,
                }
            }))
        }
    }
}

/// Exact formal war structurally bound to a Situation, when any.
pub fn situation_war(key: &SituationInstanceKey) -> Option<WarId> {
    key.bindings
        .get("war")
        .and_then(|subject| match subject {
            SituationSubject::War(war) => Some(*war),
            _ => None,
        })
        .or_else(|| match key.source.subject().ok() {
            Some(SituationSubject::War(war)) => Some(war),
            _ => None,
        })
}

/// Exact formal war carried by an action's target or structural `war` binding.
pub fn action_war(situation: &SituationInstanceKey, target: AssignmentTarget) -> Option<WarId> {
    match target {
        AssignmentTarget::War(war) | AssignmentTarget::WarSide(war, _) => Some(war),
        _ => situation_war(situation),
    }
}

/// Returns the assignment definition behind one currently projected action.
pub fn assignment_for_action(
    world: &World,
    situation: &SituationInstanceKey,
    action: &ContentKey,
    leader: CharacterId,
    target: AssignmentTarget,
) -> Result<ContentKey, SituationError> {
    if !visible_to_player(world, situation) {
        return Err(SituationError::Undeclared(
            "Situation is not visible to the player".to_owned(),
        ));
    }
    let content = world
        .get_resource::<ContentDb>()
        .ok_or_else(|| SituationError::Undeclared("no content database".to_owned()))?;
    let state = world
        .get_resource::<SituationState>()
        .ok_or_else(|| SituationError::Undeclared("no Situation state".to_owned()))?;
    let Some(lifecycle) = state.active.get(situation) else {
        return Err(SituationError::Undeclared(
            "Situation is not currently available".to_owned(),
        ));
    };
    if state.runtime_errors.contains_key(situation) {
        return Err(SituationError::Undeclared(
            "Situation is not currently available".to_owned(),
        ));
    }
    let def = content
        .0
        .situations
        .get(&situation.definition)
        .ok_or_else(|| SituationError::Undeclared("missing Situation definition".to_owned()))?;
    let projection = project(world, &content.0, situation, lifecycle.activated)?;
    projection
        .actions
        .iter()
        .find(|candidate| {
            &candidate.id == action
                && candidate.target == target
                && candidate.leader.is_none_or(|fixed| fixed == leader)
        })
        .ok_or_else(|| {
            SituationError::BadSubject(
                "action subjects differ from the current projection".to_owned(),
            )
        })?;
    def.actions
        .iter()
        .find(|candidate| &candidate.key == action)
        .map(|candidate| candidate.assignment.clone())
        .ok_or_else(|| SituationError::Undeclared("missing authored action".to_owned()))
}

/// Builds the ordinary authoritative assignment forecast for a projected
/// Situation action, retaining any exact formal-war binding.
pub fn forecast_for_action(
    world: &World,
    situation: &SituationInstanceKey,
    action: &ContentKey,
    leader: CharacterId,
    target: AssignmentTarget,
) -> Result<crate::forecast::AssignmentForecast, SituationError> {
    let owner = world
        .get_resource::<crate::politics::PlayerHouse>()
        .and_then(|player| player.0)
        .ok_or_else(|| SituationError::Undeclared("no player organisation".to_owned()))?;
    let assignment = assignment_for_action(world, situation, action, leader, target)?;
    crate::forecast::forecast_in_war(
        world,
        owner,
        &assignment,
        leader,
        target,
        action_war(situation, target),
    )
    .ok_or_else(|| SituationError::Undeclared("missing assignment definition".to_owned()))
}

/// Removes an undismissed resolution notice.
pub fn dismiss_resolution(world: &mut World, id: u64) {
    if let Some(mut state) = world.get_resource_mut::<SituationState>() {
        state.resolutions.retain(|notice| notice.id != id);
    }
}

/// Captures Situation persistence for the campaign snapshot.
pub fn capture(world: &World) -> SituationState {
    world
        .get_resource::<SituationState>()
        .cloned()
        .unwrap_or_default()
}

/// Restores Situation persistence before the initial post-restore evaluation.
pub fn restore(world: &mut World, state: &SituationState) {
    world.insert_resource(state.clone());
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<SituationState>();
    app.add_systems(SettledDay, evaluate);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_fingerprint_includes_ordered_structural_bindings() {
        let definition = ContentKey::new("formal-war").unwrap();
        let source_key = ContentKey::new("ashkarr-succession").unwrap();
        let source = SituationSource {
            kind: SituationSubjectKind::Scenario,
            key: source_key,
            id: None,
        };
        let occurrence_for = |raw, activated| SituationOccurrence {
            situation: SituationInstanceKey {
                definition: definition.clone(),
                source: source.clone(),
                bindings: BTreeMap::from([(
                    "war".to_owned(),
                    SituationSubject::War(WarId::from_raw(raw).unwrap()),
                )]),
            },
            activated: GameDate::from_days(activated),
        };
        assert_ne!(
            fingerprint(&occurrence_for(41, 1), "runtime", "broken projection"),
            fingerprint(&occurrence_for(42, 1), "runtime", "broken projection")
        );
        assert_ne!(
            fingerprint(&occurrence_for(41, 1), "runtime", "broken projection"),
            fingerprint(&occurrence_for(41, 2), "runtime", "broken projection")
        );
    }
}
