//! The attention strip: what needs attention, and a way straight to it.
//!
//! Drawn over the map rather than beside it, and takes no layout space —
//! it is a standing offer of somewhere to go, not a panel.
//!
//! The colour ledger used to live here too. It is now a dockable panel,
//! because unlike this strip it is something a player may want to keep
//! beside the map while reading it.

use aeon_sim::TextDb;
use bevy_egui::egui;

use crate::map_modes::{AttentionTarget, MapReadout};
use crate::ui::dock::{DockSide, DockState, PanelKind};
use crate::ui::layout::LayoutPlan;
use crate::ui::situations_panel::SituationUiState;
use crate::ui::theme::UiTheme;
use crate::view::{MapView, Selection, ViewState};

/// Draws the attention strip over the map.
#[allow(clippy::too_many_arguments)]
pub fn draw_overlays(
    ctx: &egui::Context,
    theme: &UiTheme,
    strings: &TextDb,
    readout: &MapReadout,
    view: &mut ViewState,
    dock: &mut DockState,
    situations: &mut SituationUiState,
    layout: LayoutPlan,
) {
    // ------------------------------------------------------------------
    // Attention strip: what needs attention, and a way straight to it.
    // ------------------------------------------------------------------
    if matches!(view.view, MapView::Body(_)) && !readout.attention.is_empty() {
        let viewport = ctx.viewport_rect();
        egui::Area::new("attention-strip".into())
            .fixed_pos(egui::pos2(
                viewport.left() + layout.left_width + 8.0,
                layout.overlay_top,
            ))
            .constrain_to(viewport)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(
                        (viewport.width() - layout.left_width - layout.right_width - 16.0)
                            .max(180.0),
                    );
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(strings.text("ui.situation.heading"))
                            .on_hover_text(strings.text("ui.situation.heading.hover"));
                        for item in &readout.attention {
                            let colour = if item.urgent {
                                egui::Color32::from(theme.semantics.urgent)
                            } else {
                                egui::Color32::from(theme.semantics.notable)
                            };
                            let response = ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(&item.headline).color(colour),
                                    )
                                    .min_size(egui::vec2(24.0, 24.0)),
                                )
                                .on_hover_text(&item.detail);
                            #[cfg(test)]
                            crate::ui::rendered_state::record_response(ui, "attention", &response);
                            if response.clicked() {
                                match &item.target {
                                    AttentionTarget::Province { province, body } => {
                                        view.view = MapView::Body(*body);
                                        view.selected = Some(Selection::Province(*province));
                                    }
                                    AttentionTarget::Situation(key) => {
                                        situations.focused = Some(key.clone());
                                        dock.dock(PanelKind::Situations, DockSide::Right);
                                    }
                                }
                            }
                        }
                    });
                });
            });
    }
}
