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

/// Folded into every new campaign's seed, so a seed is never zero even
/// when the clock reads oddly.
const SEED_SALT: u64 = 0xA301;

/// Where the running campaign autosaves, next to the executable's
/// working directory. Native only; the web build saves nothing.
#[cfg(not(target_arch = "wasm32"))]
pub const AUTOSAVE_PATH: &str = "last-aeons-autosave.ron";

/// Campaign days between autosaves.
#[cfg(not(target_arch = "wasm32"))]
const AUTOSAVE_EVERY_DAYS: u32 = 30;

/// Client-side time control. The campaign starts paused.
#[derive(Resource)]
pub struct TimeControl {
    /// Whether campaign time is flowing.
    pub paused: bool,
    /// Campaign days advanced per wall-clock second when unpaused.
    pub days_per_second: f32,
    /// Fractional-day accumulator.
    carry: f32,
    /// Campaign days advanced since the last autosave.
    #[cfg(not(target_arch = "wasm32"))]
    days_since_save: u32,
}

impl Default for TimeControl {
    fn default() -> Self {
        Self {
            paused: true,
            days_per_second: 1.0,
            carry: 0.0,
            #[cfg(not(target_arch = "wasm32"))]
            days_since_save: 0,
        }
    }
}

/// Available speed steps, in days per wall-clock second.
pub const SPEED_STEPS: [f32; 3] = [1.0, 3.0, 10.0];

/// Starts the authored scenario as a fresh campaign.
///
/// The seed is drawn from when the button was pressed — a choice the
/// presentation layer is entitled to make, since the seed is campaign
/// configuration; once folded into the campaign it is recorded state,
/// and the campaign replays deterministically from it. `spectator`
/// clears the player house before the first day ever ticks.
pub fn begin_campaign(world: &mut World, spectator: bool) {
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
    let seed = SEED_SALT ^ world.resource::<Time>().elapsed().as_nanos() as u64;
    start_campaign_with_content(
        world,
        CampaignConfig {
            name,
            seed,
            start_date,
        },
        content,
    );
    if spectator {
        aeon_sim::state::become_spectator(world);
    }
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

    // A campaign that runs writes itself down as it goes, so the title
    // screen's Continue always has something honest to offer.
    #[cfg(not(target_arch = "wasm32"))]
    if days > 0 {
        let due = {
            let mut control = world.resource_mut::<TimeControl>();
            control.days_since_save += days;
            control.days_since_save >= AUTOSAVE_EVERY_DAYS
        };
        if due {
            write_autosave(world);
            world.resource_mut::<TimeControl>().days_since_save = 0;
        }
    }
}

/// Writes the running campaign to the autosave file: the same
/// versioned, hash-verified snapshot every other embedding uses.
#[cfg(not(target_arch = "wasm32"))]
fn write_autosave(world: &World) {
    let snapshot = aeon_sim::snapshot::capture_snapshot(world);
    match aeon_sim::persistence::snapshot_to_ron(&snapshot) {
        Ok(document) => {
            if let Err(err) = std::fs::write(AUTOSAVE_PATH, document) {
                warn!("autosave failed: {err}");
            }
        }
        Err(err) => warn!("autosave failed to serialise: {err}"),
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
