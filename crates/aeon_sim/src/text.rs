//! The display-text string table, as a campaign resource.
//!
//! The table is embedded at build time and inserted by the simulation
//! plugin, so the simulation and any client attached to the same world read
//! one copy of every player-facing string. It is not campaign state — it
//! does not vary between campaigns and is not snapshotted — so a world
//! restored from a save has it for the same reason a fresh one does.
//!
//! Sim-side log lines resolve as they are written, which keeps
//! [`crate::jobs::LogEntry`] a plain `String` and leaves the snapshot
//! format, state hashing, and the client's log filter untouched.

use std::sync::Arc;

use aeon_data::StringTable;
use bevy::prelude::Resource;

/// The string table, embedded at build time.
const EMBEDDED_STRINGS: &str = include_str!("../../../assets/text/strings.csv");

/// Every display string the campaign can render.
#[derive(Resource, Clone)]
pub struct TextDb(pub Arc<StringTable>);

impl TextDb {
    /// Parses the table embedded at build time.
    ///
    /// # Panics
    /// Panics if the embedded table fails to parse. CI validates the same
    /// file on every push, so this only fires on a broken local edit — and
    /// should fire loudly.
    pub fn embedded() -> Self {
        let (table, report) = StringTable::parse(EMBEDDED_STRINGS, "assets/text/strings.csv");
        match table {
            Some(table) => Self(Arc::new(table)),
            None => {
                for finding in &report.findings {
                    eprintln!("{finding}");
                }
                panic!("the embedded string table failed to parse");
            }
        }
    }

    /// The text for a key, with no placeholders to fill.
    pub fn text(&self, key: &str) -> &str {
        self.0.text(key)
    }

    /// The text for a key with its `{named}` placeholders filled.
    pub fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.0.format(key, args)
    }

    /// The text for a pluralised key, selected on `count`.
    pub fn format_plural(&self, key: &str, count: i64, args: &[(&str, &str)]) -> String {
        self.0.format_plural(key, count, args)
    }
}

impl Default for TextDb {
    fn default() -> Self {
        Self::embedded()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_table_parses() {
        let strings = TextDb::embedded();
        assert!(!strings.0.is_empty(), "the shipped table has rows");
    }
}
