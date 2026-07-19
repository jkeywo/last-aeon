//! What the map's colours mean.
//!
//! Was an always-visible legend, then a summonable overlay, and is now a
//! panel — because a player reading an unfamiliar map mode wants it beside
//! the map for as long as they are reading, not for as long as they hold a
//! button.

use bevy_egui::egui;

use crate::map_modes::MapReadout;
use crate::view::MapMode;

/// Draws the legend for the active map mode.
pub fn draw_ledger_panel(ui: &mut egui::Ui, readout: &MapReadout, mode: MapMode) {
    ui.strong(mode.label()).on_hover_text(mode.description());
    if readout.legend.is_empty() {
        ui.weak("Open a body's map to see what its colours mean.");
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("ledger-scroll")
        .show(ui, |ui| {
            for (label, colour) in &readout.legend {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
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
}
