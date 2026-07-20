//! The campaign clock and tick schedules.
//!
//! Simulation time advances only through [`advance_one_day`]: the date
//! increments, due player commands apply, and the daily schedule runs,
//! followed by the monthly and yearly pulse schedules on their boundaries.
//! Real-time pacing, pause, and speed belong to presentation — the
//! authoritative simulation only ever sees discrete days.

use aeon_core::calendar::GameDate;
use bevy::app::App;
use bevy::ecs::schedule::{Schedule, ScheduleLabel, SystemSet};
use bevy::prelude::{IntoScheduleConfigs, Resource, World};

/// Runs once per campaign day, in [`TickSet`] order.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DailyTick;

/// Runs on the first day of each month, after that day's [`DailyTick`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MonthlyPulse;

/// Runs on the first day of each year, after that day's [`MonthlyPulse`].
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct YearlyPulse;

/// Fixed ordering of work within a [`DailyTick`].
///
/// Explicit ordering is a determinism requirement, not a style preference:
/// system insertion order must never influence outcomes.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TickSet {
    /// Apply due player commands.
    Commands,
    /// Advance the world: travel, assignments, economy, politics.
    Simulation,
    /// Derive consequences: results, popups, log entries.
    Events,
    /// Bookkeeping that must observe a settled day.
    Cleanup,
}

/// The authoritative campaign date.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct CampaignClock {
    /// The campaign's first day.
    pub start_date: GameDate,
    /// The current day.
    pub date: GameDate,
}

pub(crate) fn install(app: &mut App) {
    app.add_schedule(Schedule::new(DailyTick));
    app.add_schedule(Schedule::new(MonthlyPulse));
    app.add_schedule(Schedule::new(YearlyPulse));
    app.configure_sets(
        DailyTick,
        (
            TickSet::Commands,
            TickSet::Simulation,
            TickSet::Events,
            TickSet::Cleanup,
        )
            .chain(),
    );
}

/// Advances the campaign by exactly one day.
///
/// This is the only way simulation time moves, for every embedding: the
/// headless host, tests, tools, and the real-time clients all call this.
///
/// # Panics
/// Panics if no campaign has been started in this world.
pub fn advance_one_day(world: &mut World) {
    let new_date = {
        let mut clock = world.resource_mut::<CampaignClock>();
        clock.date = clock.date.add_days(1);
        clock.date
    };
    world.run_schedule(DailyTick);
    if new_date.is_month_start() {
        world.run_schedule(MonthlyPulse);
    }
    if new_date.is_year_start() {
        world.run_schedule(YearlyPulse);
    }
}
