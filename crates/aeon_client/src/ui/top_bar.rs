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

use crate::preferences::SettingsUi;
use crate::sim_driver::{SPEED_STEPS, TimeControl};
use crate::ui::dock::{DockSide, DockState, PanelKind};
use crate::ui::icons::{draw_mode_bar, draw_panel_icon};
use crate::ui::layout::{LayoutMode, LayoutPlan, draw_top_band};
use crate::ui::lookup::Lookup;
use crate::ui::theme::UiTheme;
use crate::ui::widgets::{draw_identity, resource_readout};
use crate::view::{MapMode, MapView, SearchState, ViewState};

const TIME_CONTROL_RESPONSE: &str = "production-top-time-control";

#[cfg(test)]
pub(crate) fn recorded_time_control(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|data| data.get_temp(egui::Id::new(TIME_CONTROL_RESPONSE)))
}

pub(crate) fn draw_pause_control(ui: &mut egui::Ui, label: &str, control: &mut TimeControl) {
    let response = ui.add(egui::Button::new(label).min_size(egui::vec2(24.0, 24.0)));
    crate::ui::keyboard::capture_action(
        ui,
        crate::ui::keyboard::LogicalFocus::new("time-control"),
        "time",
        crate::ui::keyboard::FocusBand::TopChrome,
        &response,
    )
    .register();
    #[cfg(test)]
    crate::ui::rendered_state::record_response(ui, "time", &response);
    ui.ctx().data_mut(|data| {
        data.insert_temp(egui::Id::new(TIME_CONTROL_RESPONSE), response.rect);
    });
    if response.clicked() {
        control.paused = !control.paused;
    }
}

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
    settings: &mut SettingsUi,
    layout: LayoutPlan,
) -> f32 {
    let shown = egui::Panel::top("top-bar").show(viewport, |ui| {
        if layout.mode == LayoutMode::Compact {
            draw_compact_top_bar(
                ui,
                lookup,
                theme,
                strings,
                meta,
                date,
                over,
                player_org,
                player_head,
                control,
                view,
                mode,
                dock,
                search,
                settings,
            );
            return;
        }
        ui.horizontal_wrapped(|ui| {
            // Who you are, first and always.
            if let Some(hit) = draw_identity(ui, theme, lookup, player_org, player_head) {
                view.selected = Some(hit);
            }
            ui.separator();
            ui.add(egui::Label::new(egui::RichText::new(&meta.name).strong()).wrap());
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
            draw_pause_control(ui, pause_label, control);
            for (index, speed) in SPEED_STEPS.iter().enumerate() {
                let active = (control.days_per_second - speed).abs() < f32::EPSILON;
                let response = ui.selectable_label(active, format!("{}x", index + 1));
                crate::ui::keyboard::capture_action(
                    ui,
                    crate::ui::keyboard::LogicalFocus::new(format!("speed:{}", index + 1)),
                    "speed",
                    crate::ui::keyboard::FocusBand::TopChrome,
                    &response,
                )
                .register();
                if response.clicked() {
                    control.days_per_second = *speed;
                }
            }
            ui.separator();

            match view.view {
                MapView::System => {
                    ui.label(strings.text("ui.top-bar.local-system"));
                }
                MapView::Body(id) => {
                    let back = ui.button(strings.text("ui.top-bar.back-to-system"));
                    crate::ui::keyboard::capture_action(
                        ui,
                        crate::ui::keyboard::LogicalFocus::new("back-to-system"),
                        "back-to-system",
                        crate::ui::keyboard::FocusBand::TopChrome,
                        &back,
                    )
                    .register();
                    if back.clicked() {
                        view.view = MapView::System;
                    }
                    ui.add(egui::Label::new(lookup.body_name(id)).wrap());
                    ui.separator();
                    if let Some(picked) = draw_mode_bar(ui, theme, strings, *mode) {
                        *mode = picked;
                    }
                    ui.separator();
                    // Named for what pressing it gives you, not for what
                    // you are looking at now.
                    let other = view.projection.toggled();
                    let projection = ui
                        .button(strings.text(other.label_key()))
                        .on_hover_text(strings.text("ui.projection.hover"));
                    crate::ui::keyboard::capture_action(
                        ui,
                        crate::ui::keyboard::LogicalFocus::new("projection-toggle"),
                        "projection-toggle",
                        crate::ui::keyboard::FocusBand::TopChrome,
                        &projection,
                    )
                    .register();
                    if projection.clicked() {
                        view.projection = other;
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
                let settings_response = ui.button(strings.text("ui.preferences.open"));
                crate::ui::keyboard::capture_action(
                    ui,
                    crate::ui::keyboard::LogicalFocus::new("settings"),
                    "settings",
                    crate::ui::keyboard::FocusBand::TopChrome,
                    &settings_response,
                )
                .register();
                if settings_response.clicked() {
                    if settings.open {
                        settings.close(ui.ctx());
                    } else {
                        settings.open_from(crate::ui::keyboard::LogicalFocus::new("settings"));
                    }
                }
                let _search_response = ui.add_sized(
                    [150.0, 24.0],
                    egui::TextEdit::singleline(&mut search.query)
                        .hint_text(strings.text("ui.top-bar.search-hint"))
                        .desired_width(150.0),
                );
                crate::ui::keyboard::capture_action(
                    ui,
                    crate::ui::keyboard::LogicalFocus::new("search"),
                    "search",
                    crate::ui::keyboard::FocusBand::TopChrome,
                    &_search_response,
                )
                .register();
                #[cfg(test)]
                crate::ui::rendered_state::record_response(ui, "search", &_search_response);
                ui.label("\u{1f50d}");
                ui.separator();
                draw_panel_toggles(ui, theme, strings, dock);
            });
        });
    });
    shown.response.rect.height()
}

/// Compact mode keeps every verb, but separates identity/time from navigation
/// and tools. Each band wraps independently, so long campaign or body names
/// grow the bar vertically rather than clipping a command off-screen.
#[allow(clippy::too_many_arguments)]
fn draw_compact_top_bar(
    ui: &mut egui::Ui,
    lookup: &Lookup,
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
    settings: &mut SettingsUi,
) {
    draw_top_band(ui, |ui| {
        if let Some(hit) = draw_identity(ui, theme, lookup, player_org, player_head) {
            view.selected = Some(hit);
        }
        ui.separator();
        ui.add(egui::Label::new(egui::RichText::new(&meta.name).strong()).wrap());
        ui.separator();
        ui.monospace(date.to_string());
        if let Some((_, Some(resources))) = player_org.and_then(|org| lookup.orgs.get(&org)) {
            ui.separator();
            resource_readout(ui, strings, resources);
        }
        if let Some(over) = over {
            ui.separator();
            ui.colored_label(
                egui::Color32::from(theme.semantics.urgent),
                strings.format("ui.top-bar.campaign-over", &[("reason", &over.reason)]),
            );
        }
    });
    ui.separator();
    draw_top_band(ui, |ui| {
        let pause_label = strings.text(if control.paused {
            "ui.top-bar.resume"
        } else {
            "ui.top-bar.pause"
        });
        draw_pause_control(ui, pause_label, control);
        for (index, speed) in SPEED_STEPS.iter().enumerate() {
            let active = (control.days_per_second - speed).abs() < f32::EPSILON;
            let response = ui.selectable_label(active, format!("{}x", index + 1));
            crate::ui::keyboard::capture_action(
                ui,
                crate::ui::keyboard::LogicalFocus::new(format!("speed:{}", index + 1)),
                "speed",
                crate::ui::keyboard::FocusBand::TopChrome,
                &response,
            )
            .register();
            if response.clicked() {
                control.days_per_second = *speed;
            }
        }
        ui.separator();
        match view.view {
            MapView::System => {
                ui.label(strings.text("ui.top-bar.local-system"));
            }
            MapView::Body(id) => {
                let back = ui.button(strings.text("ui.top-bar.back-to-system"));
                crate::ui::keyboard::capture_action(
                    ui,
                    crate::ui::keyboard::LogicalFocus::new("back-to-system"),
                    "back-to-system",
                    crate::ui::keyboard::FocusBand::TopChrome,
                    &back,
                )
                .register();
                if back.clicked() {
                    view.view = MapView::System;
                }
                ui.add(egui::Label::new(lookup.body_name(id)).wrap());
                if let Some(picked) = draw_mode_bar(ui, theme, strings, *mode) {
                    *mode = picked;
                }
                let other = view.projection.toggled();
                let projection = ui
                    .button(strings.text(other.label_key()))
                    .on_hover_text(strings.text("ui.projection.hover"));
                crate::ui::keyboard::capture_action(
                    ui,
                    crate::ui::keyboard::LogicalFocus::new("projection-toggle"),
                    "projection-toggle",
                    crate::ui::keyboard::FocusBand::TopChrome,
                    &projection,
                )
                .register();
                if projection.clicked() {
                    view.projection = other;
                }
            }
        }
        ui.separator();
        draw_panel_toggles(ui, theme, strings, dock);
        ui.label("\u{1f50d}");
        let _search_response = ui.add_sized(
            [110.0, 24.0],
            egui::TextEdit::singleline(&mut search.query)
                .hint_text(strings.text("ui.top-bar.search-hint"))
                .desired_width(110.0),
        );
        crate::ui::keyboard::capture_action(
            ui,
            crate::ui::keyboard::LogicalFocus::new("search"),
            "search",
            crate::ui::keyboard::FocusBand::TopChrome,
            &_search_response,
        )
        .register();
        #[cfg(test)]
        crate::ui::rendered_state::record_response(ui, "search", &_search_response);
        let settings_response = ui.button(strings.text("ui.preferences.open"));
        crate::ui::keyboard::capture_action(
            ui,
            crate::ui::keyboard::LogicalFocus::new("settings"),
            "settings",
            crate::ui::keyboard::FocusBand::TopChrome,
            &settings_response,
        )
        .register();
        if settings_response.clicked() {
            if settings.open {
                settings.close(ui.ctx());
            } else {
                settings.open_from(crate::ui::keyboard::LogicalFocus::new("settings"));
            }
        }
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
    let mut responses = Vec::new();
    for kind in PanelKind::ALL {
        let side = dock.side_of(*kind);
        // A stable id keyed by the panel kind, so egui's sizing and render
        // passes agree on it even as widgets to the left change width.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(button, button), egui::Sense::hover());
        let response = ui.interact(
            rect,
            ui.id().with(("panel-toggle", *kind)),
            egui::Sense::click(),
        );
        let visuals = ui.style().interact_selectable(&response, side.is_some());
        if side.is_some() || response.hovered() || response.has_focus() {
            ui.painter()
                .rect_filled(rect, theme.shape.radius_small as f32, visuals.bg_fill);
        }
        draw_panel_icon(ui.painter(), theme, rect, *kind, visuals.fg_stroke.color);
        crate::ui::keyboard::capture_action(
            ui,
            crate::ui::keyboard::LogicalFocus::new(format!("panel-toggle:{kind:?}")),
            "panel-toggle",
            crate::ui::keyboard::FocusBand::TopChrome,
            &response,
        )
        .register();

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
        responses.push(response);
    }
    crate::ui::keyboard::roving_group(ui, &responses);
}
