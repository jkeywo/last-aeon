//! Provincial order: local compliance and governability.
//!
//! Order is a bounded per-province value that connects rule, economy,
//! presence, and warfare rather than being a management game of its own.
//! Garrisons, a ruler's physical presence, and administrative work raise
//! it; siege, raid, blockade, supply shortage, and neglect lower it. In
//! turn it scales what a province produces and how reliably it is
//! defended, and it gates the pressure events that arise there.
//!
//! Order that stays critical eventually costs the holder the province: a
//! telegraphed revolt vacates the title, which any house can then take by
//! force. Nothing here is hidden or irreversible — the clock is visible
//! while it runs, and a vacated province can be retaken.

use bevy::app::App;
use bevy::prelude::{Component, IntoScheduleConfigs, World};
use serde::{Deserialize, Serialize};

use crate::clock::{DailyTick, TickSet};
use crate::economy::OrgResources;
use crate::forces::{ArmyRecord, ForcesIndex, ShipRecord};
use crate::ids::{OrgId, ProvinceId};
use crate::jobs::{LogChannel, LogEntry, LogSubject};
use crate::map::MapIndex;
use crate::politics::{CharacterRecord, PoliticsIndex, TitleHolder, TitleRecord};
use crate::presence::{CharacterLocation, Location};
use crate::text::TextDb;

/// The highest order a province can hold.
pub const ORDER_MAX: i32 = 1000;
/// The order a province enjoys at campaign start: settled, but not perfect.
pub const ORDER_START: i32 = 800;
/// At or below this, the province is in open unrest and the revolt clock runs.
pub const ORDER_CRITICAL: i32 = 200;
/// Order restored to a shaken but governable level after a revolt.
pub const ORDER_AFTER_REVOLT: i32 = 400;
/// Consecutive days of unrest before a province throws off its ruler.
pub const REVOLT_DAYS: i64 = 120;
/// The order a province is left in immediately after being taken by siege.
pub const ORDER_AFTER_CONQUEST: i32 = 350;
/// Order lost when a province is successfully raided.
pub const ORDER_RAID_LOSS: i32 = 150;

/// A province's order and how long it has been in unrest.
#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvincialOrder {
    /// Current order, clamped to `0..=ORDER_MAX`.
    pub order: i32,
    /// Consecutive days spent at or below [`ORDER_CRITICAL`].
    pub unrest_days: i64,
}

impl Default for ProvincialOrder {
    fn default() -> Self {
        Self {
            order: ORDER_START,
            unrest_days: 0,
        }
    }
}

impl ProvincialOrder {
    /// Whether the province is in open unrest.
    pub fn in_unrest(&self) -> bool {
        self.order <= ORDER_CRITICAL
    }

    /// Days remaining before revolt, while the clock runs.
    pub fn days_to_revolt(&self) -> Option<i64> {
        self.in_unrest()
            .then(|| (REVOLT_DAYS - self.unrest_days).max(0))
    }

    /// Adds `delta`, keeping the value inside its bounds.
    pub fn adjust(&mut self, delta: i32) {
        self.order = (self.order + delta).clamp(0, ORDER_MAX);
    }
}

/// How much of its authored output a province actually yields, in permille.
///
/// Neutral at [`ORDER_START`], so a settled realm produces exactly its
/// authored figures; perfect order pays a premium and collapse is ruinous.
pub fn output_factor_permille(order: i32) -> i64 {
    i64::from(200 + order.clamp(0, ORDER_MAX))
}

/// How reliably a province's defenders fight, in permille.
///
/// Also neutral at [`ORDER_START`]: a garrison among a hostile population
/// gives ground, one among a loyal population holds it.
pub fn defence_factor_permille(order: i32) -> i64 {
    i64::from(600 + order.clamp(0, ORDER_MAX) / 2)
}

/// Reads a province's order, defaulting for provinces without the component.
pub fn province_order(world: &World, province: ProvinceId) -> ProvincialOrder {
    crate::access::province_entity(world, province)
        .and_then(|entity| world.get::<ProvincialOrder>(entity))
        .copied()
        .unwrap_or_default()
}

/// Applies a change to a province's order, returning the new value.
pub fn adjust_order(world: &mut World, province: ProvinceId, delta: i32) -> i32 {
    let Some(entity) = crate::access::province_entity(world, province) else {
        return 0;
    };
    match world.get_mut::<ProvincialOrder>(entity) {
        Some(mut state) => {
            state.adjust(delta);
            state.order
        }
        None => {
            let mut state = ProvincialOrder::default();
            state.adjust(delta);
            let order = state.order;
            world.entity_mut(entity).insert(state);
            order
        }
    }
}

/// Sets a province's order outright and clears its unrest clock, as when
/// it changes hands.
pub fn reset_order(world: &mut World, province: ProvinceId, order: i32) {
    let Some(entity) = crate::access::province_entity(world, province) else {
        return;
    };
    let state = ProvincialOrder {
        order: order.clamp(0, ORDER_MAX),
        unrest_days: 0,
    };
    world.entity_mut(entity).insert(state);
}

/// The daily pressures acting on one province, kept as a struct so the
/// same reasoning can be shown to the player.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct OrderPressures {
    /// A garrison belonging to the holder stands here.
    pub garrison: bool,
    /// A member of the holding house is physically present.
    pub presence: bool,
    /// A hostile army occupies the province.
    pub occupied: bool,
    /// A hostile ship blockades it.
    pub blockaded: bool,
    /// The holder has run out of supplies.
    pub shortage: bool,
}

impl OrderPressures {
    /// Whether the holder is actually attending to this province.
    pub fn attended(&self) -> bool {
        self.garrison || self.presence
    }

    /// The net daily change these pressures produce.
    ///
    /// A quiet province holds its ground indefinitely — the ordinary
    /// machinery of rule keeps it where it is. Active harm always pushes
    /// it down, and damage is only *repaired* where the holder attends to
    /// the province in person or with troops. Neglect therefore costs
    /// nothing on settled ground but leaves damaged ground unmended.
    ///
    /// Attention only restores a province to [`ORDER_START`]: merely
    /// living somewhere does not make it exemplary. Order beyond that is
    /// earned by deliberate administrative work, which applies its own
    /// authored effects.
    pub fn daily_delta(&self, order: i32) -> i32 {
        let mut harm = 0;
        if self.occupied {
            harm += 6;
        }
        if self.blockaded {
            harm += 3;
        }
        if self.shortage {
            harm += 2;
        }
        if harm > 0 {
            return -harm;
        }
        if self.attended() && order < ORDER_START {
            return 2;
        }
        0
    }

    /// A short player-facing explanation of the pressures in force.
    pub fn describe(&self, strings: &TextDb) -> String {
        let mut parts = Vec::new();
        for (in_force, key) in [
            (self.occupied, "sim.pressure.occupied"),
            (self.blockaded, "sim.pressure.blockaded"),
            (self.shortage, "sim.pressure.shortage"),
            (self.garrison, "sim.pressure.garrison"),
            (self.presence, "sim.pressure.presence"),
        ] {
            if in_force {
                parts.push(strings.text(key));
            }
        }
        if parts.is_empty() {
            strings.text("sim.pressure.none").to_owned()
        } else {
            parts.join(", ")
        }
    }
}

/// Works out the pressures acting on a province today.
pub fn pressures(world: &World, province: ProvinceId) -> OrderPressures {
    let holder = crate::warfare::province_holder(world, province);
    let mut pressures = OrderPressures::default();

    if let Some(forces) = world.get_resource::<ForcesIndex>() {
        for entity in forces.armies.values() {
            let Some(army) = world.get::<ArmyRecord>(*entity) else {
                continue;
            };
            if army.location != province {
                continue;
            }
            match holder {
                Some(holder) if army.owner == holder => pressures.garrison = true,
                _ => pressures.occupied = true,
            }
        }
        for entity in forces.ships.values() {
            if world
                .get::<ShipRecord>(*entity)
                .is_some_and(|ship| ship.blockading == Some(province))
            {
                pressures.blockaded = true;
            }
        }
    }

    if let Some(holder) = holder {
        // A living member of the holding house standing in the province.
        if let Some(index) = world.get_resource::<PoliticsIndex>() {
            pressures.presence = index.characters.values().any(|entity| {
                let member = world
                    .get::<CharacterRecord>(*entity)
                    .is_some_and(|record| record.alive() && record.organisation == Some(holder));
                let here = world
                    .get::<CharacterLocation>(*entity)
                    .is_some_and(|location| location.0 == Location::Province(province));
                member && here
            });
            pressures.shortage = crate::access::org_entity(world, holder)
                .and_then(|entity| world.get::<OrgResources>(entity))
                .is_some_and(|resources| resources.supplies <= 0);
        }
    }

    pressures
}

/// Advances every province's order by one day, and revolts those that have
/// been in unrest too long.
///
/// Provinces are visited in stable ID order, and every step is integer
/// arithmetic, so the whole system replays identically.
pub fn daily_order(world: &mut World) {
    if world.get_resource::<MapIndex>().is_none() {
        return;
    }
    for province in crate::access::province_ids(world) {
        let pressures = pressures(world, province);
        let before = province_order(world, province);
        let delta = pressures.daily_delta(before.order);

        let Some(entity) = world
            .resource::<MapIndex>()
            .provinces
            .get(&province)
            .copied()
        else {
            continue;
        };
        let mut state = before;
        state.adjust(delta);

        // The unrest clock runs only while order is critical, and resets
        // the moment the province is brought back from the brink.
        if state.in_unrest() {
            state.unrest_days += 1;
        } else {
            state.unrest_days = 0;
        }
        let revolting = state.unrest_days >= REVOLT_DAYS;
        world.entity_mut(entity).insert(state);

        // Telegraph the danger on the first day of the clock, however the
        // province came to be in unrest.
        if state.unrest_days == 1 {
            let name = crate::access::province_name(world, province);
            let holder = crate::warfare::province_holder(world, province);
            let line = world.resource::<TextDb>().format(
                "sim.order.unrest-begins",
                &[
                    ("province", &name),
                    ("days", &REVOLT_DAYS.to_string()),
                ],
            );
            crate::access::log(
                world,
                LogEntry::line(line, LogChannel::Economy)
                .by(holder)
                .about(LogSubject::Province(province)),
            );
        }

        if revolting {
            revolt(world, province);
        }
    }
}

/// A province throws off its ruler: the title falls vacant and can be
/// taken by whoever can hold it.
fn revolt(world: &mut World, province: ProvinceId) {
    let name = crate::access::province_name(world, province);
    let holder = crate::warfare::province_holder(world, province);

    let title = {
        let index = world.resource::<PoliticsIndex>();
        index
            .province_titles
            .get(&province)
            .and_then(|id| index.titles.get(id))
            .copied()
    };
    if let Some(entity) = title
        && let Some(mut record) = world.get_mut::<TitleRecord>(entity)
    {
        record.holder = TitleHolder::Vacant;
    }
    reset_order(world, province, ORDER_AFTER_REVOLT);

    let line = world
        .resource::<TextDb>()
        .format("sim.order.revolted", &[("province", &name)]);
    crate::access::log(
        world,
        LogEntry::line(line, LogChannel::Politics)
        .by(holder)
        .about(LogSubject::Province(province)),
    );
}

/// Every province held by an organisation, in stable ID order.
pub fn held_provinces(world: &World, org: OrgId) -> Vec<ProvinceId> {
    let Some(index) = world.get_resource::<PoliticsIndex>() else {
        return Vec::new();
    };
    index
        .province_titles
        .iter()
        .filter(|(_, title_id)| {
            index
                .titles
                .get(title_id)
                .and_then(|entity| world.get::<TitleRecord>(*entity))
                .is_some_and(|record| record.holder == TitleHolder::Org(org))
        })
        .map(|(province, _)| *province)
        .collect()
}

/// Installs the daily order system.
pub fn install(app: &mut App) {
    app.add_systems(DailyTick, daily_order.in_set(TickSet::Simulation));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_and_defence_are_neutral_at_the_starting_order() {
        assert_eq!(output_factor_permille(ORDER_START), 1000);
        assert_eq!(defence_factor_permille(ORDER_START), 1000);
    }

    #[test]
    fn order_scales_output_and_defence_in_the_right_direction() {
        assert!(output_factor_permille(ORDER_MAX) > output_factor_permille(ORDER_START));
        assert!(output_factor_permille(0) < output_factor_permille(ORDER_START));
        assert!(defence_factor_permille(ORDER_MAX) > defence_factor_permille(ORDER_START));
        assert!(defence_factor_permille(0) < defence_factor_permille(ORDER_START));
    }

    #[test]
    fn order_stays_within_bounds() {
        let mut state = ProvincialOrder::default();
        state.adjust(10_000);
        assert_eq!(state.order, ORDER_MAX);
        state.adjust(-10_000);
        assert_eq!(state.order, 0);
    }

    #[test]
    fn occupation_outweighs_a_garrison() {
        let occupied = OrderPressures {
            occupied: true,
            garrison: true,
            ..Default::default()
        };
        assert!(
            occupied.daily_delta(ORDER_START) < 0,
            "an occupied province should still be losing order"
        );
    }

    #[test]
    fn attention_repairs_damage_but_earns_no_premium() {
        let held = OrderPressures {
            garrison: true,
            ..Default::default()
        };
        assert!(held.daily_delta(ORDER_START - 100) > 0, "should recover");
        assert_eq!(
            held.daily_delta(ORDER_START),
            0,
            "merely being present should not push a province past settled"
        );
        assert_eq!(
            held.daily_delta(ORDER_MAX),
            0,
            "and certainly not beyond the cap"
        );
    }

    #[test]
    fn a_quiet_province_holds_its_ground() {
        // Neglect costs nothing on settled ground: the ordinary machinery
        // of rule keeps a peaceful province where it is, so the campaign's
        // opening economy is not quietly eroded.
        let quiet = OrderPressures::default();
        assert_eq!(quiet.daily_delta(ORDER_START), 0);
    }

    #[test]
    fn damage_is_only_repaired_where_the_holder_attends() {
        let damaged = ORDER_START - 300;
        let unattended = OrderPressures::default();
        assert_eq!(
            unattended.daily_delta(damaged),
            0,
            "unattended damage should not mend itself"
        );
        let attended = OrderPressures {
            presence: true,
            ..Default::default()
        };
        assert!(attended.daily_delta(damaged) > 0);
    }

    #[test]
    fn harm_outweighs_attention() {
        let contested = OrderPressures {
            garrison: true,
            presence: true,
            occupied: true,
            ..Default::default()
        };
        assert!(contested.daily_delta(ORDER_START) < 0);
    }

    #[test]
    fn the_revolt_clock_only_runs_in_unrest() {
        let calm = ProvincialOrder {
            order: ORDER_CRITICAL + 1,
            unrest_days: 0,
        };
        assert!(!calm.in_unrest());
        assert_eq!(calm.days_to_revolt(), None);

        let unrest = ProvincialOrder {
            order: ORDER_CRITICAL,
            unrest_days: 20,
        };
        assert!(unrest.in_unrest());
        assert_eq!(unrest.days_to_revolt(), Some(REVOLT_DAYS - 20));
    }
}
