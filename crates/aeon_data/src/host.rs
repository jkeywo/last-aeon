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
    BodyDef, BodyKind, ContentSet, JobCategory, JobDef, JobResultDef, JobResultKind, ProvinceDef,
    ScenarioDef, ScriptFnRef,
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
            "id",
            "title",
            "summary",
            "category",
            "duration_days",
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
            &["weight", "popup", "log", "effect_fn"],
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
        results.insert(
            kind,
            JobResultDef {
                weight: weight as u32,
                popup,
                log,
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
        &["id", "name", "body", "latitude_mdeg", "longitude_mdeg"],
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
        &["id", "name", "start_year", "start_month", "start_day"],
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
    });
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
            if let Some(fn_ref) = &result.effect_fn {
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

    for (path, key, message) in findings {
        builder.report.error(&path, key.as_deref(), message);
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
