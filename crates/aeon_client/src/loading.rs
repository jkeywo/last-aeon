//! The boot loading screen: warms the presentation assets the game
//! fetches at runtime and shows a spinner until they are ready.
//!
//! The simulation's content is embedded in the binary; these are the
//! files served alongside it — the shared skybox, the political map
//! textures, and the station model. Loading them here means the first
//! campaign opens without pop-in, and on the web a cold cache spends its
//! wait behind an honest spinner rather than a blank canvas.

use bevy::asset::LoadState;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::title::Screen;

/// Image assets the client fetches at runtime, mirroring the authored
/// bodies in [`crate::scene`] plus the shared skybox.
const PRELOAD_IMAGES: &[&str] = &[
    "skybox/phoenix_space_cubemap.png",
    "textures/maps/ashkarr_surface.png",
    "textures/maps/ashkarr_province_ids.png",
    "textures/maps/vesk_surface.png",
    "textures/maps/vesk_province_ids.png",
];

/// The station model, loaded as a glTF scene.
const PRELOAD_MODEL: &str = "models/aurelian_spire.glb";

/// Handles held so the preloaded assets stay resident and their load
/// state can be polled from the loading screen.
#[derive(Resource, Default)]
pub struct GameAssets {
    handles: Vec<UntypedHandle>,
}

/// Kicks off loading every runtime asset at startup. Bevy caches by path,
/// so the later `load` calls in the scene reuse these warmed assets.
pub fn begin_preload(mut assets: ResMut<GameAssets>, asset_server: Res<AssetServer>) {
    for path in PRELOAD_IMAGES {
        assets
            .handles
            .push(asset_server.load::<Image>(*path).untyped());
    }
    assets
        .handles
        .push(asset_server.load::<Gltf>(PRELOAD_MODEL).untyped());
}

/// Draws the loading spinner and advances to the title once every asset
/// has settled. A failed load counts as settled, so a missing file yields
/// a slightly poorer scene rather than a screen that spins forever.
pub fn loading_screen(
    mut contexts: EguiContexts,
    assets: Res<GameAssets>,
    asset_server: Res<AssetServer>,
    strings: Res<aeon_sim::TextDb>,
    mut next: ResMut<NextState<Screen>>,
) {
    let settled = assets.handles.iter().all(|handle| {
        asset_server.is_loaded_with_dependencies(handle.id())
            || matches!(asset_server.load_state(handle.id()), LoadState::Failed(_))
    });
    if settled {
        next.set(Screen::Title);
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "loading".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::CentralPanel::default().show(&mut viewport, |ui| {
        let height = ui.available_height();
        ui.vertical_centered(|ui| {
            ui.add_space(height * 0.42);
            ui.add(egui::Spinner::new().size(48.0));
            ui.add_space(16.0);
            ui.label(strings.text("ui.loading"));
        });
    });
}
