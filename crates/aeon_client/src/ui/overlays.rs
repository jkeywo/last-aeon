//! The situation strip: what needs attention, and a way straight to it.
//!
//! Drawn over the map rather than beside it, and takes no layout space —
//! it is a standing offer of somewhere to go, not a panel.
//!
//! The colour ledger used to live here too. It is now a dockable panel,
//! because unlike this strip it is something a player may want to keep
//! beside the map while reading it.

use aeon_sim::TextDb;
use bevy_egui::egui;

use crate::map_modes::MapReadout;
use crate::ui::theme::UiTheme;
use crate::view::{MapView, Selection, ViewState};

/// Draws the situation strip over the map.
pub fn draw_overlays(
    ctx: &egui::Context,
    theme: &UiTheme,
    strings: &TextDb,
    readout: &MapReadout,
    view: &mut ViewState,
) {
    // ------------------------------------------------------------------
    // Situation strip: what needs attention, and a way straight to it.
    // ------------------------------------------------------------------
    if matches!(view.view, MapView::Body(_)) && !readout.situation.is_empty() {
        egui::Area::new("situation-strip".into())
            .fixed_pos(egui::pos2(
                f32::from(theme.components.strip_offset_x),
                f32::from(theme.components.strip_offset_y),
            ))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(strings.text("ui.situation.heading"))
                            .on_hover_text(strings.text("ui.situation.heading.hover"));
                        for item in &readout.situation {
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
                                view.view = MapView::Body(item.body);
                                view.selected = Some(Selection::Province(item.province));
                            }
                        }
                    });
                });
            });
    }
}
