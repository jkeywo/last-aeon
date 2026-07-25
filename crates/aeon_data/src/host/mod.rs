//! The sandboxed Rhai host: content loading and runtime function calls.
//!
//! Two engines share one sandbox profile but differ in surface:
//!
//! - the *loading* engine adds the `define_*` builder functions and runs
//!   each file's top level once, collecting definitions;
//! - the *runtime* engine has no builder functions and never re-runs top
//!   level; it only calls named functions retained in the compiled ASTs.
//!
//! The sandbox itself is the fleet's — `vellum-script`, extracted from this
//! game — and stays deny-by-default for anything nondeterministic or
//! stateful: no imports, no `eval`, no wall-clock, integer-only arithmetic,
//! and hard operation, size, and recursion limits. What remains here is the
//! *vocabulary*: which builder functions a loading engine registers, the
//! shape of the context a call receives, and how a returned value becomes
//! typed effects. Scripts read the context they are handed and return effect
//! data; they cannot reach simulation state at all.
//!
//! The pieces live in submodules: [`builders`] turns authored maps into
//! validated definitions, [`validate`] runs the cross-reference pass once
//! every file has run, and this module owns the sandbox, the load
//! orchestration, and the runtime [`ScriptHost`].

mod builders;
pub mod display;
mod validate;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use rhai::{AST, Dynamic, Engine, Map};

use crate::effect::{EffectParseError, ScriptEffect, parse_effects};
use crate::model::{ContentSet, ScriptFnRef};
use crate::report::ContentReport;
use crate::text::StringTable;

use builders::{BuilderState, loading_engine};
use display::fill_display_text;
use validate::validate_cross_references;

/// One authored source file, path-relative to the content root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentSource {
    /// Content-relative path with forward slashes, e.g. `core/assignments.rhai`.
    pub path: String,
    /// The Rhai source text.
    pub source: String,
}

/// Hashes the sorted source files; binds snapshots to exact content.
///
/// The framing (path, length, then bytes) is the fleet's, so a character
/// moved across a file boundary still changes the hash.
fn content_hash(sources: &[ContentSource]) -> aeon_core::hash::StateHash {
    let shared: Vec<vellum_script::ScriptSource> = sources
        .iter()
        .map(|source| vellum_script::ScriptSource {
            path: source.path.clone(),
            source: source.source.clone(),
        })
        .collect();
    aeon_core::hash::StateHash::from_u64(vellum_script::content_hash(&shared))
}

/// Loads and validates a content set from source files.
///
/// Files run in sorted path order. All findings are collected; the set is
/// returned only when no errors were found.
///
/// `strings` supplies every string the player reads: authored files carry
/// IDs and mechanics, and their prose is filled in from the table by the
/// key each ID derives. See [`display`].
pub fn load_content(
    sources: &[ContentSource],
    strings: &StringTable,
) -> (Option<ContentSet>, ContentReport) {
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
    fill_display_text(&mut builder, strings, "assets/text/strings.csv");

    if builder.report.has_errors() {
        return (None, builder.report);
    }

    let set = ContentSet {
        assignments: builder.assignments,
        bodies: builder.bodies,
        goods: builder.goods,
        buildings: builder.buildings,
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
        plans: builder.plans,
        goals: builder.goals,
        scenario: builder.scenario,
        asts,
        content_hash: content_hash(&sources),
    };
    (Some(set), builder.report)
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
    /// Builds the runtime host on the fleet's quiet sandbox.
    pub fn new() -> Self {
        Self {
            engine: vellum_script::quiet_sandbox(),
        }
    }

    /// Calls a named effect function with a read-only context, returning
    /// its validated effects.
    ///
    /// The simulation supplies one context schema for every invocation —
    /// assignment results, popup choices, event firings, and event answers:
    /// `source` (the assignment or event key), `result` (the result kind or
    /// chosen option, as text), `leader` (the leading character's display
    /// name, possibly empty), and `target` (a display label for what the
    /// action acted on, possibly empty).
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
        // The shared seam calls a retained function without re-running the
        // file's top level, which ran once at load time.
        let result: Dynamic =
            vellum_script::call_fn(&self.engine, ast, &fn_ref.path, &fn_ref.name, context)
                .map_err(|err| match err {
                    vellum_script::CallError::Runtime { path, message } => {
                        ScriptError::Runtime { path, message }
                    }
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
