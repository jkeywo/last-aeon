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
//!
//! The pieces live in submodules: [`builders`] turns authored maps into
//! validated definitions, [`validate`] runs the cross-reference pass once
//! every file has run, and this module owns the sandbox, the load
//! orchestration, and the runtime [`ScriptHost`].

mod builders;
mod display;
mod validate;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use aeon_core::hash::hash_bytes;
use rhai::{AST, Dynamic, Engine, Map, Scope};

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
    ///
    /// The simulation supplies one context schema for every invocation —
    /// job results, popup choices, event firings, and event answers:
    /// `source` (the job or event key), `result` (the result kind or
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
