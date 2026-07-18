//! Authoritative campaign resources.

use std::sync::Arc;

use aeon_core::id::IdAllocator;
use aeon_data::ContentSet;
use bevy::prelude::{Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::CampaignClock;
use crate::command::{CommandLog, PendingCommands};
use crate::config::CampaignConfig;

/// The loaded authored-content database this campaign runs on.
///
/// Snapshots record the content hash, and restoring requires content with
/// the same hash: a save is only meaningful against the content that
/// produced it.
#[derive(Resource, Clone)]
pub struct ContentDb(pub Arc<ContentSet>);

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

/// Starts a fresh campaign running on authored content.
///
/// Attaches the content database and spawns the map with stable IDs
/// allocated in content-key order.
pub fn start_campaign_with_content(
    world: &mut World,
    config: CampaignConfig,
    content: Arc<ContentSet>,
) {
    start_campaign(world, config);
    crate::map::spawn_from_content(world, &content);
    crate::politics::spawn_from_content(world, &content);
    crate::jobs::init_jobs(world);
    world.insert_resource(ContentDb(content));
}
