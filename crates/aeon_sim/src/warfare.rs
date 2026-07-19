//! Operational warfare: engine-owned military operations, strategic
//! engagement resolution, and standing orders.
//!
//! Military play is a sequence of jobs — move, resupply, patrol, besiege,
//! raid, blockade — led by each army's general. Engagements resolve from
//! strategic state (strength, command, supply, fortification) with a
//! bounded derived-stream swing; there is no tactical battle layer.
//! Standing orders generate reactive defence jobs for idle armies, and a
//! bespoke job on an army always takes precedence because standing orders
//! only fire for idle armies.

use aeon_core::calendar::GameDate;
use aeon_core::rng::DeterministicRng;
use aeon_data::ContentKey;
use aeon_data::model::MilitaryOp;
use bevy::app::App;
use bevy::prelude::{IntoScheduleConfigs, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::forces::{ArmyRecord, ForcesIndex, ShipRecord};
use crate::ids::{ArmyId, OrgId, ProvinceId};
use crate::jobs::{ActiveJob, JobTarget, JobsIndex, LogChannel, LogEntry, MessageLog};
use crate::politics::{PoliticsIndex, TitleHolder, TitleKind, TitleRecord};
use crate::state::{CampaignSeed, ContentDb};

/// A standing order an army follows while idle.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StandingOrder {
    /// Hold position and await orders.
    #[default]
    HoldFast,
    /// React to hostile operations against the owner's provinces on the
    /// army's body by marching to meet them.
    DefendHoldings,
}

/// Order a province loses when a blockade closes on it, before the
/// captain's own competence is counted.
pub const BLOCKADE_ORDER_LOSS: i32 = 20;

/// The content key of the reactive-defence job standing orders start.
pub const REACTIVE_DEFENCE_JOB: &str = "answer-the-alarm";

/// An army's fighting strength from strategic state.
///
/// Manpower, scaled by the general's command (+5% per point), supply
/// state (starving armies fight at 60%), and fortification when defending
/// home ground (+20%).
pub fn army_strength(world: &World, army: &ArmyRecord, defending_home: bool) -> i64 {
    let command = {
        let index = world.resource::<PoliticsIndex>();
        index
            .characters
            .get(&army.general)
            .and_then(|e| world.get::<crate::politics::CharacterSkills>(*e))
            .map(|s| i64::from(s.0.command))
            .unwrap_or(0)
    };
    let mut strength = army.manpower * (100 + command * 5) / 100;
    if army.supplies == 0 {
        strength = strength * 60 / 100;
    }
    if defending_home {
        strength = strength * 120 / 100;
    }
    strength
}

/// The organisation holding a province's title, if any.
pub fn province_holder(world: &World, province: ProvinceId) -> Option<OrgId> {
    let index = world.resource::<PoliticsIndex>();
    let title_id = index.province_titles.get(&province)?;
    let entity = index.titles.get(title_id)?;
    match world.get::<TitleRecord>(*entity)?.holder {
        TitleHolder::Org(org) => Some(org),
        _ => None,
    }
}

/// The defending army in a province: the largest garrison belonging to
/// the province holder (lowest ID on ties).
fn defending_army(world: &World, province: ProvinceId, attacker: OrgId) -> Option<ArmyId> {
    let holder = province_holder(world, province)?;
    if holder == attacker {
        return None;
    }
    let forces = world.resource::<ForcesIndex>();
    forces
        .armies
        .iter()
        .filter_map(|(id, entity)| {
            let army = world.get::<ArmyRecord>(*entity)?;
            (army.owner == holder && army.location == province).then_some((-(army.manpower), *id))
        })
        .min()
        .map(|(_, id)| id)
}

/// Outcome of a field engagement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Engagement {
    /// Whether the attacker carried the field.
    pub attacker_won: bool,
    /// Attacker losses in men.
    pub attacker_losses: i64,
    /// Defender losses in men.
    pub defender_losses: i64,
}

/// Resolves a field engagement from strategic state.
///
/// Strengths carry a bounded swing from a derived stream keyed by both
/// armies and the day; the winner loses 5-15% of their men, the loser
/// 20-35%.
pub fn resolve_engagement(
    world: &mut World,
    attacker: ArmyId,
    defender: ArmyId,
    date: GameDate,
) -> Engagement {
    let seed = world.resource::<CampaignSeed>().0;
    let forces = world.resource::<ForcesIndex>().clone();
    let attacker_record = world
        .get::<ArmyRecord>(forces.armies[&attacker])
        .expect("indexed")
        .clone();
    let defender_record = world
        .get::<ArmyRecord>(forces.armies[&defender])
        .expect("indexed")
        .clone();

    let mut rng = DeterministicRng::derive(
        seed,
        "engagement",
        &[
            attacker.raw(),
            defender.raw(),
            date.days_since_epoch() as u64,
        ],
    );
    let swing = |rng: &mut DeterministicRng, strength: i64| -> i64 {
        strength * (100 + rng.roll_range(-15, 15)) / 100
    };
    let attack = swing(&mut rng, army_strength(world, &attacker_record, false));
    // Defenders are only as reliable as the ground they stand on: a
    // garrison among a hostile population gives way sooner.
    let order_factor = crate::order::defence_factor_permille(
        crate::order::province_order(world, defender_record.location).order,
    );
    let defence =
        swing(&mut rng, army_strength(world, &defender_record, true)) * order_factor / 1000;
    let attacker_won = attack > defence;

    let loss = |rng: &mut DeterministicRng, manpower: i64, winner: bool| -> i64 {
        let permille = if winner {
            rng.roll_range(50, 150)
        } else {
            rng.roll_range(200, 350)
        };
        (manpower * permille / 1000).max(1)
    };
    let attacker_losses = loss(&mut rng, attacker_record.manpower, attacker_won);
    let defender_losses = loss(&mut rng, defender_record.manpower, !attacker_won);

    for (army_id, losses) in [(attacker, attacker_losses), (defender, defender_losses)] {
        let entity = forces.armies[&army_id];
        if let Some(mut army) = world.get_mut::<ArmyRecord>(entity) {
            army.manpower = (army.manpower - losses).max(0);
        }
    }

    // The loser retreats to the nearest owned province; a broken army
    // with no men or nowhere to go disbands.
    let loser = if attacker_won { defender } else { attacker };
    let loser_record = world
        .get::<ArmyRecord>(forces.armies[&loser])
        .expect("indexed")
        .clone();
    if loser_record.manpower == 0 {
        crate::forces::disband_army(world, loser);
    } else if attacker_won {
        let retreat = {
            let index = world.resource::<PoliticsIndex>();
            index
                .titles
                .values()
                .filter_map(|entity| {
                    let title = world.get::<TitleRecord>(*entity)?;
                    match (title.kind, title.holder) {
                        (TitleKind::Province(province), TitleHolder::Org(org))
                            if org == loser_record.owner && province != loser_record.location =>
                        {
                            Some(province)
                        }
                        _ => None,
                    }
                })
                .next()
        };
        match retreat {
            Some(province) => {
                let entity = forces.armies[&loser];
                if let Some(mut army) = world.get_mut::<ArmyRecord>(entity) {
                    army.location = province;
                }
            }
            None => crate::forces::disband_army(world, loser),
        }
    }

    Engagement {
        attacker_won,
        attacker_losses,
        defender_losses,
    }
}

fn log(world: &mut World, org: OrgId, text: String) {
    let date = world.resource::<CampaignClock>().date;
    world
        .resource_mut::<MessageLog>()
        .entries
        .push(LogEntry::new(date, text, LogChannel::Military).by(Some(org)));
}

/// Applies a military operation when its job succeeds. Returns `false`
/// when the operation was defeated (the job reports failure instead).
pub fn apply_military_op(world: &mut World, op: MilitaryOp, job: &ActiveJob) -> bool {
    let date = world.resource::<CampaignClock>().date;
    match (op, job.target) {
        (MilitaryOp::Move, JobTarget::ArmyToProvince(army, destination)) => {
            let entity = world.resource::<ForcesIndex>().armies.get(&army).copied();
            if let Some(entity) = entity
                && let Some(mut record) = world.get_mut::<ArmyRecord>(entity)
            {
                record.location = destination;
            }
            true
        }
        (MilitaryOp::Resupply, JobTarget::OwnArmy(army)) => {
            let entity = world.resource::<ForcesIndex>().armies.get(&army).copied();
            if let Some(entity) = entity {
                let need = {
                    let record = world.get::<ArmyRecord>(entity);
                    record.map(|a| (1 + a.manpower / 1000) * 6).unwrap_or(0)
                };
                let org_entity = world.resource::<PoliticsIndex>().orgs[&job.owner];
                let drawn = world
                    .get_mut::<crate::economy::OrgResources>(org_entity)
                    .map(|mut r| {
                        let drawn = need.min(r.supplies);
                        r.supplies -= drawn;
                        drawn
                    })
                    .unwrap_or(0);
                if let Some(mut record) = world.get_mut::<ArmyRecord>(entity) {
                    record.supplies += drawn;
                }
            }
            true
        }
        (MilitaryOp::Patrol, JobTarget::OwnArmy(_)) => true,
        (MilitaryOp::Besiege, JobTarget::ArmyToProvince(army, target)) => {
            // March to the walls; a defending garrison must be beaten first.
            {
                let entity = world.resource::<ForcesIndex>().armies.get(&army).copied();
                if let Some(entity) = entity
                    && let Some(mut record) = world.get_mut::<ArmyRecord>(entity)
                {
                    record.location = target;
                }
            }
            if let Some(defender) = defending_army(world, target, job.owner) {
                let engagement = resolve_engagement(world, army, defender, date);
                if !engagement.attacker_won {
                    return false;
                }
            }
            // The province falls: its title passes to the besieger.
            let title = {
                let index = world.resource::<PoliticsIndex>();
                index
                    .province_titles
                    .get(&target)
                    .and_then(|id| index.titles.get(id))
                    .copied()
            };
            if let Some(entity) = title
                && let Some(mut record) = world.get_mut::<TitleRecord>(entity)
            {
                let name = record.name.clone();
                record.holder = TitleHolder::Org(job.owner);
                log(world, job.owner, format!("{name} has fallen to siege."));
            }
            // Conquest breeds resentment: the province starts its new
            // allegiance badly out of order.
            crate::order::reset_order(world, target, crate::order::ORDER_AFTER_CONQUEST);
            true
        }
        (MilitaryOp::Raid, JobTarget::ArmyToProvince(army, target)) => {
            if let Some(defender) = defending_army(world, target, job.owner) {
                let engagement = resolve_engagement(world, army, defender, date);
                if !engagement.attacker_won {
                    return false;
                }
            }
            // Loot a tenth of the holder's wealth, up to 100.
            if let Some(holder) = province_holder(world, target)
                && holder != job.owner
            {
                let politics = world.resource::<PoliticsIndex>().clone();
                let looted = world
                    .get_mut::<crate::economy::OrgResources>(politics.orgs[&holder])
                    .map(|mut r| {
                        let looted = (r.wealth / 10).clamp(0, 100);
                        r.wealth -= looted;
                        looted
                    })
                    .unwrap_or(0);
                if let Some(mut r) =
                    world.get_mut::<crate::economy::OrgResources>(politics.orgs[&job.owner])
                {
                    r.wealth += looted;
                }
            }
            // A raided province is left shaken and harder to govern.
            crate::order::adjust_order(world, target, -crate::order::ORDER_RAID_LOSS);
            true
        }
        (MilitaryOp::Blockade, JobTarget::ShipToProvince(ship, target)) => {
            let entity = world.resource::<ForcesIndex>().ships.get(&ship).copied();
            let Some(entity) = entity else {
                return false;
            };
            // A ship without an officer aboard cannot hold a station.
            let captain = world.get::<ShipRecord>(entity).and_then(|s| s.captain);
            let Some(captain) = captain else {
                return false;
            };
            if let Some(mut record) = world.get_mut::<ShipRecord>(entity) {
                record.location = crate::forces::ShipLocation::Docked(target);
                record.blockading = Some(target);
            }
            // A blockade is only as tight as the officer keeping it. The
            // captain's command decides how hard the province feels it.
            let command = crate::forecast::governing_skill(
                world,
                captain,
                aeon_data::model::GoverningSkill::Command,
            );
            let bite = BLOCKADE_ORDER_LOSS + command.clamp(0, 20);
            crate::order::adjust_order(world, target, -bite);
            true
        }
        _ => true,
    }
}

/// Daily: standing orders create reactive defence jobs for idle armies
/// when hostile operations threaten the owner's provinces on their body.
pub fn standing_orders(world: &mut World) {
    let (Some(_), Some(_)) = (
        world.get_resource::<ForcesIndex>(),
        world.get_resource::<JobsIndex>(),
    ) else {
        return;
    };
    let content = world.resource::<ContentDb>().0.clone();
    let Ok(reactive_key) = ContentKey::new(REACTIVE_DEFENCE_JOB) else {
        return;
    };
    if !content.jobs.contains_key(&reactive_key) {
        return;
    }

    // Hostile army operations under way, by target province.
    let threats: Vec<(OrgId, ProvinceId)> = {
        let jobs_index = world.resource::<JobsIndex>();
        jobs_index
            .jobs
            .values()
            .filter_map(|entity| {
                let job = world.get::<ActiveJob>(*entity)?;
                let def = content.jobs.get(&job.def)?;
                match (def.military_op, job.target) {
                    (
                        Some(MilitaryOp::Besiege | MilitaryOp::Raid),
                        JobTarget::ArmyToProvince(_, target),
                    ) => Some((job.owner, target)),
                    _ => None,
                }
            })
            .collect()
    };
    if threats.is_empty() {
        return;
    }

    // Idle defending armies with standing orders, in ID order.
    let busy_armies: Vec<ArmyId> = {
        let jobs_index = world.resource::<JobsIndex>();
        jobs_index
            .jobs
            .values()
            .filter_map(|entity| match world.get::<ActiveJob>(*entity)?.target {
                JobTarget::OwnArmy(army) | JobTarget::ArmyToProvince(army, _) => Some(army),
                _ => None,
            })
            .collect()
    };
    let candidates: Vec<(ArmyId, OrgId, ProvinceId)> = {
        let forces = world.resource::<ForcesIndex>();
        forces
            .armies
            .iter()
            .filter_map(|(id, entity)| {
                let army = world.get::<ArmyRecord>(*entity)?;
                (army.standing_order == StandingOrder::DefendHoldings && !busy_armies.contains(id))
                    .then_some((*id, army.owner, army.location))
            })
            .collect()
    };

    for (army, owner, at) in candidates {
        // The nearest threatened owned province on the same body.
        let target = threats
            .iter()
            .filter(|(aggressor, province)| {
                *aggressor != owner
                    && province_holder(world, *province) == Some(owner)
                    && crate::presence::province_body(world, *province)
                        == crate::presence::province_body(world, at)
            })
            .map(|(_, province)| *province)
            .next();
        let Some(target) = target else {
            continue;
        };
        let general = {
            let forces = world.resource::<ForcesIndex>();
            world
                .get::<ArmyRecord>(forces.armies[&army])
                .map(|a| a.general)
        };
        let Some(general) = general else {
            continue;
        };
        if crate::jobs::validate_start(
            world,
            owner,
            &reactive_key,
            general,
            JobTarget::ArmyToProvince(army, target),
        )
        .is_ok()
        {
            crate::jobs::start_job(
                world,
                owner,
                &reactive_key,
                general,
                JobTarget::ArmyToProvince(army, target),
            );
            let name = {
                let forces = world.resource::<ForcesIndex>();
                world
                    .get::<ArmyRecord>(forces.armies[&army])
                    .map(|a| a.name.clone())
                    .unwrap_or_default()
            };
            log(world, owner, format!("{name} marches to answer the alarm."));
        }
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        DailyTick,
        standing_orders
            .in_set(TickSet::Simulation)
            .after(crate::jobs::resolve_due_jobs),
    );
}
