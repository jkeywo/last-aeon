//! Headless authoritative simulation for The Last Aeons.
//!
//! The simulation runs on Bevy ECS with no renderer, window, or asset
//! plugins. Native and web clients attach presentation to this same
//! simulation; nothing outside this crate owns or alters gameplay rules.
//!
//! # Structure
//!
//! - [`config`] — what defines a new campaign (seed, start date, name).
//! - [`clock`] — the campaign calendar clock, tick schedules, and the
//!   [`clock::advance_one_day`] entry point that drives all simulation time.
//! - [`command`] — the ordered player-command pipeline: every meaningful
//!   player decision is a validated, logged [`command::PlayerCommand`].
//! - [`state`] — the authoritative campaign resources.
//! - [`snapshot`] — versioned canonical state capture, restore, and hashing.
//! - [`persistence`] — RON snapshot files and the JSONL command log.
//! - [`host`] — [`host::SimHost`], the embedding API used by the tools CLI,
//!   tests, and anything else that drives the simulation without a client.

pub mod clock;
pub mod command;
pub mod config;
pub mod crisis;
pub mod economy;
pub mod forces;
pub mod forecast;
pub mod host;
pub mod ids;
pub mod jobs;
pub mod map;
pub mod order;
pub mod persistence;
pub mod politics;
pub mod presence;
pub mod snapshot;
pub mod state;
pub mod warfare;

use bevy::app::{App, Plugin};

pub use clock::{CampaignClock, DailyTick, MonthlyPulse, TickSet, YearlyPulse, advance_one_day};
pub use command::{CommandEnvelope, CommandRejection, PlayerCommand};
pub use config::CampaignConfig;
pub use economy::OrgResources;
pub use forces::{ArmyRecord, ForcesIndex, ShipRecord};
pub use forecast::{ForecastResult, ForecastRisk, JobForecast, Permille};
pub use host::SimHost;
pub use ids::{ArmyId, BodyId, CharacterId, JobId, OfficeId, OrgId, ProvinceId, ShipId, TitleId};
pub use jobs::{
    ActiveJob, JobRejection, JobTarget, JobsIndex, LogChannel, LogEntry, LogSubject, MessageLog,
    PendingPopups,
};
pub use map::{BodyRecord, DisplayName, GeoPosition, MapIndex, ProvinceRecord};
pub use order::ProvincialOrder;
pub use politics::{
    CampaignOver, CharacterRecord, OfficeRecord, OrgRecord, PlayerHouse, PoliticsIndex,
    TitleHolder, TitleKind, TitleRecord, opinion_between,
};
pub use presence::{CharacterLocation, Location};
pub use snapshot::{CampaignSnapshot, CampaignState, SNAPSHOT_FORMAT_VERSION, SnapshotError};

/// Root plugin installing the authoritative simulation into a Bevy [`App`].
///
/// Clients and the headless host both install exactly this plugin, which is
/// what keeps native, web, and test simulations identical. Installing the
/// plugin prepares systems and schedules; an actual campaign starts when
/// [`state::start_campaign`] inserts the campaign resources.
pub struct AeonSimPlugin;

impl Plugin for AeonSimPlugin {
    fn build(&self, app: &mut App) {
        clock::install(app);
        command::install(app);
        politics::install(app);
        jobs::install(app);
        economy::install(app);
        presence::install(app);
        forces::install(app);
        order::install(app);
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
