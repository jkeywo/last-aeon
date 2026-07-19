//! The sandboxed Rhai host: content loading and runtime function calls.
//!
//! Two engines share one sandbox profile but differ in surface:
//!
//! - the *loading* engine adds the `define_*` builder functions and runs
//!   each file's top level once, collecting definitions;
//! - the *runtime* engine has no builder functions and never re-runs top
//!   level; it only calls named functions retained in the compiled ASTs.
//!
//! The sandbox is deny-by-default for anything nondeterministic or
//! stateful: no imports, no `eval`, no wall-clock, integer-only arithmetic
//! (the crate builds Rhai with `no_float`), and hard operation, size, and
//! recursion limits. Scripts read the context they are handed and return
//! effect data; they cannot reach simulation state at all.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use aeon_core::hash::hash_bytes;
use rhai::{AST, Dynamic, Engine, Map, Scope};

use crate::effect::{EffectParseError, ScriptEffect, parse_effects};
use crate::key::ContentKey;
use crate::model::{
    AiIntent, ArmyDef, BodyDef, BodyKind, CharacterDef, ContentSet, EventChoiceDef, EventDef,
    EventFamily, EventRequires, Gender, GoverningSkill, HouseTier, JobCategory, JobDef,
    JobResultDef, JobResultKind, JobTargetKind, MilitaryOp, NamePoolDef, ObligationDef, OfficeDef,
    OrgDef, OrgKind, PopupChoiceDef, ProvinceDef, RiskTag, ScenarioDef, ScriptFnRef, ShipClass,
    ShipDef, SkillsDef, TitleDef, TitleHolderDef, TitleKindDef, TraitDef,
};
use crate::report::{ContentReport, Severity};

/// One authored source file, path-relative to the content root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentSource {
    /// Content-relative path with forward slashes, e.g. `core/jobs.rhai`.
    pub path: String,
    /// The Rhai source text.
    pub source: String,
}

/// Builds a sandboxed engine from an allow-list of language packages.
///
/// Starting from a raw engine means capabilities are opt-in: no wall-clock
/// (`timestamp` is simply absent), no I/O, no imports, no `eval`, and hard
/// operation, size, and recursion limits. Only deterministic language
/// features are registered.
fn sandboxed_engine() -> Engine {
    use rhai::packages::{
        ArithmeticPackage, BasicArrayPackage, BasicFnPackage, BasicIteratorPackage,
        BasicMapPackage, BasicMathPackage, BasicStringPackage, LanguageCorePackage, LogicPackage,
        MoreStringPackage, Package,
    };

    let mut engine = Engine::new_raw();
    engine.register_global_module(LanguageCorePackage::new().as_shared_module());
    engine.register_global_module(ArithmeticPackage::new().as_shared_module());
    engine.register_global_module(LogicPackage::new().as_shared_module());
    engine.register_global_module(BasicStringPackage::new().as_shared_module());
    engine.register_global_module(MoreStringPackage::new().as_shared_module());
    engine.register_global_module(BasicIteratorPackage::new().as_shared_module());
    engine.register_global_module(BasicArrayPackage::new().as_shared_module());
    engine.register_global_module(BasicMapPackage::new().as_shared_module());
    engine.register_global_module(BasicMathPackage::new().as_shared_module());
    engine.register_global_module(BasicFnPackage::new().as_shared_module());

    engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
    engine.disable_symbol("eval");
    engine.set_max_operations(5_000_000);
    engine.set_max_call_levels(64);
    engine.set_max_string_size(65_536);
    engine.set_max_array_size(65_536);
    engine.set_max_map_size(65_536);
    engine.set_max_expr_depths(128, 64);
    engine
}

/// Shared mutable state while the loading engine runs definition passes.
#[derive(Default)]
struct BuilderState {
    current_path: String,
    report: ContentReport,
    jobs: BTreeMap<ContentKey, JobDef>,
    bodies: BTreeMap<ContentKey, BodyDef>,
    provinces: BTreeMap<ContentKey, ProvinceDef>,
    traits: BTreeMap<ContentKey, TraitDef>,
    name_pools: BTreeMap<ContentKey, NamePoolDef>,
    characters: BTreeMap<ContentKey, CharacterDef>,
    organisations: BTreeMap<ContentKey, OrgDef>,
    titles: BTreeMap<ContentKey, TitleDef>,
    offices: BTreeMap<ContentKey, OfficeDef>,
    ships: BTreeMap<ContentKey, ShipDef>,
    armies: BTreeMap<ContentKey, ArmyDef>,
    obligations: BTreeMap<ContentKey, ObligationDef>,
    events: BTreeMap<ContentKey, EventDef>,
    scenario: Option<ScenarioDef>,
}

impl BuilderState {
    fn error(&mut self, key: Option<&str>, message: impl Into<String>) {
        let path = self.current_path.clone();
        self.report.error(&path, key, message);
    }
}

/// Reads a required string field from a definition map.
fn req_str(state: &mut BuilderState, map: &Map, field: &str) -> Option<String> {
    match map.get(field) {
        Some(value) => match value.clone().into_string() {
            Ok(text) => Some(text),
            Err(_) => {
                state.error(None, format!("field '{field}' must be a string"));
                None
            }
        },
        None => {
            state.error(None, format!("missing required field '{field}'"));
            None
        }
    }
}

/// Reads a required integer field from a definition map.
fn req_int(state: &mut BuilderState, map: &Map, field: &str) -> Option<i64> {
    match map.get(field) {
        Some(value) => match value.as_int() {
            Ok(int) => Some(int),
            Err(_) => {
                state.error(None, format!("field '{field}' must be an integer"));
                None
            }
        },
        None => {
            state.error(None, format!("missing required field '{field}'"));
            None
        }
    }
}

/// Reads an optional integer field with a default.
fn opt_int(state: &mut BuilderState, map: &Map, field: &str, default: i64) -> Option<i64> {
    match map.get(field) {
        Some(value) => match value.as_int() {
            Ok(int) => Some(int),
            Err(_) => {
                state.error(None, format!("field '{field}' must be an integer"));
                None
            }
        },
        None => Some(default),
    }
}

/// Reads an optional boolean field with a default.
fn opt_bool(state: &mut BuilderState, map: &Map, field: &str, default: bool) -> Option<bool> {
    match map.get(field) {
        Some(value) => match value.as_bool() {
            Ok(b) => Some(b),
            Err(_) => {
                state.error(None, format!("field '{field}' must be a boolean"));
                None
            }
        },
        None => Some(default),
    }
}

/// Reads and validates the `id` field of a definition map.
fn req_key(state: &mut BuilderState, map: &Map) -> Option<ContentKey> {
    let raw = req_str(state, map, "id")?;
    match ContentKey::new(&raw) {
        Ok(key) => Some(key),
        Err(err) => {
            state.error(Some(&raw), err.to_string());
            None
        }
    }
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

fn define_job(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "ai_intent",
            "id",
            "title",
            "summary",
            "category",
            "duration_days",
            "skill",
            "difficulty",
            "target",
            "risks",
            "military_op",
            "ai_available",
            "wealth_cost",
            "manpower_cost",
            "supplies_cost",
            "influence_cost",
            "results",
        ],
    );
    let Some(title) = req_str(state, &map, "title") else {
        return;
    };
    let Some(summary) = req_str(state, &map, "summary") else {
        return;
    };
    let Some(category_raw) = req_str(state, &map, "category") else {
        return;
    };
    let category = match category_raw.as_str() {
        "routine" => JobCategory::Routine,
        "consequential" => JobCategory::Consequential,
        other => {
            state.error(
                Some(key.as_str()),
                format!("unknown category '{other}' (expected routine or consequential)"),
            );
            return;
        }
    };
    let Some(duration_days) = req_int(state, &map, "duration_days") else {
        return;
    };
    if !(1..=100_000).contains(&duration_days) {
        state.error(
            Some(key.as_str()),
            format!("duration_days must be 1..=100000, got {duration_days}"),
        );
        return;
    }

    let Some(skill_raw) = req_str(state, &map, "skill") else {
        return;
    };
    let skill = match skill_raw.as_str() {
        "command" => GoverningSkill::Command,
        "diplomacy" => GoverningSkill::Diplomacy,
        "intrigue" => GoverningSkill::Intrigue,
        "stewardship" => GoverningSkill::Stewardship,
        other => {
            state.error(
                Some(key.as_str()),
                format!(
                    "unknown skill '{other}' (expected command, diplomacy, intrigue, stewardship)"
                ),
            );
            return;
        }
    };
    let Some(difficulty) = req_int(state, &map, "difficulty") else {
        return;
    };
    if !(0..=40).contains(&difficulty) {
        state.error(Some(key.as_str()), "difficulty must be 0..=40");
        return;
    }
    let target = match map.get("target") {
        None => JobTargetKind::None,
        Some(value) => match value.clone().into_string() {
            Ok(raw) => match raw.as_str() {
                "none" => JobTargetKind::None,
                "character" => JobTargetKind::Character,
                "organisation" => JobTargetKind::Organisation,
                "province" => JobTargetKind::Province,
                "own-army" => JobTargetKind::OwnArmy,
                "own-army-and-province" => JobTargetKind::OwnArmyAndProvince,
                "own-ship-and-province" => JobTargetKind::OwnShipAndProvince,
                other => {
                    state.error(Some(key.as_str()), format!("unknown target kind '{other}'"));
                    return;
                }
            },
            Err(_) => {
                state.error(Some(key.as_str()), "field 'target' must be a string");
                return;
            }
        },
    };
    let Some(risk_names) = string_list(state, &map, "risks") else {
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
                state.error(Some(key.as_str()), format!("unknown risk '{other}'"));
                return;
            }
        };
        risks.push(tag);
    }
    risks.sort();
    risks.dedup();
    let military_op = match map.get("military_op") {
        None => None,
        Some(value) => match value.clone().into_string() {
            Ok(raw) => match raw.as_str() {
                "move" => Some(MilitaryOp::Move),
                "resupply" => Some(MilitaryOp::Resupply),
                "patrol" => Some(MilitaryOp::Patrol),
                "besiege" => Some(MilitaryOp::Besiege),
                "raid" => Some(MilitaryOp::Raid),
                "blockade" => Some(MilitaryOp::Blockade),
                other => {
                    state.error(Some(key.as_str()), format!("unknown military_op '{other}'"));
                    return;
                }
            },
            Err(_) => {
                state.error(Some(key.as_str()), "field 'military_op' must be a string");
                return;
            }
        },
    };
    let op_target_ok = match military_op {
        None => true,
        Some(MilitaryOp::Resupply | MilitaryOp::Patrol) => target == JobTargetKind::OwnArmy,
        Some(MilitaryOp::Move | MilitaryOp::Besiege | MilitaryOp::Raid) => {
            target == JobTargetKind::OwnArmyAndProvince
        }
        Some(MilitaryOp::Blockade) => target == JobTargetKind::OwnShipAndProvince,
    };
    if !op_target_ok {
        state.error(
            Some(key.as_str()),
            "military_op and target kind do not match",
        );
        return;
    }
    let ai_intent = match map
        .get("ai_intent")
        .and_then(|v| v.clone().into_string().ok())
        .unwrap_or_else(|| "routine".to_owned())
        .as_str()
    {
        "routine" => AiIntent::Routine,
        "order" => AiIntent::Order,
        "muster" => AiIntent::Muster,
        "obligation" => AiIntent::Obligation,
        "resources" => AiIntent::Resources,
        "standing" => AiIntent::Standing,
        "claim" => AiIntent::Claim,
        other => {
            state.error(
                Some(key.as_str()),
                format!(
                    "unknown ai_intent '{other}' (expected routine, order, muster,                      obligation, resources, standing, claim)"
                ),
            );
            return;
        }
    };
    let Some(ai_available) = opt_bool(state, &map, "ai_available", true) else {
        return;
    };
    let (Some(wealth_cost), Some(manpower_cost), Some(supplies_cost), Some(influence_cost)) = (
        opt_int(state, &map, "wealth_cost", 0),
        opt_int(state, &map, "manpower_cost", 0),
        opt_int(state, &map, "supplies_cost", 0),
        opt_int(state, &map, "influence_cost", 0),
    ) else {
        return;
    };

    let Some(results_value) = map.get("results") else {
        state.error(Some(key.as_str()), "missing required field 'results'");
        return;
    };
    let Some(results_map) = results_value.clone().try_cast::<Map>() else {
        state.error(Some(key.as_str()), "field 'results' must be a map");
        return;
    };

    let mut results = BTreeMap::new();
    for (result_name, kind) in [
        ("critical_success", JobResultKind::CriticalSuccess),
        ("success", JobResultKind::Success),
        ("failure", JobResultKind::Failure),
        ("disaster", JobResultKind::Disaster),
    ] {
        let Some(entry) = results_map.get(result_name) else {
            continue;
        };
        let Some(entry_map) = entry.clone().try_cast::<Map>() else {
            state.error(
                Some(key.as_str()),
                format!("result '{result_name}' must be a map"),
            );
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
        let Some(weight) = req_int(state, &entry_map, "weight") else {
            continue;
        };
        if !(1..=1_000_000).contains(&weight) {
            state.error(
                Some(key.as_str()),
                format!("result '{result_name}' weight must be 1..=1000000, got {weight}"),
            );
            continue;
        }
        let Some(popup) = opt_bool(state, &entry_map, "popup", false) else {
            continue;
        };
        let Some(log) = opt_bool(state, &entry_map, "log", false) else {
            continue;
        };
        let effect_fn = match entry_map.get("effect_fn") {
            Some(value) => match value.clone().into_string() {
                Ok(name) => Some(ScriptFnRef {
                    path: state.current_path.clone(),
                    name,
                }),
                Err(_) => {
                    state.error(
                        Some(key.as_str()),
                        format!("result '{result_name}' effect_fn must be a string"),
                    );
                    continue;
                }
            },
            None => None,
        };
        let popup_text = match entry_map.get("popup_text") {
            None => None,
            Some(value) => match value.clone().into_string() {
                Ok(text) => Some(text),
                Err(_) => {
                    state.error(
                        Some(key.as_str()),
                        format!("result '{result_name}' popup_text must be a string"),
                    );
                    continue;
                }
            },
        };
        let log_text = match entry_map.get("log_text") {
            None => None,
            Some(value) => match value.clone().into_string() {
                Ok(text) => Some(text),
                Err(_) => {
                    state.error(
                        Some(key.as_str()),
                        format!("result '{result_name}' log_text must be a string"),
                    );
                    continue;
                }
            },
        };
        let mut choices = Vec::new();
        let mut choices_bad = false;
        if let Some(value) = entry_map.get("choices") {
            let Some(array) = value.clone().try_cast::<rhai::Array>() else {
                state.error(
                    Some(key.as_str()),
                    format!("result '{result_name}' choices must be an array of maps"),
                );
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
                let (Some(choice_id), Some(label)) = (
                    req_key(state, &choice_map),
                    req_str(state, &choice_map, "label"),
                ) else {
                    choices_bad = true;
                    break;
                };
                let choice_effect = match choice_map.get("effect_fn") {
                    Some(value) => match value.clone().into_string() {
                        Ok(name) => Some(ScriptFnRef {
                            path: state.current_path.clone(),
                            name,
                        }),
                        Err(_) => {
                            state.error(
                                Some(key.as_str()),
                                format!("result '{result_name}' choice effect_fn must be a string"),
                            );
                            choices_bad = true;
                            break;
                        }
                    },
                    None => None,
                };
                choices.push(PopupChoiceDef {
                    id: choice_id,
                    label,
                    effect_fn: choice_effect,
                });
            }
        }
        if choices_bad {
            continue;
        }
        results.insert(
            kind,
            JobResultDef {
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
    for name in unknown_results {
        state.error(
            Some(key.as_str()),
            format!("unknown result kind '{name}' (expected critical_success, success, failure, disaster)"),
        );
    }

    if state.jobs.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate job id");
        return;
    }
    state.jobs.insert(
        key.clone(),
        JobDef {
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
            results,
        },
    );
}

fn define_body(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "name",
            "kind",
            "radius_km",
            "orbit_radius_mm",
            "orbit_days",
            "parent",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(kind_raw) = req_str(state, &map, "kind") else {
        return;
    };
    let kind = match kind_raw.as_str() {
        "planet" => BodyKind::Planet,
        "moon" => BodyKind::Moon,
        "starbase" => BodyKind::Starbase,
        other => {
            state.error(
                Some(key.as_str()),
                format!("unknown body kind '{other}' (expected planet, moon, starbase)"),
            );
            return;
        }
    };
    let Some(radius_km) = req_int(state, &map, "radius_km") else {
        return;
    };
    let Some(orbit_radius_mm) = opt_int(state, &map, "orbit_radius_mm", 0) else {
        return;
    };
    let Some(orbit_days) = opt_int(state, &map, "orbit_days", 0) else {
        return;
    };
    if radius_km <= 0 {
        state.error(Some(key.as_str()), "radius_km must be positive");
        return;
    }
    let parent = match map.get("parent") {
        Some(value) => match value.clone().into_string() {
            Ok(raw) => match ContentKey::new(&raw) {
                Ok(parent_key) => Some(parent_key),
                Err(err) => {
                    state.error(Some(key.as_str()), err.to_string());
                    return;
                }
            },
            Err(_) => {
                state.error(Some(key.as_str()), "field 'parent' must be a string");
                return;
            }
        },
        None => None,
    };

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

fn define_province(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "name",
            "body",
            "latitude_mdeg",
            "longitude_mdeg",
            "wealth_output",
            "manpower_output",
            "supplies_output",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(body_raw) = req_str(state, &map, "body") else {
        return;
    };
    let body = match ContentKey::new(&body_raw) {
        Ok(body) => body,
        Err(err) => {
            state.error(Some(key.as_str()), err.to_string());
            return;
        }
    };
    let Some(latitude_mdeg) = req_int(state, &map, "latitude_mdeg") else {
        return;
    };
    let Some(longitude_mdeg) = req_int(state, &map, "longitude_mdeg") else {
        return;
    };
    if !(-90_000..=90_000).contains(&latitude_mdeg) {
        state.error(Some(key.as_str()), "latitude_mdeg must be -90000..=90000");
        return;
    }
    if !(-180_000..180_000).contains(&longitude_mdeg) {
        state.error(Some(key.as_str()), "longitude_mdeg must be -180000..180000");
        return;
    }
    let (Some(wealth_output), Some(manpower_output), Some(supplies_output)) = (
        opt_int(state, &map, "wealth_output", 10),
        opt_int(state, &map, "manpower_output", 10),
        opt_int(state, &map, "supplies_output", 10),
    ) else {
        return;
    };

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
        },
    );
}

fn define_scenario(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "name",
            "start_year",
            "start_month",
            "start_day",
            "player_house",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(start_year) = req_int(state, &map, "start_year") else {
        return;
    };
    let Some(start_month) = req_int(state, &map, "start_month") else {
        return;
    };
    let Some(start_day) = req_int(state, &map, "start_day") else {
        return;
    };
    if !(1..=12).contains(&start_month) || !(1..=30).contains(&start_day) {
        state.error(
            Some(key.as_str()),
            "start_month must be 1..=12 and start_day 1..=30",
        );
        return;
    }
    let Some(player_house) = opt_key(state, &map, "player_house") else {
        return;
    };
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

/// Reads an optional content-key field. `Some(None)` means absent;
/// `None` means present but invalid (already reported).
fn opt_key(state: &mut BuilderState, map: &Map, field: &str) -> Option<Option<ContentKey>> {
    match map.get(field) {
        None => Some(None),
        Some(value) => match value.clone().into_string() {
            Ok(raw) => match ContentKey::new(&raw) {
                Ok(key) => Some(Some(key)),
                Err(err) => {
                    state.error(None, format!("field '{field}': {err}"));
                    None
                }
            },
            Err(_) => {
                state.error(None, format!("field '{field}' must be a string"));
                None
            }
        },
    }
}

/// Reads a required content-key field (other than `id`).
fn req_key_field(state: &mut BuilderState, map: &Map, field: &str) -> Option<ContentKey> {
    match opt_key(state, map, field) {
        Some(Some(key)) => Some(key),
        Some(None) => {
            state.error(None, format!("missing required field '{field}'"));
            None
        }
        None => None,
    }
}

/// Reads an optional list of content keys (defaults to empty).
fn key_list(state: &mut BuilderState, map: &Map, field: &str) -> Option<Vec<ContentKey>> {
    let Some(value) = map.get(field) else {
        return Some(Vec::new());
    };
    let Some(array) = value.clone().try_cast::<rhai::Array>() else {
        state.error(None, format!("field '{field}' must be an array of keys"));
        return None;
    };
    let mut keys = Vec::with_capacity(array.len());
    for element in array {
        let Ok(raw) = element.into_string() else {
            state.error(None, format!("field '{field}' entries must be strings"));
            return None;
        };
        match ContentKey::new(&raw) {
            Ok(key) => keys.push(key),
            Err(err) => {
                state.error(None, format!("field '{field}': {err}"));
                return None;
            }
        }
    }
    Some(keys)
}

fn define_trait(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "name",
            "summary",
            "opinion_same",
            "opinion_opposed",
            "opposites",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(summary) = req_str(state, &map, "summary") else {
        return;
    };
    let Some(opinion_same) = opt_int(state, &map, "opinion_same", 0) else {
        return;
    };
    let Some(opinion_opposed) = opt_int(state, &map, "opinion_opposed", 0) else {
        return;
    };
    let Some(opposites) = key_list(state, &map, "opposites") else {
        return;
    };
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
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "name",
            "gender",
            "birth_year",
            "birth_month",
            "birth_day",
            "organisation",
            "parents",
            "spouse",
            "traits",
            "skills",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(gender_raw) = req_str(state, &map, "gender") else {
        return;
    };
    let gender = match gender_raw.as_str() {
        "male" => Gender::Male,
        "female" => Gender::Female,
        other => {
            state.error(
                Some(key.as_str()),
                format!("unknown gender '{other}' (expected male or female)"),
            );
            return;
        }
    };
    let Some(birth_year) = req_int(state, &map, "birth_year") else {
        return;
    };
    let Some(birth_month) = opt_int(state, &map, "birth_month", 1) else {
        return;
    };
    let Some(birth_day) = opt_int(state, &map, "birth_day", 1) else {
        return;
    };
    if !(1..=12).contains(&birth_month) || !(1..=30).contains(&birth_day) {
        state.error(
            Some(key.as_str()),
            "birth_month must be 1..=12 and birth_day 1..=30",
        );
        return;
    }
    let Some(organisation) = opt_key(state, &map, "organisation") else {
        return;
    };
    let Some(parents) = key_list(state, &map, "parents") else {
        return;
    };
    if parents.len() > 2 {
        state.error(Some(key.as_str()), "characters have at most two parents");
        return;
    }
    let Some(spouse) = opt_key(state, &map, "spouse") else {
        return;
    };
    let Some(traits) = key_list(state, &map, "traits") else {
        return;
    };
    let skills = match map.get("skills") {
        None => SkillsDef::default(),
        Some(value) => {
            let Some(skills_map) = value.clone().try_cast::<Map>() else {
                state.error(Some(key.as_str()), "field 'skills' must be a map");
                return;
            };
            warn_unknown_fields(
                state,
                &skills_map,
                Some(key.as_str()),
                &["command", "diplomacy", "intrigue", "stewardship"],
            );
            let (Some(command), Some(diplomacy), Some(intrigue), Some(stewardship)) = (
                opt_int(state, &skills_map, "command", 0),
                opt_int(state, &skills_map, "diplomacy", 0),
                opt_int(state, &skills_map, "intrigue", 0),
                opt_int(state, &skills_map, "stewardship", 0),
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

fn org_color(state: &mut BuilderState, map: &Map, key: &ContentKey) -> Option<(u8, u8, u8)> {
    let Some(value) = map.get("color") else {
        state.error(Some(key.as_str()), "missing required field 'color'");
        return None;
    };
    let Some(array) = value.clone().try_cast::<rhai::Array>() else {
        state.error(Some(key.as_str()), "field 'color' must be [r, g, b]");
        return None;
    };
    if array.len() != 3 {
        state.error(Some(key.as_str()), "field 'color' must be [r, g, b]");
        return None;
    }
    let mut channels = [0u8; 3];
    for (slot, element) in channels.iter_mut().zip(array) {
        match element.as_int() {
            Ok(v) if (0..=255).contains(&v) => *slot = v as u8,
            _ => {
                state.error(
                    Some(key.as_str()),
                    "colour channels must be integers 0..=255",
                );
                return None;
            }
        }
    }
    Some((channels[0], channels[1], channels[2]))
}

fn insert_org(state: &mut BuilderState, org: OrgDef) {
    if state.organisations.contains_key(&org.key) {
        state.error(Some(org.key.as_str()), "duplicate organisation id");
        return;
    }
    state.organisations.insert(org.key.clone(), org);
}

fn define_house(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "name",
            "surname",
            "tier",
            "liege",
            "head",
            "provinces",
            "color",
            "wealth",
            "manpower",
            "supplies",
            "legitimacy",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(tier_raw) = req_str(state, &map, "tier") else {
        return;
    };
    let tier = match tier_raw.as_str() {
        "great" => HouseTier::Great,
        "vassal" => HouseTier::Vassal,
        "independent" => HouseTier::Independent,
        other => {
            state.error(
                Some(key.as_str()),
                format!("unknown tier '{other}' (expected great, vassal, independent)"),
            );
            return;
        }
    };
    let Some(liege) = opt_key(state, &map, "liege") else {
        return;
    };
    let surname = match map.get("surname") {
        None => None,
        Some(value) => match value.clone().into_string() {
            Ok(text) => Some(text),
            Err(_) => {
                state.error(Some(key.as_str()), "field 'surname' must be a string");
                return;
            }
        },
    };
    let Some(head) = opt_key(state, &map, "head") else {
        return;
    };
    let Some(provinces) = key_list(state, &map, "provinces") else {
        return;
    };
    let Some(color) = org_color(state, &map, &key) else {
        return;
    };
    let Some((wealth, manpower, supplies, legitimacy)) = org_resources(state, &map, &key) else {
        return;
    };
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

/// Reads starting resources and legitimacy for an organisation.
fn org_resources(
    state: &mut BuilderState,
    map: &Map,
    key: &ContentKey,
) -> Option<(i64, i64, i64, i32)> {
    let (Some(wealth), Some(manpower), Some(supplies), Some(legitimacy)) = (
        opt_int(state, map, "wealth", 100),
        opt_int(state, map, "manpower", 1000),
        opt_int(state, map, "supplies", 200),
        opt_int(state, map, "legitimacy", 50),
    ) else {
        return None;
    };
    if !(0..=100).contains(&legitimacy) {
        state.error(Some(key.as_str()), "legitimacy must be 0..=100");
        return None;
    }
    Some((wealth, manpower, supplies, legitimacy as i32))
}

fn string_list(state: &mut BuilderState, map: &Map, field: &str) -> Option<Vec<String>> {
    let Some(value) = map.get(field) else {
        return Some(Vec::new());
    };
    let Some(array) = value.clone().try_cast::<rhai::Array>() else {
        state.error(None, format!("field '{field}' must be an array of strings"));
        return None;
    };
    let mut items = Vec::with_capacity(array.len());
    for element in array {
        match element.into_string() {
            Ok(text) => items.push(text),
            Err(_) => {
                state.error(None, format!("field '{field}' entries must be strings"));
                return None;
            }
        }
    }
    Some(items)
}

fn define_name_pool(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(state, &map, Some(key.as_str()), &["id", "male", "female"]);
    let (Some(male), Some(female)) = (
        string_list(state, &map, "male"),
        string_list(state, &map, "female"),
    ) else {
        return;
    };
    if male.is_empty() || female.is_empty() {
        state.error(
            Some(key.as_str()),
            "name pools need at least one male and one female name",
        );
        return;
    }
    if state.name_pools.contains_key(&key) {
        state.error(Some(key.as_str()), "duplicate name pool id");
        return;
    }
    state
        .name_pools
        .insert(key.clone(), NamePoolDef { key, male, female });
}

fn define_organisation(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "name",
            "kind",
            "head",
            "provinces",
            "color",
            "wealth",
            "manpower",
            "supplies",
            "legitimacy",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(kind_raw) = req_str(state, &map, "kind") else {
        return;
    };
    let kind = match kind_raw.as_str() {
        "sanctora-imperim" => OrgKind::SanctoraImperim,
        "dynastic-house" => {
            state.error(
                Some(key.as_str()),
                "dynastic houses are defined with define_house",
            );
            return;
        }
        other => {
            state.error(
                Some(key.as_str()),
                format!("unknown organisation kind '{other}'"),
            );
            return;
        }
    };
    let Some(head) = opt_key(state, &map, "head") else {
        return;
    };
    let Some(provinces) = key_list(state, &map, "provinces") else {
        return;
    };
    let Some(color) = org_color(state, &map, &key) else {
        return;
    };
    let Some((wealth, manpower, supplies, legitimacy)) = org_resources(state, &map, &key) else {
        return;
    };
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

fn define_ship(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &["id", "name", "class", "owner", "captain", "location"],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(class_raw) = req_str(state, &map, "class") else {
        return;
    };
    let class = match class_raw.as_str() {
        "capital" => ShipClass::Capital,
        "transport" => ShipClass::Transport,
        "patrol" => ShipClass::Patrol,
        other => {
            state.error(
                Some(key.as_str()),
                format!("unknown ship class '{other}' (expected capital, transport, patrol)"),
            );
            return;
        }
    };
    let Some(owner) = req_key_field(state, &map, "owner") else {
        return;
    };
    let Some(captain) = opt_key(state, &map, "captain") else {
        return;
    };
    let Some(location) = req_key_field(state, &map, "location") else {
        return;
    };
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
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id", "name", "owner", "general", "province", "manpower", "supplies",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(owner) = req_key_field(state, &map, "owner") else {
        return;
    };
    let Some(general) = req_key_field(state, &map, "general") else {
        return;
    };
    let Some(province) = req_key_field(state, &map, "province") else {
        return;
    };
    let Some(manpower) = opt_int(state, &map, "manpower", 500) else {
        return;
    };
    let Some(supplies) = opt_int(state, &map, "supplies", 100) else {
        return;
    };
    if manpower <= 0 {
        state.error(Some(key.as_str()), "manpower must be positive");
        return;
    }
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

/// Reads an optional `effect_fn` field, bound to the file being loaded.
fn opt_fn_ref(
    state: &mut BuilderState,
    map: &Map,
    field: &str,
    owner: &str,
) -> Option<ScriptFnRef> {
    match map.get(field) {
        None => None,
        Some(value) => match value.clone().into_string() {
            Ok(name) => Some(ScriptFnRef {
                path: state.current_path.clone(),
                name,
            }),
            Err(_) => {
                state.error(Some(owner), format!("{field} must be a string"));
                None
            }
        },
    }
}

fn define_event(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "title",
            "family",
            "weight",
            "cooldown_days",
            "weighty",
            "text",
            "log_text",
            "requires",
            "choices",
            "effect_fn",
        ],
    );
    let Some(title) = req_str(state, &map, "title") else {
        return;
    };
    let Some(text) = req_str(state, &map, "text") else {
        return;
    };
    let family = match map
        .get("family")
        .and_then(|v| v.clone().into_string().ok())
        .unwrap_or_else(|| "province".to_owned())
        .as_str()
    {
        "province" => EventFamily::Province,
        "political" => EventFamily::Political,
        "travel" => EventFamily::Travel,
        "job" => EventFamily::Job,
        other => {
            state.error(
                Some(key.as_str()),
                format!(
                    "unknown event family '{other}' (expected province, political, travel, job)"
                ),
            );
            return;
        }
    };
    let Some(weight) = opt_int(state, &map, "weight", 100) else {
        return;
    };
    let Some(cooldown) = opt_int(state, &map, "cooldown_days", 720) else {
        return;
    };
    let weighty = map
        .get("weighty")
        .and_then(|v| v.as_bool().ok())
        .unwrap_or(false);
    let log_text = map
        .get("log_text")
        .and_then(|v| v.clone().into_string().ok());
    let effect_fn = opt_fn_ref(state, &map, "effect_fn", key.as_str());

    let mut requires = EventRequires::default();
    if let Some(raw) = map.get("requires") {
        match raw.clone().try_cast::<Map>() {
            None => state.error(Some(key.as_str()), "requires must be a map"),
            Some(conditions) => {
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
    if let Some(raw) = map.get("choices") {
        match raw.clone().try_cast::<rhai::Array>() {
            None => state.error(Some(key.as_str()), "choices must be an array"),
            Some(array) => {
                for element in array {
                    let Some(choice) = element.try_cast::<Map>() else {
                        state.error(Some(key.as_str()), "each choice must be a map");
                        continue;
                    };
                    let Some(id) = req_key(state, &choice) else {
                        continue;
                    };
                    let Some(label) = req_str(state, &choice, "label") else {
                        continue;
                    };
                    let effect_fn = opt_fn_ref(state, &choice, "effect_fn", id.as_str());
                    choices.push(EventChoiceDef {
                        id,
                        label,
                        effect_fn,
                    });
                }
            }
        }
    }
    if weighty && choices.is_empty() {
        state.error(
            Some(key.as_str()),
            "a weighty event must offer at least one choice",
        );
        return;
    }
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

fn define_obligation(state: &mut BuilderState, map: Map) {
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id", "kind", "debtor", "creditor", "origin", "weight", "days",
        ],
    );
    let Some(kind) = req_str(state, &map, "kind") else {
        return;
    };
    let Some(kind) = crate::model::ObligationKind::parse(&kind) else {
        state.error(
            Some(key.as_str()),
            "kind must be favour, promise, or grievance",
        );
        return;
    };
    let Some(debtor) = req_key_field(state, &map, "debtor") else {
        return;
    };
    let Some(creditor) = req_key_field(state, &map, "creditor") else {
        return;
    };
    if debtor == creditor {
        state.error(
            Some(key.as_str()),
            "an obligation needs two different parties",
        );
        return;
    }
    let Some(origin) = req_str(state, &map, "origin") else {
        return;
    };
    let Some(weight) = opt_int(state, &map, "weight", 20) else {
        return;
    };
    let days = match map.get("days") {
        None => None,
        Some(_) => match opt_int(state, &map, "days", 0) {
            Some(days) if days > 0 => Some(days),
            Some(_) => {
                state.error(Some(key.as_str()), "days must be positive when given");
                return;
            }
            None => return,
        },
    };
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
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &[
            "id",
            "name",
            "kind",
            "body",
            "holder_org",
            "holder_character",
        ],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(kind_raw) = req_str(state, &map, "kind") else {
        return;
    };
    let kind = match kind_raw.as_str() {
        "paramount" => {
            let Some(body) = req_key_field(state, &map, "body") else {
                return;
            };
            TitleKindDef::Paramount { body }
        }
        "consul" => TitleKindDef::Consul,
        other => {
            state.error(
                Some(key.as_str()),
                format!("unknown title kind '{other}' (expected paramount or consul)"),
            );
            return;
        }
    };
    let (Some(holder_org), Some(holder_character)) = (
        opt_key(state, &map, "holder_org"),
        opt_key(state, &map, "holder_character"),
    ) else {
        return;
    };
    let holder = match (holder_org, holder_character) {
        (Some(_), Some(_)) => {
            state.error(
                Some(key.as_str()),
                "a title declares holder_org or holder_character, not both",
            );
            return;
        }
        (Some(org), None) => TitleHolderDef::Organisation(org),
        (None, Some(character)) => TitleHolderDef::Character(character),
        (None, None) => TitleHolderDef::Vacant,
    };
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
    let Some(key) = req_key(state, &map) else {
        return;
    };
    warn_unknown_fields(
        state,
        &map,
        Some(key.as_str()),
        &["id", "name", "organisation", "province", "holder"],
    );
    let Some(name) = req_str(state, &map, "name") else {
        return;
    };
    let Some(organisation) = req_key_field(state, &map, "organisation") else {
        return;
    };
    let (Some(province), Some(holder)) = (
        opt_key(state, &map, "province"),
        opt_key(state, &map, "holder"),
    ) else {
        return;
    };
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

/// Builds the loading engine with `define_*` functions bound to `state`.
fn loading_engine(state: Arc<Mutex<BuilderState>>) -> Engine {
    let mut engine = sandboxed_engine();

    let print_state = state.clone();
    engine.on_print(move |text| {
        let mut s = print_state.lock().expect("builder state lock");
        let path = s.current_path.clone();
        s.report.info(&path, format!("print: {text}"));
    });
    engine.on_debug(|_, _, _| {});

    let job_state = state.clone();
    engine.register_fn("define_job", move |map: Map| {
        let mut s = job_state.lock().expect("builder state lock");
        define_job(&mut s, map);
    });
    let body_state = state.clone();
    engine.register_fn("define_body", move |map: Map| {
        let mut s = body_state.lock().expect("builder state lock");
        define_body(&mut s, map);
    });
    let province_state = state.clone();
    engine.register_fn("define_province", move |map: Map| {
        let mut s = province_state.lock().expect("builder state lock");
        define_province(&mut s, map);
    });
    let scenario_state = state.clone();
    engine.register_fn("define_scenario", move |map: Map| {
        let mut s = scenario_state.lock().expect("builder state lock");
        define_scenario(&mut s, map);
    });
    let trait_state = state.clone();
    engine.register_fn("define_trait", move |map: Map| {
        let mut s = trait_state.lock().expect("builder state lock");
        define_trait(&mut s, map);
    });
    let character_state = state.clone();
    engine.register_fn("define_character", move |map: Map| {
        let mut s = character_state.lock().expect("builder state lock");
        define_character(&mut s, map);
    });
    let house_state = state.clone();
    engine.register_fn("define_house", move |map: Map| {
        let mut s = house_state.lock().expect("builder state lock");
        define_house(&mut s, map);
    });
    let org_state = state.clone();
    engine.register_fn("define_organisation", move |map: Map| {
        let mut s = org_state.lock().expect("builder state lock");
        define_organisation(&mut s, map);
    });
    let title_state = state.clone();
    engine.register_fn("define_title", move |map: Map| {
        let mut s = title_state.lock().expect("builder state lock");
        define_title(&mut s, map);
    });
    let office_state = state.clone();
    engine.register_fn("define_office", move |map: Map| {
        let mut s = office_state.lock().expect("builder state lock");
        define_office(&mut s, map);
    });
    let name_pool_state = state.clone();
    engine.register_fn("define_name_pool", move |map: Map| {
        let mut s = name_pool_state.lock().expect("builder state lock");
        define_name_pool(&mut s, map);
    });
    let ship_state = state.clone();
    engine.register_fn("define_ship", move |map: Map| {
        let mut s = ship_state.lock().expect("builder state lock");
        define_ship(&mut s, map);
    });
    let army_state = state.clone();
    engine.register_fn("define_army", move |map: Map| {
        let mut s = army_state.lock().expect("builder state lock");
        define_army(&mut s, map);
    });
    let obligation_state = state.clone();
    engine.register_fn("define_obligation", move |map: Map| {
        let mut s = obligation_state.lock().expect("builder state lock");
        define_obligation(&mut s, map);
    });
    let event_state = state.clone();
    engine.register_fn("define_event", move |map: Map| {
        let mut s = event_state.lock().expect("builder state lock");
        define_event(&mut s, map);
    });
    engine
}

/// Hashes the sorted source files; binds snapshots to exact content.
fn content_hash(sources: &[ContentSource]) -> aeon_core::hash::StateHash {
    let mut buffer = Vec::new();
    for source in sources {
        buffer.extend_from_slice(source.path.as_bytes());
        buffer.push(0);
        buffer.extend_from_slice(&(source.source.len() as u64).to_le_bytes());
        buffer.extend_from_slice(source.source.as_bytes());
        buffer.push(0);
    }
    hash_bytes(&buffer)
}

/// Loads and validates a content set from source files.
///
/// Files run in sorted path order. All findings are collected; the set is
/// returned only when no errors were found.
pub fn load_content(sources: &[ContentSource]) -> (Option<ContentSet>, ContentReport) {
    let mut sources: Vec<ContentSource> = sources.to_vec();
    sources.sort_by(|a, b| a.path.cmp(&b.path));

    let state = Arc::new(Mutex::new(BuilderState::default()));
    let engine = loading_engine(state.clone());

    let mut asts: BTreeMap<String, AST> = BTreeMap::new();
    let mut fn_names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for source in &sources {
        if asts.contains_key(&source.path) {
            let mut s = state.lock().expect("builder state lock");
            s.report
                .error(&source.path, None, "duplicate content file path");
            continue;
        }
        state.lock().expect("builder state lock").current_path = source.path.clone();

        let ast = match engine.compile(&source.source) {
            Ok(ast) => ast,
            Err(err) => {
                let mut s = state.lock().expect("builder state lock");
                s.report
                    .error(&source.path, None, format!("parse error: {err}"));
                continue;
            }
        };
        if let Err(err) = engine.run_ast(&ast) {
            let mut s = state.lock().expect("builder state lock");
            s.report
                .error(&source.path, None, format!("runtime error: {err}"));
            continue;
        }

        let names: BTreeSet<String> = ast.iter_functions().map(|f| f.name.to_string()).collect();
        fn_names.insert(source.path.clone(), names);
        asts.insert(source.path.clone(), ast);
    }

    let mut builder = Arc::try_unwrap(state)
        .map(|mutex| mutex.into_inner().expect("builder state lock"))
        .unwrap_or_else(|arc| {
            // The engine still holds handler clones; copy out instead.
            arc.lock().expect("builder state lock").take()
        });

    validate_cross_references(&mut builder, &fn_names);

    if builder.report.has_errors() {
        return (None, builder.report);
    }

    let set = ContentSet {
        jobs: builder.jobs,
        bodies: builder.bodies,
        provinces: builder.provinces,
        traits: builder.traits,
        name_pools: builder.name_pools,
        characters: builder.characters,
        organisations: builder.organisations,
        titles: builder.titles,
        offices: builder.offices,
        ships: builder.ships,
        armies: builder.armies,
        obligations: builder.obligations,
        events: builder.events,
        scenario: builder.scenario,
        asts,
        content_hash: content_hash(&sources),
    };
    (Some(set), builder.report)
}

impl BuilderState {
    /// Moves the collected state out, leaving an empty shell behind.
    fn take(&mut self) -> BuilderState {
        BuilderState {
            current_path: std::mem::take(&mut self.current_path),
            report: std::mem::take(&mut self.report),
            jobs: std::mem::take(&mut self.jobs),
            bodies: std::mem::take(&mut self.bodies),
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
            scenario: self.scenario.take(),
        }
    }
}

/// Post-pass validation once every file has run.
fn validate_cross_references(
    builder: &mut BuilderState,
    fn_names: &BTreeMap<String, BTreeSet<String>>,
) {
    // Jobs: mandatory results, effect functions must exist in their file.
    let mut findings: Vec<(String, Option<String>, String)> = Vec::new();
    for (key, job) in &builder.jobs {
        for required in [JobResultKind::Success, JobResultKind::Failure] {
            if !job.results.contains_key(&required) {
                findings.push((
                    fn_ref_path(job),
                    Some(key.to_string()),
                    format!("jobs must define a {required:?} result"),
                ));
            }
        }
        for result in job.results.values() {
            let fn_refs = result
                .effect_fn
                .iter()
                .chain(result.choices.iter().filter_map(|c| c.effect_fn.as_ref()));
            for fn_ref in fn_refs {
                let exists = fn_names
                    .get(&fn_ref.path)
                    .is_some_and(|names| names.contains(&fn_ref.name));
                if !exists {
                    findings.push((
                        fn_ref.path.clone(),
                        Some(key.to_string()),
                        format!(
                            "effect_fn '{}' is not defined in this file (function references are file-local)",
                            fn_ref.name
                        ),
                    ));
                }
            }
            if result.popup && result.popup_text.is_none() {
                findings.push((
                    fn_ref_path(job),
                    Some(key.to_string()),
                    "popup results should declare popup_text".to_owned(),
                ));
            }
        }
    }

    // Bodies: parent structure.
    for (key, body) in &builder.bodies {
        match (&body.kind, &body.parent) {
            (BodyKind::Planet, Some(_)) => findings.push((
                String::new(),
                Some(key.to_string()),
                "planets must not declare a parent".to_owned(),
            )),
            (BodyKind::Planet, None) => {}
            (_, None) => findings.push((
                String::new(),
                Some(key.to_string()),
                "moons and starbases must declare a parent".to_owned(),
            )),
            (_, Some(parent)) => match builder.bodies.get(parent) {
                None => findings.push((
                    String::new(),
                    Some(key.to_string()),
                    format!("parent body '{parent}' is not defined"),
                )),
                Some(parent_body) if parent_body.kind != BodyKind::Planet => findings.push((
                    String::new(),
                    Some(key.to_string()),
                    format!("parent body '{parent}' must be a planet"),
                )),
                Some(_) if body.orbit_days == 0 => findings.push((
                    String::new(),
                    Some(key.to_string()),
                    "orbiting bodies must declare orbit_days".to_owned(),
                )),
                Some(_) => {}
            },
        }
    }

    // Provinces: bodies must exist.
    for (key, province) in &builder.provinces {
        if !builder.bodies.contains_key(&province.body) {
            findings.push((
                String::new(),
                Some(key.to_string()),
                format!("body '{}' is not defined", province.body),
            ));
        }
    }

    validate_political_references(builder, &mut findings);

    for (path, key, message) in findings {
        builder.report.error(&path, key.as_deref(), message);
    }
}

/// Cross-reference validation for traits, characters, organisations,
/// titles, and offices.
fn validate_political_references(
    builder: &BuilderState,
    findings: &mut Vec<(String, Option<String>, String)>,
) {
    if !builder.characters.is_empty() && builder.name_pools.is_empty() {
        findings.push((
            String::new(),
            None,
            "content with characters must define a name pool (births need names)".to_owned(),
        ));
    }

    let mut err = |key: &ContentKey, message: String| {
        findings.push((String::new(), Some(key.to_string()), message));
    };

    for (key, trait_def) in &builder.traits {
        for opposite in &trait_def.opposites {
            if !builder.traits.contains_key(opposite) {
                err(key, format!("opposite trait '{opposite}' is not defined"));
            }
        }
    }

    for (key, character) in &builder.characters {
        if let Some(org) = &character.organisation
            && !builder.organisations.contains_key(org)
        {
            err(key, format!("organisation '{org}' is not defined"));
        }
        for parent in &character.parents {
            if !builder.characters.contains_key(parent) {
                err(key, format!("parent '{parent}' is not defined"));
            }
        }
        if let Some(spouse) = &character.spouse {
            match builder.characters.get(spouse) {
                None => err(key, format!("spouse '{spouse}' is not defined")),
                Some(_) if spouse == key => {
                    err(key, "characters cannot marry themselves".to_owned());
                }
                Some(other) => {
                    // Marriage is symmetric; the simulation mirrors
                    // one-sided declarations, but a conflicting
                    // declaration is an authoring error.
                    if let Some(their_spouse) = &other.spouse
                        && their_spouse != key
                    {
                        err(
                            key,
                            format!(
                                "spouse '{spouse}' declares a different spouse '{their_spouse}'"
                            ),
                        );
                    }
                }
            }
        }
        for trait_key in &character.traits {
            if !builder.traits.contains_key(trait_key) {
                err(key, format!("trait '{trait_key}' is not defined"));
            }
        }
    }

    let mut province_holders: BTreeMap<&ContentKey, &ContentKey> = BTreeMap::new();
    for (key, org) in &builder.organisations {
        match org.kind {
            OrgKind::DynasticHouse => match (org.tier, &org.liege) {
                (Some(HouseTier::Vassal), None) => {
                    err(key, "vassal houses must declare a liege".to_owned());
                }
                (Some(HouseTier::Vassal), Some(liege)) => match builder.organisations.get(liege) {
                    None => err(key, format!("liege '{liege}' is not defined")),
                    Some(liege_org) if liege_org.tier != Some(HouseTier::Great) => {
                        err(key, format!("liege '{liege}' must be a great house"));
                    }
                    Some(_) => {}
                },
                (Some(_), Some(_)) => {
                    err(key, "only vassal houses declare a liege".to_owned());
                }
                (Some(_), None) => {}
                (None, _) => err(key, "houses must declare a tier".to_owned()),
            },
            OrgKind::SanctoraImperim => {
                if org.tier.is_some() || org.liege.is_some() {
                    err(
                        key,
                        "the Sanctora Imperim has neither tier nor liege".to_owned(),
                    );
                }
            }
        }
        if let Some(head) = &org.head {
            match builder.characters.get(head) {
                None => err(key, format!("head '{head}' is not defined")),
                Some(character) if character.organisation.as_ref() != Some(key) => {
                    err(
                        key,
                        format!("head '{head}' does not belong to this organisation"),
                    );
                }
                Some(_) => {}
            }
        }
        for province in &org.provinces {
            if !builder.provinces.contains_key(province) {
                err(key, format!("province '{province}' is not defined"));
            }
            if let Some(other) = province_holders.insert(province, key) {
                err(
                    key,
                    format!("province '{province}' is already held by '{other}'"),
                );
            }
        }
    }

    for (key, title) in &builder.titles {
        if let TitleKindDef::Paramount { body } = &title.kind
            && !builder.bodies.contains_key(body)
        {
            err(key, format!("body '{body}' is not defined"));
        }
        match &title.holder {
            TitleHolderDef::Organisation(org) => {
                if !builder.organisations.contains_key(org) {
                    err(key, format!("holder organisation '{org}' is not defined"));
                }
            }
            TitleHolderDef::Character(character) => {
                if !builder.characters.contains_key(character) {
                    err(
                        key,
                        format!("holder character '{character}' is not defined"),
                    );
                }
            }
            TitleHolderDef::Vacant => {}
        }
        if matches!(title.kind, TitleKindDef::Consul)
            && matches!(title.holder, TitleHolderDef::Organisation(_))
        {
            err(
                key,
                "the Consul title is held personally, not by an organisation".to_owned(),
            );
        }
    }

    for (key, office) in &builder.offices {
        if !builder.organisations.contains_key(&office.organisation) {
            err(
                key,
                format!("organisation '{}' is not defined", office.organisation),
            );
        }
        if let Some(province) = &office.province
            && !builder.provinces.contains_key(province)
        {
            err(key, format!("province '{province}' is not defined"));
        }
        if let Some(holder) = &office.holder
            && !builder.characters.contains_key(holder)
        {
            err(key, format!("holder '{holder}' is not defined"));
        }
    }

    for (key, ship) in &builder.ships {
        if !builder.organisations.contains_key(&ship.owner) {
            err(key, format!("owner '{}' is not defined", ship.owner));
        }
        if !builder.provinces.contains_key(&ship.location) {
            err(key, format!("location '{}' is not defined", ship.location));
        }
        match (&ship.class, &ship.captain) {
            (ShipClass::Capital, None) => {
                err(key, "capital ships must have a captain".to_owned());
            }
            (_, Some(captain)) if !builder.characters.contains_key(captain) => {
                err(key, format!("captain '{captain}' is not defined"));
            }
            // A ship is commanded by one of its owner's own officers, the
            // same rule an army's general has always had.
            (_, Some(captain))
                if builder
                    .characters
                    .get(captain)
                    .is_some_and(|c| c.organisation.as_ref() != Some(&ship.owner)) =>
            {
                err(
                    key,
                    format!("captain '{captain}' does not belong to the owning house"),
                );
            }
            _ => {}
        }
    }

    for (key, army) in &builder.armies {
        if !builder.organisations.contains_key(&army.owner) {
            err(key, format!("owner '{}' is not defined", army.owner));
        }
        if !builder.provinces.contains_key(&army.province) {
            err(key, format!("province '{}' is not defined", army.province));
        }
        match builder.characters.get(&army.general) {
            None => err(key, format!("general '{}' is not defined", army.general)),
            Some(general) if general.organisation.as_ref() != Some(&army.owner) => {
                err(
                    key,
                    format!(
                        "general '{}' does not belong to the owning house",
                        army.general
                    ),
                );
            }
            Some(_) => {}
        }
    }

    for (key, obligation) in &builder.obligations {
        if !builder.organisations.contains_key(&obligation.debtor) {
            err(
                key,
                format!("debtor '{}' is not defined", obligation.debtor),
            );
        }
        if !builder.organisations.contains_key(&obligation.creditor) {
            err(
                key,
                format!("creditor '{}' is not defined", obligation.creditor),
            );
        }
    }

    if let Some(scenario) = &builder.scenario
        && let Some(player_house) = &scenario.player_house
    {
        match builder.organisations.get(player_house) {
            None => {
                findings.push((
                    String::new(),
                    Some(scenario.key.to_string()),
                    format!("player_house '{player_house}' is not defined"),
                ));
            }
            Some(org) if org.kind != OrgKind::DynasticHouse => {
                findings.push((
                    String::new(),
                    Some(scenario.key.to_string()),
                    "player_house must be a dynastic house".to_owned(),
                ));
            }
            Some(org) if org.head.is_none() => {
                findings.push((
                    String::new(),
                    Some(scenario.key.to_string()),
                    "player_house must have an authored head".to_owned(),
                ));
            }
            Some(_) => {}
        }
    }
}

fn fn_ref_path(job: &JobDef) -> String {
    job.results
        .values()
        .find_map(|r| r.effect_fn.as_ref().map(|f| f.path.clone()))
        .unwrap_or_default()
}

/// Why a runtime script call failed.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// The referenced file is not in the content set.
    #[error("no content file '{path}' in the loaded set")]
    UnknownFile {
        /// The missing path.
        path: String,
    },
    /// The script raised or the engine refused (limits, missing function).
    #[error("script error in {path}: {message}")]
    Runtime {
        /// The file whose function was called.
        path: String,
        /// Engine-reported failure.
        message: String,
    },
    /// The function returned malformed effects.
    #[error("bad effects from {path}: {source}")]
    BadEffects {
        /// The file whose function was called.
        path: String,
        /// The parse failure.
        source: EffectParseError,
    },
}

/// The runtime script host.
///
/// Owns the restricted engine used for all authored function calls. It has
/// no `define_*` functions: definitions exist only at load time.
pub struct ScriptHost {
    engine: Engine,
}

impl ScriptHost {
    /// Builds the runtime host.
    pub fn new() -> Self {
        let mut engine = sandboxed_engine();
        engine.on_print(|_| {});
        engine.on_debug(|_, _, _| {});
        Self { engine }
    }

    /// Calls a named effect function with a read-only context, returning
    /// its validated effects.
    pub fn call_effect_fn(
        &self,
        set: &ContentSet,
        fn_ref: &ScriptFnRef,
        context: Map,
    ) -> Result<Vec<ScriptEffect>, ScriptError> {
        let ast = set
            .asts
            .get(&fn_ref.path)
            .ok_or_else(|| ScriptError::UnknownFile {
                path: fn_ref.path.clone(),
            })?;
        let mut scope = Scope::new();
        // eval_ast(false): the file's top level ran once at load time;
        // runtime calls invoke retained functions only.
        let options = rhai::CallFnOptions::new().eval_ast(false);
        let result: Dynamic = self
            .engine
            .call_fn_with_options(options, &mut scope, ast, &fn_ref.name, (context,))
            .map_err(|err| ScriptError::Runtime {
                path: fn_ref.path.clone(),
                message: err.to_string(),
            })?;
        parse_effects(result).map_err(|source| ScriptError::BadEffects {
            path: fn_ref.path.clone(),
            source,
        })
    }
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}
