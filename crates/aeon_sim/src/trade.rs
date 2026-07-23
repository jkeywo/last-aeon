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

use crate::ids::{BodyId, OrgId, ProvinceId};
use crate::map::{MapIndex, ProvinceRecord};
use crate::state::ContentDb;

/// The buildings a province has raised, in the order they were built.
///
/// A per-province component, persisted like provincial order: buildings
/// are dynamic state, raised during play, so they ride the snapshot.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Buildings(pub Vec<ContentKey>);

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

/// Whether a body cannot cover its own consumption of some good.
///
/// A negative balance in any good means the body wants more than it
/// makes — a privation felt across the whole world.
pub fn body_in_want(world: &World, body: BodyId) -> bool {
    body_balance(world, body).values().any(|net| *net < 0)
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

/// Installs the monthly goods trade.
pub(crate) fn install(app: &mut bevy::prelude::App) {
    use crate::clock::MonthlyPulse;
    use bevy::prelude::IntoScheduleConfigs;
    // After the base economy accrues, so trade is a distinct line on top
    // of a province's plain output, and before opinion cleanup like the
    // rest of the monthly chain.
    app.add_systems(
        MonthlyPulse,
        monthly_goods_trade
            .after(crate::economy::monthly_economy)
            .before(crate::politics::expire_opinion_modifiers),
    );
}
