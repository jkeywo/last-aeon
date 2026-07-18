//! Read-only 2D information panels.
//!
//! A top bar (campaign, date, time control, view breadcrumb), a left
//! inspector for the current selection, and a right list of bodies or the
//! focused body's provinces. Panels display simulation state; they never
//! mutate it directly.

use aeon_data::model::BodyKind;
use aeon_sim::map::{BodyRecord, DisplayName, GeoPosition, ProvinceRecord};
use aeon_sim::{CampaignClock, state::CampaignMeta};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::sim_driver::{SPEED_STEPS, TimeControl};
use crate::view::{MapView, Selection, ViewState};

fn kind_label(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::Planet => "Planet",
        BodyKind::Moon => "Moon",
        BodyKind::Starbase => "Starbase",
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_panels(
    mut contexts: EguiContexts,
    clock: Option<Res<CampaignClock>>,
    meta: Option<Res<CampaignMeta>>,
    mut control: ResMut<TimeControl>,
    mut view: ResMut<ViewState>,
    bodies: Query<(&BodyRecord, &DisplayName)>,
    provinces: Query<(&ProvinceRecord, &DisplayName, &GeoPosition)>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let (Some(clock), Some(meta)) = (clock, meta) else {
        return;
    };

    // egui 0.35 shows panels inside a Ui; build the root viewport Ui.
    let mut viewport = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("top-bar").show(&mut viewport, |ui| {
        ui.horizontal(|ui| {
            ui.strong(&meta.name);
            ui.separator();
            ui.monospace(clock.date.to_string());
            ui.separator();

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
                    let name = bodies
                        .iter()
                        .find(|(record, _)| record.id == id)
                        .map(|(_, name)| name.0.as_str())
                        .unwrap_or("Unknown");
                    ui.label(name);
                }
            }
        });
    });

    egui::Panel::left("inspector")
        .default_size(240.0)
        .show(&mut viewport, |ui| {
            ui.heading("Inspector");
            ui.separator();
            match view.selected {
                None => {
                    ui.label("Click a body or province to inspect it.");
                }
                Some(Selection::Body(id)) => {
                    if let Some((record, name)) = bodies.iter().find(|(record, _)| record.id == id)
                    {
                        ui.strong(&name.0);
                        ui.label(kind_label(record.kind));
                        ui.separator();
                        egui::Grid::new("body-facts").show(ui, |ui| {
                            ui.label("Stable ID");
                            ui.monospace(record.id.to_string());
                            ui.end_row();
                            ui.label("Content key");
                            ui.monospace(record.key.as_str());
                            ui.end_row();
                            ui.label("Radius");
                            ui.label(format!("{} km", record.radius_km));
                            ui.end_row();
                            if record.orbit_days > 0 {
                                ui.label("Orbit");
                                ui.label(format!(
                                    "{} Mm, {} days",
                                    record.orbit_radius_mm, record.orbit_days
                                ));
                                ui.end_row();
                            }
                            ui.label("Provinces");
                            ui.label(
                                provinces
                                    .iter()
                                    .filter(|(p, _, _)| p.body == id)
                                    .count()
                                    .to_string(),
                            );
                            ui.end_row();
                        });
                        if ui.button("Open strategic view").clicked() {
                            view.view = MapView::Body(id);
                        }
                    }
                }
                Some(Selection::Province(id)) => {
                    if let Some((record, name, geo)) =
                        provinces.iter().find(|(record, _, _)| record.id == id)
                    {
                        ui.strong(&name.0);
                        let body_name = bodies
                            .iter()
                            .find(|(body, _)| body.id == record.body)
                            .map(|(_, name)| name.0.as_str())
                            .unwrap_or("Unknown");
                        ui.label(format!("Province of {body_name}"));
                        ui.separator();
                        egui::Grid::new("province-facts").show(ui, |ui| {
                            ui.label("Stable ID");
                            ui.monospace(record.id.to_string());
                            ui.end_row();
                            ui.label("Content key");
                            ui.monospace(record.key.as_str());
                            ui.end_row();
                            ui.label("Latitude");
                            ui.label(format!("{:.2}\u{00b0}", geo.latitude_mdeg as f32 / 1000.0));
                            ui.end_row();
                            ui.label("Longitude");
                            ui.label(format!("{:.2}\u{00b0}", geo.longitude_mdeg as f32 / 1000.0));
                            ui.end_row();
                        });
                    }
                }
            }
        });

    egui::Panel::right("listing")
        .default_size(220.0)
        .show(&mut viewport, |ui| match view.view {
            MapView::System => {
                ui.heading("Bodies");
                ui.separator();
                let mut sorted: Vec<_> = bodies.iter().collect();
                sorted.sort_by_key(|(record, _)| record.id);
                for (record, name) in sorted {
                    let selected = view.selected == Some(Selection::Body(record.id));
                    if ui.selectable_label(selected, &name.0).clicked() {
                        view.selected = Some(Selection::Body(record.id));
                    }
                }
            }
            MapView::Body(body_id) => {
                ui.heading("Provinces");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut sorted: Vec<_> = provinces
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
