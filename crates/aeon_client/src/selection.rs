//! Picking and selection.
//!
//! Click selects a body or (on a globe) the nearest province to the hit
//! point; double-clicking a body opens its strategic view; Escape returns
//! to the system view.

use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;

use crate::scene::{GlobeVisual, SystemBodyVisual, nearest_province};
use crate::view::{MapView, Selection, ViewState};

/// Attaches pointer observers to every map visual once the scene exists.
pub fn attach_pickers(
    mut commands: Commands,
    new_bodies: Query<Entity, Added<SystemBodyVisual>>,
    new_globes: Query<Entity, Added<GlobeVisual>>,
) {
    for entity in &new_bodies {
        commands.entity(entity).observe(on_body_click);
    }
    for entity in &new_globes {
        commands.entity(entity).observe(on_globe_click);
    }
}

fn on_body_click(
    event: On<Pointer<Click>>,
    bodies: Query<&SystemBodyVisual>,
    mut view: ResMut<ViewState>,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    let Ok(visual) = bodies.get(event.entity) else {
        return;
    };
    view.selected = Some(Selection::Body(visual.body));
    if event.count >= 2 {
        view.view = MapView::Body(visual.body);
    }
}

fn on_globe_click(
    event: On<Pointer<Click>>,
    globes: Query<&GlobeVisual>,
    mut view: ResMut<ViewState>,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    let Ok(globe) = globes.get(event.entity) else {
        return;
    };
    // The globe sits at the origin with identity transform, so the world
    // hit point is already the surface direction.
    let Some(position) = event.hit.position else {
        return;
    };
    if let Some(province) = nearest_province(position.normalize_or_zero(), &globe.centroids) {
        view.selected = Some(Selection::Province(province));
    }
}

/// Escape backs out of a body view.
pub fn view_hotkeys(keys: Res<ButtonInput<KeyCode>>, mut view: ResMut<ViewState>) {
    if keys.just_pressed(KeyCode::Escape) && matches!(view.view, MapView::Body(_)) {
        view.view = MapView::System;
    }
}
