//! Authoritative campaign resources.

use aeon_core::id::IdAllocator;
use bevy::prelude::{Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::CampaignClock;
use crate::command::{CommandLog, PendingCommands};
use crate::config::CampaignConfig;

/// The campaign seed every derived random stream folds in.
#[derive(Resource, Copy, Clone, Debug, PartialEq, Eq)]
pub struct CampaignSeed(pub u64);

/// Player-facing campaign metadata.
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignMeta {
    /// The campaign's display name.
    pub name: String,
}

/// The campaign's stable-ID allocator.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CampaignIds(pub IdAllocator);

/// Starts a fresh campaign in this world.
///
/// Inserts every authoritative campaign resource. The plugin must already be
/// installed; a world hosts at most one campaign.
pub fn start_campaign(world: &mut World, config: CampaignConfig) {
    world.insert_resource(CampaignSeed(config.seed));
    world.insert_resource(CampaignMeta { name: config.name });
    world.insert_resource(CampaignClock {
        start_date: config.start_date,
        date: config.start_date,
    });
    world.insert_resource(CampaignIds::default());
    world.insert_resource(PendingCommands::default());
    world.insert_resource(CommandLog::default());
}
