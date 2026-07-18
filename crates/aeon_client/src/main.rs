//! Native and web entry point for The Last Aeons.
//!
//! The client attaches presentation — the 3D system and globe maps, orbit
//! camera, picking, and 2D information panels — to the authoritative
//! simulation from `aeon_sim`. It never owns gameplay rules. Until the
//! authored scenario lands, it starts a fixed development campaign on the
//! embedded content.

mod camera;
mod content;
mod jobs_ui;
mod panels;
mod scene;
mod selection;
mod sim_driver;
mod view;

use aeon_sim::AeonSimPlugin;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "The Last Aeons".to_owned(),
                // On the web, track the canvas' CSS size (full window).
                fit_canvas_to_parent: true,
                ..Default::default()
            }),
            ..Default::default()
        }))
        .add_plugins(MeshPickingPlugin)
        .add_plugins(EguiPlugin::default())
        .add_plugins(AeonSimPlugin)
        .init_resource::<sim_driver::TimeControl>()
        .init_resource::<view::ViewState>()
        .init_resource::<camera::OrbitCamera>()
        .init_resource::<jobs_ui::UiCommandQueue>()
        .init_resource::<jobs_ui::JobForm>()
        .add_systems(
            Startup,
            (
                sim_driver::begin_dev_campaign,
                camera::spawn_camera,
                scene::spawn_scene,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                sim_driver::drive_simulation,
                sim_driver::time_hotkeys,
                selection::attach_pickers,
                selection::view_hotkeys,
                scene::update_system_positions,
                scene::apply_view_visibility,
                scene::apply_political_colors,
                scene::apply_selection_tint,
                camera::retarget_on_view_change,
                camera::drive_camera,
                jobs_ui::auto_pause_on_popups,
                jobs_ui::flush_ui_commands,
            ),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (panels::draw_panels, jobs_ui::draw_jobs_ui).chain(),
        )
        .run();
}
