//! Versioned campaign snapshots and canonical state hashing.
//!
//! A snapshot is a complete, self-contained capture of authoritative
//! campaign state, taken from a dedicated serialisable model rather than raw
//! ECS internals. Its canonical RON serialisation is what the SHA-256 state
//! hash covers, so two worlds are gameplay-identical exactly when their
//! state hashes agree.

use std::sync::Arc;

use aeon_core::hash::{StateHash, hash_bytes};
use aeon_core::{calendar::GameDate, id::IdAllocator};
use aeon_data::ContentSet;
use bevy::prelude::World;
use serde::{Deserialize, Serialize};

use crate::clock::CampaignClock;
use crate::command::{CommandEnvelope, CommandLog, PendingCommands};
use crate::state::{CampaignIds, CampaignMeta, CampaignSeed, ContentDb};

/// Current snapshot format version.
///
/// Bump on any change to [`CampaignState`]'s serialised shape, and provide a
/// migration for every version a release has ever written. No release has
/// shipped yet, so pre-release bumps carry no migrations.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 11;

/// The complete authoritative campaign state.
///
/// Grows a section per milestone. Everything here must be deterministic:
/// no wall-clock times, no platform-dependent values, no hash-ordered
/// collections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignState {
    /// Player-facing campaign name.
    pub name: String,
    /// The campaign seed.
    pub seed: u64,
    /// The campaign's first day.
    pub start_date: GameDate,
    /// The current day.
    pub date: GameDate,
    /// Hash of the authored content this campaign runs on, if any.
    pub content_hash: Option<StateHash>,
    /// The stable-ID allocator.
    pub id_allocator: IdAllocator,
    /// The map's ID-to-key bindings.
    pub map: crate::map::MapState,
    /// The political world.
    pub politics: crate::politics::PoliticsState,
    /// The assignment world.
    pub assignments: crate::assignments::AssignmentsState,
    /// Ships and armies.
    pub forces: crate::forces::ForcesState,
    /// The political obligation ledger.
    #[serde(default)]
    pub obligations: crate::obligations::Obligations,
    /// Contextual-event cooldowns and history.
    #[serde(default)]
    pub events: crate::events::EventState,
    /// Plans autonomous characters are pursuing, and their cooldowns.
    #[serde(default)]
    pub plans: crate::plans::Plans,
    /// Next command sequence number.
    pub next_command_seq: u64,
    /// Commands accepted but not yet applied, in `(day, seq)` order.
    pub pending_commands: Vec<CommandEnvelope>,
    /// Every command applied so far, in application order.
    pub applied_commands: Vec<CommandEnvelope>,
}

/// A versioned, hash-verified campaign snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignSnapshot {
    /// The snapshot format version this was written with.
    pub format_version: u32,
    /// SHA-256 of the canonical serialisation of `state`.
    pub state_hash: StateHash,
    /// The captured state.
    pub state: CampaignState,
}

/// Why a snapshot could not be restored.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// The snapshot was written by an unknown or future format version.
    #[error("unsupported snapshot format version {found} (supported: {supported})")]
    UnsupportedVersion {
        /// Version found in the snapshot.
        found: u32,
        /// Version this build supports.
        supported: u32,
    },
    /// The recorded hash does not match the state, so the snapshot is
    /// corrupt or was edited.
    #[error("snapshot state hash mismatch: recorded {recorded}, computed {computed}")]
    HashMismatch {
        /// Hash recorded in the snapshot.
        recorded: StateHash,
        /// Hash recomputed from the state.
        computed: StateHash,
    },
    /// The snapshot was taken against authored content, which must be
    /// supplied to restore it.
    #[error("this snapshot requires its authored content (hash {required}) to restore")]
    ContentRequired {
        /// The content hash the snapshot was taken against.
        required: StateHash,
    },
    /// The supplied content does not match what the snapshot was taken
    /// against.
    #[error("content mismatch: snapshot was taken against {required}, supplied {supplied}")]
    ContentMismatch {
        /// The content hash the snapshot was taken against.
        required: StateHash,
        /// The hash of the content actually supplied.
        supplied: StateHash,
    },
    /// The snapshot was taken without authored content, so attaching
    /// content on restore would change what future snapshots record.
    #[error("this snapshot was taken without authored content; restore it without content")]
    ContentNotExpected,
}

/// Computes the canonical hash of a campaign state.
pub fn hash_state(state: &CampaignState) -> StateHash {
    let canonical = ron::to_string(state).expect("campaign state always serialises to RON");
    hash_bytes(canonical.as_bytes())
}

/// Captures the current authoritative state of a campaign world.
///
/// Every section is captured here, in one place; restoring is the same
/// list split into its two halves, [`restore_state`] (content-free) and
/// [`restore_content_state`] (content-bound).
///
/// # Panics
/// Panics if no campaign has been started in this world.
pub fn capture_state(world: &World) -> CampaignState {
    let clock = world.resource::<CampaignClock>();
    let log = world.resource::<CommandLog>();
    CampaignState {
        name: world.resource::<CampaignMeta>().name.clone(),
        seed: world.resource::<CampaignSeed>().0,
        start_date: clock.start_date,
        date: clock.date,
        content_hash: world
            .get_resource::<crate::state::ContentDb>()
            .map(|db| db.0.content_hash),
        id_allocator: world.resource::<CampaignIds>().0.clone(),
        map: crate::map::capture_map(world),
        politics: crate::politics::capture_politics(world),
        assignments: crate::assignments::capture_assignments(world),
        forces: crate::forces::capture_forces(world),
        obligations: crate::obligations::capture(world),
        events: crate::events::capture(world),
        plans: crate::plans::capture(world),
        next_command_seq: log.next_seq,
        pending_commands: world.resource::<PendingCommands>().entries().to_vec(),
        applied_commands: log.applied.clone(),
    }
}

/// Captures a versioned, hashed snapshot of a campaign world.
pub fn capture_snapshot(world: &World) -> CampaignSnapshot {
    let state = capture_state(world);
    CampaignSnapshot {
        format_version: SNAPSHOT_FORMAT_VERSION,
        state_hash: hash_state(&state),
        state,
    }
}

/// Verifies a snapshot's version and hash, returning its state.
pub fn verify_snapshot(snapshot: CampaignSnapshot) -> Result<CampaignState, SnapshotError> {
    if snapshot.format_version != SNAPSHOT_FORMAT_VERSION {
        return Err(SnapshotError::UnsupportedVersion {
            found: snapshot.format_version,
            supported: SNAPSHOT_FORMAT_VERSION,
        });
    }
    let computed = hash_state(&snapshot.state);
    if computed != snapshot.state_hash {
        return Err(SnapshotError::HashMismatch {
            recorded: snapshot.state_hash,
            computed,
        });
    }
    Ok(snapshot.state)
}

/// Installs a verified state into a world as the active campaign.
///
/// This is the content-free half of a restore, mirroring the top half of
/// [`capture_state`]: campaign identity, clock, allocator, the command
/// pipeline, and the sections that never materialise entities from
/// authored definitions (obligations, events). A snapshot taken against
/// authored content is only fully restored once
/// [`restore_content_state`] has respawned the content-bound half; a
/// content-free campaign needs nothing more.
///
/// The plugin must already be installed. Replaces any existing campaign.
pub fn restore_state(world: &mut World, state: CampaignState) {
    use crate::command::PendingCommands;

    world.insert_resource(CampaignSeed(state.seed));
    world.insert_resource(CampaignMeta { name: state.name });
    world.insert_resource(CampaignClock {
        start_date: state.start_date,
        date: state.date,
    });
    world.insert_resource(CampaignIds(state.id_allocator));
    world.insert_resource(PendingCommands::from_entries(state.pending_commands));
    world.insert_resource(CommandLog {
        next_seq: state.next_command_seq,
        applied: state.applied_commands,
    });
    crate::obligations::restore(world, &state.obligations);
    crate::events::restore(world, &state.events);
    crate::plans::restore(world, &state.plans);
}

/// Respawns the content-bound half of a restore against hash-verified
/// content: the map, the political world, forces, and the assignment world,
/// then the script runtime and the content database — the same sections
/// [`crate::state::start_campaign_with_content`] creates on a fresh
/// campaign, in the same order.
///
/// Callers have already verified that `content`'s hash matches the
/// snapshot's recorded hash; pair with [`restore_state`], which installs
/// the content-free half.
pub fn restore_content_state(world: &mut World, state: &CampaignState, content: Arc<ContentSet>) {
    crate::map::restore_map(world, &state.map, &content);
    crate::politics::restore_politics(world, &state.politics, &content);
    crate::forces::restore_forces(world, &state.forces, &content);
    crate::assignments::restore_assignments(world, &state.assignments);
    world.insert_resource(crate::assignments::ScriptRuntime(
        aeon_data::ScriptHost::new(),
    ));
    world.insert_resource(ContentDb(content));
}
