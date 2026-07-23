//! Picking and selection.
//!
//! Click selects a body or (on a textured globe) the province whose ID colour
//! is under the hit point; double-clicking a body opens its strategic view; Escape returns
//! to the system view.

use aeon_sim::{AssignmentTarget, BodyId};
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;

use crate::assignment_ui::{AssignmentForm, ProvinceSlot};
use crate::scene::{GlobeVisual, SystemBodyVisual, nearest_province, province_from_id_texture};
use crate::view::{MapView, Selection, ViewState};

/// Attaches pointer observers to every map visual once the scene exists.
pub fn attach_pickers(
    mut commands: Commands,
    new_bodies: Query<Entity, Added<SystemBodyVisual>>,
    new_globes: Query<Entity, Added<GlobeVisual>>,
    new_model_meshes: Query<(Entity, &ChildOf), Added<Mesh3d>>,
    body_visuals: Query<&SystemBodyVisual>,
    parents: Query<&ChildOf>,
) {
    for entity in &new_bodies {
        commands.entity(entity).observe(on_body_click);
    }
    for entity in &new_globes {
        commands.entity(entity).observe(on_globe_click);
    }
    // GLTF scenes add their mesh children asynchronously. Attach the same
    // observer to those children, then resolve their system body by walking
    // up to the model root in `on_body_click`.
    for (entity, _) in &new_model_meshes {
        if body_for_entity(entity, &body_visuals, &parents).is_some() {
            commands.entity(entity).observe(on_body_click);
        }
    }
}

fn body_for_entity(
    mut entity: Entity,
    bodies: &Query<&SystemBodyVisual>,
    parents: &Query<&ChildOf>,
) -> Option<BodyId> {
    loop {
        if let Ok(visual) = bodies.get(entity) {
            return Some(visual.body);
        }
        entity = parents.get(entity).ok()?.parent();
    }
}

fn on_body_click(
    event: On<Pointer<Click>>,
    bodies: Query<&SystemBodyVisual>,
    parents: Query<&ChildOf>,
    mut view: ResMut<ViewState>,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    let Some(body) = body_for_entity(event.entity, &bodies, &parents) else {
        return;
    };
    view.selected = Some(Selection::Body(body));
    if event.count >= 2 {
        view.view = MapView::Body(body);
    }
}

fn on_globe_click(
    event: On<Pointer<Click>>,
    globes: Query<&GlobeVisual>,
    images: Res<Assets<Image>>,
    mut view: ResMut<ViewState>,
    mut form: ResMut<AssignmentForm>,
) {
    if event.button != PointerButton::Primary {
        return;
    }
    let Ok(globe) = globes.get(event.entity) else {
        return;
    };
    // The surface sits at the origin with an identity transform, so the
    // world hit point needs only un-projecting to become a direction —
    // which is the one thing that differs between a sphere and a plane.
    let Some(position) = event.hit.position else {
        return;
    };
    let dir = view.projection.direction_at(position);
    // Authored bodies use their hard-edged ID texture. The starbase has no
    // province-ID texture, so it retains the single-region centroid fallback.
    let province = province_from_id_texture(dir, globe, &images).or_else(|| {
        globe
            .province_id_texture
            .is_none()
            .then(|| nearest_province(dir, &globe.centroids))
            .flatten()
    });
    let Some(province) = province else {
        return;
    };
    // A province pick requested from the assignment popup consumes the click:
    // it fills the slot the "pick on map" button was standing in for, and
    // leaves the selection where it was so the popup stays about its subject.
    if let Some(slot) = form.map_pick.take() {
        match slot {
            ProvinceSlot::Target => form.target = Some(AssignmentTarget::Province(province)),
            ProvinceSlot::Destination => form.province = Some(province),
        }
        return;
    }
    view.selected = Some(Selection::Province(province));
}

/// Escape backs out of a body view.
pub fn view_hotkeys(keys: Res<ButtonInput<KeyCode>>, mut view: ResMut<ViewState>) {
    if keys.just_pressed(KeyCode::Escape) && matches!(view.view, MapView::Body(_)) {
        view.view = MapView::System;
    }
}
