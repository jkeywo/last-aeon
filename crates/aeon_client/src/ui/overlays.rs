//! Things drawn over the map rather than beside it: the situation strip
//! that says what needs attention, and the summonable colour ledger.
//!
//! Neither takes layout space. The strip is a standing offer of somewhere
//! to go; the ledger appears only when asked for.

use bevy_egui::egui;

use crate::map_modes::MapReadout;
use crate::ui::theme::UiTheme;
use crate::view::{MapLedger, MapMode, MapView, Selection, ViewState};

/// Draws the situation strip and, if it is summoned, the colour ledger.
pub fn draw_overlays(
    ctx: &egui::Context,
    theme: &UiTheme,
    readout: &MapReadout,
    mode: MapMode,
    ledger: &MapLedger,
    view: &mut ViewState,
) {
    // ------------------------------------------------------------------
    // Situation strip: what needs attention, and a way straight to it.
    // ------------------------------------------------------------------
    if matches!(view.view, MapView::Body(_)) && !readout.situation.is_empty() {
        egui::Area::new("situation-strip".into())
            .fixed_pos(egui::pos2(276.0, 34.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong("Needs attention:")
                            .on_hover_text("Threats to your holdings. Click one to go to it.");
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

    // ------------------------------------------------------------------
    // Legend for the active map mode.
    // ------------------------------------------------------------------
    if ledger.open && matches!(view.view, MapView::Body(_)) && !readout.legend.is_empty() {
        let bottom = ctx.viewport_rect().height() - 180.0;
        egui::Area::new("map-legend".into())
            .fixed_pos(egui::pos2(276.0, bottom))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.strong(mode.label()).on_hover_text(mode.description());
                    for (label, colour) in &readout.legend {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                rect,
                                2.0,
                                egui::Color32::from_rgb(colour[0], colour[1], colour[2]),
                            );
                            ui.label(label);
                        });
                    }
                    if !mode.is_political() {
                        ui.weak("Values are printed on the map.");
                    }
                });
            });
    }
}
