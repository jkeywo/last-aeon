//! The title screen: the game's front door.
//!
//! The client boots to a title rather than into a campaign; nothing
//! simulation-shaped exists until the player steps through. New Game
//! starts the authored scenario on a fresh seed, Continue (native only)
//! restores the autosave, and the spectator tickbox starts either with
//! no player house — every court acting on its own, the interface a
//! window to watch through.
//!
//! Button clicks record an intent; an exclusive system launches it. The
//! split keeps the drawing code free of world surgery, and the surgery
//! free of egui.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::sim_driver;

/// Which screen the client is showing.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Screen {
    /// The front door.
    #[default]
    Title,
    /// A campaign, running.
    Playing,
}

/// What the title screen has been told, but not yet acted on.
#[derive(Resource, Default)]
pub struct TitleState {
    /// Start without a player house and watch.
    pub spectator: bool,
    /// A click waiting for the launch system.
    pub pending: Option<TitleAction>,
    /// The autosave, loaded and content-checked when the title appears.
    /// Always `None` on the web build, which offers no Continue.
    pub autosave: Option<aeon_sim::CampaignSnapshot>,
}

/// A choice made on the title screen.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TitleAction {
    /// Start the authored scenario afresh.
    NewGame,
    /// Restore the autosaved campaign. Never constructed on the web
    /// build, which offers no Continue.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Continue,
}

/// Draws the title screen and records what the player picks.
pub fn draw_title(
    mut contexts: EguiContexts,
    mut title: ResMut<TitleState>,
    strings: Res<aeon_sim::TextDb>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "title".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::CentralPanel::default().show(&mut viewport, |ui| {
        let height = ui.available_height();
        ui.vertical_centered(|ui| {
            ui.add_space(height * 0.28);
            ui.heading(strings.text("ui.window.title"));
            ui.add_space(24.0);

            let new_game = ui.add_sized(
                [220.0, 32.0],
                egui::Button::new(strings.text("ui.title.new-game")),
            );
            if new_game.clicked() {
                title.pending = Some(TitleAction::NewGame);
            }

            // Continue is a native affordance: the web build saves
            // nothing, so it honestly offers nothing.
            #[cfg(not(target_arch = "wasm32"))]
            {
                let enabled = title.autosave.is_some();
                let resume = ui.add_enabled(
                    enabled,
                    egui::Button::new(strings.text("ui.title.continue"))
                        .min_size(egui::vec2(220.0, 32.0)),
                );
                if resume.clicked() {
                    title.pending = Some(TitleAction::Continue);
                }
            }

            ui.add_space(16.0);
            ui.checkbox(
                &mut title.spectator,
                strings.text("ui.title.spectator").to_owned(),
            );
            ui.weak(strings.text("ui.title.spectator-note"));
        });
    });
}

/// Loads and vets the autosave when the title screen appears.
///
/// A missing, unreadable, corrupt, or content-mismatched file simply
/// leaves Continue disabled: the failure happens here, quietly, rather
/// than after the click.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_autosave(mut title: ResMut<TitleState>) {
    let embedded_hash = crate::content::load_embedded().content_hash;
    title.autosave = std::fs::read_to_string(sim_driver::AUTOSAVE_PATH)
        .ok()
        .and_then(|document| aeon_sim::persistence::snapshot_from_ron(&document).ok())
        .filter(|snapshot| snapshot.state.content_hash == Some(embedded_hash));
}

/// Acts on a recorded title choice: starts or restores the campaign,
/// then enters play. Exclusive, because campaigns are world surgery.
pub fn launch(world: &mut World) {
    let Some(action) = world.resource_mut::<TitleState>().pending.take() else {
        return;
    };
    let spectator = world.resource::<TitleState>().spectator;
    match action {
        TitleAction::NewGame => {
            sim_driver::begin_campaign(world, spectator);
        }
        TitleAction::Continue => {
            let Some(snapshot) = world.resource_mut::<TitleState>().autosave.take() else {
                return;
            };
            // The same verify-and-restore path the headless host takes:
            // hash-verify the snapshot, then the content-bound half
            // before the content-free half.
            let state = match aeon_sim::snapshot::verify_snapshot(snapshot) {
                Ok(state) => state,
                Err(err) => {
                    warn!("autosave failed verification: {err}");
                    return;
                }
            };
            let content = crate::content::load_embedded();
            aeon_sim::snapshot::restore_content_state(world, &state, content);
            aeon_sim::snapshot::restore_state(world, state);
        }
    }
    world
        .resource_mut::<NextState<Screen>>()
        .set(Screen::Playing);
}
