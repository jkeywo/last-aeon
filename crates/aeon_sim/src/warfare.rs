//! Operational warfare: engine-owned military operations, strategic
//! engagement resolution, and standing orders.
//!
//! Military play is a sequence of assignments — move, resupply, patrol, besiege,
//! raid, blockade — led by each army's general. Engagements resolve from
//! strategic state (strength, command, supply, fortification) with a
//! bounded derived-stream swing; there is no tactical battle layer.
//! Standing orders generate reactive defence assignments for idle armies, and a
//! bespoke assignment on an army always takes precedence because standing orders
//! only fire for idle armies.

use aeon_core::calendar::GameDate;
use aeon_core::rng::DeterministicRng;
use aeon_data::ContentKey;
use aeon_data::model::MilitaryOp;
use bevy::app::App;
use bevy::prelude::{IntoScheduleConfigs, World};
use serde::{Deserialize, Serialize};

use crate::assignments::{
    ActiveAssignment, AssignmentTarget, AssignmentsIndex, LogChannel, LogEntry,
};
use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::forces::{ArmyRecord, ForcesIndex, ShipRecord};
use crate::ids::{ArmyId, OrgId, ProvinceId};
use crate::politics::{PoliticsIndex, TitleHolder, TitleKind, TitleRecord};
use crate::state::ContentDb;
use crate::text::TextDb;

/// What a force does when nobody has told it anything.
///
/// An ordered list of assignments it may start on its own. Each day the
/// first one whose authored requirements are satisfied is started, so the
/// same `requires` block that decides whether a button is offered decides
/// whether a standing order fires — a force can never do unbidden what its
/// owner could not have ordered.
///
/// This was two hardcoded variants and a content key spelled out in the
/// engine. Being a list means new behaviour is authored rather than
/// compiled, and reordering is a decision the player can make.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandingOrders(pub Vec<ContentKey>);

impl StandingOrders {
    /// Whether anything at all is standing.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Order a province loses when a blockade closes on it, before the
/// captain's own competence is counted.
pub const BLOCKADE_ORDER_LOSS: i32 = 20;

/// An army's fighting strength from strategic state.
///
/// Manpower, scaled by the general's command (+5% per point), supply
/// state (starving armies fight at 60%), and fortification when defending
/// home ground (+20%).
pub fn army_strength(world: &World, army: &ArmyRecord, defending_home: bool) -> i64 {
    let command =
        crate::access::on_character::<crate::politics::CharacterSkills>(world, army.general)
            .map(|s| i64::from(s.0.command))
            .unwrap_or(0);
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

/// The strategic inputs to a field engagement, gathered before any dice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngagementInputs {
    /// Attacker strength from [`army_strength`].
    pub attack_strength: i64,
    /// Defender strength from [`army_strength`], before the order factor.
    pub defence_strength: i64,
    /// Defence reliability of the ground, in permille.
    pub order_factor: i64,
    /// Attacker manpower, for losses.
    pub attacker_manpower: i64,
    /// Defender manpower, for losses.
    pub defender_manpower: i64,
}

/// Decides a field engagement from its strategic inputs.
///
/// Pure but for the stream: each strength carries a bounded ±15% swing,
/// the winner loses 5-15% of their men, the loser 20-35%. Deciding apart
/// from the world lets the formula be tested without mustering a
/// campaign.
pub fn decide_engagement(inputs: EngagementInputs, rng: &mut DeterministicRng) -> Engagement {
    let swing = |rng: &mut DeterministicRng, strength: i64| -> i64 {
        strength * (100 + rng.roll_range(-15, 15)) / 100
    };
    let attack = swing(rng, inputs.attack_strength);
    // Defenders are only as reliable as the ground they stand on: a
    // garrison among a hostile population gives way sooner.
    let defence = swing(rng, inputs.defence_strength) * inputs.order_factor / 1000;
    let attacker_won = attack > defence;

    let loss = |rng: &mut DeterministicRng, manpower: i64, winner: bool| -> i64 {
        let permille = if winner {
            rng.roll_range(50, 150)
        } else {
            rng.roll_range(200, 350)
        };
        (manpower * permille / 1000).max(1)
    };
    let attacker_losses = loss(rng, inputs.attacker_manpower, attacker_won);
    let defender_losses = loss(rng, inputs.defender_manpower, !attacker_won);

    Engagement {
        attacker_won,
        attacker_losses,
        defender_losses,
    }
}

/// Resolves a field engagement: gathers the strategic inputs, lets
/// [`decide_engagement`] settle the field, and applies the losses and
/// the loser's retreat or destruction.
pub fn resolve_engagement(
    world: &mut World,
    attacker: ArmyId,
    defender: ArmyId,
    date: GameDate,
) -> Engagement {
    let attacker_record = crate::access::army(world, attacker)
        .expect("indexed")
        .clone();
    let defender_record = crate::access::army(world, defender)
        .expect("indexed")
        .clone();

    let mut rng = crate::access::derived_rng(
        world,
        "engagement",
        &[
            attacker.raw(),
            defender.raw(),
            date.days_since_epoch() as u64,
        ],
    );
    let inputs = EngagementInputs {
        attack_strength: army_strength(world, &attacker_record, false),
        defence_strength: army_strength(world, &defender_record, true),
        order_factor: crate::order::defence_factor_permille(
            crate::order::province_order(world, defender_record.location).order,
        ),
        attacker_manpower: attacker_record.manpower,
        defender_manpower: defender_record.manpower,
    };
    let Engagement {
        attacker_won,
        attacker_losses,
        defender_losses,
    } = decide_engagement(inputs, &mut rng);

    for (army_id, losses) in [(attacker, attacker_losses), (defender, defender_losses)] {
        let entity = crate::access::army_entity(world, army_id).expect("indexed");
        if let Some(mut army) = world.get_mut::<ArmyRecord>(entity) {
            army.manpower = (army.manpower - losses).max(0);
        }
    }

    // The loser retreats to the nearest owned province; a broken army
    // with no men or nowhere to go disbands.
    let loser = if attacker_won { defender } else { attacker };
    let loser_record = crate::access::army(world, loser).expect("indexed").clone();
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
                let entity = crate::access::army_entity(world, loser).expect("indexed");
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
    crate::access::log(
        world,
        LogEntry::line(text, LogChannel::Military).by(Some(org)),
    );
}

/// Applies a military operation when its assignment succeeds. Returns `false`
/// when the operation was defeated (the assignment reports failure instead).
pub fn apply_military_op(world: &mut World, op: MilitaryOp, assignment: &ActiveAssignment) -> bool {
    let date = world.resource::<CampaignClock>().date;
    match (op, assignment.target) {
        (MilitaryOp::Move, AssignmentTarget::ArmyToProvince(army, destination)) => {
            if let Some(entity) = crate::access::army_entity(world, army)
                && let Some(mut record) = world.get_mut::<ArmyRecord>(entity)
            {
                record.location = destination;
            }
            true
        }
        (MilitaryOp::Resupply, AssignmentTarget::OwnArmy(army)) => {
            if let Some(entity) = crate::access::army_entity(world, army) {
                let need = {
                    let record = world.get::<ArmyRecord>(entity);
                    record.map(|a| (1 + a.manpower / 1000) * 6).unwrap_or(0)
                };
                let org_entity =
                    crate::access::org_entity(world, assignment.owner).expect("indexed");
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
        (MilitaryOp::Patrol, AssignmentTarget::OwnArmy(_)) => true,
        (MilitaryOp::Besiege, AssignmentTarget::ArmyToProvince(army, target)) => {
            // March to the walls; a defending garrison must be beaten first.
            if let Some(entity) = crate::access::army_entity(world, army)
                && let Some(mut record) = world.get_mut::<ArmyRecord>(entity)
            {
                record.location = target;
            }
            if let Some(defender) = defending_army(world, target, assignment.owner) {
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
                record.holder = TitleHolder::Org(assignment.owner);
                let line = world
                    .resource::<TextDb>()
                    .format("sim.warfare.fallen-to-siege", &[("place", &name)]);
                log(world, assignment.owner, line);
            }
            // Conquest breeds resentment: the province starts its new
            // allegiance badly out of order.
            crate::order::reset_order(world, target, crate::order::ORDER_AFTER_CONQUEST);
            true
        }
        (MilitaryOp::Raid, AssignmentTarget::ArmyToProvince(army, target)) => {
            if let Some(defender) = defending_army(world, target, assignment.owner) {
                let engagement = resolve_engagement(world, army, defender, date);
                if !engagement.attacker_won {
                    return false;
                }
            }
            // Loot a tenth of the holder's wealth, up to 100.
            if let Some(holder) = province_holder(world, target)
                && holder != assignment.owner
            {
                let holder_entity = crate::access::org_entity(world, holder).expect("indexed");
                let looted = world
                    .get_mut::<crate::economy::OrgResources>(holder_entity)
                    .map(|mut r| {
                        let looted = (r.wealth / 10).clamp(0, 100);
                        r.wealth -= looted;
                        looted
                    })
                    .unwrap_or(0);
                let owner_entity =
                    crate::access::org_entity(world, assignment.owner).expect("indexed");
                if let Some(mut r) = world.get_mut::<crate::economy::OrgResources>(owner_entity) {
                    r.wealth += looted;
                }
            }
            // A raided province is left shaken and harder to govern.
            crate::order::adjust_order(world, target, -crate::order::ORDER_RAID_LOSS);
            true
        }
        (MilitaryOp::Blockade, AssignmentTarget::ShipToProvince(ship, target)) => {
            let Some(entity) = crate::access::ship_entity(world, ship) else {
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

/// The provinces of `owner` that something is currently being done to.
///
/// One definition of "under threat", shared by the requirement that gates
/// an alarm-answering assignment and by the standing order that starts
/// one — because an alarm you can answer and an alarm worth answering
/// must be the same alarm.
///
/// Both senses count: a hostile operation aimed at the holding, which is
/// a threat before anyone has arrived, and a hostile force standing in
/// it, which is a threat whether or not an operation is under way.
pub fn threatened_holdings(world: &World, owner: OrgId) -> Vec<ProvinceId> {
    let Some(content) = world.get_resource::<ContentDb>().map(|db| db.0.clone()) else {
        return Vec::new();
    };
    let held = crate::order::held_provinces(world, owner);

    let mut threatened: Vec<ProvinceId> = Vec::new();
    if let Some(index) = world.get_resource::<AssignmentsIndex>() {
        for entity in index.assignments.values() {
            let Some(assignment) = world.get::<ActiveAssignment>(*entity) else {
                continue;
            };
            if assignment.owner == owner {
                continue;
            }
            let Some(def) = content.assignments.get(&assignment.def) else {
                continue;
            };
            if let (
                Some(MilitaryOp::Besiege | MilitaryOp::Raid),
                AssignmentTarget::ArmyToProvince(_, target),
            ) = (def.military_op, assignment.target)
                && held.contains(&target)
                && !threatened.contains(&target)
            {
                threatened.push(target);
            }
        }
    }
    for province in &held {
        if !threatened.contains(province) && hostile_force_in(world, owner, *province) {
            threatened.push(*province);
        }
    }
    // Stable order, so which holding a standing order answers first does
    // not depend on iteration accidents.
    threatened.sort();
    threatened
}

/// Daily: forces with standing orders start the first of them they can.
///
/// Priority is the list's own order, so what a force reaches for first is
/// authored — or chosen by the player — rather than decided here. Every
/// start goes through `validate_start` exactly as a player's would, which
/// is what guarantees a standing order cannot do something its owner could
/// not have ordered by hand.
///
/// Forces are visited in stable ID order and the list in its own order, so
/// this replays identically.
pub fn standing_orders(world: &mut World) {
    let (Some(_), Some(_)) = (
        world.get_resource::<ForcesIndex>(),
        world.get_resource::<AssignmentsIndex>(),
    ) else {
        return;
    };

    // Forces already busy are left alone.
    let busy: Vec<ArmyId> = {
        let index = world.resource::<AssignmentsIndex>();
        index
            .assignments
            .values()
            .filter_map(
                |entity| match world.get::<ActiveAssignment>(*entity)?.target {
                    AssignmentTarget::OwnArmy(army) | AssignmentTarget::ArmyToProvince(army, _) => {
                        Some(army)
                    }
                    _ => None,
                },
            )
            .collect()
    };

    let idle: Vec<(ArmyId, OrgId, StandingOrders)> = {
        let forces = world.resource::<ForcesIndex>();
        forces
            .armies
            .iter()
            .filter_map(|(id, entity)| {
                let army = world.get::<ArmyRecord>(*entity)?;
                (!army.standing_order.is_empty() && !busy.contains(id))
                    .then(|| (*id, army.owner, army.standing_order.clone()))
            })
            .collect()
    };

    for (army, owner, orders) in idle {
        let Some(general) = crate::access::army(world, army).map(|a| a.general) else {
            continue;
        };
        for assignment in &orders.0 {
            if let Some(target) = standing_target(world, owner, army, assignment)
                && crate::assignments::validate_start(world, owner, assignment, general, target)
                    .is_ok()
            {
                crate::assignments::start_assignment(world, owner, assignment, general, target);
                let name = crate::access::army(world, army)
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                let title = world
                    .resource::<ContentDb>()
                    .0
                    .assignments
                    .get(assignment)
                    .map(|def| def.title.clone())
                    .unwrap_or_default();
                let line = world.resource::<TextDb>().format(
                    "sim.warfare.standing-order",
                    &[("army", &name), ("assignment", &title)],
                );
                log(world, owner, line);
                break;
            }
        }
    }
}

/// The target a standing order would act on, if it has one available.
///
/// An assignment that acts on the force itself needs nothing more. One
/// that marches somewhere needs somewhere worth marching to, and the only
/// destination a force picks unbidden is a holding of its owner's with a
/// hostile force standing in it — which is what the old reactive defence
/// did, now expressed as a target rather than as a hardcoded assignment.
fn standing_target(
    world: &World,
    owner: OrgId,
    army: ArmyId,
    assignment: &ContentKey,
) -> Option<AssignmentTarget> {
    let content = world.get_resource::<ContentDb>()?.0.clone();
    let def = content.assignments.get(assignment)?;
    match def.target {
        aeon_data::model::AssignmentTargetKind::OwnArmy => Some(AssignmentTarget::OwnArmy(army)),
        aeon_data::model::AssignmentTargetKind::OwnArmyAndProvince => {
            let at = crate::access::army(world, army)?.location;
            let body = crate::presence::province_body(world, at);
            threatened_holdings(world, owner)
                .into_iter()
                .find(|province| crate::presence::province_body(world, *province) == body)
                .map(|province| AssignmentTarget::ArmyToProvince(army, province))
        }
        _ => None,
    }
}

/// Whether a force belonging to anyone but `owner` stands in `province`.
fn hostile_force_in(world: &World, owner: OrgId, province: ProvinceId) -> bool {
    let Some(forces) = world.get_resource::<ForcesIndex>() else {
        return false;
    };
    forces.armies.values().any(|entity| {
        world
            .get::<ArmyRecord>(*entity)
            .is_some_and(|army| army.location == province && army.owner != owner)
    })
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        DailyTick,
        standing_orders
            .in_set(TickSet::Simulation)
            .after(crate::assignments::resolve_due_assignments),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(attack: i64, defence: i64) -> EngagementInputs {
        EngagementInputs {
            attack_strength: attack,
            defence_strength: defence,
            order_factor: 1000,
            attacker_manpower: attack,
            defender_manpower: defence,
        }
    }

    fn rng(seed: u64) -> DeterministicRng {
        DeterministicRng::derive(seed, "engagement-test", &[seed])
    }

    #[test]
    fn losses_stay_inside_their_stated_bands() {
        for seed in 0..500 {
            let result = decide_engagement(inputs(10_000, 10_000), &mut rng(seed));
            let (winner, loser) = if result.attacker_won {
                (result.attacker_losses, result.defender_losses)
            } else {
                (result.defender_losses, result.attacker_losses)
            };
            assert!((500..=1500).contains(&winner), "winner lost {winner}");
            assert!((2000..=3500).contains(&loser), "loser lost {loser}");
        }
    }

    #[test]
    fn overwhelming_strength_always_carries_the_field() {
        // The swing is bounded at ±15%, so twice the strength cannot lose.
        for seed in 0..500 {
            let result = decide_engagement(inputs(20_000, 10_000), &mut rng(seed));
            assert!(result.attacker_won, "seed {seed} lost a 2:1 field");
        }
    }

    #[test]
    fn hostile_ground_costs_the_defender_the_bounded_swing_cannot_save() {
        // At the minimum defence factor the ground gives way: an equal
        // defender on fully disordered ground loses even their best roll
        // against the attacker's worst.
        let collapsed = EngagementInputs {
            order_factor: crate::order::defence_factor_permille(0),
            ..inputs(10_000, 10_000)
        };
        for seed in 0..500 {
            let result = decide_engagement(collapsed, &mut rng(seed));
            assert!(result.attacker_won, "seed {seed} held untenable ground");
        }
    }

    #[test]
    fn the_same_stream_decides_the_same_field() {
        let a = decide_engagement(inputs(12_000, 9_000), &mut rng(42));
        let b = decide_engagement(inputs(12_000, 9_000), &mut rng(42));
        assert_eq!(a, b);
    }
}
