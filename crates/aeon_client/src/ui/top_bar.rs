//! The top bar: who you are, when it is, how fast time runs, where you
//! are looking, and how the map is coloured.
//!
//! Everything here is on screen in every view, so it is deliberately the
//! only place that spends permanent width.

use aeon_core::calendar::GameDate;
use aeon_data::ContentSet;
use aeon_sim::state::CampaignMeta;
use aeon_sim::{CampaignOver, CharacterId, OrgId, TextDb};
use bevy_egui::egui;

use crate::sim_driver::{SPEED_STEPS, TimeControl};
use crate::ui::dock::{DockSide, DockState, PanelKind};
use crate::ui::icons::{draw_mode_bar, draw_panel_icon};
use crate::ui::lookup::Lookup;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{draw_identity, resource_readout};
use crate::view::{MapMode, MapView, SearchState, ViewState};

/// Draws the top bar into the shell's viewport.
#[allow(clippy::too_many_arguments)]
pub fn draw_top_bar(
    viewport: &mut egui::Ui,
    lookup: &Lookup,
    _content: &ContentSet,
    theme: &UiTheme,
    strings: &TextDb,
    meta: &CampaignMeta,
    date: GameDate,
    over: Option<&CampaignOver>,
    player_org: Option<OrgId>,
    player_head: Option<CharacterId>,
    control: &mut TimeControl,
    view: &mut ViewState,
    mode: &mut MapMode,
    dock: &mut DockState,
    search: &mut SearchState,
) {
    egui::Panel::top("top-bar").show(viewport, |ui| {
        ui.horizontal(|ui| {
            // Who you are, first and always.
            if let Some(hit) = draw_identity(ui, theme, lookup, player_org, player_head) {
                view.selected = Some(hit);
            }
            ui.separator();
            ui.strong(&meta.name);
            ui.separator();
            ui.monospace(date.to_string());
            ui.separator();

            if let Some((_, Some(resources))) = player_org.and_then(|org| lookup.orgs.get(&org)) {
                resource_readout(ui, strings, resources);
                ui.separator();
            }

            let pause_label = strings.text(if control.paused {
                "ui.top-bar.resume"
            } else {
                "ui.top-bar.pause"
            });
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
                    ui.label(strings.text("ui.top-bar.local-system"));
                }
                MapView::Body(id) => {
                    if ui
                        .button(strings.text("ui.top-bar.back-to-system"))
                        .clicked()
                    {
                        view.view = MapView::System;
                    }
                    ui.label(lookup.body_name(id));
                    ui.separator();
                    if let Some(picked) = draw_mode_bar(ui, theme, strings, *mode) {
                        *mode = picked;
                    }
                }
            }

            if let Some(over) = &over {
                ui.separator();
                ui.colored_label(
                    egui::Color32::from(theme.semantics.urgent),
                    strings.format("ui.top-bar.campaign-over", &[("reason", &over.reason)]),
                );
            }

            // Search box, pushed to the right end of the bar.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut search.query)
                        .hint_text(strings.text("ui.top-bar.search-hint"))
                        .desired_width(150.0),
                );
                ui.label("\u{1f50d}");
                ui.separator();
                draw_panel_toggles(ui, theme, strings, dock);
            });
        });
    });
}

/// The panel toggles, at the right end of the top bar.
///
/// Left-click docks a panel to the left, right-click to the right, and
/// clicking the side it is already on puts it away. The right-click
/// affordance is spelled out in the tooltip, because a control whose
/// second function is invisible has, for most players, only one.
fn draw_panel_toggles(ui: &mut egui::Ui, theme: &UiTheme, strings: &TextDb, dock: &mut DockState) {
    let button = f32::from(theme.components.icon_button);
    for kind in PanelKind::ALL {
        let side = dock.side_of(*kind);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(button, button), egui::Sense::click());
        let visuals = ui.style().interact_selectable(&response, side.is_some());
        if side.is_some() || response.hovered() {
            ui.painter()
                .rect_filled(rect, theme.shape.radius_small as f32, visuals.bg_fill);
        }
        draw_panel_icon(ui.painter(), theme, rect, *kind, visuals.fg_stroke.color);

        let where_now = match side {
            Some(side) => strings.format(
                "ui.panel-toggle.showing",
                &[("side", strings.text(side.label_key()))],
            ),
            None => strings.text("ui.panel-toggle.hidden").to_owned(),
        };
        let response = response.on_hover_text(format!(
            "{}\n{}\n\n{}\n{}",
            strings.text(kind.title_key()),
            strings.text(kind.description_key()),
            where_now,
            strings.text("ui.panel-toggle.how"),
        ));
        if response.clicked() {
            dock.toggle(*kind, DockSide::Left);
        } else if response.secondary_clicked() {
            dock.toggle(*kind, DockSide::Right);
        }
    }
}
