//! Native and web entry point for The Last Aeons.
//!
//! The client attaches presentation (windowing, 3D maps, 2D panels) to the
//! authoritative simulation from `aeon_sim`; it never owns gameplay rules.

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use aeon_sim::AeonSimPlugin;

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
        .run();
}
