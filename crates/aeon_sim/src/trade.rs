//! The goods economy: what worlds make, want, and trade.
//!
//! Provinces produce and consume typed goods. Within a body — a planet,
//! moon, or starbase — the goods net freely: a body's balance in each
//! good is what its provinces make less what they want. A body that
//! cannot cover its own consumption is in want, and the shortfall presses
//! on the order of every province on it, through the same privation the
//! order system already knows how to feel. A surplus sells, and the
//! proceeds enrich the houses that hold the world.
//!
//! The model is logistics, not a market: every flow is integer, and a
//! body's balance is a pure function of content — of what its provinces
//! are authored to make and want — so it needs no stored state and is
//! recomputed whenever it is read. Between bodies, goods do not net; they
//! must be carried, which is a later milestone's work.

use std::collections::BTreeMap;

use aeon_data::ContentKey;
use bevy::prelude::{Component, World};
use serde::{Deserialize, Serialize};

use crate::clock::CampaignClock;
use crate::ids::{BodyId, OrgId, ProvinceId};
use crate::map::{MapIndex, ProvinceRecord};
use crate::politics::PlayerHouse;
use crate::state::ContentDb;

/// The buildings a province has raised, in the order they were built.
///
/// A per-province component, persisted like provincial order: buildings
/// are dynamic state, raised during play, so they ride the snapshot.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buildings(pub Vec<ContentKey>);

/// A standing trade route a transport plies: carry one good from a source
/// province to a sink on another world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradeRoute {
    /// The good carried.
    pub good: ContentKey,
    /// Where it is loaded.
    pub source: ProvinceId,
    /// Where it is delivered.
    pub sink: ProvinceId,
}

/// How much of a good a transport ferries in a month while its route runs.
pub const TRANSPORT_CAPACITY: i64 = 20;

/// The extra monthly wealth a province's buildings add to its output.
pub fn building_wealth_bonus(world: &World, province: ProvinceId) -> i64 {
    let (Some(db), Some(index)) = (
        world.get_resource::<ContentDb>(),
        world.get_resource::<MapIndex>(),
    ) else {
        return 0;
    };
    let content = db.0.clone();
    let Some(buildings) = index
        .provinces
        .get(&province)
        .and_then(|entity| world.get::<Buildings>(*entity))
    else {
        return 0;
    };
    buildings
        .0
        .iter()
        .filter_map(|key| content.buildings.get(key))
        .map(|def| def.adds_wealth)
        .sum()
}

/// The net monthly balance of every good on a body: production less
/// consumption, summed across the body's provinces.
///
/// A pure function of content — nothing here is stored — so a world's
/// wants and surpluses are recomputed whenever they are read. Provinces
/// are visited in stable ID order.
pub fn body_balance(world: &World, body: BodyId) -> BTreeMap<ContentKey, i64> {
    let mut net: BTreeMap<ContentKey, i64> = BTreeMap::new();
    let (Some(db), Some(index)) = (
        world.get_resource::<ContentDb>(),
        world.get_resource::<MapIndex>(),
    ) else {
        return net;
    };
    let content = db.0.clone();
    for (province, entity) in &index.provinces {
        if world.get::<ProvinceRecord>(*entity).map(|r| r.body) != Some(body) {
            continue;
        }
        let Some(def) = index
            .province_ids
            .get(province)
            .and_then(|key| content.provinces.get(key))
        else {
            continue;
        };
        for (good, rate) in &def.produces {
            *net.entry(good.clone()).or_default() += *rate;
        }
        for (good, rate) in &def.consumes {
            *net.entry(good.clone()).or_default() -= *rate;
        }
        // What the province's buildings add to the balance.
        if let Some(buildings) = world.get::<Buildings>(*entity) {
            for building in buildings.0.iter().filter_map(|k| content.buildings.get(k)) {
                for (good, rate) in &building.produces {
                    *net.entry(good.clone()).or_default() += *rate;
                }
                for (good, rate) in &building.consumes {
                    *net.entry(good.clone()).or_default() -= *rate;
                }
            }
        }
    }
    net
}

/// Whether a body cannot cover its own consumption of some good, once the
/// trade carried in is counted.
///
/// A native deficit in a good is a want only if the standing routes
/// running goods to this world do not answer it.
pub fn body_in_want(world: &World, body: BodyId) -> bool {
    body_balance(world, body)
        .iter()
        .any(|(good, net)| *net + route_relief(world, body, good) < 0)
}

/// Whether a hostile presence blockades a province.
fn is_blockaded(world: &World, province: ProvinceId) -> bool {
    world
        .get_resource::<crate::forces::ForcesIndex>()
        .is_some_and(|forces| {
            forces.ships.values().any(|entity| {
                world
                    .get::<crate::forces::ShipRecord>(*entity)
                    .is_some_and(|ship| ship.blockading == Some(province))
            })
        })
}

/// Whether a ship's route is running: it is a transport with a route
/// neither end of which is blockaded. A blockade at either dock cuts the
/// line.
fn route_running(world: &World, ship: &crate::forces::ShipRecord) -> bool {
    let Some(route) = &ship.route else {
        return false;
    };
    ship.class == aeon_data::model::ShipClass::Transport
        && !is_blockaded(world, route.source)
        && !is_blockaded(world, route.sink)
}

/// The monthly relief the running routes bring a good to a body.
pub fn route_relief(world: &World, body: BodyId, good: &ContentKey) -> i64 {
    let Some(forces) = world.get_resource::<crate::forces::ForcesIndex>() else {
        return 0;
    };
    let mut relief = 0;
    for entity in forces.ships.values() {
        let Some(ship) = world.get::<crate::forces::ShipRecord>(*entity) else {
            continue;
        };
        let Some(route) = &ship.route else {
            continue;
        };
        if &route.good == good
            && crate::presence::province_body(world, route.sink) == Some(body)
            && route_running(world, ship)
        {
            relief += TRANSPORT_CAPACITY;
        }
    }
    relief
}

/// The wealth a body's surplus goods fetch when sold this month.
fn surplus_value(world: &World, body: BodyId) -> i64 {
    let Some(db) = world.get_resource::<ContentDb>() else {
        return 0;
    };
    let content = db.0.clone();
    body_balance(world, body)
        .iter()
        .filter(|(_, net)| **net > 0)
        .map(|(good, net)| {
            let value = content.goods.get(good).map(|g| g.value).unwrap_or(0);
            net * value
        })
        .sum()
}

/// Monthly: a body's surplus goods sell, and the proceeds go to the
/// houses that hold its lands.
///
/// The surplus value is split equally among the body's held provinces,
/// each province's holder taking one share — so a world's trade enriches
/// the houses that hold it, in proportion to how much of it they hold.
/// Bodies and provinces are walked in stable ID order and the split is
/// integer, so this replays identically.
pub fn monthly_goods_trade(world: &mut World) {
    let Some(index) = world.get_resource::<MapIndex>() else {
        return;
    };
    let bodies: Vec<BodyId> = index.bodies.keys().copied().collect();

    for body in bodies {
        let value = surplus_value(world, body);
        if value <= 0 {
            continue;
        }
        // The body's provinces that a house actually holds; ungoverned
        // ground collects nothing, so its share of the surplus is lost.
        let index = world.resource::<MapIndex>();
        let held: Vec<OrgId> = index
            .provinces
            .iter()
            .filter(|(_, entity)| {
                world.get::<ProvinceRecord>(**entity).map(|r| r.body) == Some(body)
            })
            .filter_map(|(province, _)| crate::warfare::province_holder(world, *province))
            .collect();
        if held.is_empty() {
            continue;
        }
        let share = value / held.len() as i64;
        if share == 0 {
            continue;
        }
        for holder in held {
            if let Some(entity) = crate::access::org_entity(world, holder)
                && let Some(mut resources) = world.get_mut::<crate::economy::OrgResources>(entity)
            {
                resources.wealth += share;
            }
        }
    }
}

/// The native monthly deficit of a good on a body, before any trade.
fn native_deficit(world: &World, body: BodyId, good: &ContentKey) -> i64 {
    (-body_balance(world, body).get(good).copied().unwrap_or(0)).max(0)
}

/// Monthly: every running route earns its owner the trade margin on what
/// it sells into scarcity — what it delivers, capped by the want it
/// answers, at the good's worth.
pub fn monthly_trade_profit(world: &mut World) {
    let Some(forces) = world.get_resource::<crate::forces::ForcesIndex>().cloned() else {
        return;
    };
    let Some(db) = world.get_resource::<ContentDb>() else {
        return;
    };
    let content = db.0.clone();

    for entity in forces.ships.values() {
        let Some(ship) = world.get::<crate::forces::ShipRecord>(*entity) else {
            continue;
        };
        let (Some(route), owner) = (ship.route.clone(), ship.owner) else {
            continue;
        };
        if !route_running(world, ship) {
            continue;
        }
        let Some(sink_body) = crate::presence::province_body(world, route.sink) else {
            continue;
        };
        let value = content.goods.get(&route.good).map(|g| g.value).unwrap_or(0);
        // Only what answers a real want turns a profit; carrying goods to
        // a world that does not want them earns nothing.
        let delivered = TRANSPORT_CAPACITY.min(native_deficit(world, sink_body, &route.good));
        let profit = delivered * value;
        if profit <= 0 {
            continue;
        }
        if let Some(org_entity) = crate::access::org_entity(world, owner)
            && let Some(mut resources) = world.get_mut::<crate::economy::OrgResources>(org_entity)
        {
            resources.wealth += profit;
        }
    }
}

/// Daily: a transport plying a route shuttles between its two docks,
/// reusing the transit and docking machinery ships already have.
///
/// The route's economic effect does not depend on the ship's position —
/// a running route ferries steadily — but the shuttle gives the carrier
/// something to do and a place to be interdicted.
pub fn run_trade_routes(world: &mut World) {
    let Some(forces) = world.get_resource::<crate::forces::ForcesIndex>().cloned() else {
        return;
    };
    let date = world.resource::<CampaignClock>().date;
    for entity in forces.ships.values() {
        let Some(ship) = world.get::<crate::forces::ShipRecord>(*entity) else {
            continue;
        };
        let Some(route) = ship.route.clone() else {
            continue;
        };
        let crate::forces::ShipLocation::Docked(at) = ship.location else {
            continue; // already under way
        };
        // Head for the dock it is not at; a transport that has wandered
        // elsewhere makes for the source first.
        let destination = if at == route.source {
            route.sink
        } else {
            route.source
        };
        if destination == at {
            continue;
        }
        let days = crate::presence::travel_days(world, at, destination).max(1);
        if let Some(mut record) = world.get_mut::<crate::forces::ShipRecord>(*entity) {
            record.location = crate::forces::ShipLocation::Transit {
                to: destination,
                arrives: date.add_days(days),
            };
        }
    }
}

/// Monthly: an idle transport whose owner has a surplus to sell and a
/// hungry world to sell it to takes up the route unbidden.
///
/// Deterministic throughout: ships, goods, and bodies are walked in
/// stable ID order, and the first workable pairing wins — a surplus
/// world the owner holds, and a world in native want of the same good.
pub fn auto_route_transports(world: &mut World) {
    let (Some(forces), Some(map)) = (
        world.get_resource::<crate::forces::ForcesIndex>().cloned(),
        world.get_resource::<MapIndex>().cloned(),
    ) else {
        return;
    };
    let player = world.get_resource::<PlayerHouse>().and_then(|p| p.0);

    for entity in forces.ships.values() {
        let Some(ship) = world.get::<crate::forces::ShipRecord>(*entity) else {
            continue;
        };
        // Only an owned, un-routed transport of a non-player house.
        if ship.route.is_some()
            || ship.class != aeon_data::model::ShipClass::Transport
            || Some(ship.owner) == player
        {
            continue;
        }
        let owner = ship.owner;

        // A good the owner makes a surplus of somewhere, and the province
        // to load it at: the owner's own holding on a surplus world.
        let mut source: Option<(ProvinceId, ContentKey)> = None;
        'find: for province in crate::order::held_provinces(world, owner) {
            let Some(body) = crate::presence::province_body(world, province) else {
                continue;
            };
            for (good, net) in body_balance(world, body) {
                if net > 0 {
                    source = Some((province, good));
                    break 'find;
                }
            }
        }
        let Some((source_province, good)) = source else {
            continue;
        };
        let source_body = crate::presence::province_body(world, source_province);

        // A world in native want of that good, and a province to unload at.
        let sink = map.provinces.keys().copied().find(|province| {
            let body = crate::presence::province_body(world, *province);
            body != source_body && body.is_some_and(|b| native_deficit(world, b, &good) > 0)
        });
        let Some(sink_province) = sink else {
            continue;
        };

        set_route(
            world,
            ship.id,
            TradeRoute {
                good,
                source: source_province,
                sink: sink_province,
            },
        );
    }
}

/// Sets a transport's standing trade route, if the route makes sense.
///
/// The ship must be an owned transport and the two docks must lie on
/// different worlds — goods within a world need no carrying. Returns
/// whether the route was set.
pub fn set_route(world: &mut World, ship: crate::ids::ShipId, route: TradeRoute) -> bool {
    let Some(entity) = crate::access::ship_entity(world, ship) else {
        return false;
    };
    let is_transport = world
        .get::<crate::forces::ShipRecord>(entity)
        .is_some_and(|s| s.class == aeon_data::model::ShipClass::Transport);
    let different_worlds = crate::presence::province_body(world, route.source)
        != crate::presence::province_body(world, route.sink);
    let good_defined = world
        .get_resource::<ContentDb>()
        .is_some_and(|db| db.0.goods.contains_key(&route.good));
    if !is_transport || !different_worlds || !good_defined {
        return false;
    }
    if let Some(mut record) = world.get_mut::<crate::forces::ShipRecord>(entity) {
        record.route = Some(route);
        return true;
    }
    false
}

/// Clears a ship's trade route.
pub fn clear_route(world: &mut World, ship: crate::ids::ShipId) {
    if let Some(entity) = crate::access::ship_entity(world, ship)
        && let Some(mut record) = world.get_mut::<crate::forces::ShipRecord>(entity)
    {
        record.route = None;
    }
}

/// Installs the goods trade: the monthly surplus sale and route profit,
/// and the daily shuttle that keeps transports on their routes.
pub(crate) fn install(app: &mut bevy::prelude::App) {
    use crate::clock::{DailyTick, MonthlyPulse, TickSet};
    use bevy::prelude::IntoScheduleConfigs;
    // After the base economy accrues, so trade is a distinct line on top
    // of a province's plain output, and before opinion cleanup like the
    // rest of the monthly chain.
    app.add_systems(
        MonthlyPulse,
        (
            auto_route_transports,
            monthly_goods_trade,
            monthly_trade_profit,
        )
            .chain()
            .after(crate::economy::monthly_economy)
            .before(crate::politics::expire_opinion_modifiers),
    );
    // Before ships dock, so a route set today can depart today.
    app.add_systems(
        DailyTick,
        run_trade_routes
            .in_set(TickSet::Simulation)
            .before(crate::forces::dock_arrivals),
    );
}
