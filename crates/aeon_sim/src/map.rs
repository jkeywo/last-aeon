//! The authoritative local-system map: bodies and provinces as ECS
//! entities.
//!
//! Map entities spawn from authored content when a campaign starts,
//! receiving stable IDs in content-key order (deterministic). Snapshots
//! persist the ID-to-key bindings; restoring respawns the same entities
//! with the same IDs from the hash-verified content.

use std::collections::BTreeMap;

use aeon_data::model::{BodyDef, BodyKind, ProvinceDef};
use aeon_data::{ContentKey, ContentSet};
use bevy::prelude::{Component, Entity, Resource, World};
use serde::{Deserialize, Serialize};

use crate::ids::{BodyId, ProvinceId};
use crate::state::CampaignIds;

/// A celestial body's identity and orbital facts.
///
/// Everything here is authored-content data plus the stable ID; mutable
/// body state (should any arrive) gets its own components.
#[derive(Component, Clone, Debug)]
pub struct BodyRecord {
    /// Stable ID.
    pub id: BodyId,
    /// Authored content key.
    pub key: ContentKey,
    /// What kind of body this is.
    pub kind: BodyKind,
    /// Visual radius in kilometres.
    pub radius_km: u32,
    /// Orbit radius around the parent in megametres.
    pub orbit_radius_mm: u32,
    /// Days per orbit; zero for the primary.
    pub orbit_days: u32,
    /// The body this one orbits.
    pub parent: Option<BodyId>,
}

/// A province's identity and location.
#[derive(Component, Clone, Debug)]
pub struct ProvinceRecord {
    /// Stable ID.
    pub id: ProvinceId,
    /// Authored content key.
    pub key: ContentKey,
    /// The body this province is on.
    pub body: BodyId,
}

/// A player-facing display name.
#[derive(Component, Clone, Debug)]
pub struct DisplayName(pub String);

/// A position on a body's surface, in millidegrees.
#[derive(Component, Copy, Clone, Debug)]
pub struct GeoPosition {
    /// Latitude, -90000..=90000.
    pub latitude_mdeg: i32,
    /// Longitude, -180000..180000.
    pub longitude_mdeg: i32,
}

/// Lookup between stable IDs, content keys, and live ECS entities.
///
/// BTreeMaps so every iteration over the map is in stable-ID order.
#[derive(Resource, Clone, Debug, Default)]
pub struct MapIndex {
    /// Bodies by stable ID.
    pub bodies: BTreeMap<BodyId, Entity>,
    /// Provinces by stable ID.
    pub provinces: BTreeMap<ProvinceId, Entity>,
    /// Body IDs by content key.
    pub body_keys: BTreeMap<ContentKey, BodyId>,
    /// Province IDs by content key.
    pub province_keys: BTreeMap<ContentKey, ProvinceId>,
    /// Content keys by body ID (inverse of `body_keys`).
    pub body_ids: BTreeMap<BodyId, ContentKey>,
    /// Content keys by province ID (inverse of `province_keys`).
    pub province_ids: BTreeMap<ProvinceId, ContentKey>,
}

/// The persisted ID-to-key bindings of the map.
///
/// Definition data lives in content (hash-bound to the snapshot), so only
/// the bindings persist. Sorted by stable ID.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapState {
    /// Body bindings in ID order.
    pub bodies: Vec<(BodyId, ContentKey)>,
    /// Province bindings in ID order.
    pub provinces: Vec<(ProvinceId, ContentKey)>,
    /// Provincial order in ID order; absent entries take the default.
    #[serde(default)]
    pub order: Vec<(ProvinceId, crate::order::ProvincialOrder)>,
}

fn spawn_body(world: &mut World, id: BodyId, def: &BodyDef, parent: Option<BodyId>) -> Entity {
    world
        .spawn((
            BodyRecord {
                id,
                key: def.key.clone(),
                kind: def.kind,
                radius_km: def.radius_km,
                orbit_radius_mm: def.orbit_radius_mm,
                orbit_days: def.orbit_days,
                parent,
            },
            DisplayName(def.name.clone()),
        ))
        .id()
}

fn spawn_province(world: &mut World, id: ProvinceId, def: &ProvinceDef, body: BodyId) -> Entity {
    world
        .spawn((
            ProvinceRecord {
                id,
                key: def.key.clone(),
                body,
            },
            DisplayName(def.name.clone()),
            GeoPosition {
                latitude_mdeg: def.latitude_mdeg,
                longitude_mdeg: def.longitude_mdeg,
            },
            crate::order::ProvincialOrder::default(),
        ))
        .id()
}

/// Spawns the map for a fresh campaign, allocating stable IDs in
/// content-key order.
pub fn spawn_from_content(world: &mut World, content: &ContentSet) {
    let mut index = MapIndex::default();

    // Pass one: allocate body IDs in key order so parents can resolve
    // regardless of alphabetical position.
    for key in content.bodies.keys() {
        let id: BodyId = world.resource_mut::<CampaignIds>().0.allocate();
        index.body_keys.insert(key.clone(), id);
        index.body_ids.insert(id, key.clone());
    }
    for (key, def) in &content.bodies {
        let id = index.body_keys[key];
        let parent = def.parent.as_ref().map(|p| index.body_keys[p]);
        let entity = spawn_body(world, id, def, parent);
        index.bodies.insert(id, entity);
    }

    for (key, def) in &content.provinces {
        let id: ProvinceId = world.resource_mut::<CampaignIds>().0.allocate();
        let body = index.body_keys[&def.body];
        let entity = spawn_province(world, id, def, body);
        index.province_keys.insert(key.clone(), id);
        index.province_ids.insert(id, key.clone());
        index.provinces.insert(id, entity);
    }

    world.insert_resource(index);
}

/// Captures the map bindings for a snapshot.
pub fn capture_map(world: &World) -> MapState {
    match world.get_resource::<MapIndex>() {
        None => MapState::default(),
        Some(index) => MapState {
            bodies: index
                .body_ids
                .iter()
                .map(|(id, key)| (*id, key.clone()))
                .collect(),
            provinces: index
                .province_ids
                .iter()
                .map(|(id, key)| (*id, key.clone()))
                .collect(),
            order: index
                .provinces
                .iter()
                .filter_map(|(id, entity)| {
                    world
                        .get::<crate::order::ProvincialOrder>(*entity)
                        .map(|state| (*id, *state))
                })
                .collect(),
        },
    }
}

/// Respawns the map from persisted bindings against hash-verified content.
///
/// # Panics
/// Panics if a binding references a key missing from the content; the
/// caller has already verified the content hash, so that would mean the
/// snapshot's own state hash check failed to do its job.
pub fn restore_map(world: &mut World, state: &MapState, content: &ContentSet) {
    let mut index = MapIndex::default();

    for (id, key) in &state.bodies {
        index.body_keys.insert(key.clone(), *id);
        index.body_ids.insert(*id, key.clone());
    }
    for (id, key) in &state.bodies {
        let def = content
            .bodies
            .get(key)
            .expect("hash-verified content defines every persisted body");
        let parent = def.parent.as_ref().map(|p| index.body_keys[p]);
        let entity = spawn_body(world, *id, def, parent);
        index.bodies.insert(*id, entity);
    }

    for (id, key) in &state.provinces {
        let def = content
            .provinces
            .get(key)
            .expect("hash-verified content defines every persisted province");
        let body = index.body_keys[&def.body];
        let entity = spawn_province(world, *id, def, body);
        index.province_keys.insert(key.clone(), *id);
        index.province_ids.insert(*id, key.clone());
        index.provinces.insert(*id, entity);
    }

    // Restore persisted order over the spawned defaults.
    for (id, order) in &state.order {
        if let Some(entity) = index.provinces.get(id) {
            world.entity_mut(*entity).insert(*order);
        }
    }

    world.insert_resource(index);
}
