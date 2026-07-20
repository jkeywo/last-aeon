//! Headless authoritative simulation for The Last Aeons.
//!
//! The simulation runs on Bevy ECS with no renderer, window, or asset
//! plugins. Native and web clients attach presentation to this same
//! simulation; nothing outside this crate owns or alters gameplay rules.
//!
//! # Structure
//!
//! - [`access`] — world-read helpers: records by stable ID, display
//!   names, log stamping, and derived random streams.
//! - [`config`] — what defines a new campaign (seed, start date, name).
//! - [`clock`] — the campaign calendar clock, tick schedules, and the
//!   [`clock::advance_one_day`] entry point that drives all simulation time.
//! - [`command`] — the ordered player-command pipeline: every meaningful
//!   player decision is a validated, logged [`command::PlayerCommand`].
//! - [`state`] — the authoritative campaign resources.
//! - [`text`] — the display-text string table every player-facing string
//!   is resolved through.
//! - [`snapshot`] — versioned canonical state capture, restore, and hashing.
//! - [`persistence`] — RON snapshot files and the JSONL command log.
//! - [`host`] — [`host::SimHost`], the embedding API used by the tools CLI,
//!   tests, and anything else that drives the simulation without a client.

pub mod access;
pub mod agency;
pub mod assignments;
pub mod clock;
pub mod command;
pub mod config;
pub mod crisis;
pub mod economy;
pub mod events;
pub mod forces;
pub mod forecast;
pub mod host;
pub mod ids;
pub mod map;
pub mod obligations;
pub mod order;
pub mod persistence;
pub mod politics;
pub mod presence;
pub mod snapshot;
pub mod state;
pub mod text;
pub mod warfare;

use bevy::app::{App, Plugin};

pub use assignments::{
    ActiveAssignment, AssignmentRejection, AssignmentTarget, AssignmentsIndex, LeaderAvailability,
    LogChannel, LogEntry, LogSubject, MessageLog, PendingPopups, Post, leader_availability,
    request_cancel, start_assignment, target_allowed,
};
pub use clock::{CampaignClock, DailyTick, MonthlyPulse, TickSet, YearlyPulse, advance_one_day};
pub use command::{CommandEnvelope, CommandRejection, PlayerCommand};
pub use config::CampaignConfig;
pub use economy::OrgResources;
pub use events::{EventOccurrence, EventState, EventSubject};
pub use forces::{ArmyRecord, ForcesIndex, ShipRecord};
pub use forecast::{AssignmentForecast, ForecastResult, ForecastRisk, Permille};
pub use host::SimHost;
pub use ids::{
    ArmyId, AssignmentId, BodyId, CharacterId, OfficeId, OrgId, ProvinceId, ShipId, TitleId,
};
pub use map::{BodyRecord, DisplayName, GeoPosition, MapIndex, ProvinceRecord};
pub use obligations::{ObligationKind, ObligationRecord, ObligationStatus, Obligations};
pub use order::ProvincialOrder;
pub use politics::{
    CampaignOver, CharacterRecord, OfficeRecord, OrgRecord, PlayerHouse, PoliticsIndex,
    TitleHolder, TitleKind, TitleRecord, answers_to, opinion_between,
};
pub use presence::{CharacterLocation, Location};
pub use snapshot::{CampaignSnapshot, CampaignState, SNAPSHOT_FORMAT_VERSION, SnapshotError};
pub use text::TextDb;

/// Root plugin installing the authoritative simulation into a Bevy [`App`].
///
/// Clients and the headless host both install exactly this plugin, which is
/// what keeps native, web, and test simulations identical. Installing the
/// plugin prepares systems and schedules; an actual campaign starts when
/// [`state::start_campaign`] inserts the campaign resources.
pub struct AeonSimPlugin;

impl Plugin for AeonSimPlugin {
    fn build(&self, app: &mut App) {
        // The string table is not campaign state — it is embedded at build
        // time and identical for every campaign — so it belongs to the
        // plugin. A world restored from a snapshot has it for the same
        // reason a fresh one does.
        app.init_resource::<text::TextDb>();
        clock::install(app);
        command::install(app);
        politics::install(app);
        assignments::install(app);
        economy::install(app);
        presence::install(app);
        forces::install(app);
        order::install(app);
        obligations::install(app);
        events::install(app);
        warfare::install(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_plugin_installs_headlessly() {
        let mut app = App::new();
        app.add_plugins(AeonSimPlugin);
        app.update();
    }
}
