//! Force officer appointments, automatic succession, and leaderless retreat.

use aeon_core::calendar::GameDate;
use aeon_data::model::RouteKind;
use bevy::app::App;
use bevy::prelude::{Component, Entity, IntoScheduleConfigs, World};
use serde::{Deserialize, Serialize};

use crate::assignments::{ActiveAssignment, AssignmentTarget, AssignmentsIndex};
use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::forces::{ArmyLocation, ArmyRecord, ForcesIndex, ShipLocation, ShipRecord};
use crate::ids::{ArmyId, CharacterId, OrgId, ProvinceId, ShipId};
use crate::presence::{CharacterLocation, Location};
use crate::routes::{Journey, JourneyPurpose, JourneySpeed, RouteGraph};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OfficerTarget {
    Ship(ShipId),
    Army(ArmyId),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OfficerPost {
    Captain,
    FirstOfficer,
    General,
    Lieutenant,
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppointmentJob {
    pub officer: CharacterId,
    pub post: OfficerPost,
    pub handover_due: Option<GameDate>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransportJobKind {
    Embark { ship: ShipId },
    Disembark { province: ProvinceId },
}

#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportJob {
    pub kind: TransportJobKind,
    pub completes: GameDate,
}

pub fn target_owner(world: &World, target: OfficerTarget) -> Option<OrgId> {
    match target {
        OfficerTarget::Ship(ship) => crate::access::ship(world, ship).map(|record| record.owner),
        OfficerTarget::Army(army) => crate::access::army(world, army).map(|record| record.owner),
    }
}

pub fn target_entity(world: &World, target: OfficerTarget) -> Option<Entity> {
    match target {
        OfficerTarget::Ship(ship) => crate::access::ship_entity(world, ship),
        OfficerTarget::Army(army) => crate::access::army_entity(world, army),
    }
}

fn compatible(target: OfficerTarget, post: OfficerPost) -> bool {
    matches!(
        (target, post),
        (
            OfficerTarget::Ship(_),
            OfficerPost::Captain | OfficerPost::FirstOfficer
        ) | (
            OfficerTarget::Army(_),
            OfficerPost::General | OfficerPost::Lieutenant
        )
    )
}

pub fn validate_appointment(
    world: &World,
    owner: OrgId,
    target: OfficerTarget,
    post: OfficerPost,
    officer: CharacterId,
) -> bool {
    if !compatible(target, post) || target_owner(world, target) != Some(owner) {
        return false;
    }
    let date = world.resource::<CampaignClock>().date;
    let eligible = crate::access::character(world, officer).is_some_and(|record| {
        record.alive()
            && record.organisation == Some(owner)
            && record.age_years(date) >= crate::politics::ADULT_AGE
    });
    if !eligible {
        return false;
    }
    let already_here = match target {
        OfficerTarget::Ship(ship) => crate::access::ship(world, ship).is_some_and(|record| {
            record.captain == Some(officer) || record.first_officer == Some(officer)
        }),
        OfficerTarget::Army(army) => crate::access::army(world, army).is_some_and(|record| {
            record.general == Some(officer) || record.lieutenant == Some(officer)
        }),
    };
    already_here || !character_has_other_commitment(world, officer, target)
}

pub fn begin_appointment(
    world: &mut World,
    target: OfficerTarget,
    post: OfficerPost,
    officer: CharacterId,
) {
    let Some(entity) = target_entity(world, target) else {
        return;
    };
    world.entity_mut(entity).insert(AppointmentJob {
        officer,
        post,
        handover_due: None,
    });
}

fn assignment_matches(target: OfficerTarget, assignment: AssignmentTarget) -> bool {
    match (target, assignment) {
        (OfficerTarget::Ship(a), AssignmentTarget::ShipToProvince(b, _)) => a == b,
        (
            OfficerTarget::Army(a),
            AssignmentTarget::OwnArmy(b) | AssignmentTarget::ArmyToProvince(b, _),
        ) => a == b,
        _ => false,
    }
}

fn character_has_other_commitment(
    world: &World,
    officer: CharacterId,
    target: OfficerTarget,
) -> bool {
    if world
        .get_resource::<AssignmentsIndex>()
        .is_some_and(|index| {
            index.assignments.values().any(|entity| {
                world
                    .get::<ActiveAssignment>(*entity)
                    .is_some_and(|assignment| {
                        assignment.leader == officer
                            && !assignment_matches(target, assignment.target)
                    })
            })
        })
    {
        return true;
    }
    world.get_resource::<ForcesIndex>().is_some_and(|index| {
        let standing_elsewhere = index.ships.iter().any(|(id, entity)| {
            world.get::<ShipRecord>(*entity).is_some_and(|record| {
                (record.captain == Some(officer) || record.first_officer == Some(officer))
                    && target != OfficerTarget::Ship(*id)
            })
        }) || index.armies.iter().any(|(id, entity)| {
            world.get::<ArmyRecord>(*entity).is_some_and(|record| {
                (record.general == Some(officer) || record.lieutenant == Some(officer))
                    && target != OfficerTarget::Army(*id)
            })
        });
        standing_elsewhere
            || index
                .ships
                .values()
                .chain(index.armies.values())
                .any(|entity| {
                    world
                        .get::<AppointmentJob>(*entity)
                        .is_some_and(|job| job.officer == officer)
                })
    })
}

fn force_province(world: &World, target: OfficerTarget) -> Option<ProvinceId> {
    match target {
        OfficerTarget::Ship(ship) => match crate::access::ship(world, ship)?.location {
            ShipLocation::Docked(province) => Some(province),
            ShipLocation::OnRoute { .. } => None,
        },
        OfficerTarget::Army(army) => match crate::access::army(world, army)?.location {
            ArmyLocation::Province(province) => Some(province),
            ArmyLocation::Embarked(ship) => match crate::access::ship(world, ship)?.location {
                ShipLocation::Docked(province) => Some(province),
                ShipLocation::OnRoute { .. } => None,
            },
        },
    }
}

fn transfer_force_assignments(
    world: &mut World,
    target: OfficerTarget,
    from: Option<CharacterId>,
    to: CharacterId,
) {
    let Some(index) = world.get_resource::<AssignmentsIndex>().cloned() else {
        return;
    };
    for entity in index.assignments.values() {
        let should_transfer = world
            .get::<ActiveAssignment>(*entity)
            .is_some_and(|assignment| {
                from == Some(assignment.leader) && assignment_matches(target, assignment.target)
            });
        if should_transfer && let Some(mut assignment) = world.get_mut::<ActiveAssignment>(*entity)
        {
            assignment.leader = to;
        }
    }
}

fn resume_force_orders(world: &mut World, target: OfficerTarget) {
    let Some(entity) = target_entity(world, target) else {
        return;
    };
    let active_destination = world.get_resource::<AssignmentsIndex>().and_then(|index| {
        index.assignments.values().find_map(|assignment_entity| {
            let assignment = world.get::<ActiveAssignment>(*assignment_entity)?;
            if !assignment_matches(target, assignment.target) {
                return None;
            }
            match assignment.target {
                AssignmentTarget::ArmyToProvince(_, destination)
                | AssignmentTarget::ShipToProvince(_, destination) => Some(destination),
                _ => None,
            }
        })
    });
    let Some(destination) = active_destination else {
        if world
            .get::<Journey>(entity)
            .is_some_and(|journey| journey.purpose == JourneyPurpose::Retreat)
        {
            world.entity_mut(entity).remove::<Journey>();
        }
        return;
    };
    let (from, kind) = match target {
        OfficerTarget::Ship(ship) => {
            match crate::access::ship(world, ship).map(|record| record.location) {
                Some(ShipLocation::Docked(province)) => (province, RouteKind::Space),
                Some(ShipLocation::OnRoute { .. }) | None => {
                    if let Some(mut journey) = world.get_mut::<Journey>(entity) {
                        journey.speed = JourneySpeed::Normal;
                    }
                    return;
                }
            }
        }
        OfficerTarget::Army(army) => {
            match crate::access::army(world, army).map(|record| record.location) {
                Some(ArmyLocation::Province(province)) => (province, RouteKind::Surface),
                Some(ArmyLocation::Embarked(_)) | None => return,
            }
        }
    };
    if from == destination {
        world.entity_mut(entity).remove::<Journey>();
    } else if let Some(path) = world
        .resource::<RouteGraph>()
        .fastest_path(kind, from, destination)
    {
        world.entity_mut(entity).insert(Journey::new(
            destination,
            path,
            JourneyPurpose::Assignment,
        ));
    }
}

fn complete_appointment(world: &mut World, target: OfficerTarget, job: &AppointmentJob) {
    let Some(entity) = target_entity(world, target) else {
        return;
    };
    match target {
        OfficerTarget::Ship(ship_id) => {
            let Some(current) = world.get::<ShipRecord>(entity).cloned() else {
                return;
            };
            match job.post {
                OfficerPost::Captain => {
                    let old = current.captain;
                    let move_old_to_deputy = old.is_some()
                        && current.first_officer.is_none()
                        && !character_has_other_commitment(world, old.expect("some"), target);
                    if let Some(mut ship) = world.get_mut::<ShipRecord>(entity) {
                        ship.captain = Some(job.officer);
                        ship.orders_suspended = false;
                        ship.retreat_destination = None;
                        if move_old_to_deputy {
                            ship.first_officer = old;
                        }
                    }
                    transfer_force_assignments(world, target, old, job.officer);
                    resume_force_orders(world, target);
                }
                OfficerPost::FirstOfficer => {
                    if let Some(mut ship) = world.get_mut::<ShipRecord>(entity) {
                        ship.first_officer = Some(job.officer);
                    }
                }
                _ => return,
            }
            if let Some(officer_entity) = crate::access::character_entity(world, job.officer) {
                world
                    .entity_mut(officer_entity)
                    .insert(CharacterLocation(Location::Aboard(ship_id)));
            }
        }
        OfficerTarget::Army(army_id) => {
            let Some(current) = world.get::<ArmyRecord>(entity).cloned() else {
                return;
            };
            match job.post {
                OfficerPost::General => {
                    let old = current.general;
                    let move_old_to_deputy = old.is_some()
                        && current.lieutenant.is_none()
                        && !character_has_other_commitment(world, old.expect("some"), target);
                    if let Some(mut army) = world.get_mut::<ArmyRecord>(entity) {
                        army.general = Some(job.officer);
                        army.orders_suspended = false;
                        army.retreat_destination = None;
                        if move_old_to_deputy {
                            army.lieutenant = old;
                        }
                    }
                    transfer_force_assignments(world, target, old, job.officer);
                    resume_force_orders(world, target);
                }
                OfficerPost::Lieutenant => {
                    if let Some(mut army) = world.get_mut::<ArmyRecord>(entity) {
                        army.lieutenant = Some(job.officer);
                    }
                }
                _ => return,
            }
            if let Some(location) = force_officer_location(world, target)
                && let Some(officer_entity) = crate::access::character_entity(world, job.officer)
            {
                world
                    .entity_mut(officer_entity)
                    .insert(CharacterLocation(location));
            }
            let _ = army_id;
        }
    }
    world.entity_mut(entity).remove::<AppointmentJob>();
}

fn force_officer_location(world: &World, target: OfficerTarget) -> Option<Location> {
    match target {
        OfficerTarget::Ship(ship) => Some(Location::Aboard(ship)),
        OfficerTarget::Army(army) => match crate::access::army(world, army)?.location {
            ArmyLocation::Province(province) => Some(Location::Province(province)),
            ArmyLocation::Embarked(ship) => Some(Location::Aboard(ship)),
        },
    }
}

fn process_appointments(world: &mut World) {
    let date = world.resource::<CampaignClock>().date;
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    let jobs: Vec<(OfficerTarget, Entity, AppointmentJob)> = index
        .ships
        .iter()
        .filter_map(|(id, entity)| {
            world
                .get::<AppointmentJob>(*entity)
                .cloned()
                .map(|job| (OfficerTarget::Ship(*id), *entity, job))
        })
        .chain(index.armies.iter().filter_map(|(id, entity)| {
            world
                .get::<AppointmentJob>(*entity)
                .cloned()
                .map(|job| (OfficerTarget::Army(*id), *entity, job))
        }))
        .collect();
    for (target, entity, mut job) in jobs {
        let Some(province) = force_province(world, target) else {
            continue;
        };
        let officer_location = crate::presence::character_location(world, job.officer);
        let arrived = officer_location == Some(Location::Province(province))
            || matches!((target, officer_location), (OfficerTarget::Ship(ship), Some(Location::Aboard(aboard))) if ship == aboard)
            || matches!((target, officer_location), (OfficerTarget::Army(army), Some(location)) if force_officer_location(world, OfficerTarget::Army(army)) == Some(location));
        if !arrived {
            let travelling = crate::access::character_entity(world, job.officer)
                .is_some_and(|candidate| world.get::<Journey>(candidate).is_some());
            if !travelling {
                crate::presence::begin_travel(world, job.officer, province);
            }
            continue;
        }
        match job.handover_due {
            Some(due) if due <= date => complete_appointment(world, target, &job),
            None => {
                job.handover_due = Some(date.add_days(1));
                world.entity_mut(entity).insert(job);
            }
            _ => {}
        }
    }
}

pub fn begin_transport_job(world: &mut World, army: ArmyId, kind: TransportJobKind) {
    let Some(entity) = crate::access::army_entity(world, army) else {
        return;
    };
    let completes = world.resource::<CampaignClock>().date.add_days(1);
    world
        .entity_mut(entity)
        .insert(TransportJob { kind, completes });
}

fn process_transport_jobs(world: &mut World) {
    let date = world.resource::<CampaignClock>().date;
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    let due: Vec<(ArmyId, Entity, TransportJob)> = index
        .armies
        .iter()
        .filter_map(|(id, entity)| {
            world
                .get::<TransportJob>(*entity)
                .filter(|job| job.completes <= date)
                .cloned()
                .map(|job| (*id, *entity, job))
        })
        .collect();
    for (army_id, entity, job) in due {
        match job.kind {
            TransportJobKind::Embark { ship } => {
                if let Some(mut army) = world.get_mut::<ArmyRecord>(entity) {
                    army.location = ArmyLocation::Embarked(ship);
                }
                if let Some(army) = crate::access::army(world, army_id) {
                    for officer in [army.general, army.lieutenant].into_iter().flatten() {
                        if let Some(character) = crate::access::character_entity(world, officer) {
                            world
                                .entity_mut(character)
                                .insert(CharacterLocation(Location::Aboard(ship)));
                        }
                    }
                }
            }
            TransportJobKind::Disembark { province } => {
                if let Some(mut army) = world.get_mut::<ArmyRecord>(entity) {
                    army.location = ArmyLocation::Province(province);
                }
                if let Some(army) = crate::access::army(world, army_id) {
                    for officer in [army.general, army.lieutenant].into_iter().flatten() {
                        if let Some(character) = crate::access::character_entity(world, officer) {
                            world
                                .entity_mut(character)
                                .insert(CharacterLocation(Location::Province(province)));
                        }
                    }
                }
            }
        }
        world.entity_mut(entity).remove::<TransportJob>();
    }
}

pub fn vacate_character_posts(world: &mut World, character: CharacterId) {
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    for (id, entity) in &index.ships {
        let Some(current) = world.get::<ShipRecord>(*entity).cloned() else {
            continue;
        };
        if current.captain == Some(character) {
            if let Some(mut ship) = world.get_mut::<ShipRecord>(*entity) {
                ship.captain = None;
            }
            promote_ship(world, *id, Some(character));
        } else if current.first_officer == Some(character)
            && let Some(mut ship) = world.get_mut::<ShipRecord>(*entity)
        {
            ship.first_officer = None;
        }
    }
    for (id, entity) in &index.armies {
        let Some(current) = world.get::<ArmyRecord>(*entity).cloned() else {
            continue;
        };
        if current.general == Some(character) {
            if let Some(mut army) = world.get_mut::<ArmyRecord>(*entity) {
                army.general = None;
            }
            promote_army(world, *id, Some(character));
        } else if current.lieutenant == Some(character)
            && let Some(mut army) = world.get_mut::<ArmyRecord>(*entity)
        {
            army.lieutenant = None;
        }
    }
}

pub fn clear_primary(world: &mut World, target: OfficerTarget) {
    match target {
        OfficerTarget::Ship(id) => {
            let former = crate::access::ship(world, id).and_then(|record| record.captain);
            if let Some(entity) = crate::access::ship_entity(world, id)
                && let Some(mut record) = world.get_mut::<ShipRecord>(entity)
            {
                record.captain = None;
            }
            promote_ship(world, id, former);
        }
        OfficerTarget::Army(id) => {
            let former = crate::access::army(world, id).and_then(|record| record.general);
            if let Some(entity) = crate::access::army_entity(world, id)
                && let Some(mut record) = world.get_mut::<ArmyRecord>(entity)
            {
                record.general = None;
            }
            promote_army(world, id, former);
        }
    }
}

fn alive(world: &World, character: CharacterId) -> bool {
    crate::access::character(world, character).is_some_and(|record| record.alive())
}

fn promote_ship(world: &mut World, ship_id: ShipId, former: Option<CharacterId>) {
    let Some(entity) = crate::access::ship_entity(world, ship_id) else {
        return;
    };
    let Some(current) = world.get::<ShipRecord>(entity).cloned() else {
        return;
    };
    if current.captain.is_none()
        && current
            .first_officer
            .is_some_and(|officer| alive(world, officer))
    {
        let promoted = current.first_officer.expect("checked");
        if let Some(mut ship) = world.get_mut::<ShipRecord>(entity) {
            ship.captain = Some(promoted);
            ship.first_officer = None;
            ship.orders_suspended = false;
            ship.retreat_destination = None;
        }
        transfer_force_assignments(world, OfficerTarget::Ship(ship_id), former, promoted);
        resume_force_orders(world, OfficerTarget::Ship(ship_id));
    }
}

fn promote_army(world: &mut World, army_id: ArmyId, former: Option<CharacterId>) {
    let Some(entity) = crate::access::army_entity(world, army_id) else {
        return;
    };
    let Some(current) = world.get::<ArmyRecord>(entity).cloned() else {
        return;
    };
    if current.general.is_none()
        && current
            .lieutenant
            .is_some_and(|officer| alive(world, officer))
    {
        let promoted = current.lieutenant.expect("checked");
        if let Some(mut army) = world.get_mut::<ArmyRecord>(entity) {
            army.general = Some(promoted);
            army.lieutenant = None;
            army.orders_suspended = false;
            army.retreat_destination = None;
        }
        transfer_force_assignments(world, OfficerTarget::Army(army_id), former, promoted);
        resume_force_orders(world, OfficerTarget::Army(army_id));
    }
}

fn liege_chain(world: &World, owner: OrgId) -> Vec<OrgId> {
    let mut result = Vec::new();
    let mut current = owner;
    for _ in 0..16 {
        let Some(liege) = crate::access::org(world, current).and_then(|record| record.liege) else {
            break;
        };
        result.push(liege);
        current = liege;
    }
    result
}

fn retreat_candidates(world: &World, owner: OrgId, starports_only: bool) -> Vec<Vec<ProvinceId>> {
    let map = world.resource::<crate::map::MapIndex>();
    let eligible: Vec<ProvinceId> = map
        .provinces
        .iter()
        .filter_map(|(id, entity)| {
            let province = world.get::<crate::map::ProvinceRecord>(*entity)?;
            (!starports_only || province.starport).then_some(*id)
        })
        .collect();
    let held_by = |org: OrgId| {
        eligible
            .iter()
            .copied()
            .filter(|province| crate::warfare::province_holder(world, *province) == Some(org))
            .collect::<Vec<_>>()
    };
    let mut tiers = vec![held_by(owner)];
    tiers.extend(liege_chain(world, owner).into_iter().map(held_by));
    let owner_head = crate::access::org_head(world, owner);
    tiers.push(eligible.iter().copied().filter(|province| {
        let holder_head = crate::warfare::province_holder(world, *province).and_then(|holder| crate::access::org_head(world, holder));
        matches!((holder_head, owner_head), (Some(from), Some(to)) if crate::politics::opinion_between(world, from, to) > 0)
    }).collect());
    tiers
}

fn choose_retreat(
    world: &World,
    owner: OrgId,
    from: ProvinceId,
    kind: RouteKind,
    retained: Option<ProvinceId>,
) -> Option<ProvinceId> {
    let tiers = retreat_candidates(world, owner, kind == RouteKind::Space);
    for tier in tiers {
        if tier.is_empty() {
            continue;
        }
        if tier.contains(&from) {
            return Some(from);
        }
        if retained.is_some_and(|destination| tier.contains(&destination)) {
            return retained;
        }
        return tier
            .into_iter()
            .filter_map(|destination| {
                world
                    .resource::<RouteGraph>()
                    .fastest_path(kind, from, destination)
                    .map(|path| (RouteGraph::path_days(&path), destination))
            })
            .min()
            .map(|(_, destination)| destination);
    }
    None
}

fn begin_leaderless_retreats(world: &mut World) {
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    for (id, entity) in &index.ships {
        let Some(ship) = world.get::<ShipRecord>(*entity).cloned() else {
            continue;
        };
        if ship.personal_transport || ship.captain.is_some() {
            continue;
        }
        if let Some(mut record) = world.get_mut::<ShipRecord>(*entity) {
            record.orders_suspended = true;
        }
        let ShipLocation::Docked(from) = ship.location else {
            if let Some(mut journey) = world.get_mut::<Journey>(*entity) {
                journey.speed = JourneySpeed::Half;
            }
            continue;
        };
        let Some(destination) = choose_retreat(
            world,
            ship.owner,
            from,
            RouteKind::Space,
            ship.retreat_destination,
        ) else {
            continue;
        };
        if let Some(mut record) = world.get_mut::<ShipRecord>(*entity) {
            record.retreat_destination = Some(destination);
        }
        if destination != from
            && world.get::<Journey>(*entity).is_none()
            && let Some(path) =
                world
                    .resource::<RouteGraph>()
                    .fastest_path(RouteKind::Space, from, destination)
        {
            let mut journey = Journey::new(destination, path, JourneyPurpose::Retreat);
            journey.speed = JourneySpeed::Half;
            world.entity_mut(*entity).insert(journey);
        }
        let _ = id;
    }
    for entity in index.armies.values() {
        let Some(army) = world.get::<ArmyRecord>(*entity).cloned() else {
            continue;
        };
        if army.general.is_some() || matches!(army.location, ArmyLocation::Embarked(_)) {
            continue;
        }
        if let Some(mut record) = world.get_mut::<ArmyRecord>(*entity) {
            record.orders_suspended = true;
        }
        let ArmyLocation::Province(from) = army.location else {
            continue;
        };
        let Some(destination) = choose_retreat(
            world,
            army.owner,
            from,
            RouteKind::Surface,
            army.retreat_destination,
        ) else {
            continue;
        };
        if let Some(mut record) = world.get_mut::<ArmyRecord>(*entity) {
            record.retreat_destination = Some(destination);
        }
        if destination != from
            && world.get::<Journey>(*entity).is_none()
            && let Some(path) =
                world
                    .resource::<RouteGraph>()
                    .fastest_path(RouteKind::Surface, from, destination)
        {
            let mut journey = Journey::new(destination, path, JourneyPurpose::Retreat);
            journey.speed = JourneySpeed::Half;
            world.entity_mut(*entity).insert(journey);
        }
    }
}

fn maintain_officers(world: &mut World) {
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    for (id, entity) in &index.ships {
        let Some(ship) = world.get::<ShipRecord>(*entity).cloned() else {
            continue;
        };
        let dead_captain = ship.captain.filter(|officer| !alive(world, *officer));
        if dead_captain.is_some()
            && let Some(mut record) = world.get_mut::<ShipRecord>(*entity)
        {
            record.captain = None;
        }
        if ship
            .first_officer
            .is_some_and(|officer| !alive(world, officer))
            && let Some(mut record) = world.get_mut::<ShipRecord>(*entity)
        {
            record.first_officer = None;
        }
        promote_ship(world, *id, dead_captain);
    }
    for (id, entity) in &index.armies {
        let Some(army) = world.get::<ArmyRecord>(*entity).cloned() else {
            continue;
        };
        let dead_general = army.general.filter(|officer| !alive(world, *officer));
        if dead_general.is_some()
            && let Some(mut record) = world.get_mut::<ArmyRecord>(*entity)
        {
            record.general = None;
        }
        if army
            .lieutenant
            .is_some_and(|officer| !alive(world, officer))
            && let Some(mut record) = world.get_mut::<ArmyRecord>(*entity)
        {
            record.lieutenant = None;
        }
        promote_army(world, *id, dead_general);
    }
    begin_leaderless_retreats(world);
}

fn best_ai_officer(
    world: &World,
    owner: OrgId,
    target: OfficerTarget,
    post: OfficerPost,
) -> Option<CharacterId> {
    let index = world.get_resource::<crate::politics::PoliticsIndex>()?;
    index
        .characters
        .iter()
        .filter_map(|(id, entity)| {
            let record = world.get::<crate::politics::CharacterRecord>(*entity)?;
            if record.organisation != Some(owner)
                || !validate_appointment(world, owner, target, post, *id)
            {
                return None;
            }
            let command = world
                .get::<crate::politics::CharacterSkills>(*entity)
                .map(|skills| skills.0.command)
                .unwrap_or_default();
            Some((std::cmp::Reverse(command), *id))
        })
        .min()
        .map(|(_, id)| id)
}

fn fill_ai_vacancies(world: &mut World) {
    let player = world
        .get_resource::<crate::politics::PlayerHouse>()
        .and_then(|player| player.0);
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    let mut vacancies = Vec::new();
    for (id, entity) in &index.ships {
        if world.get::<ShipRecord>(*entity).is_some_and(|record| {
            record.captain.is_none() && !record.personal_transport && Some(record.owner) != player
        }) && world.get::<AppointmentJob>(*entity).is_none()
        {
            vacancies.push((OfficerTarget::Ship(*id), OfficerPost::Captain));
        }
    }
    for (id, entity) in &index.armies {
        if world
            .get::<ArmyRecord>(*entity)
            .is_some_and(|record| record.general.is_none() && Some(record.owner) != player)
            && world.get::<AppointmentJob>(*entity).is_none()
        {
            vacancies.push((OfficerTarget::Army(*id), OfficerPost::General));
        }
    }
    for (target, post) in vacancies {
        let Some(owner) = target_owner(world, target) else {
            continue;
        };
        if let Some(officer) = best_ai_officer(world, owner, target, post) {
            begin_appointment(world, target, post, officer);
        }
    }
}

fn postpone_suspended_work(world: &mut World) {
    let Some(index) = world.get_resource::<AssignmentsIndex>().cloned() else {
        return;
    };
    for entity in index.assignments.values() {
        let suspended = world
            .get::<ActiveAssignment>(*entity)
            .is_some_and(|assignment| match assignment.target {
                AssignmentTarget::OwnArmy(army) | AssignmentTarget::ArmyToProvince(army, _) => {
                    crate::access::army(world, army).is_some_and(|record| record.orders_suspended)
                }
                AssignmentTarget::ShipToProvince(ship, _) => {
                    crate::access::ship(world, ship).is_some_and(|record| record.orders_suspended)
                }
                _ => false,
            });
        if suspended && let Some(mut assignment) = world.get_mut::<ActiveAssignment>(*entity) {
            assignment.started = assignment.started.add_days(1);
            assignment.completes = assignment.completes.add_days(1);
        }
    }
}

pub fn place_starting_officers(world: &mut World) {
    let Some(index) = world.get_resource::<ForcesIndex>().cloned() else {
        return;
    };
    for (id, entity) in &index.ships {
        if let Some(ship) = world.get::<ShipRecord>(*entity) {
            for officer in [ship.captain, ship.first_officer].into_iter().flatten() {
                if let Some(character) = crate::access::character_entity(world, officer) {
                    world
                        .entity_mut(character)
                        .insert(CharacterLocation(Location::Aboard(*id)));
                }
            }
        }
    }
    for (id, entity) in &index.armies {
        if let Some(location) = force_officer_location(world, OfficerTarget::Army(*id))
            && let Some(army) = world.get::<ArmyRecord>(*entity)
        {
            for officer in [army.general, army.lieutenant].into_iter().flatten() {
                if let Some(character) = crate::access::character_entity(world, officer) {
                    world
                        .entity_mut(character)
                        .insert(CharacterLocation(location));
                }
            }
        }
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        DailyTick,
        (
            maintain_officers,
            fill_ai_vacancies,
            process_appointments,
            process_transport_jobs,
            postpone_suspended_work,
        )
            .chain()
            .in_set(TickSet::Simulation)
            .before(crate::assignments::resolve_due_assignments),
    );
}
