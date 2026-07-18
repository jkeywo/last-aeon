//! Wall-clock driving of the authoritative simulation.
//!
//! The client owns pause and speed as presentation state; the simulation
//! only ever sees discrete daily ticks through the same
//! [`advance_one_day`] every other embedding uses.

use aeon_core::calendar::CalendarDate;
use aeon_sim::state::start_campaign_with_content;
use aeon_sim::{CampaignConfig, advance_one_day};
use bevy::prelude::*;

use crate::content;

/// Seed for the fixed development campaign.
const DEV_SEED: u64 = 0xA301;

/// Client-side time control. The campaign starts paused.
#[derive(Resource)]
pub struct TimeControl {
    /// Whether campaign time is flowing.
    pub paused: bool,
    /// Campaign days advanced per wall-clock second when unpaused.
    pub days_per_second: f32,
    /// Fractional-day accumulator.
    carry: f32,
}

impl Default for TimeControl {
    fn default() -> Self {
        Self {
            paused: true,
            days_per_second: 1.0,
            carry: 0.0,
        }
    }
}

/// Available speed steps, in days per wall-clock second.
pub const SPEED_STEPS: [f32; 3] = [1.0, 3.0, 10.0];

pub fn begin_dev_campaign(world: &mut World) {
    let content = content::load_embedded();
    // The campaign name and start date come from the authored scenario, so
    // the client runs exactly what the scenario declares.
    let (name, start_date) = content
        .scenario
        .as_ref()
        .map(|scenario| {
            (
                scenario.name.clone(),
                CalendarDate {
                    year: scenario.start_year,
                    month: scenario.start_month,
                    day: scenario.start_day,
                }
                .to_date()
                .expect("authored scenario start date is valid"),
            )
        })
        .unwrap_or_else(|| {
            (
                "Development Campaign".to_owned(),
                CalendarDate {
                    year: 411,
                    month: 1,
                    day: 1,
                }
                .to_date()
                .expect("fallback start date is valid"),
            )
        });
    start_campaign_with_content(
        world,
        CampaignConfig {
            name,
            seed: DEV_SEED,
            start_date,
        },
        content,
    );
}

/// Advances the simulation according to wall time, pause, and speed.
pub fn drive_simulation(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs();
    let (paused, rate) = {
        let control = world.resource::<TimeControl>();
        (control.paused, control.days_per_second)
    };
    if paused {
        return;
    }
    let mut days = 0u32;
    {
        let mut control = world.resource_mut::<TimeControl>();
        control.carry += delta * rate;
        while control.carry >= 1.0 {
            control.carry -= 1.0;
            days += 1;
        }
    }
    for _ in 0..days {
        advance_one_day(world);
    }
}

/// Keyboard shortcuts: space pauses, digits pick speeds.
pub fn time_hotkeys(keys: Res<ButtonInput<KeyCode>>, mut control: ResMut<TimeControl>) {
    if keys.just_pressed(KeyCode::Space) {
        control.paused = !control.paused;
    }
    for (index, key) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
        .into_iter()
        .enumerate()
    {
        if keys.just_pressed(key) {
            control.days_per_second = SPEED_STEPS[index];
        }
    }
}
