//! Authored-content pipeline for The Last Aeons.
//!
//! Owns the sandboxed Rhai script host and the definitions of authored
//! content: jobs, celestial bodies, provinces, and scenarios, growing with
//! each milestone. Content files declare data through `define_*` builder
//! functions and provide behaviour as named file-local functions; the
//! simulation calls those functions with read-only context and applies the
//! typed effects they return. Scripts never mutate simulation state.

pub mod effect;
pub mod fs;
pub mod host;
pub mod key;
pub mod model;
pub mod report;
pub mod text;

pub use effect::{EffectRole, ScriptEffect};
pub use host::{ContentSource, ScriptError, ScriptHost, load_content};
pub use key::{ContentKey, TextKey};
pub use model::ContentSet;
pub use report::{ContentFinding, ContentReport, Severity};
pub use text::{StringTable, TextRow, placeholders_in};
