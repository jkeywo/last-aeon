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
use crate::ui::situations_panel::SituationUiState;
use crate::ui::theme::UiTheme;
use crate::view::{MapView, Selection, ViewState};

/// Draws the attention strip over the map.
pub fn draw_overlays(
    ctx: &egui::Context,
    theme: &UiTheme,
    strings: &TextDb,
    readout: &MapReadout,
    view: &mut ViewState,
    dock: &mut DockState,
    situations: &mut SituationUiState,
) {
    // ------------------------------------------------------------------
    // Attention strip: what needs attention, and a way straight to it.
    // ------------------------------------------------------------------
    if matches!(view.view, MapView::Body(_)) && !readout.attention.is_empty() {
        egui::Area::new("attention-strip".into())
            .fixed_pos(egui::pos2(
                f32::from(theme.components.strip_offset_x),
                f32::from(theme.components.strip_offset_y),
            ))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(strings.text("ui.situation.heading"))
                            .on_hover_text(strings.text("ui.situation.heading.hover"));
                        for item in &readout.attention {
                            let colour = if item.urgent {
                                egui::Color32::from(theme.semantics.urgent)
                            } else {
                                egui::Color32::from(theme.semantics.notable)
                            };
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new(&item.headline).color(colour),
                                ))
                                .on_hover_text(&item.detail)
                                .clicked()
                            {
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
