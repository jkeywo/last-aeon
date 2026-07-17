//! Snapshot files and the command log.
//!
//! Snapshots are pretty-printed RON with an explicit format version; the
//! command log is JSON Lines, one envelope per line, append-only. Both are
//! deliberately human-readable while the game is young — save debugging and
//! authored-scenario diagnosis stay cheap, and the format version gives a
//! later binary migration a hook.
//!
//! Filesystem helpers are native-only; the web build persists through
//! in-memory serialisation until a browser storage backend lands with the
//! delivery milestone.

use std::io::{BufRead, Write};

use crate::command::CommandEnvelope;
use crate::snapshot::CampaignSnapshot;

/// Why persistence failed.
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    /// The underlying I/O failed.
    #[error("i/o failure: {0}")]
    Io(#[from] std::io::Error),
    /// A snapshot document could not be parsed.
    #[error("malformed snapshot: {0}")]
    MalformedSnapshot(#[from] ron::error::SpannedError),
    /// A snapshot could not be serialised (unexpected).
    #[error("snapshot serialisation failed: {0}")]
    SnapshotSerialisation(#[from] ron::Error),
    /// A command-log line could not be parsed.
    #[error("malformed command log line {line}: {source}")]
    MalformedLogLine {
        /// One-based line number.
        line: usize,
        /// The parse failure.
        source: serde_json::Error,
    },
    /// A command envelope could not be serialised (unexpected).
    #[error("command serialisation failed: {0}")]
    CommandSerialisation(#[from] serde_json::Error),
}

/// Serialises a snapshot to its RON document form.
pub fn snapshot_to_ron(snapshot: &CampaignSnapshot) -> Result<String, PersistenceError> {
    Ok(ron::ser::to_string_pretty(
        snapshot,
        ron::ser::PrettyConfig::default(),
    )?)
}

/// Parses a snapshot from its RON document form.
///
/// Parsing does not verify integrity; restore through
/// [`crate::snapshot::verify_snapshot`] for that.
pub fn snapshot_from_ron(document: &str) -> Result<CampaignSnapshot, PersistenceError> {
    Ok(ron::from_str(document)?)
}

/// Appends one command envelope as a JSONL line.
pub fn append_command<W: Write>(
    writer: &mut W,
    envelope: &CommandEnvelope,
) -> Result<(), PersistenceError> {
    let line = serde_json::to_string(envelope)?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Writes a full command log as JSONL.
pub fn write_command_log<W: Write>(
    writer: &mut W,
    envelopes: &[CommandEnvelope],
) -> Result<(), PersistenceError> {
    for envelope in envelopes {
        append_command(writer, envelope)?;
    }
    Ok(())
}

/// Reads a JSONL command log. Blank lines are ignored.
pub fn read_command_log<R: BufRead>(reader: R) -> Result<Vec<CommandEnvelope>, PersistenceError> {
    let mut envelopes = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let envelope =
            serde_json::from_str(trimmed).map_err(|source| PersistenceError::MalformedLogLine {
                line: index + 1,
                source,
            })?;
        envelopes.push(envelope);
    }
    Ok(envelopes)
}

/// Native filesystem helpers.
#[cfg(not(target_arch = "wasm32"))]
pub mod files {
    use std::fs;
    use std::io::BufReader;
    use std::path::Path;

    use super::*;

    /// Writes a snapshot file, creating parent directories as needed.
    pub fn save_snapshot(path: &Path, snapshot: &CampaignSnapshot) -> Result<(), PersistenceError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, snapshot_to_ron(snapshot)?)?;
        Ok(())
    }

    /// Reads a snapshot file.
    pub fn load_snapshot(path: &Path) -> Result<CampaignSnapshot, PersistenceError> {
        snapshot_from_ron(&fs::read_to_string(path)?)
    }

    /// Writes a command log file, creating parent directories as needed.
    pub fn save_command_log(
        path: &Path,
        envelopes: &[CommandEnvelope],
    ) -> Result<(), PersistenceError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(path)?;
        write_command_log(&mut file, envelopes)
    }

    /// Reads a command log file.
    pub fn load_command_log(path: &Path) -> Result<Vec<CommandEnvelope>, PersistenceError> {
        read_command_log(BufReader::new(fs::File::open(path)?))
    }
}

#[cfg(test)]
mod tests {
    use aeon_core::calendar::GameDate;

    use super::*;
    use crate::command::PlayerCommand;

    #[test]
    fn command_log_round_trips_and_skips_blank_lines() {
        let envelopes = vec![
            CommandEnvelope {
                seq: 0,
                day: GameDate::from_days(1),
                command: PlayerCommand::Noop,
            },
            CommandEnvelope {
                seq: 1,
                day: GameDate::from_days(3),
                command: PlayerCommand::RenameCampaign {
                    name: "Renamed".to_owned(),
                },
            },
        ];
        let mut buffer = Vec::new();
        write_command_log(&mut buffer, &envelopes).unwrap();
        let mut text = String::from_utf8(buffer).unwrap();
        text.push('\n');
        let back = read_command_log(text.as_bytes()).unwrap();
        assert_eq!(back, envelopes);
    }

    #[test]
    fn malformed_log_lines_report_their_line_number() {
        let text = "{\"seq\":0,\"day\":1,\"command\":{\"type\":\"noop\"}}\nnot json\n";
        let error = read_command_log(text.as_bytes()).unwrap_err();
        match error {
            PersistenceError::MalformedLogLine { line, .. } => assert_eq!(line, 2),
            other => panic!("unexpected error: {other}"),
        }
    }
}
