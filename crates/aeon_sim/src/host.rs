//! The headless simulation host.
//!
//! [`SimHost`] owns a bare Bevy [`App`] with only the simulation plugin
//! installed — no renderer, window, or assets — and exposes the complete
//! embedding surface: submit commands, advance days, capture and restore
//! snapshots, and read state. The tools CLI, the test suite, and replay
//! verification all drive campaigns exclusively through this API.

use aeon_core::calendar::GameDate;
use aeon_core::hash::StateHash;
use bevy::app::App;
use bevy::prelude::World;

use crate::AeonSimPlugin;
use crate::clock::{CampaignClock, advance_one_day};
use crate::command::{
    CommandEnvelope, CommandLog, CommandRejection, PendingCommands, PlayerCommand, validate_command,
};
use crate::config::CampaignConfig;
use crate::snapshot::{
    CampaignSnapshot, SnapshotError, capture_snapshot, capture_state, hash_state, restore_state,
    verify_snapshot,
};
use crate::state::{CampaignMeta, start_campaign};

/// Why a recorded envelope could not be re-submitted during replay.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// The envelope's execution day is not in the future of the campaign.
    #[error("envelope for day {envelope_day} is not after the current date {current}")]
    DayNotInFuture {
        /// The envelope's execution day.
        envelope_day: GameDate,
        /// The campaign's current date.
        current: GameDate,
    },
    /// The envelope's sequence number reuses an already-assigned value.
    #[error("envelope seq {seq} is below the next sequence number {next_seq}")]
    SequenceRegression {
        /// The envelope's sequence number.
        seq: u64,
        /// The campaign's next unassigned sequence number.
        next_seq: u64,
    },
    /// The recorded command no longer validates.
    #[error("recorded command failed validation: {0}")]
    Rejected(#[from] CommandRejection),
}

/// A headless campaign simulation.
pub struct SimHost {
    app: App,
}

impl SimHost {
    /// Starts a fresh campaign.
    pub fn new(config: CampaignConfig) -> Self {
        let mut app = App::new();
        app.add_plugins(AeonSimPlugin);
        start_campaign(app.world_mut(), config);
        Self { app }
    }

    /// Restores a campaign from a snapshot, verifying version and hash.
    pub fn restore(snapshot: CampaignSnapshot) -> Result<Self, SnapshotError> {
        let state = verify_snapshot(snapshot)?;
        let mut app = App::new();
        app.add_plugins(AeonSimPlugin);
        restore_state(app.world_mut(), state);
        Ok(Self { app })
    }

    /// Submits a player command, to execute at the start of the next day.
    ///
    /// Returns the recorded envelope; persist it via the command log to make
    /// the campaign replayable.
    pub fn submit(&mut self, command: PlayerCommand) -> Result<CommandEnvelope, CommandRejection> {
        let world = self.app.world_mut();
        validate_command(world, &command)?;
        let day = world.resource::<CampaignClock>().date.add_days(1);
        let seq = {
            let mut log = world.resource_mut::<CommandLog>();
            let seq = log.next_seq;
            log.next_seq += 1;
            seq
        };
        let envelope = CommandEnvelope { seq, day, command };
        world
            .resource_mut::<PendingCommands>()
            .insert(envelope.clone());
        Ok(envelope)
    }

    /// Re-submits a recorded envelope during replay, preserving its day and
    /// sequence number.
    pub fn submit_recorded(&mut self, envelope: CommandEnvelope) -> Result<(), ReplayError> {
        let world = self.app.world_mut();
        let current = world.resource::<CampaignClock>().date;
        if envelope.day <= current {
            return Err(ReplayError::DayNotInFuture {
                envelope_day: envelope.day,
                current,
            });
        }
        let next_seq = world.resource::<CommandLog>().next_seq;
        if envelope.seq < next_seq {
            return Err(ReplayError::SequenceRegression {
                seq: envelope.seq,
                next_seq,
            });
        }
        validate_command(world, &envelope.command)?;
        world.resource_mut::<CommandLog>().next_seq = envelope.seq + 1;
        world.resource_mut::<PendingCommands>().insert(envelope);
        Ok(())
    }

    /// Advances the campaign by whole days.
    pub fn advance_days(&mut self, days: u32) {
        for _ in 0..days {
            advance_one_day(self.app.world_mut());
        }
    }

    /// The current campaign date.
    pub fn date(&self) -> GameDate {
        self.app.world().resource::<CampaignClock>().date
    }

    /// The player-facing campaign name.
    pub fn campaign_name(&self) -> String {
        self.app.world().resource::<CampaignMeta>().name.clone()
    }

    /// Every command applied so far, in application order.
    pub fn applied_commands(&self) -> Vec<CommandEnvelope> {
        self.app.world().resource::<CommandLog>().applied.clone()
    }

    /// Captures a versioned, hashed snapshot.
    pub fn snapshot(&self) -> CampaignSnapshot {
        capture_snapshot(self.app.world())
    }

    /// The canonical hash of current authoritative state.
    pub fn state_hash(&self) -> StateHash {
        hash_state(&capture_state(self.app.world()))
    }

    /// Direct world access, for tests and advanced embedding.
    ///
    /// Presentation code must treat this as read-only; all mutation goes
    /// through commands so campaigns stay replayable.
    pub fn world_mut(&mut self) -> &mut World {
        self.app.world_mut()
    }
}
