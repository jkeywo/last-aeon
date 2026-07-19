//! Read-only 2D information panels plus presence and forces controls.
//!
//! A top bar (campaign, date, resources, time control, view breadcrumb),
//! a left inspector for the current selection (body, province, house, or
//! character, including location and travel), and a right listing panel
//! (bodies, houses, and the player's forces). Mutations travel through
//! the UI command queue into the authoritative command pipeline.

use aeon_data::model::ShipClass;
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::state::{CampaignMeta, ContentDb};
use aeon_sim::{
    CampaignClock, CampaignOver, CharacterId, OrgId, PlayerCommand, PlayerHouse, PoliticsIndex,
    ProvinceId,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::jobs_ui::UiCommandQueue;
use crate::sim_driver::{SPEED_STEPS, TimeControl};
use crate::ui::data::{JobUi, MapUi, PanelData};
use crate::ui::icons::draw_mode_bar;
use crate::ui::inspector::{Inspector, draw_inspector};
use crate::ui::lookup::Lookup;
use crate::ui::widgets::{draw_identity, resource_readout};
use crate::view::{MapView, SearchState, Selection, ViewState};

/// One global-search result.
enum SearchHit {
    Character(CharacterId),
    Org(OrgId),
    /// A province and the body it sits on (so the view can focus it).
    Province(ProvinceId, aeon_sim::BodyId),
}
#[allow(clippy::too_many_arguments)]
pub fn draw_panels(
    mut contexts: EguiContexts,
    clock: Option<Res<CampaignClock>>,
    meta: Option<Res<CampaignMeta>>,
    content: Option<Res<ContentDb>>,
    politics: Option<Res<PoliticsIndex>>,
    player: Option<Res<PlayerHouse>>,
    over: Option<Res<CampaignOver>>,
    mut control: ResMut<TimeControl>,
    mut view: ResMut<ViewState>,
    mut queue: ResMut<UiCommandQueue>,
    mut search: ResMut<SearchState>,
    map_ui: MapUi,
    job_ui: JobUi,
    log: Option<Res<aeon_sim::MessageLog>>,
    mut filter: ResMut<crate::jobs_ui::LogFilter>,
    data: PanelData,
) {
    let JobUi {
        mut form,
        mut picker,
    } = job_ui;
    let MapUi {
        mut mode,
        mut ledger,
    } = map_ui;
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(meta), Some(content), Some(politics)) = (clock, meta, content, politics)
    else {
        return;
    };
    let date = clock.date;
    let theme = &data.theme;
    let player_org = player.as_ref().and_then(|p| p.0);

    // Every name, label and hover summary the panels need, built once.
    let lookup = Lookup::build(&data, &content.0, date);
    let player_head: Option<CharacterId> =
        player_org.and_then(|org| lookup.orgs.get(&org).and_then(|(r, _)| r.head));

    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("top-bar").show(&mut viewport, |ui| {
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
                    let name = data
                        .bodies
                        .iter()
                        .find(|(record, _)| record.id == id)
                        .map(|(_, name)| name.0.as_str())
                        .unwrap_or("Unknown");
                    ui.label(name);
                    ui.separator();
                    if let Some(picked) = draw_mode_bar(ui, &data.theme, *mode) {
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

    // Search results, floating below the top bar while the query is set.
    let query = search.query.trim().to_lowercase();
    if !query.is_empty() {
        let mut hits: Vec<(String, SearchHit)> = Vec::new();
        for (id, (record, ..)) in &lookup.chars {
            if record.name.to_lowercase().contains(&query) {
                hits.push((record.name.clone(), SearchHit::Character(*id)));
            }
        }
        for (id, (record, _)) in &lookup.orgs {
            let name = lookup.org_name(*id);
            if name.to_lowercase().contains(&query) {
                let _ = record;
                hits.push((name, SearchHit::Org(*id)));
            }
        }
        for (record, name, _) in &data.provinces {
            if name.0.to_lowercase().contains(&query) {
                hits.push((name.0.clone(), SearchHit::Province(record.id, record.body)));
            }
        }
        hits.sort_by(|a, b| a.0.cmp(&b.0));
        hits.truncate(30);

        egui::Area::new("search-results".into())
            .fixed_pos(egui::pos2(ctx.viewport_rect().width() - 260.0, 34.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(240.0);
                    if hits.is_empty() {
                        ui.label("No matches.");
                    }
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for (label, hit) in &hits {
                                let tag = match hit {
                                    SearchHit::Character(_) => "character",
                                    SearchHit::Org(_) => "house",
                                    SearchHit::Province(..) => "province",
                                };
                                if ui
                                    .selectable_label(false, format!("{label}  ({tag})"))
                                    .clicked()
                                {
                                    match hit {
                                        SearchHit::Character(id) => {
                                            view.selected = Some(Selection::Character(*id));
                                        }
                                        SearchHit::Org(id) => {
                                            view.selected = Some(Selection::Org(*id));
                                        }
                                        SearchHit::Province(id, body) => {
                                            view.view = MapView::Body(*body);
                                            view.selected = Some(Selection::Province(*id));
                                        }
                                    }
                                    search.query.clear();
                                }
                            }
                        });
                });
            });
    }

    // ------------------------------------------------------------------
    // Situation strip: what needs attention, and a way straight to it.
    // ------------------------------------------------------------------
    if matches!(view.view, MapView::Body(_)) && !data.readout.situation.is_empty() {
        egui::Area::new("situation-strip".into())
            .fixed_pos(egui::pos2(276.0, 34.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong("Needs attention:")
                            .on_hover_text("Threats to your holdings. Click one to go to it.");
                        for item in &data.readout.situation {
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
    if ledger.open && matches!(view.view, MapView::Body(_)) && !data.readout.legend.is_empty() {
        let bottom = ctx.viewport_rect().height() - 180.0;
        egui::Area::new("map-legend".into())
            .fixed_pos(egui::pos2(276.0, bottom))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.strong(mode.label()).on_hover_text(mode.description());
                    for (label, colour) in &data.readout.legend {
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

    // The bottom bar is declared before the side panels so they stop
    // above it rather than running underneath.
    if let Some(log) = &log {
        crate::jobs_ui::draw_bottom_bar(
            &mut viewport,
            &content,
            &politics,
            date,
            player_org,
            log,
            &mut filter,
            &mut view,
            &mut queue,
            &data.active_jobs,
            &data.character_records,
            &data.province_records,
        );
    }

    // Everything the inspector reads, gathered once rather than by each
    // arm for itself.
    let inspector = Inspector {
        lookup: &lookup,
        data: &data,
        content: &content.0,
        politics: &politics,
        date,
        mode: *mode,
        player_org,
        player_head,
    };

    egui::Panel::left("inspector")
        .default_size(260.0)
        .show(&mut viewport, |ui| {
            ui.heading("Inspector");
            ui.separator();
            // A forecast and its leader comparison can outgrow the panel,
            // so the whole inspector scrolls.
            egui::ScrollArea::vertical()
                .id_salt("inspector-scroll")
                .show(ui, |ui| {
                    draw_inspector(
                        ui,
                        &inspector,
                        &mut view,
                        &mut form,
                        &mut queue,
                        &mut picker,
                    );
                });
        });

    egui::Panel::right("listing")
        .default_size(230.0)
        .show(&mut viewport, |ui| match view.view {
            MapView::System => {
                ui.heading("Bodies");
                ui.separator();
                let mut sorted: Vec<_> = data.bodies.iter().collect();
                sorted.sort_by_key(|(record, _)| record.id);
                for (record, name) in sorted {
                    let selected = view.selected == Some(Selection::Body(record.id));
                    if ui.selectable_label(selected, &name.0).clicked() {
                        view.selected = Some(Selection::Body(record.id));
                    }
                }

                ui.add_space(8.0);
                ui.heading("Houses");
                ui.separator();
                for (org_id, (record, _)) in &lookup.orgs {
                    let def = content.0.organisations.get(&record.key);
                    let label = def.map(|d| d.name.clone()).unwrap_or_default();
                    let selected = view.selected == Some(Selection::Org(*org_id));
                    if ui.selectable_label(selected, label).clicked() {
                        view.selected = Some(Selection::Org(*org_id));
                    }
                }

                ui.add_space(8.0);
                ui.heading("Forces");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("forces")
                    .show(ui, |ui| {
                        let mut ships: Vec<&ShipRecord> = data.ships.iter().collect();
                        ships.sort_by_key(|s| s.id);
                        for ship in ships {
                            let class = match ship.class {
                                ShipClass::Capital => "Capital",
                                ShipClass::Transport => "Transport",
                                ShipClass::Patrol => "Patrol",
                            };
                            let place = match ship.location {
                                ShipLocation::Docked(province) => lookup
                                    .province_names
                                    .get(&province)
                                    .map(|n| (*n).to_owned())
                                    .unwrap_or_default(),
                                ShipLocation::Transit { to, .. } => format!(
                                    "-> {}",
                                    lookup.province_names.get(&to).copied().unwrap_or("...")
                                ),
                            };
                            ui.horizontal(|ui| {
                                ui.label(format!("{} ({class}) — {place}", ship.name));
                            });
                            if Some(ship.owner) == player_org
                                && matches!(ship.location, ShipLocation::Docked(_))
                            {
                                egui::ComboBox::from_id_salt(("move-ship", ship.id))
                                    .selected_text("Move to...")
                                    .show_ui(ui, |ui| {
                                        let mut sorted: Vec<_> =
                                            lookup.province_names.iter().collect();
                                        sorted.sort_by_key(|(id, _)| **id);
                                        for (province, name) in sorted {
                                            if ui.selectable_label(false, *name).clicked() {
                                                queue.0.push(PlayerCommand::MoveShip {
                                                    ship: ship.id,
                                                    destination: *province,
                                                });
                                            }
                                        }
                                    });
                            }
                        }

                        let mut armies: Vec<&ArmyRecord> = data.armies.iter().collect();
                        armies.sort_by_key(|a| a.id);
                        for army in armies {
                            let place = lookup
                                .province_names
                                .get(&army.location)
                                .copied()
                                .unwrap_or("...");
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{} — {} men, {} supplies — {place}",
                                    army.name, army.manpower, army.supplies
                                ));
                                if Some(army.owner) == player_org {
                                    let defending = army.standing_order
                                        == aeon_sim::warfare::StandingOrder::DefendHoldings;
                                    let label = if defending { "Defending" } else { "Hold fast" };
                                    if ui
                                        .small_button(label)
                                        .on_hover_text(
                                            "Toggle the standing order: defending armies \
                                             march to answer threats against your holdings",
                                        )
                                        .clicked()
                                    {
                                        let order = if defending {
                                            aeon_sim::warfare::StandingOrder::HoldFast
                                        } else {
                                            aeon_sim::warfare::StandingOrder::DefendHoldings
                                        };
                                        queue.0.push(PlayerCommand::SetStandingOrder {
                                            army: army.id,
                                            order,
                                        });
                                    }
                                    if ui.small_button("Disband").clicked() {
                                        queue.0.push(PlayerCommand::DisbandArmy { army: army.id });
                                    }
                                }
                            });
                        }
                    });
            }
            MapView::Body(body_id) => {
                ui.heading("Provinces");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut sorted: Vec<_> = data
                        .provinces
                        .iter()
                        .filter(|(record, _, _)| record.body == body_id)
                        .collect();
                    sorted.sort_by_key(|(record, _, _)| record.id);
                    for (record, name, _) in sorted {
                        let selected = view.selected == Some(Selection::Province(record.id));
                        if ui.selectable_label(selected, &name.0).clicked() {
                            view.selected = Some(Selection::Province(record.id));
                        }
                    }
                });
            }
        });
}
