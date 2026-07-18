//! Picking and selection.
//!
//! Click selects a body or province; double-clicking a body opens its
//! strategic view; Escape returns to the system view.

use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;

use crate::scene::{ProvinceMarker, SystemBodyVisual};
use crate::view::{MapView, Selection, ViewState};

/// Attaches pointer observers to every map visual once the scene exists.
pub fn attach_pickers(
    mut commands: Commands,
    new_bodies: Query<Entity, Added<SystemBodyVisual>>,
    new_markers: Query<Entity, Added<ProvinceMarker>>,
) {
    for entity in &new_bodies {
        commands.entity(entity).observe(on_body_click);
    }
    for entity in &new_markers {
        commands.entity(entity).observe(on_marker_click);
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

fn on_marker_click(
    event: On<Pointer<Click>>,
    markers: Query<&ProvinceMarker>,
    mut view: ResMut<ViewState>,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    let Ok(marker) = markers.get(event.entity) else {
        return;
    };
    view.selected = Some(Selection::Province(marker.province));
}

/// Escape backs out of a body view.
pub fn view_hotkeys(keys: Res<ButtonInput<KeyCode>>, mut view: ResMut<ViewState>) {
    if keys.just_pressed(KeyCode::Escape) && matches!(view.view, MapView::Body(_)) {
        view.view = MapView::System;
    }
}
