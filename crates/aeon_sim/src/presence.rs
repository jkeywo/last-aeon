//! Physical presence and order delay.
//!
//! Every simulated character is somewhere: at a province or in transit
//! between bodies. Orders issued across distance are delayed, and while
//! the player's head travels through space every order is delayed until
//! after arrival — physical location is a strategic commitment.

use aeon_core::calendar::GameDate;
use bevy::app::App;
use bevy::prelude::{Component, IntoScheduleConfigs, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::ids::{BodyId, CharacterId, ProvinceId};
use crate::map::{BodyRecord, MapIndex, ProvinceRecord};
use crate::politics::{OrgRecord, PlayerHouse, PoliticsIndex};

/// Where a character is.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    /// At a province.
    Province(ProvinceId),
    /// Travelling to a province.
    Transit {
        /// Destination.
        to: ProvinceId,
        /// Arrival day.
        arrives: GameDate,
    },
}

/// A character's physical location.
#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterLocation(pub Location);

/// The body a province sits on.
pub fn province_body(world: &World, province: ProvinceId) -> Option<BodyId> {
    let map = world.resource::<MapIndex>();
    map.provinces
        .get(&province)
        .and_then(|e| world.get::<ProvinceRecord>(*e))
        .map(|r| r.body)
}

/// Travel days between two provinces: quick locally, longer across
/// bodies, scaled by how far apart their orbits are.
pub fn travel_days(world: &World, from: ProvinceId, to: ProvinceId) -> i64 {
    let (Some(from_body), Some(to_body)) = (province_body(world, from), province_body(world, to))
    else {
        return 3;
    };
    if from_body == to_body {
        return 3;
    }
    let map = world.resource::<MapIndex>();
    let orbit = |body: BodyId| -> i64 {
        map.bodies
            .get(&body)
            .and_then(|e| world.get::<BodyRecord>(*e))
            .map(|r| i64::from(r.orbit_radius_mm))
            .unwrap_or(0)
    };
    4 + (orbit(from_body) - orbit(to_body)).abs() / 50
}

/// A character's current location, if tracked.
pub fn character_location(world: &World, character: CharacterId) -> Option<Location> {
    let index = world.resource::<PoliticsIndex>();
    index
        .characters
        .get(&character)
        .and_then(|e| world.get::<CharacterLocation>(*e))
        .map(|l| l.0)
}

/// Extra days of order delay for the player's commands: distance between
/// the head and the acting character, plus the head's own transit.
pub fn order_delay(world: &World, actor: Option<CharacterId>) -> i64 {
    let Some(player_org) = world.get_resource::<PlayerHouse>().and_then(|p| p.0) else {
        return 0;
    };
    let index = world.resource::<PoliticsIndex>();
    let Some(head) = index
        .orgs
        .get(&player_org)
        .and_then(|e| world.get::<OrgRecord>(*e))
        .and_then(|r| r.head)
    else {
        return 0;
    };
    let date = world.resource::<CampaignClock>().date;

    let mut delay = 0i64;
    let head_province = match character_location(world, head) {
        // A head in space delays everything until after arrival.
        Some(Location::Transit { to, arrives }) => {
            delay += date.days_until(arrives).max(0) + 1;
            Some(to)
        }
        Some(Location::Province(province)) => Some(province),
        None => None,
    };

    // Orders to someone on another body lag by half the travel time.
    if let (Some(actor), Some(head_province)) = (actor, head_province)
        && actor != head
        && let Some(Location::Province(actor_province)) = character_location(world, actor)
        && province_body(world, actor_province) != province_body(world, head_province)
    {
        delay += (travel_days(world, head_province, actor_province) / 2).max(1);
    }
    delay
}

/// Starts a character's journey. Callers must have validated.
pub fn begin_travel(world: &mut World, character: CharacterId, destination: ProvinceId) {
    let date = world.resource::<CampaignClock>().date;
    let from = match character_location(world, character) {
        Some(Location::Province(province)) => province,
        _ => return,
    };
    let days = travel_days(world, from, destination);
    let entity = world.resource::<PoliticsIndex>().characters[&character];
    if let Some(mut location) = world.get_mut::<CharacterLocation>(entity) {
        location.0 = Location::Transit {
            to: destination,
            arrives: date.add_days(days),
        };
    }
}

/// Daily: travellers whose arrival day has come land at their destination.
pub fn land_arrivals(world: &mut World) {
    if world.get_resource::<PoliticsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let travellers: Vec<(CharacterId, ProvinceId)> = {
        let index = world.resource::<PoliticsIndex>();
        index
            .characters
            .iter()
            .filter_map(
                |(id, entity)| match world.get::<CharacterLocation>(*entity).map(|l| l.0) {
                    Some(Location::Transit { to, arrives }) if arrives <= date => Some((*id, to)),
                    _ => None,
                },
            )
            .collect()
    };
    for (character, destination) in travellers {
        let entity = world.resource::<PoliticsIndex>().characters[&character];
        if let Some(mut location) = world.get_mut::<CharacterLocation>(entity) {
            location.0 = Location::Province(destination);
        }
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        DailyTick,
        land_arrivals
            .in_set(TickSet::Simulation)
            .before(crate::jobs::resolve_due_jobs),
    );
}
