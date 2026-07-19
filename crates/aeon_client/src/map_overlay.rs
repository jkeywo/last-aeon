//! egui overlay for the body view: province name labels and force badges.
//!
//! Projects each front-facing province centroid to the screen and draws
//! its name, plus a small badge counting armies and docked ships standing
//! there. Purely presentational; reads simulation state, never writes it.

use std::collections::BTreeMap;

use aeon_sim::ProvinceId;
use aeon_sim::forces::{ArmyRecord, ShipLocation, ShipRecord};
use aeon_sim::map::{DisplayName, GeoPosition, ProvinceRecord};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::map_modes::MapReadout;
use crate::scene::GLOBE_RADIUS;
use crate::ui::theme::UiTheme;
use crate::view::{MapView, Selection, ViewState, geo_to_unit};

/// Draws province name labels and force badges over the focused globe.
#[allow(clippy::too_many_arguments)]
pub fn draw_map_overlay(
    mut contexts: EguiContexts,
    view: Res<ViewState>,
    readout: Res<MapReadout>,
    theme: Res<UiTheme>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    provinces: Query<(&ProvinceRecord, &DisplayName, &GeoPosition)>,
    armies: Query<&ArmyRecord>,
    ships: Query<&ShipRecord>,
) {
    let MapView::Body(body) = view.view else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Some((camera, camera_transform)) = cameras.iter().next() else {
        return;
    };
    let camera_pos = camera_transform.translation();

    // Forces standing at each province on this body.
    let mut army_count: BTreeMap<ProvinceId, u32> = BTreeMap::new();
    for army in &armies {
        *army_count.entry(army.location).or_default() += 1;
    }
    let mut ship_count: BTreeMap<ProvinceId, u32> = BTreeMap::new();
    for ship in &ships {
        if let ShipLocation::Docked(province) = ship.location {
            *ship_count.entry(province).or_default() += 1;
        }
    }

    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("map_overlay"),
    ));

    for (record, name, geo) in &provinces {
        if record.body != body {
            continue;
        }
        let dir = geo_to_unit(geo.latitude_mdeg, geo.longitude_mdeg);
        let world = dir * GLOBE_RADIUS;
        // Cull provinces on the far side of the globe.
        if (camera_pos - world).dot(dir) <= 0.0 {
            continue;
        }
        let Ok(screen) = camera.world_to_viewport(camera_transform, world) else {
            continue;
        };
        let pos = egui::pos2(screen.x, screen.y);

        let selected = view.selected == Some(Selection::Province(record.id));
        let entry = readout.provinces.get(&record.id);
        let mut label = name.0.clone();
        // The active mode's value is printed on the map, so the reading
        // never depends on telling colours apart.
        if let Some(value) = entry.and_then(|entry| entry.value.as_deref()) {
            label.push_str(&format!("  {value}"));
        }
        if entry.is_some_and(|entry| entry.alert) {
            label.push_str("  !");
        }
        let armies_here = army_count.get(&record.id).copied().unwrap_or(0);
        let ships_here = ship_count.get(&record.id).copied().unwrap_or(0);
        if armies_here > 0 {
            label.push_str(&format!("  ⚔{armies_here}"));
        }
        if ships_here > 0 {
            label.push_str(&format!("  ⚓{ships_here}"));
        }

        let font = egui::FontId::proportional(if selected { 15.0 } else { 12.5 });
        let text_color = if selected {
            egui::Color32::from(theme.semantics.map_label_selected)
        } else if entry.is_some_and(|entry| entry.alert) {
            egui::Color32::from(theme.semantics.map_label_alert)
        } else {
            egui::Color32::from(theme.semantics.map_label)
        };
        // A dark drop-shadow first, for legibility over any region colour.
        painter.text(
            pos + egui::vec2(1.0, 1.0),
            egui::Align2::CENTER_CENTER,
            &label,
            font.clone(),
            egui::Color32::from(theme.semantics.map_label_shadow),
        );
        painter.text(pos, egui::Align2::CENTER_CENTER, &label, font, text_color);
    }
}
