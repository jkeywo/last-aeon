//! The top bar: who you are, when it is, how fast time runs, where you
//! are looking, and how the map is coloured.
//!
//! Everything here is on screen in every view, so it is deliberately the
//! only place that spends permanent width.

use aeon_core::calendar::GameDate;
use aeon_data::ContentSet;
use aeon_sim::state::CampaignMeta;
use aeon_sim::{CampaignOver, CharacterId, OrgId};
use bevy_egui::egui;

use crate::sim_driver::{SPEED_STEPS, TimeControl};
use crate::ui::icons::draw_mode_bar;
use crate::ui::lookup::Lookup;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{draw_identity, resource_readout};
use crate::view::{MapLedger, MapMode, MapView, SearchState, ViewState};

/// Draws the top bar into the shell's viewport.
#[allow(clippy::too_many_arguments)]
pub fn draw_top_bar(
    viewport: &mut egui::Ui,
    lookup: &Lookup,
    _content: &ContentSet,
    theme: &UiTheme,
    meta: &CampaignMeta,
    date: GameDate,
    over: Option<&CampaignOver>,
    player_org: Option<OrgId>,
    player_head: Option<CharacterId>,
    control: &mut TimeControl,
    view: &mut ViewState,
    mode: &mut MapMode,
    ledger: &mut MapLedger,
    search: &mut SearchState,
) {
    egui::Panel::top("top-bar").show(viewport, |ui| {
        ui.horizontal(|ui| {
            // Who you are, first and always.
            if let Some(hit) = draw_identity(ui, &lookup, player_org, player_head) {
                view.selected = Some(hit);
            }
            ui.separator();
            ui.strong(&meta.name);
            ui.separator();
            ui.monospace(date.to_string());
            ui.separator();

            if let Some((_, Some(resources))) = player_org.and_then(|org| lookup.orgs.get(&org)) {
                resource_readout(ui, resources);
                ui.separator();
            }

            let pause_label = if control.paused { "Resume" } else { "Pause" };
            if ui.button(pause_label).clicked() {
                control.paused = !control.paused;
            }
            for (index, speed) in SPEED_STEPS.iter().enumerate() {
                let active = (control.days_per_second - speed).abs() < f32::EPSILON;
                if ui
                    .selectable_label(active, format!("{}x", index + 1))
                    .clicked()
                {
                    control.days_per_second = *speed;
                }
            }
            ui.separator();

            match view.view {
                MapView::System => {
                    ui.label("Local System");
                }
                MapView::Body(id) => {
                    if ui.button("< System").clicked() {
                        view.view = MapView::System;
                    }
                    ui.label(lookup.body_name(id));
                    ui.separator();
                    if let Some(picked) = draw_mode_bar(ui, theme, *mode) {
                        *mode = picked;
                    }
                    ui.toggle_value(&mut ledger.open, "?")
                        .on_hover_text("Show what the map's colours mean.");
                }
            }

            if let Some(over) = &over {
                ui.separator();
                ui.colored_label(
                    egui::Color32::from(theme.semantics.urgent),
                    format!("CAMPAIGN OVER — {}", over.reason),
                );
            }

            // Search box, pushed to the right end of the bar.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut search.query)
                        .hint_text("Search…")
                        .desired_width(150.0),
                );
                ui.label("\u{1f50d}");
            });
        });
    });
}
