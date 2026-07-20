//! What the map's colours mean.
//!
//! Was an always-visible legend, then a summonable overlay, and is now a
//! panel — because a player reading an unfamiliar map mode wants it beside
//! the map for as long as they are reading, not for as long as they hold a
//! button.

use aeon_sim::TextDb;
use bevy_egui::egui;

use crate::map_modes::MapReadout;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::swatch;
use crate::view::MapMode;

/// Draws the legend for the active map mode.
pub fn draw_ledger_panel(
    ui: &mut egui::Ui,
    theme: &UiTheme,
    strings: &TextDb,
    readout: &MapReadout,
    mode: MapMode,
) {
    ui.strong(strings.text(&mode.label_key()))
        .on_hover_text(strings.text(&mode.description_key()));
    if readout.legend.is_empty() {
        ui.weak(strings.text("ui.ledger.no-map"));
        return;
    }
    egui::ScrollArea::vertical()
        .id_salt("ledger-scroll")
        .show(ui, |ui| {
            for (label, colour) in &readout.legend {
                ui.horizontal(|ui| {
                    swatch(
                        ui,
                        theme,
                        egui::Color32::from_rgb(colour[0], colour[1], colour[2]),
                    );
                    ui.label(label);
                });
            }
            if !mode.is_political() {
                ui.weak(strings.text("ui.ledger.values-on-map"));
            }
        });
}
