//! Physical presence and order delay.
//!
//! Every simulated character is somewhere concrete: at a province or
//! aboard a ship. Route progress is separate persisted state.

use aeon_data::model::RouteKind;
use bevy::app::App;
use bevy::prelude::{Component, Entity, IntoScheduleConfigs, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::ids::{BodyId, CharacterId, ProvinceId, ShipId};
use crate::map::{MapIndex, ProvinceRecord};
use crate::politics::{PlayerHouse, PoliticsIndex};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    Province(ProvinceId),
    Aboard(ShipId),
}

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterLocation(pub Location);

pub fn province_body(world: &World, province: ProvinceId) -> Option<BodyId> {
    let map = world.resource::<MapIndex>();
    map.provinces
        .get(&province)
        .and_then(|entity| world.get::<ProvinceRecord>(*entity))
        .map(|record| record.body)
}

pub fn travel_days(world: &World, from: ProvinceId, to: ProvinceId) -> i64 {
    world
        .get_resource::<crate::routes::RouteGraph>()
        .and_then(|graph| graph.fastest_mixed_path(from, to))
        .map_or(0, |path| crate::routes::RouteGraph::path_days(&path))
}

pub fn character_location(world: &World, character: CharacterId) -> Option<Location> {
    crate::access::on_character::<CharacterLocation>(world, character).map(|location| location.0)
}

pub fn effective_province(world: &World, character: CharacterId) -> Option<ProvinceId> {
    match character_location(world, character)? {
        Location::Province(province) => Some(province),
        Location::Aboard(ship) => match crate::access::ship(world, ship)?.location {
            crate::forces::ShipLocation::Docked(province) => Some(province),
            crate::forces::ShipLocation::OnRoute { to, .. } => Some(to),
        },
    }
}

pub fn order_delay(world: &World, actor: Option<CharacterId>) -> i64 {
    let Some(player_org) = world
        .get_resource::<PlayerHouse>()
        .and_then(|player| player.0)
    else {
        return 0;
    };
    let Some(head) = crate::access::org_head(world, player_org) else {
        return 0;
    };

    let mut delay = 0i64;
    let head_province = match character_location(world, head) {
        Some(Location::Aboard(ship)) => {
            match crate::access::ship(world, ship).map(|s| s.location) {
                Some(crate::forces::ShipLocation::OnRoute { to, arrives, .. }) => {
                    let date = world.resource::<CampaignClock>().date;
                    delay += date.days_until(arrives).max(0) + 1;
                    Some(to)
                }
                Some(crate::forces::ShipLocation::Docked(at)) => Some(at),
                None => None,
            }
        }
        Some(Location::Province(province)) => Some(province),
        None => None,
    };

    if let (Some(actor), Some(head_province)) = (actor, head_province)
        && actor != head
        && let Some(actor_province) = effective_province(world, actor)
        && province_body(world, actor_province) != province_body(world, head_province)
    {
        delay += (travel_days(world, head_province, actor_province) / 2).max(1);
    }
    delay
}

pub fn begin_travel(world: &mut World, character: CharacterId, destination: ProvinceId) {
    let from = match character_location(world, character) {
        Some(Location::Province(province)) => province,
        _ => return,
    };
    let Some(path) = world
        .resource::<crate::routes::RouteGraph>()
        .fastest_mixed_path(from, destination)
    else {
        return;
    };
    let entity = crate::access::character_entity(world, character).expect("indexed character");
    world.entity_mut(entity).insert(crate::routes::Journey::new(
        destination,
        path,
        crate::routes::JourneyPurpose::Travel,
    ));
}

fn traveller_owner(world: &World, character: CharacterId) -> Option<crate::ids::OrgId> {
    crate::access::character(world, character)?.organisation
}

pub fn land_arrivals(world: &mut World) {
    if world.get_resource::<PoliticsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let travellers: Vec<(CharacterId, Entity)> = {
        let index = world.resource::<PoliticsIndex>();
        index
            .characters
            .iter()
            .filter(|(_, entity)| world.get::<crate::routes::Journey>(**entity).is_some())
            .map(|(id, entity)| (*id, *entity))
            .collect()
    };

    for (character, entity) in travellers {
        let Some(mut journey) = world.get::<crate::routes::Journey>(entity).cloned() else {
            continue;
        };
        if journey
            .current
            .as_ref()
            .is_some_and(|progress| progress.arrives <= date)
        {
            let progress = journey.current.take().expect("checked current leg");
            match progress.leg.kind {
                RouteKind::Surface => {
                    world
                        .entity_mut(entity)
                        .insert(CharacterLocation(Location::Province(progress.leg.to)));
                }
                RouteKind::Space => {
                    let ship = match world
                        .get::<CharacterLocation>(entity)
                        .map(|location| location.0)
                    {
                        Some(Location::Aboard(ship)) => ship,
                        _ => continue,
                    };
                    if let Some(ship_entity) = crate::access::ship_entity(world, ship)
                        && let Some(mut record) =
                            world.get_mut::<crate::forces::ShipRecord>(ship_entity)
                    {
                        record.location = crate::forces::ShipLocation::Docked(progress.leg.to);
                    }
                    if journey
                        .remaining
                        .first()
                        .is_none_or(|next| next.kind != RouteKind::Space)
                    {
                        world
                            .entity_mut(entity)
                            .insert(CharacterLocation(Location::Province(progress.leg.to)));
                        crate::forces::retire_personal_transport(world, ship);
                    }
                }
            }
        }

        if journey.current.is_none() && !journey.remaining.is_empty() {
            let leg = journey.remaining.remove(0);
            let multiplier = if journey.speed == crate::routes::JourneySpeed::Half {
                2
            } else {
                1
            };
            let arrives = date.add_days(i64::from(leg.travel_days) * multiplier);
            if leg.kind == RouteKind::Space {
                let ship = match world
                    .get::<CharacterLocation>(entity)
                    .map(|location| location.0)
                {
                    Some(Location::Aboard(ship)) => ship,
                    _ => {
                        let Some(owner) = traveller_owner(world, character) else {
                            continue;
                        };
                        let ship = crate::forces::spawn_personal_transport(world, owner, leg.from);
                        world
                            .entity_mut(entity)
                            .insert(CharacterLocation(Location::Aboard(ship)));
                        ship
                    }
                };
                if let Some(ship_entity) = crate::access::ship_entity(world, ship)
                    && let Some(mut record) =
                        world.get_mut::<crate::forces::ShipRecord>(ship_entity)
                {
                    record.location = crate::forces::ShipLocation::OnRoute {
                        from: leg.from,
                        to: leg.to,
                        arrives,
                    };
                }
            }
            journey.current = Some(crate::routes::RouteProgress { leg, arrives });
        }

        if journey.current.is_none() && journey.remaining.is_empty() {
            world.entity_mut(entity).remove::<crate::routes::Journey>();
        } else {
            world.entity_mut(entity).insert(journey);
        }
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        DailyTick,
        land_arrivals
            .in_set(TickSet::Simulation)
            .before(crate::assignments::resolve_due_assignments),
    );
}
