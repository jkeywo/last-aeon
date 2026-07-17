//! Native and web entry point for The Last Aeons.
//!
//! The client attaches presentation (windowing, 3D maps, 2D panels) to the
//! authoritative simulation from `aeon_sim`; it never owns gameplay rules.
//! Until the scenario pipeline lands, it starts a fixed development
//! campaign and drives it in real time.

use aeon_core::calendar::CalendarDate;
use aeon_sim::state::start_campaign;
use aeon_sim::{AeonSimPlugin, CampaignClock, CampaignConfig, advance_one_day};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiPlugin;

/// Seed for the fixed development campaign.
const DEV_SEED: u64 = 0xA301;

/// Wall-clock seconds per campaign day at the default speed.
const SECONDS_PER_DAY: f32 = 1.0;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "The Last Aeons".to_owned(),
                ..Default::default()
            }),
            ..Default::default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(AeonSimPlugin)
        .add_systems(Startup, begin_dev_campaign)
        .add_systems(Update, drive_simulation)
        .run();
}

fn begin_dev_campaign(world: &mut World) {
    start_campaign(
        world,
        CampaignConfig {
            name: "Development Campaign".to_owned(),
            seed: DEV_SEED,
            start_date: CalendarDate {
                year: 411,
                month: 1,
                day: 1,
            }
            .to_date()
            .expect("dev start date is valid"),
        },
    );
}

/// Maps wall-clock time to discrete daily ticks of the authoritative
/// simulation. Pause and speed selection arrive with the map milestone.
fn drive_simulation(world: &mut World, mut carry: Local<f32>) {
    let delta = world.resource::<Time>().delta_secs();
    *carry += delta;
    let mut advanced = false;
    while *carry >= SECONDS_PER_DAY {
        *carry -= SECONDS_PER_DAY;
        advance_one_day(world);
        advanced = true;
    }
    if advanced {
        let date = world.resource::<CampaignClock>().date;
        let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
        for mut window in windows.iter_mut(world) {
            window.title = format!("The Last Aeons — {date}");
        }
    }
}
