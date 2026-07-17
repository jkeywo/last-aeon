//! The ordered player-command pipeline.
//!
//! Every meaningful player decision enters the simulation as a
//! [`PlayerCommand`] wrapped in a [`CommandEnvelope`] carrying its execution
//! day and a monotonic sequence number. Commands validate at submission,
//! queue until their day's tick, and apply in strict `(day, seq)` order —
//! which is what makes a recorded command log replayable.

use aeon_core::calendar::GameDate;
use bevy::app::App;
use bevy::prelude::{IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::state::CampaignMeta;

/// A meaningful player decision.
///
/// Variants grow with each milestone; every variant must remain
/// deserialisable forever once a release has written it to a command log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PlayerCommand {
    /// Does nothing. Used by tests and as a log keep-alive.
    Noop,
    /// Renames the campaign.
    RenameCampaign {
        /// The new player-facing campaign name.
        name: String,
    },
}

/// A command bound to its execution day and order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    /// Global monotonic sequence number; total order within a day.
    pub seq: u64,
    /// The day this command executes, at the start of that day's tick.
    pub day: GameDate,
    /// The decision itself.
    pub command: PlayerCommand,
}

/// Why a submitted command was refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommandRejection {
    /// A campaign name was empty or unreasonably long.
    #[error("campaign names must be 1..=120 characters, got {length}")]
    InvalidCampaignName {
        /// Length of the rejected name in characters.
        length: usize,
    },
}

/// Commands accepted but not yet applied, sorted by `(day, seq)`.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingCommands {
    entries: Vec<CommandEnvelope>,
}

impl PendingCommands {
    /// Inserts an envelope, keeping `(day, seq)` order.
    pub fn insert(&mut self, envelope: CommandEnvelope) {
        let key = (envelope.day, envelope.seq);
        let index = self.entries.partition_point(|e| (e.day, e.seq) <= key);
        self.entries.insert(index, envelope);
    }

    /// Removes and returns every envelope due on or before `date`, in order.
    pub fn take_due(&mut self, date: GameDate) -> Vec<CommandEnvelope> {
        let split = self.entries.partition_point(|e| e.day <= date);
        self.entries.drain(..split).collect()
    }

    /// The queued envelopes, in execution order.
    pub fn entries(&self) -> &[CommandEnvelope] {
        &self.entries
    }

    pub(crate) fn from_entries(entries: Vec<CommandEnvelope>) -> Self {
        let mut pending = Self::default();
        for envelope in entries {
            pending.insert(envelope);
        }
        pending
    }
}

/// The append-only record of accepted commands.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandLog {
    /// The next sequence number to assign.
    pub next_seq: u64,
    /// Every command applied so far, in application order.
    pub applied: Vec<CommandEnvelope>,
}

/// Validates a command against the current world.
///
/// Validation must be deterministic and side-effect free: replays re-run it.
pub fn validate_command(_world: &World, command: &PlayerCommand) -> Result<(), CommandRejection> {
    match command {
        PlayerCommand::Noop => Ok(()),
        PlayerCommand::RenameCampaign { name } => {
            let length = name.chars().count();
            if (1..=120).contains(&length) {
                Ok(())
            } else {
                Err(CommandRejection::InvalidCampaignName { length })
            }
        }
    }
}

/// Applies a single command's effects to the world.
fn apply_command(world: &mut World, command: &PlayerCommand) {
    match command {
        PlayerCommand::Noop => {}
        PlayerCommand::RenameCampaign { name } => {
            world.resource_mut::<CampaignMeta>().name = name.clone();
        }
    }
}

/// Applies every command due this tick, in `(day, seq)` order.
fn apply_due_commands(world: &mut World) {
    let date = world.resource::<CampaignClock>().date;
    let due = world.resource_mut::<PendingCommands>().take_due(date);
    for envelope in due {
        apply_command(world, &envelope.command);
        world.resource_mut::<CommandLog>().applied.push(envelope);
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(DailyTick, apply_due_commands.in_set(TickSet::Commands));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(seq: u64, day: i64) -> CommandEnvelope {
        CommandEnvelope {
            seq,
            day: GameDate::from_days(day),
            command: PlayerCommand::Noop,
        }
    }

    #[test]
    fn pending_commands_keep_day_then_seq_order() {
        let mut pending = PendingCommands::default();
        pending.insert(envelope(3, 5));
        pending.insert(envelope(1, 2));
        pending.insert(envelope(2, 5));
        let due = pending.take_due(GameDate::from_days(5));
        let order: Vec<u64> = due.iter().map(|e| e.seq).collect();
        assert_eq!(order, vec![1, 2, 3]);
    }

    #[test]
    fn take_due_leaves_future_commands_queued() {
        let mut pending = PendingCommands::default();
        pending.insert(envelope(1, 2));
        pending.insert(envelope(2, 9));
        let due = pending.take_due(GameDate::from_days(5));
        assert_eq!(due.len(), 1);
        assert_eq!(pending.entries().len(), 1);
        assert_eq!(pending.entries()[0].seq, 2);
    }

    #[test]
    fn envelopes_serialise_to_stable_json() {
        let env = CommandEnvelope {
            seq: 4,
            day: GameDate::from_days(12),
            command: PlayerCommand::RenameCampaign {
                name: "House Veyrin Ascendant".to_owned(),
            },
        };
        let json = serde_json::to_string(&env).unwrap();
        assert_eq!(
            json,
            r#"{"seq":4,"day":12,"command":{"type":"rename-campaign","name":"House Veyrin Ascendant"}}"#
        );
        let back: CommandEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
    }
}
