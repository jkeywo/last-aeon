//! Individually tracked ships and persistent armies.
//!
//! Ships spawn from authored content, dock at provinces, and travel
//! between them; capital ships have simulated captains. Armies are
//! created during play by army-formation assignments that commit a general,
//! manpower, and supplies, and persist until disbanded or destroyed.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::model::ShipClass;
use aeon_data::{ContentKey, ContentSet};
use bevy::app::App;
use bevy::prelude::{Component, Entity, IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::MonthlyPulse;
use crate::ids::{ArmyId, CharacterId, OrgId, ProvinceId, ShipId, WarId};
use crate::map::MapIndex;
use crate::politics::PoliticsIndex;
use crate::state::CampaignIds;
use crate::text::TextDb;

/// Where a ship is.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipLocation {
    /// Docked at a province.
    Docked(ProvinceId),
    /// Under way to a province.
    OnRoute {
        /// Segment origin.
        from: ProvinceId,
        /// Segment destination.
        to: ProvinceId,
        /// Arrival day.
        arrives: GameDate,
    },
}

/// Concrete location of one whole army.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArmyLocation {
    Province(ProvinceId),
    Embarked(ShipId),
}

impl ArmyLocation {
    pub fn province(self) -> Option<ProvinceId> {
        match self {
            Self::Province(province) => Some(province),
            Self::Embarked(_) => None,
        }
    }
}

/// An individually tracked starship.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blockade {
    /// Province whose routes and order are being suppressed.
    pub province: ProvinceId,
    /// Exact active formal-war occurrence authorising the blockade.
    pub war: WarId,
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
    /// Optional deputy commander.
    pub first_officer: Option<CharacterId>,
    /// Whole-army troop capacity.
    pub troop_capacity: i64,
    /// Temporary simulation-controlled civilian vessel.
    pub personal_transport: bool,
    /// Retained automatic refuge.
    pub retreat_destination: Option<ProvinceId>,
    /// Active work is paused while the primary command is vacant.
    pub orders_suspended: bool,
    /// Current location.
    pub location: ShipLocation,
    /// The exact war-bound blockade this ship is maintaining, if any.
    pub blockading: Option<Blockade>,
    /// The standing trade route this ship plies, if any. Transports only.
    pub route: Option<crate::trade::TradeRoute>,
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
    pub general: Option<CharacterId>,
    /// Optional deputy commander.
    pub lieutenant: Option<CharacterId>,
    /// Soldiers under arms.
    pub manpower: i64,
    /// Supplies in train.
    pub supplies: i64,
    /// The province it stands in.
    pub location: ArmyLocation,
    /// Retained automatic refuge.
    pub retreat_destination: Option<ProvinceId>,
    /// Active work is paused while the primary command is vacant.
    pub orders_suspended: bool,
    /// The order followed while idle.
    pub standing_order: crate::warfare::StandingOrders,
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
                first_officer: def
                    .first_officer
                    .as_ref()
                    .map(|c| politics.character_keys[c]),
                troop_capacity: def.troop_capacity,
                personal_transport: false,
                retreat_destination: None,
                orders_suspended: def.captain.is_none(),
                location: ShipLocation::Docked(map_index.province_keys[&def.location]),
                blockading: None,
                route: None,
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
                general: def
                    .general
                    .as_ref()
                    .map(|general| politics.character_keys[general]),
                lieutenant: def
                    .lieutenant
                    .as_ref()
                    .map(|lieutenant| politics.character_keys[lieutenant]),
                manpower: def.manpower,
                supplies: def.supplies,
                location: ArmyLocation::Province(map_index.province_keys[&def.province]),
                retreat_destination: None,
                orders_suspended: def.general.is_none(),
                standing_order: crate::warfare::StandingOrders::default(),
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
        if army.location != ArmyLocation::Province(province) {
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
    let army_name = world.resource::<TextDb>().format(
        "sim.forces.levy-name",
        &[("ordinal", &ordinal.to_string()), ("house", &owner_name)],
    );
    let id: ArmyId = world.resource_mut::<CampaignIds>().0.allocate();
    let entity = world
        .spawn(ArmyRecord {
            id,
            name: army_name,
            owner,
            general: Some(general),
            lieutenant: None,
            manpower,
            supplies,
            location: ArmyLocation::Province(location),
            retreat_destination: None,
            orders_suspended: false,
            standing_order: crate::warfare::StandingOrders::default(),
        })
        .id();
    world
        .resource_mut::<ForcesIndex>()
        .armies
        .insert(id, entity);
    id
}

/// Spawns a visible, simulation-owned vessel for one civilian crossing.
/// Its allocated ID is never reused, even after the vessel is retired.
pub fn spawn_personal_transport(world: &mut World, owner: OrgId, location: ProvinceId) -> ShipId {
    let id: ShipId = world.resource_mut::<CampaignIds>().0.allocate();
    let key = ContentKey::new(&format!("personal-transport-{}", id.raw()))
        .expect("generated personal transport key is valid");
    let entity = world
        .spawn(ShipRecord {
            id,
            key: key.clone(),
            name: format!("Personal Transport {}", id.raw()),
            class: ShipClass::Transport,
            owner,
            captain: None,
            first_officer: None,
            troop_capacity: 0,
            personal_transport: true,
            retreat_destination: None,
            orders_suspended: false,
            location: ShipLocation::Docked(location),
            blockading: None,
            route: None,
        })
        .id();
    let mut index = world.resource_mut::<ForcesIndex>();
    index.ships.insert(id, entity);
    index.ship_keys.insert(key, id);
    id
}

/// Permanently retires a completed personal transport.
pub fn retire_personal_transport(world: &mut World, ship: ShipId) {
    let Some(entity) = world.resource::<ForcesIndex>().ships.get(&ship).copied() else {
        return;
    };
    if !world
        .get::<ShipRecord>(entity)
        .is_some_and(|record| record.personal_transport)
    {
        return;
    }
    let key = world
        .get::<ShipRecord>(entity)
        .map(|record| record.key.clone());
    world.despawn(entity);
    let mut index = world.resource_mut::<ForcesIndex>();
    index.ships.remove(&ship);
    if let Some(key) = key {
        index.ship_keys.remove(&key);
    }
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
        let Some((owner, personal)) = world
            .get::<ShipRecord>(*entity)
            .map(|s| (s.owner, s.personal_transport))
        else {
            continue;
        };
        if personal {
            continue;
        }
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
    /// Persisted because dynamic personal transports have no authored def.
    pub name: String,
    pub class: ShipClass,
    pub owner: OrgId,
    /// Captain.
    pub captain: Option<CharacterId>,
    #[serde(default)]
    pub first_officer: Option<CharacterId>,
    #[serde(default)]
    pub troop_capacity: i64,
    #[serde(default)]
    pub personal_transport: bool,
    #[serde(default)]
    pub retreat_destination: Option<ProvinceId>,
    #[serde(default)]
    pub orders_suspended: bool,
    #[serde(default)]
    pub appointment: Option<crate::officers::AppointmentJob>,
    /// Location.
    pub location: ShipLocation,
    /// Exact war-bound blockade.
    #[serde(default)]
    pub blockading: Option<Blockade>,
    /// Standing trade route.
    #[serde(default)]
    pub route: Option<crate::trade::TradeRoute>,
    #[serde(default)]
    pub journey: Option<crate::routes::Journey>,
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
    pub general: Option<CharacterId>,
    #[serde(default)]
    pub lieutenant: Option<CharacterId>,
    /// Soldiers.
    pub manpower: i64,
    /// Supplies.
    pub supplies: i64,
    /// Location.
    pub location: ArmyLocation,
    #[serde(default)]
    pub retreat_destination: Option<ProvinceId>,
    #[serde(default)]
    pub orders_suspended: bool,
    #[serde(default)]
    pub appointment: Option<crate::officers::AppointmentJob>,
    #[serde(default)]
    pub transport_job: Option<crate::officers::TransportJob>,
    /// Standing order.
    #[serde(default)]
    pub standing_order: crate::warfare::StandingOrders,
    #[serde(default)]
    pub journey: Option<crate::routes::Journey>,
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
                    name: ship.name.clone(),
                    class: ship.class,
                    owner: ship.owner,
                    captain: ship.captain,
                    first_officer: ship.first_officer,
                    troop_capacity: ship.troop_capacity,
                    personal_transport: ship.personal_transport,
                    retreat_destination: ship.retreat_destination,
                    orders_suspended: ship.orders_suspended,
                    appointment: world
                        .get::<crate::officers::AppointmentJob>(*entity)
                        .cloned(),
                    location: ship.location,
                    blockading: ship.blockading,
                    route: ship.route.clone(),
                    journey: world.get::<crate::routes::Journey>(*entity).cloned(),
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
                    lieutenant: army.lieutenant,
                    manpower: army.manpower,
                    supplies: army.supplies,
                    location: army.location,
                    retreat_destination: army.retreat_destination,
                    orders_suspended: army.orders_suspended,
                    appointment: world
                        .get::<crate::officers::AppointmentJob>(*entity)
                        .cloned(),
                    transport_job: world.get::<crate::officers::TransportJob>(*entity).cloned(),
                    standing_order: army.standing_order.clone(),
                    journey: world.get::<crate::routes::Journey>(*entity).cloned(),
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
        let def = content.ships.get(&ship.key);
        assert!(
            ship.personal_transport || def.is_some(),
            "persistent ship is authored"
        );
        let entity = world
            .spawn(ShipRecord {
                id: ship.id,
                key: ship.key.clone(),
                name: def.map_or_else(|| ship.name.clone(), |def| def.name.clone()),
                class: def.map_or(ship.class, |def| def.class),
                owner: def.map_or(ship.owner, |def| politics.org_keys[&def.owner]),
                captain: ship.captain,
                first_officer: ship.first_officer,
                troop_capacity: ship.troop_capacity,
                personal_transport: ship.personal_transport,
                retreat_destination: ship.retreat_destination,
                orders_suspended: ship.orders_suspended,
                location: ship.location,
                blockading: ship.blockading,
                route: ship.route.clone(),
            })
            .id();
        if let Some(journey) = &ship.journey {
            world.entity_mut(entity).insert(journey.clone());
        }
        if let Some(appointment) = &ship.appointment {
            world.entity_mut(entity).insert(appointment.clone());
        }
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
                lieutenant: army.lieutenant,
                manpower: army.manpower,
                supplies: army.supplies,
                location: army.location,
                retreat_destination: army.retreat_destination,
                orders_suspended: army.orders_suspended,
                standing_order: army.standing_order.clone(),
            })
            .id();
        if let Some(journey) = &army.journey {
            world.entity_mut(entity).insert(journey.clone());
        }
        if let Some(appointment) = &army.appointment {
            world.entity_mut(entity).insert(appointment.clone());
        }
        if let Some(job) = &army.transport_job {
            world.entity_mut(entity).insert(job.clone());
        }
        index.armies.insert(army.id, entity);
    }
    index.armies_raised = state.armies_raised.iter().copied().collect();
    world.insert_resource(index);
}

/// Daily: advance ships over their explicit authored route segments.
pub fn dock_arrivals(world: &mut World) {
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    let date = world.resource::<crate::clock::CampaignClock>().date;
    for entity in index.ships.values() {
        let journey = world.get::<crate::routes::Journey>(*entity).cloned();
        let Some(mut journey) = journey else { continue };
        if journey
            .current
            .as_ref()
            .is_some_and(|progress| progress.arrives <= date)
        {
            let to = journey.current.as_ref().expect("checked").leg.to;
            if let Some(mut ship) = world.get_mut::<ShipRecord>(*entity) {
                ship.location = ShipLocation::Docked(to);
            }
            journey.current = None;
        }
        let held_for_appointment = world
            .get::<crate::officers::AppointmentJob>(*entity)
            .is_some();
        if journey.current.is_none() && !journey.remaining.is_empty() && !held_for_appointment {
            let leg = journey.remaining.remove(0);
            let multiplier = if journey.speed == crate::routes::JourneySpeed::Half {
                2
            } else {
                1
            };
            let arrives = date.add_days(i64::from(leg.travel_days) * multiplier);
            if let Some(mut ship) = world.get_mut::<ShipRecord>(*entity) {
                ship.location = ShipLocation::OnRoute {
                    from: leg.from,
                    to: leg.to,
                    arrives,
                };
            }
            journey.current = Some(crate::routes::RouteProgress { leg, arrives });
        }
        if journey.current.is_none() && journey.remaining.is_empty() {
            world.entity_mut(*entity).remove::<crate::routes::Journey>();
        } else {
            world.entity_mut(*entity).insert(journey);
        }
    }
}

/// Daily: advance marching armies over explicit surface edges. Embarked
/// armies inherit their ship's location and never run an independent route.
pub fn advance_army_journeys(world: &mut World) {
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    let date = world.resource::<crate::clock::CampaignClock>().date;
    for entity in index.armies.values() {
        if world
            .get::<ArmyRecord>(*entity)
            .is_some_and(|army| matches!(army.location, ArmyLocation::Embarked(_)))
        {
            continue;
        }
        let Some(mut journey) = world.get::<crate::routes::Journey>(*entity).cloned() else {
            continue;
        };
        if journey
            .current
            .as_ref()
            .is_some_and(|progress| progress.arrives <= date)
        {
            let to = journey.current.take().expect("checked").leg.to;
            let officers = world
                .get::<ArmyRecord>(*entity)
                .map(|army| [army.general, army.lieutenant])
                .unwrap_or([None, None]);
            if let Some(mut army) = world.get_mut::<ArmyRecord>(*entity) {
                army.location = ArmyLocation::Province(to);
            }
            for officer in officers.into_iter().flatten() {
                if let Some(character) = crate::access::character_entity(world, officer) {
                    world
                        .entity_mut(character)
                        .insert(crate::presence::CharacterLocation(
                            crate::presence::Location::Province(to),
                        ));
                }
            }
        }
        let held = world
            .get::<crate::officers::AppointmentJob>(*entity)
            .is_some()
            || world
                .get::<crate::officers::TransportJob>(*entity)
                .is_some();
        if journey.current.is_none() && !journey.remaining.is_empty() && !held {
            let leg = journey.remaining.remove(0);
            let multiplier = if journey.speed == crate::routes::JourneySpeed::Half {
                2
            } else {
                1
            };
            let arrives = date.add_days(i64::from(leg.travel_days) * multiplier);
            journey.current = Some(crate::routes::RouteProgress { leg, arrives });
        }
        if journey.current.is_none() && journey.remaining.is_empty() {
            world.entity_mut(*entity).remove::<crate::routes::Journey>();
        } else {
            world.entity_mut(*entity).insert(journey);
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
        (dock_arrivals, advance_army_journeys)
            .chain()
            .in_set(crate::clock::TickSet::Simulation)
            .before(crate::assignments::resolve_due_assignments),
    );
}
