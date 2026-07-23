//! Native and web entry point for The Last Aeons.
//!
//! The client attaches presentation — the 3D system and globe maps, orbit
//! camera, picking, and 2D information panels — to the authoritative
//! simulation from `aeon_sim`. It never owns gameplay rules. It boots to
//! a title screen; a campaign exists only once the player starts or
//! continues one.

mod assignment_ui;
mod camera;
mod content;
mod forecast_view;
mod map_modes;
mod map_overlay;
mod offer_view;
mod scene;
mod selection;
mod sim_driver;
mod skybox;
mod title;
mod ui;
mod view;

use aeon_sim::AeonSimPlugin;
use bevy::pbr::MaterialPlugin;
use bevy::picking::mesh_picking::MeshPickingPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    // Set before any world exists, so it reads the embedded
                    // table directly rather than the campaign resource.
                    title: aeon_sim::TextDb::embedded()
                        .text("ui.window.title")
                        .to_owned(),
                    // On the web, track the canvas' CSS size (full window).
                    fit_canvas_to_parent: true,
                    ..Default::default()
                }),
                ..Default::default()
            }),
        )
        .add_plugins(MeshPickingPlugin)
        .add_plugins(MaterialPlugin::<scene::GlobeSurfaceMaterial>::default())
        .add_plugins(skybox::SpaceSkyboxPlugin)
        .add_plugins(EguiPlugin::default())
        .add_plugins(AeonSimPlugin)
        .init_resource::<sim_driver::TimeControl>()
        .init_resource::<view::ViewState>()
        .init_resource::<camera::OrbitCamera>()
        .init_resource::<assignment_ui::UiCommandQueue>()
        .init_resource::<assignment_ui::AssignmentForm>()
        .init_resource::<view::SearchState>()
        .init_resource::<view::MapMode>()
        .init_resource::<scene::GlobeBake>()
        .init_resource::<forecast_view::ForecastCache>()
        .init_resource::<forecast_view::AvailabilityView>()
        .init_resource::<offer_view::OfferView>()
        .init_resource::<assignment_ui::LogFilter>()
        .init_resource::<map_modes::MapReadout>()
        .init_resource::<ui::theme::UiTheme>()
        .init_resource::<ui::picker::PickerState>()
        .init_resource::<ui::assignment_popup::AssignmentPopup>()
        .init_resource::<ui::dock::DockState>()
        .init_state::<title::Screen>()
        .init_resource::<title::TitleState>()
        // The client boots to the title screen; no campaign resource
        // exists until the player steps through, and the scene has
        // nothing to spawn a globe from until one does.
        .add_systems(Startup, camera::spawn_camera)
        .add_systems(OnEnter(title::Screen::Playing), scene::spawn_scene)
        .add_systems(
            OnEnter(title::Screen::Title),
            #[cfg(not(target_arch = "wasm32"))]
            title::load_autosave,
            #[cfg(target_arch = "wasm32")]
            || {},
        )
        .add_systems(Update, title::launch.run_if(in_state(title::Screen::Title)))
        .add_systems(
            Update,
            (
                sim_driver::drive_simulation,
                sim_driver::time_hotkeys,
                selection::attach_pickers,
                selection::view_hotkeys,
                scene::spawn_loaded_starbases,
                scene::update_system_positions,
                scene::apply_view_visibility,
                scene::apply_projection,
                scene::update_globe_selection_glow,
                scene::apply_selection_tint,
                camera::retarget_on_view_change,
                camera::drive_camera,
                assignment_ui::auto_pause_on_popups,
                assignment_ui::flush_ui_commands,
                forecast_view::refresh_availability,
                offer_view::refresh_offers,
                forecast_view::refresh_forecast,
                // The bake must observe the readout computed this frame.
                (map_modes::refresh_map_readout, scene::refresh_globe_texture).chain(),
            )
                .run_if(in_state(title::Screen::Playing)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (
                // Watching the file must happen before the style is
                // written, or an edit is a frame late.
                #[cfg(not(target_arch = "wasm32"))]
                ui::theme::reload_theme_from_disk,
                ui::theme::apply_theme,
                title::draw_title.run_if(in_state(title::Screen::Title)),
                (
                    map_overlay::draw_map_overlay,
                    ui::shell::draw_panels,
                    // The picker floats above the panels that open it, so
                    // it is drawn after them and needs no place in the
                    // layout.
                    ui::assignment_popup::draw_assignment_popup,
                    ui::picker::draw_picker,
                    assignment_ui::draw_popups,
                )
                    .run_if(in_state(title::Screen::Playing)),
            )
                .chain(),
        )
        .run();
}
