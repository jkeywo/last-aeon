//! The `define_*` builders: authored maps become validated definitions.
//!
//! Each builder states its definition's schema exactly once, as the reads
//! themselves: a [`Fields`] reader tracks which fields a builder consumed
//! and warns about the rest, so the unknown-field allow-list cannot drift
//! from the fields actually read. Enum-valued fields are read against a
//! spelling table, so every "expected ..." message lists the whole
//! vocabulary.
//!
//! Builders collect findings rather than stopping: a definition that
//! fails reports why and is skipped; loading continues so one pass
//! surfaces every problem in a content set.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use rhai::{Engine, Map};

use crate::key::ContentKey;
use crate::model::{
    AiIntent, ArmyDef, AssignmentCategory, AssignmentDef, AssignmentRequires, AssignmentTargetKind,
    BodyDef, BodyKind, BuildingDef, CharacterDef, DirectiveDef, DirectiveTarget, EventChoiceDef,
    EventDef, EventFamily, EventRequires, Gender, GoalDef, GoalRequires, GoodDef, GoverningSkill,
    HolderRelation, HouseTier, MilitaryOp, NamePoolDef, ObligationDef, ObligationKind, OfficeDef,
    OrgDef, OrgKind, OutcomeDef, OutcomeKind, PlanArmySelector, PlanDef, PlanMethodDef,
    PlanRequires, PlanStepAction, PlanStepDef, PlanTargetSelector, PopupChoiceDef, ProvinceDef,
    RiskTag, ScenarioDef, ScriptFnRef, ShipClass, ShipDef, SkillsDef, StageDef, TitleDef,
    TitleHolderDef, TitleKindDef, TitleNeed, TraitDef, Urgency,
};
use crate::report::{ContentReport, Severity};

/// Shared mutable state while the loading engine runs definition passes.
#[derive(Default)]
pub(super) struct BuilderState {
    pub(super) current_path: String,
    pub(super) report: ContentReport,
    pub(super) assignments: BTreeMap<ContentKey, AssignmentDef>,
    pub(super) bodies: BTreeMap<ContentKey, BodyDef>,
    pub(super) goods: BTreeMap<ContentKey, GoodDef>,
    pub(super) buildings: BTreeMap<ContentKey, BuildingDef>,
    pub(super) provinces: BTreeMap<ContentKey, ProvinceDef>,
    pub(super) traits: BTreeMap<ContentKey, TraitDef>,
    pub(super) name_pools: BTreeMap<ContentKey, NamePoolDef>,
    pub(super) characters: BTreeMap<ContentKey, CharacterDef>,
    pub(super) organisations: BTreeMap<ContentKey, OrgDef>,
    pub(super) titles: BTreeMap<ContentKey, TitleDef>,
    pub(super) offices: BTreeMap<ContentKey, OfficeDef>,
    pub(super) ships: BTreeMap<ContentKey, ShipDef>,
    pub(super) armies: BTreeMap<ContentKey, ArmyDef>,
    pub(super) obligations: BTreeMap<ContentKey, ObligationDef>,
    pub(super) events: BTreeMap<ContentKey, EventDef>,
    pub(super) plans: BTreeMap<ContentKey, PlanDef>,
    pub(super) goals: BTreeMap<ContentKey, GoalDef>,
    pub(super) scenario: Option<ScenarioDef>,
}

impl BuilderState {
    pub(super) fn error(&mut self, key: Option<&str>, message: impl Into<String>) {
        let path = self.current_path.clone();
        self.report.error(&path, key, message);
    }

    /// Moves the collected state out, leaving an empty shell behind.
    pub(super) fn take(&mut self) -> BuilderState {
        BuilderState {
            current_path: std::mem::take(&mut self.current_path),
            report: std::mem::take(&mut self.report),
            assignments: std::mem::take(&mut self.assignments),
            bodies: std::mem::take(&mut self.bodies),
            goods: std::mem::take(&mut self.goods),
            buildings: std::mem::take(&mut self.buildings),
            provinces: std::mem::take(&mut self.provinces),
            traits: std::mem::take(&mut self.traits),
            name_pools: std::mem::take(&mut self.name_pools),
            characters: std::mem::take(&mut self.characters),
            organisations: std::mem::take(&mut self.organisations),
            titles: std::mem::take(&mut self.titles),
            offices: std::mem::take(&mut self.offices),
            ships: std::mem::take(&mut self.ships),
            armies: std::mem::take(&mut self.armies),
            obligations: std::mem::take(&mut self.obligations),
            events: std::mem::take(&mut self.events),
            plans: std::mem::take(&mut self.plans),
            goals: std::mem::take(&mut self.goals),
            scenario: self.scenario.take(),
        }
    }
}

// ---------------------------------------------------------------------------
// Field reading
// ---------------------------------------------------------------------------
//
// The free helpers below validate one field against one map. Builders use
// them through `Fields`, which additionally tracks what was read; nested
// maps (assignment results, choices, skills) use them directly with an explicit
// allow-list because they live inside a single field of the parent.

/// Reports a display-text field a nested map still carries.
///
/// The `Fields` reader has [`Fields::from_table`] for the same assignment;
/// nested maps — assignment-result choices, event choices — read their fields
/// directly and use this instead. The key such a choice fills is only
/// known to the pass that fills it, so this names the field rather than
/// the row.
fn reject_authored_text(state: &mut BuilderState, key: &str, map: &Map, field: &str) {
    if map.contains_key(field) {
        state.error(
            Some(key),
            format!(
                "'{field}' is display text and now lives in the string table; \
                 move it to assets/text/strings.csv"
            ),
        );
    }
}

/// Reads a required string field from a definition map.
fn req_str(state: &mut BuilderState, key: Option<&str>, map: &Map, field: &str) -> Option<String> {
    match map.get(field) {
        Some(value) => match value.clone().into_string() {
            Ok(text) => Some(text),
            Err(_) => {
                state.error(key, format!("field '{field}' must be a string"));
                None
            }
        },
        None => {
            state.error(key, format!("missing required field '{field}'"));
            None
        }
    }
}

/// Reads an optional string field.
fn opt_str(
    state: &mut BuilderState,
    key: Option<&str>,
    map: &Map,
    field: &str,
) -> Option<Option<String>> {
    match map.get(field) {
        None => Some(None),
        Some(value) => match value.clone().into_string() {
            Ok(text) => Some(Some(text)),
            Err(_) => {
                state.error(key, format!("field '{field}' must be a string"));
                None
            }
        },
    }
}

/// Reads a required integer field from a definition map.
fn req_int(state: &mut BuilderState, key: Option<&str>, map: &Map, field: &str) -> Option<i64> {
    match map.get(field) {
        Some(value) => match value.as_int() {
            Ok(int) => Some(int),
            Err(_) => {
                state.error(key, format!("field '{field}' must be an integer"));
                None
            }
        },
        None => {
            state.error(key, format!("missing required field '{field}'"));
            None
        }
    }
}

/// Reads an optional integer field with a default.
fn opt_int(
    state: &mut BuilderState,
    key: Option<&str>,
    map: &Map,
    field: &str,
    default: i64,
) -> Option<i64> {
    match map.get(field) {
        Some(value) => match value.as_int() {
            Ok(int) => Some(int),
            Err(_) => {
                state.error(key, format!("field '{field}' must be an integer"));
                None
            }
        },
        None => Some(default),
    }
}

/// Reads an optional boolean field with a default.
fn opt_bool(
    state: &mut BuilderState,
    key: Option<&str>,
    map: &Map,
    field: &str,
    default: bool,
) -> Option<bool> {
    match map.get(field) {
        Some(value) => match value.as_bool() {
            Ok(b) => Some(b),
            Err(_) => {
                state.error(key, format!("field '{field}' must be a boolean"));
                None
            }
        },
        None => Some(default),
    }
}

/// Reads an optional content-key field. `Some(None)` means absent;
/// `None` means present but invalid (already reported).
fn opt_key(
    state: &mut BuilderState,
    key: Option<&str>,
    map: &Map,
    field: &str,
) -> Option<Option<ContentKey>> {
    match map.get(field) {
        None => Some(None),
        Some(value) => match value.clone().into_string() {
            Ok(raw) => match ContentKey::new(&raw) {
                Ok(parsed) => Some(Some(parsed)),
                Err(err) => {
                    state.error(key, format!("field '{field}': {err}"));
                    None
                }
            },
            Err(_) => {
                state.error(key, format!("field '{field}' must be a string"));
                None
            }
        },
    }
}

/// Reads an optional list of content keys (defaults to empty).
fn key_list(
    state: &mut BuilderState,
    key: Option<&str>,
    map: &Map,
    field: &str,
) -> Option<Vec<ContentKey>> {
    let Some(value) = map.get(field) else {
        return Some(Vec::new());
    };
    let Some(array) = value.clone().try_cast::<rhai::Array>() else {
        state.error(key, format!("field '{field}' must be an array of keys"));
        return None;
    };
    let mut keys = Vec::with_capacity(array.len());
    for element in array {
        let Ok(raw) = element.into_string() else {
            state.error(key, format!("field '{field}' entries must be strings"));
            return None;
        };
        match ContentKey::new(&raw) {
            Ok(parsed) => keys.push(parsed),
            Err(err) => {
                state.error(key, format!("field '{field}': {err}"));
                return None;
            }
        }
    }
    Some(keys)
}

/// Reads an optional list of plain strings (defaults to empty).
fn string_list(
    state: &mut BuilderState,
    key: Option<&str>,
    map: &Map,
    field: &str,
) -> Option<Vec<String>> {
    let Some(value) = map.get(field) else {
        return Some(Vec::new());
    };
    let Some(array) = value.clone().try_cast::<rhai::Array>() else {
        state.error(key, format!("field '{field}' must be an array of strings"));
        return None;
    };
    let mut items = Vec::with_capacity(array.len());
    for element in array {
        match element.into_string() {
            Ok(text) => items.push(text),
            Err(_) => {
                state.error(key, format!("field '{field}' entries must be strings"));
                return None;
            }
        }
    }
    Some(items)
}

/// Warns about unknown fields so typos surface instead of silently doing
/// nothing.
fn warn_unknown_fields(state: &mut BuilderState, map: &Map, key: Option<&str>, allowed: &[&str]) {
    let mut unknown: Vec<String> = map
        .keys()
        .filter(|k| !allowed.contains(&k.as_str()))
        .map(|k| k.to_string())
        .collect();
    unknown.sort();
    for field in unknown {
        let path = state.current_path.clone();
        state.report.findings.push(crate::report::ContentFinding {
            severity: Severity::Warning,
            path,
            key: key.map(str::to_owned),
            message: format!("unknown field '{field}' is ignored"),
        });
    }
}

/// One definition's fields, read against the map they arrived in.
///
/// Every read marks its field name; [`Fields::finish`] warns about the
/// fields nobody read. The reads therefore *are* the schema — there is no
/// separate allow-list to fall out of step with them. Unknown-field
/// warnings fire only for definitions that otherwise load; a failed
/// definition already reports the error that matters.
struct Fields<'s> {
    state: &'s mut BuilderState,
    map: Map,
    key: ContentKey,
    read: BTreeSet<&'static str>,
}

impl<'s> Fields<'s> {
    /// Reads and validates the `id` field, beginning the tracked reads.
    fn begin(state: &'s mut BuilderState, map: Map) -> Option<Self> {
        let raw = req_str(state, None, &map, "id")?;
        let key = match ContentKey::new(&raw) {
            Ok(key) => key,
            Err(err) => {
                state.error(Some(&raw), err.to_string());
                return None;
            }
        };
        let mut read = BTreeSet::new();
        read.insert("id");
        Some(Self {
            state,
            map,
            key,
            read,
        })
    }

    fn key_str(&self) -> String {
        self.key.to_string()
    }

    /// Reports an error against this definition.
    fn error(&mut self, message: impl Into<String>) {
        let key = self.key.to_string();
        self.state.error(Some(&key), message);
    }

    fn req_str(&mut self, field: &'static str) -> Option<String> {
        self.read.insert(field);
        let key = self.key.to_string();
        req_str(self.state, Some(&key), &self.map, field)
    }

    /// Marks a display-text field as belonging to the string table.
    ///
    /// Returns an empty placeholder, which [`fill_display_text`] replaces
    /// with the row this definition's ID derives. A file that still carries
    /// the field is an error rather than a warning: silently ignoring it
    /// would leave an author editing prose that never reaches the screen.
    /// The error names the row to move it to.
    ///
    /// [`fill_display_text`]: super::display::fill_display_text
    fn moved_to_table(&mut self, field: &'static str, kind: &str) -> String {
        self.read.insert(field);
        if self.map.contains_key(field) {
            let derived = format!("{kind}.{}.{}", self.key, field.replace('_', "-"));
            self.error(format!(
                "'{field}' is display text and now lives in the string table; \
                 move it to assets/text/strings.csv under '{derived}'"
            ));
        }
        String::new()
    }

    fn opt_str(&mut self, field: &'static str) -> Option<Option<String>> {
        self.read.insert(field);
        let key = self.key.to_string();
        opt_str(self.state, Some(&key), &self.map, field)
    }

    fn req_int(&mut self, field: &'static str) -> Option<i64> {
        self.read.insert(field);
        let key = self.key.to_string();
        req_int(self.state, Some(&key), &self.map, field)
    }

    fn opt_int(&mut self, field: &'static str, default: i64) -> Option<i64> {
        self.read.insert(field);
        let key = self.key.to_string();
        opt_int(self.state, Some(&key), &self.map, field, default)
    }

    fn opt_bool(&mut self, field: &'static str, default: bool) -> Option<bool> {
        self.read.insert(field);
        let key = self.key.to_string();
        opt_bool(self.state, Some(&key), &self.map, field, default)
    }

    fn opt_key(&mut self, field: &'static str) -> Option<Option<ContentKey>> {
        self.read.insert(field);
        let key = self.key.to_string();
        opt_key(self.state, Some(&key), &self.map, field)
    }

    fn req_key_field(&mut self, field: &'static str) -> Option<ContentKey> {
        match self.opt_key(field)? {
            Some(key) => Some(key),
            None => {
                self.error(format!("missing required field '{field}'"));
                None
            }
        }
    }

    fn key_list(&mut self, field: &'static str) -> Option<Vec<ContentKey>> {
        self.read.insert(field);
        let key = self.key.to_string();
        key_list(self.state, Some(&key), &self.map, field)
    }

    fn string_list(&mut self, field: &'static str) -> Option<Vec<String>> {
        self.read.insert(field);
        let key = self.key.to_string();
        string_list(self.state, Some(&key), &self.map, field)
    }

    /// Reads a required field spelled from a fixed vocabulary.
    fn req_enum<T: Copy>(&mut self, field: &'static str, options: &[(&str, T)]) -> Option<T> {
        let raw = self.req_str(field)?;
        self.parse_enum(field, &raw, options)
    }

    /// Reads an optional vocabulary field, taking `default` when absent.
    fn opt_enum<T: Copy>(
        &mut self,
        field: &'static str,
        options: &[(&str, T)],
        default: T,
    ) -> Option<T> {
        match self.opt_str(field)? {
            None => Some(default),
            Some(raw) => self.parse_enum(field, &raw, options),
        }
    }

    /// Reads an optional vocabulary field with no default (`Some(None)`
    /// when absent).
    fn opt_enum_value<T: Copy>(
        &mut self,
        field: &'static str,
        options: &[(&str, T)],
    ) -> Option<Option<T>> {
        match self.opt_str(field)? {
            None => Some(None),
            Some(raw) => self.parse_enum(field, &raw, options).map(Some),
        }
    }

    fn parse_enum<T: Copy>(
        &mut self,
        field: &'static str,
        raw: &str,
        options: &[(&str, T)],
    ) -> Option<T> {
        if let Some((_, value)) = options.iter().find(|(name, _)| *name == raw) {
            return Some(*value);
        }
        let expected: Vec<&str> = options.iter().map(|(name, _)| *name).collect();
        self.error(format!(
            "unknown {field} '{raw}' (expected {})",
            expected.join(", ")
        ));
        None
    }

    /// Takes a raw field for bespoke handling (nested maps and arrays).
    fn take_raw(&mut self, field: &'static str) -> Option<rhai::Dynamic> {
        self.read.insert(field);
        self.map.get(field).cloned()
    }

    /// Reads an optional `effect_fn`-style field, bound to the file being
    /// loaded.
    fn opt_fn_ref(&mut self, field: &'static str) -> Option<Option<ScriptFnRef>> {
        match self.opt_str(field)? {
            None => Some(None),
            Some(name) => Some(Some(ScriptFnRef {
                path: self.state.current_path.clone(),
                name,
            })),
        }
    }

    /// Warns about every field nobody read. Call on the success path,
    /// before inserting the definition.
    fn finish(self) -> (&'s mut BuilderState, ContentKey) {
        let unknown: Vec<String> = self
            .map
            .keys()
            .filter(|k| !self.read.contains(k.as_str()))
            .map(|k| k.to_string())
            .collect();
        let key = self.key;
        let state = self.state;
        let mut unknown = unknown;
        unknown.sort();
        for field in unknown {
            let path = state.current_path.clone();
            state.report.findings.push(crate::report::ContentFinding {
                severity: Severity::Warning,
                path,
                key: Some(key.to_string()),
                message: format!("unknown field '{field}' is ignored"),
            });
        }
        (state, key)
    }
}

// ---------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------

fn define_assignment(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let title = f.moved_to_table("title", "assignment");
    let summary = f.moved_to_table("summary", "assignment");
    let Some(category) = f.req_enum(
        "category",
        &[
            ("routine", AssignmentCategory::Routine),
            ("consequential", AssignmentCategory::Consequential),
        ],
    ) else {
        return;
    };
    let Some(duration_days) = f.req_int("duration_days") else {
        return;
    };
    if !(1..=100_000).contains(&duration_days) {
        f.error(format!(
            "duration_days must be 1..=100000, got {duration_days}"
        ));
        return;
    }
    let Some(skill) = f.req_enum(
        "skill",
        &[
            ("command", GoverningSkill::Command),
            ("diplomacy", GoverningSkill::Diplomacy),
            ("intrigue", GoverningSkill::Intrigue),
            ("stewardship", GoverningSkill::Stewardship),
        ],
    ) else {
        return;
    };
    let Some(difficulty) = f.req_int("difficulty") else {
        return;
    };
    if !(0..=40).contains(&difficulty) {
        f.error("difficulty must be 0..=40");
        return;
    }
    let Some(target) = f.opt_enum(
        "target",
        &[
            ("none", AssignmentTargetKind::None),
            ("character", AssignmentTargetKind::Character),
            ("organisation", AssignmentTargetKind::Organisation),
            ("province", AssignmentTargetKind::Province),
            ("own-army", AssignmentTargetKind::OwnArmy),
            (
                "own-army-and-province",
                AssignmentTargetKind::OwnArmyAndProvince,
            ),
            (
                "own-ship-and-province",
                AssignmentTargetKind::OwnShipAndProvince,
            ),
        ],
        AssignmentTargetKind::None,
    ) else {
        return;
    };
    let Some(risk_names) = f.string_list("risks") else {
        return;
    };
    let mut risks = Vec::with_capacity(risk_names.len());
    for name in &risk_names {
        let tag = match name.as_str() {
            "injury" => RiskTag::Injury,
            "capture" => RiskTag::Capture,
            "scandal" => RiskTag::Scandal,
            "incapacity" => RiskTag::Incapacity,
            "death" => RiskTag::Death,
            other => {
                f.error(format!(
                    "unknown risk '{other}' (expected injury, capture, scandal, \
                     incapacity, death)"
                ));
                return;
            }
        };
        risks.push(tag);
    }
    risks.sort();
    risks.dedup();
    let Some(military_op) = f.opt_enum_value(
        "military_op",
        &[
            ("move", MilitaryOp::Move),
            ("resupply", MilitaryOp::Resupply),
            ("patrol", MilitaryOp::Patrol),
            ("besiege", MilitaryOp::Besiege),
            ("raid", MilitaryOp::Raid),
            ("blockade", MilitaryOp::Blockade),
        ],
    ) else {
        return;
    };
    let op_target_ok = match military_op {
        None => true,
        Some(MilitaryOp::Resupply | MilitaryOp::Patrol) => target == AssignmentTargetKind::OwnArmy,
        Some(MilitaryOp::Move | MilitaryOp::Besiege | MilitaryOp::Raid) => {
            target == AssignmentTargetKind::OwnArmyAndProvince
        }
        Some(MilitaryOp::Blockade) => target == AssignmentTargetKind::OwnShipAndProvince,
    };
    if !op_target_ok {
        f.error("military_op and target kind do not match");
        return;
    }
    let Some(ai_intent) = f.opt_enum(
        "ai_intent",
        &[
            ("routine", AiIntent::Routine),
            ("order", AiIntent::Order),
            ("muster", AiIntent::Muster),
            ("obligation", AiIntent::Obligation),
            ("resources", AiIntent::Resources),
            ("standing", AiIntent::Standing),
            ("claim", AiIntent::Claim),
        ],
        AiIntent::Routine,
    ) else {
        return;
    };
    let Some(ai_available) = f.opt_bool("ai_available", true) else {
        return;
    };
    let (Some(wealth_cost), Some(manpower_cost), Some(supplies_cost), Some(influence_cost)) = (
        f.opt_int("wealth_cost", 0),
        f.opt_int("manpower_cost", 0),
        f.opt_int("supplies_cost", 0),
        f.opt_int("influence_cost", 0),
    ) else {
        return;
    };

    let requires = assignment_requires(&mut f);
    let stages = assignment_stages(&mut f, duration_days as u32);
    let Some(urgency) = f.opt_enum(
        "urgency",
        &[
            ("routine", Urgency::Routine),
            ("pressing", Urgency::Pressing),
            ("urgent", Urgency::Urgent),
        ],
        Urgency::Routine,
    ) else {
        return;
    };

    let Some(results_value) = f.take_raw("results") else {
        f.error("missing required field 'results'");
        return;
    };
    let Some(results_map) = results_value.try_cast::<Map>() else {
        f.error("field 'results' must be a map");
        return;
    };
    let (state, key) = f.finish();
    let Some(results) = assignment_results(state, &key, &results_map) else {
        return;
    };

    if state.assignments.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate assignment id");
        return;
    }
    state.assignments.insert(
        key.clone(),
        AssignmentDef {
            key,
            title,
            summary,
            category,
            duration_days: duration_days as u32,
            skill,
            difficulty: difficulty as i32,
            target,
            risks,
            military_op,
            ai_available,
            ai_intent,
            wealth_cost,
            manpower_cost,
            supplies_cost,
            influence_cost,
            requires,
            urgency,
            stages,
            results,
        },
    );
}

/// Reads an assignment's phases.
///
/// Content that authors none gets one interruptible phase covering the
/// whole duration, which is exactly how assignments behaved before phases
/// existed. Content that authors them owns the duration: `duration_days`
/// is checked against their sum rather than believed, because two numbers
/// that can disagree eventually will.
fn assignment_stages(f: &mut Fields, duration_days: u32) -> Vec<StageDef> {
    let default = || {
        vec![StageDef {
            id: "whole".to_owned(),
            days: duration_days,
            interruptible: true,
            on_interrupt: None,
        }]
    };
    let Some(raw) = f.take_raw("stages") else {
        return default();
    };
    let Some(list) = raw.try_cast::<rhai::Array>() else {
        f.error("stages must be a list");
        return default();
    };

    let mut stages = Vec::new();
    for entry in list {
        let Some(map) = entry.try_cast::<Map>() else {
            f.error("each stage must be a map");
            return default();
        };
        warn_unknown_fields(
            f.state,
            &map,
            Some(f.key.as_str()),
            &["id", "days", "interruptible", "on_interrupt"],
        );
        let Some(id) = map.get("id").and_then(|v| v.clone().into_string().ok()) else {
            f.error("a stage needs an id");
            return default();
        };
        let Some(days) = map.get("days").and_then(|v| v.as_int().ok()) else {
            f.error("a stage needs days");
            return default();
        };
        if days <= 0 {
            f.error("a stage must last at least a day");
            return default();
        }
        let interruptible = map
            .get("interruptible")
            .and_then(|v| v.as_bool().ok())
            .unwrap_or(true);
        // Script references carry the file they were written in, so a
        // stage's effect resolves the same way a result's does.
        let on_interrupt = map
            .get("on_interrupt")
            .and_then(|v| v.clone().into_string().ok())
            .map(|name| ScriptFnRef {
                path: f.state.current_path.clone(),
                name,
            });
        stages.push(StageDef {
            id,
            days: days as u32,
            interruptible,
            on_interrupt,
        });
    }

    if stages.is_empty() {
        f.error("stages must not be empty");
        return default();
    }
    let total: u32 = stages.iter().map(|s| s.days).sum();
    if total != duration_days {
        f.error(format!(
            "stages last {total} days but duration_days says {duration_days}"
        ));
    }
    stages
}

/// Reads an assignment's target requirements.
///
/// Every field is optional and defaults to "do not care", so an assignment
/// that declares nothing is offered exactly as widely as it was before
/// requirements existed. Unknown keys warn rather than fail, matching how
/// every other authored block behaves.
fn assignment_requires(f: &mut Fields) -> AssignmentRequires {
    let mut requires = AssignmentRequires::default();
    let Some(raw) = f.take_raw("requires") else {
        return requires;
    };
    let Some(conditions) = raw.try_cast::<Map>() else {
        f.error("requires must be a map");
        return requires;
    };
    warn_unknown_fields(
        f.state,
        &conditions,
        Some(f.key.as_str()),
        &[
            "target_holder",
            "target_occupied",
            "army_present",
            "target_house",
            "target_holds_title",
            "target_owes_favour",
            "owner_threatened",
            "max_order",
            "min_order",
        ],
    );

    let relation = |conditions: &Map, name: &str| -> HolderRelation {
        match conditions
            .get(name)
            .and_then(|v| v.clone().into_string().ok())
            .as_deref()
        {
            Some("own") => HolderRelation::Own,
            Some("other") => HolderRelation::Other,
            _ => HolderRelation::Any,
        }
    };
    let flag = |conditions: &Map, name: &str| -> bool {
        conditions
            .get(name)
            .and_then(|v| v.as_bool().ok())
            .unwrap_or(false)
    };

    requires.target_holder = relation(&conditions, "target_holder");
    requires.target_house = relation(&conditions, "target_house");
    requires.target_occupied = flag(&conditions, "target_occupied");
    requires.army_present = flag(&conditions, "army_present");
    requires.target_owes_favour = flag(&conditions, "target_owes_favour");
    requires.owner_threatened = flag(&conditions, "owner_threatened");
    requires.target_holds_title = conditions
        .get("target_holds_title")
        .and_then(|v| v.clone().into_string().ok())
        .and_then(|name| match name.as_str() {
            "consul" => Some(TitleNeed::Consul),
            "paramount" => Some(TitleNeed::Paramount),
            "province" => Some(TitleNeed::Province),
            _ => None,
        });
    requires.max_order = conditions
        .get("max_order")
        .and_then(|v| v.as_int().ok())
        .map(|v| v as i32);
    requires.min_order = conditions
        .get("min_order")
        .and_then(|v| v.as_int().ok())
        .map(|v| v as i32);
    requires
}

/// Reads a assignment's graded results, each an optional nested map.
fn assignment_results(
    state: &mut BuilderState,
    key: &ContentKey,
    results_map: &Map,
) -> Option<BTreeMap<OutcomeKind, OutcomeDef>> {
    let mut results = BTreeMap::new();
    let mut ok = true;
    for (result_name, kind) in [
        ("critical_success", OutcomeKind::CriticalSuccess),
        ("success", OutcomeKind::Success),
        ("failure", OutcomeKind::Failure),
        ("disaster", OutcomeKind::Disaster),
    ] {
        let Some(entry) = results_map.get(result_name) else {
            continue;
        };
        let Some(entry_map) = entry.clone().try_cast::<Map>() else {
            state.error(
                Some(key.as_str()),
                format!("result '{result_name}' must be a map"),
            );
            ok = false;
            continue;
        };
        warn_unknown_fields(
            state,
            &entry_map,
            Some(key.as_str()),
            &[
                "weight",
                "popup",
                "popup_text",
                "choices",
                "log",
                "log_text",
                "effect_fn",
            ],
        );
        let key_str = key.to_string();
        let Some(weight) = req_int(state, Some(&key_str), &entry_map, "weight") else {
            ok = false;
            continue;
        };
        if !(1..=1_000_000).contains(&weight) {
            state.error(
                Some(key.as_str()),
                format!("result '{result_name}' weight must be 1..=1000000, got {weight}"),
            );
            ok = false;
            continue;
        }
        let (Some(popup), Some(log)) = (
            opt_bool(state, Some(&key_str), &entry_map, "popup", false),
            opt_bool(state, Some(&key_str), &entry_map, "log", false),
        ) else {
            ok = false;
            continue;
        };
        let (Some(popup_text), Some(log_text), Some(effect_name)) = (
            opt_str(state, Some(&key_str), &entry_map, "popup_text"),
            opt_str(state, Some(&key_str), &entry_map, "log_text"),
            opt_str(state, Some(&key_str), &entry_map, "effect_fn"),
        ) else {
            ok = false;
            continue;
        };
        let effect_fn = effect_name.map(|name| ScriptFnRef {
            path: state.current_path.clone(),
            name,
        });
        let mut choices = Vec::new();
        let mut choices_bad = false;
        if let Some(value) = entry_map.get("choices") {
            let Some(array) = value.clone().try_cast::<rhai::Array>() else {
                state.error(
                    Some(key.as_str()),
                    format!("result '{result_name}' choices must be an array of maps"),
                );
                ok = false;
                continue;
            };
            for element in array {
                let Some(choice_map) = element.try_cast::<Map>() else {
                    state.error(
                        Some(key.as_str()),
                        format!("result '{result_name}' choices must be maps"),
                    );
                    choices_bad = true;
                    break;
                };
                warn_unknown_fields(
                    state,
                    &choice_map,
                    Some(key.as_str()),
                    &["id", "label", "effect_fn"],
                );
                let choice_id = req_str(state, Some(&key_str), &choice_map, "id").and_then(|raw| {
                    match ContentKey::new(&raw) {
                        Ok(parsed) => Some(parsed),
                        Err(err) => {
                            state.error(Some(&raw), err.to_string());
                            None
                        }
                    }
                });
                let Some(choice_id) = choice_id else {
                    choices_bad = true;
                    break;
                };
                reject_authored_text(state, &key_str, &choice_map, "label");
                let Some(choice_effect) = opt_str(state, Some(&key_str), &choice_map, "effect_fn")
                else {
                    choices_bad = true;
                    break;
                };
                choices.push(PopupChoiceDef {
                    id: choice_id,
                    label: String::new(),
                    effect_fn: choice_effect.map(|name| ScriptFnRef {
                        path: state.current_path.clone(),
                        name,
                    }),
                });
            }
        }
        if choices_bad {
            ok = false;
            continue;
        }
        results.insert(
            kind,
            OutcomeDef {
                weight: weight as u32,
                popup,
                popup_text,
                choices,
                log,
                log_text,
                effect_fn,
            },
        );
    }

    let mut unknown_results: Vec<String> = results_map
        .keys()
        .map(|k| k.to_string())
        .filter(|k| !["critical_success", "success", "failure", "disaster"].contains(&k.as_str()))
        .collect();
    unknown_results.sort();
    for name in &unknown_results {
        state.error(
            Some(key.as_str()),
            format!(
                "unknown result kind '{name}' (expected critical_success, success, \
                 failure, disaster)"
            ),
        );
    }
    (ok && unknown_results.is_empty()).then_some(results)
}

fn define_body(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "body");
    let Some(kind) = f.req_enum(
        "kind",
        &[
            ("planet", BodyKind::Planet),
            ("moon", BodyKind::Moon),
            ("starbase", BodyKind::Starbase),
        ],
    ) else {
        return;
    };
    let Some(radius_km) = f.req_int("radius_km") else {
        return;
    };
    let (Some(orbit_radius_mm), Some(orbit_days)) =
        (f.opt_int("orbit_radius_mm", 0), f.opt_int("orbit_days", 0))
    else {
        return;
    };
    if radius_km <= 0 {
        f.error("radius_km must be positive");
        return;
    }
    let Some(parent) = f.opt_key("parent") else {
        return;
    };

    let (state, key) = f.finish();
    if state.bodies.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate body id");
        return;
    }
    state.bodies.insert(
        key.clone(),
        BodyDef {
            key,
            name,
            kind,
            radius_km: radius_km as u32,
            orbit_radius_mm: orbit_radius_mm.max(0) as u32,
            orbit_days: orbit_days.max(0) as u32,
            parent,
        },
    );
}

fn define_good(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "good");
    let Some(value) = f.opt_int("value", 1) else {
        return;
    };
    if !(0..=1_000_000).contains(&value) {
        f.error(format!("value must be 0..=1000000, got {value}"));
        return;
    }
    let (state, key) = f.finish();
    if state.goods.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate good id");
        return;
    }
    state
        .goods
        .insert(key.clone(), GoodDef { key, name, value });
}

fn define_building(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "building");
    let summary = f.moved_to_table("summary", "building");
    let Some(build_days) = f.req_int("build_days") else {
        return;
    };
    if !(1..=100_000).contains(&build_days) {
        f.error(format!("build_days must be 1..=100000, got {build_days}"));
        return;
    }
    let (Some(wealth_cost), Some(supplies_cost), Some(adds_wealth)) = (
        f.opt_int("wealth_cost", 0),
        f.opt_int("supplies_cost", 0),
        f.opt_int("adds_wealth", 0),
    ) else {
        return;
    };
    let (Some(produces), Some(consumes)) =
        (goods_map(&mut f, "produces"), goods_map(&mut f, "consumes"))
    else {
        return;
    };
    let (state, key) = f.finish();
    if state.buildings.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate building id");
        return;
    }
    state.buildings.insert(
        key.clone(),
        BuildingDef {
            key,
            name,
            summary,
            wealth_cost,
            supplies_cost,
            build_days: build_days as u32,
            adds_wealth,
            produces,
            consumes,
        },
    );
}

/// Reads an authored map of good keys to monthly rates.
///
/// Open-ended — any defined good may appear — so it iterates the map's
/// entries rather than a fixed field list, the same way skills are read
/// for a fixed one. Keys are validated as content keys here; that they
/// name a *defined* good is a cross-reference check, done at load.
fn goods_map(f: &mut Fields, field: &'static str) -> Option<BTreeMap<ContentKey, i64>> {
    let Some(raw) = f.take_raw(field) else {
        return Some(BTreeMap::new());
    };
    let Some(map) = raw.try_cast::<Map>() else {
        f.error(format!("field '{field}' must be a map of goods to rates"));
        return None;
    };
    let mut goods = BTreeMap::new();
    for (good, amount) in map {
        let good_key = match ContentKey::new(&good) {
            Ok(key) => key,
            Err(err) => {
                f.error(format!("field '{field}': {err}"));
                return None;
            }
        };
        let Ok(rate) = amount.as_int() else {
            f.error(format!("field '{field}': '{good}' must be an integer rate"));
            return None;
        };
        if rate < 0 {
            f.error(format!("field '{field}': '{good}' must not be negative"));
            return None;
        }
        goods.insert(good_key, rate);
    }
    Some(goods)
}

fn define_province(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "province");
    let Some(body) = f.req_key_field("body") else {
        return;
    };
    let (Some(latitude_mdeg), Some(longitude_mdeg)) =
        (f.req_int("latitude_mdeg"), f.req_int("longitude_mdeg"))
    else {
        return;
    };
    if !(-90_000..=90_000).contains(&latitude_mdeg) {
        f.error("latitude_mdeg must be -90000..=90000");
        return;
    }
    if !(-180_000..180_000).contains(&longitude_mdeg) {
        f.error("longitude_mdeg must be -180000..180000");
        return;
    }
    let (Some(wealth_output), Some(manpower_output), Some(supplies_output)) = (
        f.opt_int("wealth_output", 10),
        f.opt_int("manpower_output", 10),
        f.opt_int("supplies_output", 10),
    ) else {
        return;
    };
    let (Some(produces), Some(consumes)) =
        (goods_map(&mut f, "produces"), goods_map(&mut f, "consumes"))
    else {
        return;
    };

    let (state, key) = f.finish();
    if state.provinces.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate province id");
        return;
    }
    state.provinces.insert(
        key.clone(),
        ProvinceDef {
            key,
            name,
            body,
            latitude_mdeg: latitude_mdeg as i32,
            longitude_mdeg: longitude_mdeg as i32,
            wealth_output,
            manpower_output,
            supplies_output,
            produces,
            consumes,
        },
    );
}

fn define_scenario(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "scenario");
    let (Some(start_year), Some(start_month), Some(start_day)) = (
        f.req_int("start_year"),
        f.req_int("start_month"),
        f.req_int("start_day"),
    ) else {
        return;
    };
    if !(1..=12).contains(&start_month) || !(1..=30).contains(&start_day) {
        f.error("start_month must be 1..=12 and start_day 1..=30");
        return;
    }
    let Some(player_house) = f.opt_key("player_house") else {
        return;
    };
    let (state, key) = f.finish();
    if state.scenario.is_some() {
        state.error(
            Some(key.as_str()),
            "a content set may define only one scenario",
        );
        return;
    }
    state.scenario = Some(ScenarioDef {
        key,
        name,
        start_year,
        start_month: start_month as u8,
        start_day: start_day as u8,
        player_house,
    });
}

fn define_trait(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "trait");
    let summary = f.moved_to_table("summary", "trait");
    let (Some(opinion_same), Some(opinion_opposed)) = (
        f.opt_int("opinion_same", 0),
        f.opt_int("opinion_opposed", 0),
    ) else {
        return;
    };
    let Some(opposites) = f.key_list("opposites") else {
        return;
    };
    let (state, key) = f.finish();
    if state.traits.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate trait id");
        return;
    }
    state.traits.insert(
        key.clone(),
        TraitDef {
            key,
            name,
            summary,
            opinion_same: opinion_same as i32,
            opinion_opposed: opinion_opposed as i32,
            opposites,
        },
    );
}

fn define_character(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "character");
    let Some(gender) = f.req_enum(
        "gender",
        &[("male", Gender::Male), ("female", Gender::Female)],
    ) else {
        return;
    };
    let (Some(birth_year), Some(birth_month), Some(birth_day)) = (
        f.req_int("birth_year"),
        f.opt_int("birth_month", 1),
        f.opt_int("birth_day", 1),
    ) else {
        return;
    };
    if !(1..=12).contains(&birth_month) || !(1..=30).contains(&birth_day) {
        f.error("birth_month must be 1..=12 and birth_day 1..=30");
        return;
    }
    let Some(organisation) = f.opt_key("organisation") else {
        return;
    };
    let Some(parents) = f.key_list("parents") else {
        return;
    };
    if parents.len() > 2 {
        f.error("characters have at most two parents");
        return;
    }
    let Some(spouse) = f.opt_key("spouse") else {
        return;
    };
    let Some(traits) = f.key_list("traits") else {
        return;
    };
    let skills_raw = f.take_raw("skills");
    let (state, key) = f.finish();
    let skills = match skills_raw {
        None => SkillsDef::default(),
        Some(value) => {
            let Some(skills_map) = value.try_cast::<Map>() else {
                state.error(Some(key.as_str()), "field 'skills' must be a map");
                return;
            };
            warn_unknown_fields(
                state,
                &skills_map,
                Some(key.as_str()),
                &["command", "diplomacy", "intrigue", "stewardship"],
            );
            let key_str = key.to_string();
            let (Some(command), Some(diplomacy), Some(intrigue), Some(stewardship)) = (
                opt_int(state, Some(&key_str), &skills_map, "command", 0),
                opt_int(state, Some(&key_str), &skills_map, "diplomacy", 0),
                opt_int(state, Some(&key_str), &skills_map, "intrigue", 0),
                opt_int(state, Some(&key_str), &skills_map, "stewardship", 0),
            ) else {
                return;
            };
            SkillsDef {
                command: command as i32,
                diplomacy: diplomacy as i32,
                intrigue: intrigue as i32,
                stewardship: stewardship as i32,
            }
        }
    };
    if state.characters.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate character id");
        return;
    }
    state.characters.insert(
        key.clone(),
        CharacterDef {
            key,
            name,
            gender,
            birth_year,
            birth_month: birth_month as u8,
            birth_day: birth_day as u8,
            organisation,
            parents,
            spouse,
            traits,
            skills,
        },
    );
}

/// Reads an organisation's `[r, g, b]` colour.
fn org_color(f: &mut Fields) -> Option<(u8, u8, u8)> {
    let Some(value) = f.take_raw("color") else {
        f.error("missing required field 'color'");
        return None;
    };
    let Some(array) = value.try_cast::<rhai::Array>() else {
        f.error("field 'color' must be [r, g, b]");
        return None;
    };
    if array.len() != 3 {
        f.error("field 'color' must be [r, g, b]");
        return None;
    }
    let mut channels = [0u8; 3];
    for (slot, element) in channels.iter_mut().zip(array) {
        match element.as_int() {
            Ok(v) if (0..=255).contains(&v) => *slot = v as u8,
            _ => {
                f.error("colour channels must be integers 0..=255");
                return None;
            }
        }
    }
    Some((channels[0], channels[1], channels[2]))
}

/// Reads starting resources and legitimacy for an organisation.
fn org_resources(f: &mut Fields) -> Option<(i64, i64, i64, i32)> {
    let (Some(wealth), Some(manpower), Some(supplies), Some(legitimacy)) = (
        f.opt_int("wealth", 100),
        f.opt_int("manpower", 1000),
        f.opt_int("supplies", 200),
        f.opt_int("legitimacy", 50),
    ) else {
        return None;
    };
    if !(0..=100).contains(&legitimacy) {
        f.error("legitimacy must be 0..=100");
        return None;
    }
    Some((wealth, manpower, supplies, legitimacy as i32))
}

fn insert_org(state: &mut BuilderState, org: OrgDef) {
    if state.organisations.contains_key(&org.key) {
        state.error(Some(org.key.as_str()), "duplicate organisation id");
        return;
    }
    state.organisations.insert(org.key.clone(), org);
}

fn define_house(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "organisation");
    let Some(tier) = f.req_enum(
        "tier",
        &[
            ("great", HouseTier::Great),
            ("vassal", HouseTier::Vassal),
            ("independent", HouseTier::Independent),
        ],
    ) else {
        return;
    };
    let Some(liege) = f.opt_key("liege") else {
        return;
    };
    f.moved_to_table("surname", "organisation");
    let surname = None;
    let Some(head) = f.opt_key("head") else {
        return;
    };
    let Some(provinces) = f.key_list("provinces") else {
        return;
    };
    let Some(color) = org_color(&mut f) else {
        return;
    };
    let Some((wealth, manpower, supplies, legitimacy)) = org_resources(&mut f) else {
        return;
    };
    let (state, key) = f.finish();
    insert_org(
        state,
        OrgDef {
            key,
            name,
            kind: OrgKind::DynasticHouse,
            tier: Some(tier),
            liege,
            surname,
            head,
            provinces,
            color,
            wealth,
            manpower,
            supplies,
            legitimacy,
        },
    );
}

fn define_organisation(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "organisation");
    let Some(kind_raw) = f.req_str("kind") else {
        return;
    };
    let kind = match kind_raw.as_str() {
        "sanctora-imperim" => OrgKind::SanctoraImperim,
        "dynastic-house" => {
            f.error("dynastic houses are defined with define_house");
            return;
        }
        other => {
            f.error(format!(
                "unknown organisation kind '{other}' (expected sanctora-imperim)"
            ));
            return;
        }
    };
    let Some(head) = f.opt_key("head") else {
        return;
    };
    let Some(provinces) = f.key_list("provinces") else {
        return;
    };
    let Some(color) = org_color(&mut f) else {
        return;
    };
    let Some((wealth, manpower, supplies, legitimacy)) = org_resources(&mut f) else {
        return;
    };
    let (state, key) = f.finish();
    insert_org(
        state,
        OrgDef {
            key,
            name,
            kind,
            tier: None,
            liege: None,
            surname: None,
            head,
            provinces,
            color,
            wealth,
            manpower,
            supplies,
            legitimacy,
        },
    );
}

fn define_name_pool(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let (Some(male), Some(female)) = (f.string_list("male"), f.string_list("female")) else {
        return;
    };
    if male.is_empty() || female.is_empty() {
        f.error("name pools need at least one male and one female name");
        return;
    }
    let (state, key) = f.finish();
    if state.name_pools.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate name pool id");
        return;
    }
    state
        .name_pools
        .insert(key.clone(), NamePoolDef { key, male, female });
}

fn define_ship(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "ship");
    let Some(class) = f.req_enum(
        "class",
        &[
            ("capital", ShipClass::Capital),
            ("transport", ShipClass::Transport),
            ("patrol", ShipClass::Patrol),
        ],
    ) else {
        return;
    };
    let Some(owner) = f.req_key_field("owner") else {
        return;
    };
    let Some(captain) = f.opt_key("captain") else {
        return;
    };
    let Some(location) = f.req_key_field("location") else {
        return;
    };
    let (state, key) = f.finish();
    if state.ships.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate ship id");
        return;
    }
    state.ships.insert(
        key.clone(),
        ShipDef {
            key,
            name,
            class,
            owner,
            captain,
            location,
        },
    );
}

fn define_army(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "army");
    let (Some(owner), Some(general), Some(province)) = (
        f.req_key_field("owner"),
        f.req_key_field("general"),
        f.req_key_field("province"),
    ) else {
        return;
    };
    let (Some(manpower), Some(supplies)) = (f.opt_int("manpower", 500), f.opt_int("supplies", 100))
    else {
        return;
    };
    if manpower <= 0 {
        f.error("manpower must be positive");
        return;
    }
    let (state, key) = f.finish();
    if state.armies.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate army id");
        return;
    }
    state.armies.insert(
        key.clone(),
        ArmyDef {
            key,
            name,
            owner,
            general,
            province,
            manpower,
            supplies: supplies.max(0),
        },
    );
}

fn define_event(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let title = f.moved_to_table("title", "event");
    let text = f.moved_to_table("text", "event");
    let Some(family) = f.opt_enum(
        "family",
        &[
            ("province", EventFamily::Province),
            ("political", EventFamily::Political),
            ("travel", EventFamily::Travel),
            ("assignment", EventFamily::Assignment),
        ],
        EventFamily::Province,
    ) else {
        return;
    };
    let (Some(weight), Some(cooldown)) =
        (f.opt_int("weight", 100), f.opt_int("cooldown_days", 720))
    else {
        return;
    };
    let Some(weighty) = f.opt_bool("weighty", false) else {
        return;
    };
    f.moved_to_table("log_text", "event");
    let log_text = None;
    let Some(effect_fn) = f.opt_fn_ref("effect_fn") else {
        return;
    };

    let mut requires = EventRequires::default();
    if let Some(raw) = f.take_raw("requires") {
        match raw.try_cast::<Map>() {
            None => f.error("requires must be a map"),
            Some(conditions) => {
                warn_unknown_fields(
                    f.state,
                    &conditions,
                    Some(f.key.as_str()),
                    &[
                        "player_only",
                        "occupied",
                        "has_open_obligation",
                        "max_order",
                        "min_order",
                    ],
                );
                requires.player_only = conditions
                    .get("player_only")
                    .and_then(|v| v.as_bool().ok())
                    .unwrap_or(false);
                requires.occupied = conditions
                    .get("occupied")
                    .and_then(|v| v.as_bool().ok())
                    .unwrap_or(false);
                requires.has_open_obligation = conditions
                    .get("has_open_obligation")
                    .and_then(|v| v.as_bool().ok())
                    .unwrap_or(false);
                requires.max_order = conditions
                    .get("max_order")
                    .and_then(|v| v.as_int().ok())
                    .map(|v| v as i32);
                requires.min_order = conditions
                    .get("min_order")
                    .and_then(|v| v.as_int().ok())
                    .map(|v| v as i32);
            }
        }
    }

    let mut choices = Vec::new();
    let choices_raw = f.take_raw("choices");
    if let Some(raw) = choices_raw {
        match raw.try_cast::<rhai::Array>() {
            None => f.error("choices must be an array"),
            Some(array) => {
                for element in array {
                    let Some(choice) = element.try_cast::<Map>() else {
                        f.error("each choice must be a map");
                        continue;
                    };
                    let key_str = f.key_str();
                    let id = req_str(f.state, Some(&key_str), &choice, "id").and_then(|raw| {
                        match ContentKey::new(&raw) {
                            Ok(parsed) => Some(parsed),
                            Err(err) => {
                                f.state.error(Some(&raw), err.to_string());
                                None
                            }
                        }
                    });
                    let Some(id) = id else {
                        continue;
                    };
                    reject_authored_text(f.state, &key_str, &choice, "label");
                    let effect_fn = opt_str(f.state, Some(&key_str), &choice, "effect_fn")
                        .flatten()
                        .map(|name| ScriptFnRef {
                            path: f.state.current_path.clone(),
                            name,
                        });
                    choices.push(EventChoiceDef {
                        id,
                        label: String::new(),
                        effect_fn,
                    });
                }
            }
        }
    }
    if weighty && choices.is_empty() {
        f.error("a weighty event must offer at least one choice");
        return;
    }
    let (state, key) = f.finish();
    if state.events.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate event id");
        return;
    }
    state.events.insert(
        key.clone(),
        EventDef {
            key,
            title,
            family,
            weight: weight.max(1) as u32,
            cooldown_days: cooldown.max(0) as u32,
            weighty,
            text,
            log_text,
            requires,
            choices,
            effect_fn,
        },
    );
}

fn define_plan(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let title = f.moved_to_table("title", "plan");
    let summary = f.moved_to_table("summary", "plan");
    let Some(goal) = f.req_enum(
        "goal",
        &[
            ("routine", AiIntent::Routine),
            ("order", AiIntent::Order),
            ("muster", AiIntent::Muster),
            ("obligation", AiIntent::Obligation),
            ("resources", AiIntent::Resources),
            ("standing", AiIntent::Standing),
            ("claim", AiIntent::Claim),
        ],
    ) else {
        return;
    };
    let Some(score_bonus) = f.opt_int("score_bonus", 0) else {
        return;
    };
    // A plan aims at coarse things only; the finer target kinds belong to
    // the assignments its steps start.
    let Some(target) = f.opt_enum(
        "target",
        &[
            ("none", AssignmentTargetKind::None),
            ("organisation", AssignmentTargetKind::Organisation),
            ("province", AssignmentTargetKind::Province),
        ],
        AssignmentTargetKind::None,
    ) else {
        return;
    };
    let Some(cooldown_days) = f.opt_int("cooldown_days", 0) else {
        return;
    };
    if !(0..=100_000).contains(&cooldown_days) {
        f.error(format!(
            "cooldown_days must be 0..=100000, got {cooldown_days}"
        ));
        return;
    }
    let Some(max_days) = f.req_int("max_days") else {
        return;
    };
    if !(1..=100_000).contains(&max_days) {
        f.error(format!("max_days must be 1..=100000, got {max_days}"));
        return;
    }
    let Some(max_step_retries) = f.opt_int("max_step_retries", 1) else {
        return;
    };
    if !(0..=100).contains(&max_step_retries) {
        f.error(format!(
            "max_step_retries must be 0..=100, got {max_step_retries}"
        ));
        return;
    }

    let Some(raw_methods) = f.take_raw("methods") else {
        f.error("missing required field 'methods'");
        return;
    };
    let Some(list) = raw_methods.try_cast::<rhai::Array>() else {
        f.error("methods must be a list");
        return;
    };
    let mut methods: Vec<PlanMethodDef> = Vec::new();
    for entry in list {
        let Some(method) = plan_method(&mut f, entry) else {
            return;
        };
        if methods.iter().any(|m| m.id == method.id) {
            f.error(format!("duplicate method id '{}'", method.id));
            return;
        }
        methods.push(method);
    }
    if methods.is_empty() {
        f.error("a plan needs at least one method");
        return;
    }

    let (state, key) = f.finish();
    if state.plans.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate plan id");
        return;
    }
    state.plans.insert(
        key.clone(),
        PlanDef {
            key,
            title,
            summary,
            goal,
            score_bonus,
            target,
            cooldown_days: cooldown_days as u32,
            max_days: max_days as u32,
            max_step_retries: max_step_retries as u32,
            methods,
        },
    );
}

/// Reads one of a plan's methods: a gate and its ordered steps.
fn plan_method(f: &mut Fields, entry: rhai::Dynamic) -> Option<PlanMethodDef> {
    let Some(map) = entry.try_cast::<Map>() else {
        f.error("each method must be a map");
        return None;
    };
    warn_unknown_fields(
        f.state,
        &map,
        Some(f.key.as_str()),
        &["id", "requires", "steps"],
    );
    let key_str = f.key_str();
    let id = req_str(f.state, Some(&key_str), &map, "id")?;
    let requires = match map.get("requires") {
        None => PlanRequires::default(),
        Some(raw) => plan_requires(f, raw.clone())?,
    };
    let Some(raw_steps) = map.get("steps").cloned() else {
        f.error(format!("method '{id}' needs steps"));
        return None;
    };
    let Some(list) = raw_steps.try_cast::<rhai::Array>() else {
        f.error(format!("method '{id}' steps must be a list"));
        return None;
    };
    let mut steps: Vec<PlanStepDef> = Vec::new();
    for entry in list {
        let step = plan_step(f, entry)?;
        if steps.iter().any(|s| s.id == step.id) {
            f.error(format!("method '{id}' repeats step id '{}'", step.id));
            return None;
        }
        steps.push(step);
    }
    if steps.is_empty() {
        f.error(format!("method '{id}' needs at least one step"));
        return None;
    }
    Some(PlanMethodDef {
        id,
        requires,
        steps,
    })
}

/// Reads one step: an assignment to start or a sub-plan to expand.
fn plan_step(f: &mut Fields, entry: rhai::Dynamic) -> Option<PlanStepDef> {
    let Some(map) = entry.try_cast::<Map>() else {
        f.error("each step must be a map");
        return None;
    };
    warn_unknown_fields(
        f.state,
        &map,
        Some(f.key.as_str()),
        // "start" rather than "do": `do` is a reserved word in Rhai, and a
        // map literal keyed with it fails to parse.
        &["id", "start", "plan", "orders", "army", "target", "skip_if"],
    );
    let key_str = f.key_str();
    let doing = opt_key(f.state, Some(&key_str), &map, "start")?;
    let sub = opt_key(f.state, Some(&key_str), &map, "plan")?;
    let ordering = map.contains_key("orders");
    if ordering {
        if doing.is_some() || sub.is_some() {
            f.error("a step is exactly one of 'start', 'plan', or 'orders'");
            return None;
        }
        let orders = key_list(f.state, Some(&key_str), &map, "orders")?;
        if orders.is_empty() {
            f.error("an orders step needs at least one standing order");
            return None;
        }
        let army = match map
            .get("army")
            .and_then(|v| v.clone().into_string().ok())
            .as_deref()
        {
            None | Some("own") => PlanArmySelector::Own,
            Some(other) => {
                f.error(format!("unknown step army '{other}' (expected own)"));
                return None;
            }
        };
        if map.contains_key("target") {
            f.error("an orders step takes no target");
            return None;
        }
        let id = match opt_str(f.state, Some(&key_str), &map, "id")? {
            Some(id) => id,
            None => "orders".to_owned(),
        };
        let skip_if = match map.get("skip_if") {
            None => None,
            Some(raw) => Some(plan_requires(f, raw.clone())?),
        };
        return Some(PlanStepDef {
            id,
            action: PlanStepAction::Orders { army, orders },
            skip_if,
        });
    }
    let action = match (doing, sub) {
        (Some(assignment), None) => {
            let target = match map
                .get("target")
                .and_then(|v| v.clone().into_string().ok())
                .as_deref()
            {
                None | Some("none") => PlanTargetSelector::None,
                Some("plan") => PlanTargetSelector::PlanTarget,
                Some("worst-holding") => PlanTargetSelector::WorstHolding,
                Some(other) => {
                    f.error(format!(
                        "unknown step target '{other}' (expected none, plan, worst-holding)"
                    ));
                    return None;
                }
            };
            PlanStepAction::Assignment {
                key: assignment,
                target,
            }
        }
        (None, Some(plan)) => {
            if map.contains_key("target") {
                f.error("a sub-plan step takes no target");
                return None;
            }
            PlanStepAction::SubPlan(plan)
        }
        (Some(_), Some(_)) => {
            f.error("a step is exactly one of 'start', 'plan', or 'orders'");
            return None;
        }
        (None, None) => {
            f.error(
                "a step needs 'start' (an assignment), 'plan' (a sub-plan), \
                 or 'orders' (standing orders for an army)",
            );
            return None;
        }
    };
    let id = match opt_str(f.state, Some(&key_str), &map, "id")? {
        Some(id) => id,
        None => match &action {
            PlanStepAction::Assignment { key, .. } => key.to_string(),
            PlanStepAction::SubPlan(key) => key.to_string(),
            PlanStepAction::Orders { .. } => "orders".to_owned(),
        },
    };
    let skip_if = match map.get("skip_if") {
        None => None,
        Some(raw) => Some(plan_requires(f, raw.clone())?),
    };
    Some(PlanStepDef {
        id,
        action,
        skip_if,
    })
}

/// Reads a plan's declarative conditions.
///
/// Every field defaults to "do not care", the same contract
/// [`assignment_requires`] keeps: a gate that says nothing gates nothing.
fn plan_requires(f: &mut Fields, raw: rhai::Dynamic) -> Option<PlanRequires> {
    let Some(map) = raw.try_cast::<Map>() else {
        f.error("plan conditions must be a map");
        return None;
    };
    warn_unknown_fields(
        f.state,
        &map,
        Some(f.key.as_str()),
        &[
            "min_wealth",
            "min_manpower",
            "min_influence",
            "min_legitimacy",
            "has_army",
            "target_owes_favour",
            "dominant_claimant",
        ],
    );
    let int = |name: &str| map.get(name).and_then(|v| v.as_int().ok());
    let flag = |name: &str| {
        map.get(name)
            .and_then(|v| v.as_bool().ok())
            .unwrap_or(false)
    };
    Some(PlanRequires {
        min_wealth: int("min_wealth"),
        min_manpower: int("min_manpower"),
        min_influence: int("min_influence"),
        min_legitimacy: int("min_legitimacy").map(|v| v as i32),
        has_army: map.get("has_army").and_then(|v| v.as_bool().ok()),
        target_owes_favour: flag("target_owes_favour"),
        dominant_claimant: flag("dominant_claimant"),
    })
}

/// The authored spellings of every [`AiIntent`], shared by the goal and
/// directive builders so a pressure is named one way everywhere.
const INTENT_SPELLINGS: &[(&str, AiIntent)] = &[
    ("routine", AiIntent::Routine),
    ("order", AiIntent::Order),
    ("muster", AiIntent::Muster),
    ("obligation", AiIntent::Obligation),
    ("resources", AiIntent::Resources),
    ("standing", AiIntent::Standing),
    ("claim", AiIntent::Claim),
];

fn parse_intent(raw: &str) -> Option<AiIntent> {
    INTENT_SPELLINGS
        .iter()
        .find(|(name, _)| *name == raw)
        .map(|(_, intent)| *intent)
}

fn define_goal(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let title = f.moved_to_table("title", "goal");
    let summary = f.moved_to_table("summary", "goal");

    let Some(favour_names) = f.string_list("favours") else {
        return;
    };
    if favour_names.is_empty() {
        f.error("a goal must favour at least one pressure");
        return;
    }
    let mut favours = Vec::with_capacity(favour_names.len());
    for name in &favour_names {
        let Some(intent) = parse_intent(name) else {
            f.error(format!(
                "unknown favoured pressure '{name}' (expected {})",
                INTENT_SPELLINGS
                    .iter()
                    .map(|(n, _)| *n)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            return;
        };
        favours.push(intent);
    }

    let Some(favour_bonus) = f.opt_int("favour_bonus", 0) else {
        return;
    };
    // A goal aims at coarse things only, like a plan.
    let Some(target) = f.opt_enum(
        "target",
        &[
            ("none", AssignmentTargetKind::None),
            ("organisation", AssignmentTargetKind::Organisation),
            ("province", AssignmentTargetKind::Province),
        ],
        AssignmentTargetKind::None,
    ) else {
        return;
    };
    let Some(max_days) = f.req_int("max_days") else {
        return;
    };
    if !(1..=1_000_000).contains(&max_days) {
        f.error(format!("max_days must be 1..=1000000, got {max_days}"));
        return;
    }
    let Some(cooldown_days) = f.opt_int("cooldown_days", 0) else {
        return;
    };
    if !(0..=1_000_000).contains(&cooldown_days) {
        f.error(format!(
            "cooldown_days must be 0..=1000000, got {cooldown_days}"
        ));
        return;
    }

    let trigger = match f.take_raw("trigger") {
        None => GoalRequires::default(),
        Some(raw) => {
            let Some(req) = goal_requires(&mut f, raw) else {
                return;
            };
            req
        }
    };

    let mut directives: Vec<DirectiveDef> = Vec::new();
    if let Some(raw) = f.take_raw("directives") {
        let Some(list) = raw.try_cast::<rhai::Array>() else {
            f.error("directives must be a list");
            return;
        };
        for entry in list {
            let Some(directive) = goal_directive(&mut f, entry, target) else {
                return;
            };
            directives.push(directive);
        }
    }

    let (state, key) = f.finish();
    if state.goals.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate goal id");
        return;
    }
    state.goals.insert(
        key.clone(),
        GoalDef {
            key,
            title,
            summary,
            trigger,
            favours,
            favour_bonus,
            target,
            directives,
            max_days: max_days as u32,
            cooldown_days: cooldown_days as u32,
        },
    );
}

/// Reads one of a goal's advisory directives.
///
/// A directive that carries the goal's target down needs the goal to
/// have one — the same rule a plan step aiming at "plan" keeps.
fn goal_directive(
    f: &mut Fields,
    entry: rhai::Dynamic,
    goal_target: AssignmentTargetKind,
) -> Option<DirectiveDef> {
    let Some(map) = entry.try_cast::<Map>() else {
        f.error("each directive must be a map");
        return None;
    };
    warn_unknown_fields(f.state, &map, Some(f.key.as_str()), &["intent", "target"]);
    let key_str = f.key_str();
    let raw = req_str(f.state, Some(&key_str), &map, "intent")?;
    let Some(intent) = parse_intent(&raw) else {
        f.error(format!("unknown directive intent '{raw}'"));
        return None;
    };
    let target = match map
        .get("target")
        .and_then(|v| v.clone().into_string().ok())
        .as_deref()
    {
        None | Some("none") => DirectiveTarget::None,
        Some("goal") => {
            if goal_target == AssignmentTargetKind::None {
                f.error("a directive aims at the goal's target, but the goal has none");
                return None;
            }
            DirectiveTarget::GoalTarget
        }
        Some(other) => {
            f.error(format!(
                "unknown directive target '{other}' (expected none, goal)"
            ));
            return None;
        }
    };
    Some(DirectiveDef { intent, target })
}

/// Reads a goal's trigger conditions.
///
/// Every field defaults to "do not care", the same contract
/// [`plan_requires`] keeps.
fn goal_requires(f: &mut Fields, raw: rhai::Dynamic) -> Option<GoalRequires> {
    let Some(map) = raw.try_cast::<Map>() else {
        f.error("goal conditions must be a map");
        return None;
    };
    warn_unknown_fields(
        f.state,
        &map,
        Some(f.key.as_str()),
        &[
            "min_wealth",
            "min_manpower",
            "min_legitimacy",
            "has_army",
            "dominant_claimant",
            "has_vassals",
            "is_vassal",
        ],
    );
    let int = |name: &str| map.get(name).and_then(|v| v.as_int().ok());
    let flag = |name: &str| map.get(name).and_then(|v| v.as_bool().ok());
    Some(GoalRequires {
        min_wealth: int("min_wealth"),
        min_manpower: int("min_manpower"),
        min_legitimacy: int("min_legitimacy").map(|v| v as i32),
        has_army: flag("has_army"),
        dominant_claimant: flag("dominant_claimant").unwrap_or(false),
        has_vassals: flag("has_vassals"),
        is_vassal: flag("is_vassal"),
    })
}

fn define_obligation(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let Some(kind) = f.req_enum(
        "kind",
        &[
            ("favour", ObligationKind::Favour),
            ("promise", ObligationKind::Promise),
            ("grievance", ObligationKind::Grievance),
        ],
    ) else {
        return;
    };
    let (Some(debtor), Some(creditor)) = (f.req_key_field("debtor"), f.req_key_field("creditor"))
    else {
        return;
    };
    if debtor == creditor {
        f.error("an obligation needs two different parties");
        return;
    }
    let Some(origin) = f.req_str("origin") else {
        return;
    };
    let Some(weight) = f.opt_int("weight", 20) else {
        return;
    };
    let days = match f.take_raw("days") {
        None => None,
        Some(value) => match value.as_int() {
            Ok(days) if days > 0 => Some(days),
            Ok(_) => {
                f.error("days must be positive when given");
                return;
            }
            Err(_) => {
                f.error("field 'days' must be an integer");
                return;
            }
        },
    };
    let (state, key) = f.finish();
    if state.obligations.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate obligation id");
        return;
    }
    state.obligations.insert(
        key.clone(),
        ObligationDef {
            key,
            kind,
            debtor,
            creditor,
            origin,
            weight: weight.max(0) as i32,
            days,
        },
    );
}

fn define_title(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "title");
    let Some(kind_raw) = f.req_str("kind") else {
        return;
    };
    let kind = match kind_raw.as_str() {
        "paramount" => {
            let Some(body) = f.req_key_field("body") else {
                return;
            };
            TitleKindDef::Paramount { body }
        }
        "consul" => {
            f.read.insert("body");
            TitleKindDef::Consul
        }
        other => {
            f.error(format!(
                "unknown title kind '{other}' (expected paramount or consul)"
            ));
            return;
        }
    };
    let (Some(holder_org), Some(holder_character)) =
        (f.opt_key("holder_org"), f.opt_key("holder_character"))
    else {
        return;
    };
    let holder = match (holder_org, holder_character) {
        (Some(_), Some(_)) => {
            f.error("a title declares holder_org or holder_character, not both");
            return;
        }
        (Some(org), None) => TitleHolderDef::Organisation(org),
        (None, Some(character)) => TitleHolderDef::Character(character),
        (None, None) => TitleHolderDef::Vacant,
    };
    let (state, key) = f.finish();
    if state.titles.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate title id");
        return;
    }
    state.titles.insert(
        key.clone(),
        TitleDef {
            key,
            name,
            kind,
            holder,
        },
    );
}

fn define_office(state: &mut BuilderState, map: Map) {
    let Some(mut f) = Fields::begin(state, map) else {
        return;
    };
    let name = f.moved_to_table("name", "office");
    let Some(organisation) = f.req_key_field("organisation") else {
        return;
    };
    let (Some(province), Some(holder)) = (f.opt_key("province"), f.opt_key("holder")) else {
        return;
    };
    let (state, key) = f.finish();
    if state.offices.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate office id");
        return;
    }
    state.offices.insert(
        key.clone(),
        OfficeDef {
            key,
            name,
            organisation,
            province,
            holder,
        },
    );
}

// ---------------------------------------------------------------------------
// Engine registration
// ---------------------------------------------------------------------------

/// Builds the loading engine with `define_*` functions bound to `state`.
pub(super) fn loading_engine(state: Arc<Mutex<BuilderState>>) -> Engine {
    let mut engine = super::sandboxed_engine();

    let print_state = state.clone();
    engine.on_print(move |text| {
        let mut s = print_state.lock().expect("builder state lock");
        let path = s.current_path.clone();
        s.report.info(&path, format!("print: {text}"));
    });
    engine.on_debug(|_, _, _| {});

    macro_rules! register {
        ($name:literal, $builder:ident) => {
            let builder_state = state.clone();
            engine.register_fn($name, move |map: Map| {
                let mut s = builder_state.lock().expect("builder state lock");
                $builder(&mut s, map);
            });
        };
    }
    register!("define_assignment", define_assignment);
    register!("define_body", define_body);
    register!("define_good", define_good);
    register!("define_building", define_building);
    register!("define_province", define_province);
    register!("define_scenario", define_scenario);
    register!("define_trait", define_trait);
    register!("define_character", define_character);
    register!("define_house", define_house);
    register!("define_organisation", define_organisation);
    register!("define_title", define_title);
    register!("define_office", define_office);
    register!("define_name_pool", define_name_pool);
    register!("define_ship", define_ship);
    register!("define_army", define_army);
    register!("define_obligation", define_obligation);
    register!("define_event", define_event);
    register!("define_plan", define_plan);
    register!("define_goal", define_goal);
    engine
}
