//! Individually tracked ships and persistent armies.
//!
//! Ships spawn from authored content, dock at provinces, and travel
//! between them; capital ships have simulated captains. Armies are
//! created during play by army-formation jobs that commit a general,
//! manpower, and supplies, and persist until disbanded or destroyed.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::model::ShipClass;
use aeon_data::{ContentKey, ContentSet};
use bevy::app::App;
use bevy::prelude::{Component, Entity, IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::MonthlyPulse;
use crate::ids::{ArmyId, CharacterId, OrgId, ProvinceId, ShipId};
use crate::map::MapIndex;
use crate::politics::PoliticsIndex;
use crate::state::CampaignIds;

/// Where a ship is.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipLocation {
    /// Docked at a province.
    Docked(ProvinceId),
    /// Under way to a province.
    Transit {
        /// Destination.
        to: ProvinceId,
        /// Arrival day.
        arrives: GameDate,
    },
}

/// An individually tracked starship.
#[derive(Component, Clone, Debug)]
pub struct ShipRecord {
    /// Stable ID.
    pub id: ShipId,
    /// Authored content key.
    pub key: ContentKey,
    /// Display name.
    pub name: String,
    /// Broad class.
    pub class: ShipClass,
    /// Owning organisation.
    pub owner: OrgId,
    /// Captain; capital ships always have one.
    pub captain: Option<CharacterId>,
    /// Current location.
    pub location: ShipLocation,
    /// The province this ship is blockading, if any.
    pub blockading: Option<ProvinceId>,
}

/// A persistent army.
#[derive(Component, Clone, Debug)]
pub struct ArmyRecord {
    /// Stable ID.
    pub id: ArmyId,
    /// Display name.
    pub name: String,
    /// Owning organisation.
    pub owner: OrgId,
    /// The general commanding it.
    pub general: CharacterId,
    /// Soldiers under arms.
    pub manpower: i64,
    /// Supplies in train.
    pub supplies: i64,
    /// The province it stands in.
    pub location: ProvinceId,
    /// The order followed while idle.
    pub standing_order: crate::warfare::StandingOrder,
}

/// Lookup for ships and armies.
#[derive(Resource, Clone, Debug, Default)]
pub struct ForcesIndex {
    /// Ships by stable ID.
    pub ships: BTreeMap<ShipId, Entity>,
    /// Armies by stable ID.
    pub armies: BTreeMap<ArmyId, Entity>,
    /// Ship IDs by authored key.
    pub ship_keys: BTreeMap<ContentKey, ShipId>,
    /// How many armies each organisation has ever raised (for names).
    pub armies_raised: BTreeMap<OrgId, u32>,
}

/// Spawns authored ships and starting armies for a fresh campaign.
///
/// IDs are allocated in content-key order (ships then armies) so the
/// starting forces are identical across runs of the same content.
pub fn spawn_from_content(world: &mut World, content: &ContentSet) {
    let mut index = ForcesIndex::default();
    let map_index = world.resource::<MapIndex>().clone();
    let politics = world.resource::<PoliticsIndex>().clone();

    for (key, def) in &content.ships {
        let id: ShipId = world.resource_mut::<CampaignIds>().0.allocate();
        let entity = world
            .spawn(ShipRecord {
                id,
                key: key.clone(),
                name: def.name.clone(),
                class: def.class,
                owner: politics.org_keys[&def.owner],
                captain: def.captain.as_ref().map(|c| politics.character_keys[c]),
                location: ShipLocation::Docked(map_index.province_keys[&def.location]),
                blockading: None,
            })
            .id();
        index.ships.insert(id, entity);
        index.ship_keys.insert(key.clone(), id);
    }

    for def in content.armies.values() {
        let owner = politics.org_keys[&def.owner];
        let id: ArmyId = world.resource_mut::<CampaignIds>().0.allocate();
        let entity = world
            .spawn(ArmyRecord {
                id,
                name: def.name.clone(),
                owner,
                general: politics.character_keys[&def.general],
                manpower: def.manpower,
                supplies: def.supplies,
                location: map_index.province_keys[&def.province],
                standing_order: crate::warfare::StandingOrder::default(),
            })
            .id();
        index.armies.insert(id, entity);
        // Count authored armies toward the owner's raised total so
        // later mustered armies are numbered after them.
        *index.armies_raised.entry(owner).or_default() += 1;
    }

    world.insert_resource(index);
}

/// Total manpower standing in a province, and the owner of its strongest
/// army there.
///
/// Armies are visited in stable-ID order, so a tie between equal armies
/// always answers with the earliest-raised one rather than whatever
/// iteration order the ECS happened to have.
pub fn garrison_in(world: &World, province: ProvinceId) -> (i64, Option<OrgId>) {
    let Some(index) = world.get_resource::<ForcesIndex>() else {
        return (0, None);
    };
    let mut total = 0;
    let mut strongest: Option<(i64, OrgId)> = None;
    for entity in index.armies.values() {
        let Some(army) = world.get::<ArmyRecord>(*entity) else {
            continue;
        };
        if army.location != province {
            continue;
        }
        total += army.manpower;
        if strongest.is_none_or(|(men, _)| army.manpower > men) {
            strongest = Some((army.manpower, army.owner));
        }
    }
    (total, strongest.map(|(_, org)| org))
}

/// Creates a persistent army. Callers must already have deducted the
/// manpower and supplies from the owner.
pub fn form_army(
    world: &mut World,
    owner: OrgId,
    general: CharacterId,
    manpower: i64,
    supplies: i64,
    location: ProvinceId,
) -> ArmyId {
    let ordinal = {
        let mut index = world.resource_mut::<ForcesIndex>();
        let counter = index.armies_raised.entry(owner).or_default();
        *counter += 1;
        *counter
    };
    let owner_name = crate::access::org_name(world, owner);
    let id: ArmyId = world.resource_mut::<CampaignIds>().0.allocate();
    let entity = world
        .spawn(ArmyRecord {
            id,
            name: format!("{ordinal}. {owner_name} Levy"),
            owner,
            general,
            manpower,
            supplies,
            location,
            standing_order: crate::warfare::StandingOrder::default(),
        })
        .id();
    world
        .resource_mut::<ForcesIndex>()
        .armies
        .insert(id, entity);
    id
}

/// Disbands an army, returning its soldiers to the owner's pool.
pub fn disband_army(world: &mut World, army: ArmyId) {
    let Some(entity) = world.resource::<ForcesIndex>().armies.get(&army).copied() else {
        return;
    };
    let record = world.get::<ArmyRecord>(entity).cloned();
    if let Some(record) = record {
        let org_entity = crate::access::org_entity(world, record.owner).expect("indexed");
        if let Some(mut resources) = world.get_mut::<crate::economy::OrgResources>(org_entity) {
            resources.manpower += record.manpower;
        }
    }
    world.despawn(entity);
    world.resource_mut::<ForcesIndex>().armies.remove(&army);
}

/// Monthly: ships draw supplies from their owners; armies consume their
/// trains and waste away when the train runs dry.
pub fn monthly_upkeep(world: &mut World) {
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };

    // Ships: one supply per ship from the owning organisation.
    for entity in index.ships.values() {
        let Some(owner) = world.get::<ShipRecord>(*entity).map(|s| s.owner) else {
            continue;
        };
        let org_entity = crate::access::org_entity(world, owner).expect("indexed");
        if let Some(mut resources) = world.get_mut::<crate::economy::OrgResources>(org_entity) {
            resources.supplies = (resources.supplies - 1).max(0);
        }
    }

    // Armies: eat from their own trains; starvation causes attrition.
    for entity in index.armies.values() {
        let Some(mut army) = world.get_mut::<ArmyRecord>(*entity) else {
            continue;
        };
        let consumption = 1 + army.manpower / 1000;
        if army.supplies >= consumption {
            army.supplies -= consumption;
        } else {
            army.supplies = 0;
            army.manpower -= (army.manpower / 20).max(1);
        }
    }

    // Armies that starved away disband.
    let dead: Vec<ArmyId> = index
        .armies
        .iter()
        .filter(|(_, entity)| {
            world
                .get::<ArmyRecord>(**entity)
                .is_some_and(|a| a.manpower <= 0)
        })
        .map(|(id, _)| *id)
        .collect();
    for army in dead {
        let entity = world.resource::<ForcesIndex>().armies[&army];
        world.despawn(entity);
        world.resource_mut::<ForcesIndex>().armies.remove(&army);
    }
}

// ---------------------------------------------------------------------------
// Snapshot state
// ---------------------------------------------------------------------------

/// Serialised ship.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipState {
    /// Stable ID.
    pub id: ShipId,
    /// Authored key.
    pub key: ContentKey,
    /// Captain.
    pub captain: Option<CharacterId>,
    /// Location.
    pub location: ShipLocation,
    /// Blockade target.
    #[serde(default)]
    pub blockading: Option<ProvinceId>,
}

/// Serialised army.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmyState {
    /// Stable ID.
    pub id: ArmyId,
    /// Display name.
    pub name: String,
    /// Owner.
    pub owner: OrgId,
    /// General.
    pub general: CharacterId,
    /// Soldiers.
    pub manpower: i64,
    /// Supplies.
    pub supplies: i64,
    /// Location.
    pub location: ProvinceId,
    /// Standing order.
    #[serde(default)]
    pub standing_order: crate::warfare::StandingOrder,
}

/// The complete serialised forces state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForcesState {
    /// Ships in ID order.
    pub ships: Vec<ShipState>,
    /// Armies in ID order.
    pub armies: Vec<ArmyState>,
    /// Army-name counters per organisation.
    pub armies_raised: Vec<(OrgId, u32)>,
}

/// Captures ships and armies for a snapshot.
pub fn capture_forces(world: &World) -> ForcesState {
    let Some(index) = world.get_resource::<ForcesIndex>() else {
        return ForcesState::default();
    };
    ForcesState {
        ships: index
            .ships
            .values()
            .map(|entity| {
                let ship = world.get::<ShipRecord>(*entity).expect("indexed");
                ShipState {
                    id: ship.id,
                    key: ship.key.clone(),
                    captain: ship.captain,
                    location: ship.location,
                    blockading: ship.blockading,
                }
            })
            .collect(),
        armies: index
            .armies
            .values()
            .map(|entity| {
                let army = world.get::<ArmyRecord>(*entity).expect("indexed");
                ArmyState {
                    id: army.id,
                    name: army.name.clone(),
                    owner: army.owner,
                    general: army.general,
                    manpower: army.manpower,
                    supplies: army.supplies,
                    location: army.location,
                    standing_order: army.standing_order,
                }
            })
            .collect(),
        armies_raised: index
            .armies_raised
            .iter()
            .map(|(org, count)| (*org, *count))
            .collect(),
    }
}

/// Respawns ships and armies from a snapshot against verified content.
pub fn restore_forces(world: &mut World, state: &ForcesState, content: &ContentSet) {
    let politics = world.resource::<PoliticsIndex>().clone();
    let mut index = ForcesIndex::default();

    for ship in &state.ships {
        let def = content
            .ships
            .get(&ship.key)
            .expect("hash-verified content defines every persisted ship");
        let entity = world
            .spawn(ShipRecord {
                id: ship.id,
                key: ship.key.clone(),
                name: def.name.clone(),
                class: def.class,
                owner: politics.org_keys[&def.owner],
                captain: ship.captain,
                location: ship.location,
                blockading: ship.blockading,
            })
            .id();
        index.ships.insert(ship.id, entity);
        index.ship_keys.insert(ship.key.clone(), ship.id);
    }
    for army in &state.armies {
        let entity = world
            .spawn(ArmyRecord {
                id: army.id,
                name: army.name.clone(),
                owner: army.owner,
                general: army.general,
                manpower: army.manpower,
                supplies: army.supplies,
                location: army.location,
                standing_order: army.standing_order,
            })
            .id();
        index.armies.insert(army.id, entity);
    }
    index.armies_raised = state.armies_raised.iter().copied().collect();
    world.insert_resource(index);
}

/// Daily: ships whose arrival day has come dock at their destination.
pub fn dock_arrivals(world: &mut World) {
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    let date = world.resource::<crate::clock::CampaignClock>().date;
    for entity in index.ships.values() {
        let due = matches!(
            world.get::<ShipRecord>(*entity).map(|s| s.location),
            Some(ShipLocation::Transit { arrives, .. }) if arrives <= date
        );
        if due
            && let Some(mut ship) = world.get_mut::<ShipRecord>(*entity)
            && let ShipLocation::Transit { to, .. } = ship.location
        {
            ship.location = ShipLocation::Docked(to);
        }
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        MonthlyPulse,
        monthly_upkeep.after(crate::economy::monthly_economy),
    );
    app.add_systems(
        crate::clock::DailyTick,
        dock_arrivals
            .in_set(crate::clock::TickSet::Simulation)
            .before(crate::jobs::resolve_due_jobs),
    );
}
