//! Orbit camera for the system and globe views.
//!
//! Right-drag rotates, wheel zooms. Switching views retargets the camera
//! and eases toward the new framing. The wheel belongs to whatever the
//! pointer is over: while egui holds the pointer, the map ignores it.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::skybox::{SpaceSkybox, space_skybox};
use crate::view::{FLAT_HEIGHT, FLAT_WIDTH, MapProjection, MapView, ViewState};

/// Orbit parameters, eased toward `goal_distance` when views change.
#[derive(Resource)]
pub struct OrbitCamera {
    /// Horizontal angle in radians.
    pub yaw: f32,
    /// Vertical angle in radians, clamped short of the poles.
    pub pitch: f32,
    /// Current distance from the target.
    pub distance: f32,
    /// Distance the camera eases toward.
    pub goal_distance: f32,
    /// Allowed zoom range for the active view.
    pub zoom_range: (f32, f32),
    /// Where a flat map is centred. Unused by the orbiting views.
    pub pan: Vec2,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.45,
            distance: 26.0,
            goal_distance: 22.0,
            zoom_range: (10.0, 40.0),
            pan: Vec2::ZERO,
        }
    }
}

pub fn spawn_camera(mut commands: Commands, skybox: Res<SpaceSkybox>) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.015, 0.017, 0.03)),
            ..Default::default()
        },
        AmbientLight {
            color: Color::WHITE,
            brightness: 220.0,
            ..Default::default()
        },
        space_skybox(&skybox),
        Transform::from_xyz(0.0, 10.0, 26.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Reframes the camera when the map view (not merely the selection)
/// changes.
pub fn retarget_on_view_change(
    view: Res<ViewState>,
    mut previous: Local<Option<(MapView, MapProjection)>>,
    mut orbit: ResMut<OrbitCamera>,
) {
    let now = (view.view, view.projection);
    if *previous == Some(now) {
        return;
    }
    *previous = Some(now);
    match (view.view, view.projection) {
        (MapView::System, _) => {
            orbit.goal_distance = 22.0;
            orbit.zoom_range = (10.0, 40.0);
        }
        (MapView::Body(_), MapProjection::Globe) => {
            orbit.goal_distance = 9.5;
            orbit.zoom_range = (4.0, 16.0);
        }
        // Far enough out to see the whole sheet, and allowed much closer
        // than a globe since a flat map can be read right down at a
        // province.
        (MapView::Body(_), MapProjection::Flat) => {
            // Far enough that the whole sheet is on screen at a wide
            // aspect. A narrow window sees the poles but not both edges,
            // which is what the generous zoom-out is for.
            orbit.goal_distance = FLAT_WIDTH * 0.8;
            orbit.zoom_range = (2.0, FLAT_WIDTH * 1.15);
            orbit.pan = Vec2::ZERO;
        }
    }
}

/// Applies drag rotation, wheel zoom, and easing, then writes the camera
/// transform. Both views orbit the origin: the system centres on the
/// primary, the globe sits at the origin of its own view.
#[allow(clippy::too_many_arguments)]
pub fn drive_camera(
    time: Res<Time>,
    view: Res<ViewState>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut contexts: EguiContexts,
    mut orbit: ResMut<OrbitCamera>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    // Asked in Update, so this reports the layout egui drew last frame —
    // the layout the player was looking at when they turned the wheel.
    // Before any context exists the map is the only surface there is.
    let egui_holds_pointer = contexts
        .ctx_mut()
        .map(|ctx| ctx.egui_wants_pointer_input())
        .unwrap_or(false);

    // A flat map has nothing to orbit: dragging slides the map under a
    // camera that always looks straight at it.
    let flat = view.projection == MapProjection::Flat && matches!(view.view, MapView::Body(_));

    if buttons.pressed(MouseButton::Right) {
        if flat {
            // Scaled by distance so a drag moves the same amount of map
            // however far out the view is zoomed.
            let scale = orbit.distance * 0.0016;
            orbit.pan.x -= motion.delta.x * scale;
            orbit.pan.y += motion.delta.y * scale;
        } else {
            orbit.yaw -= motion.delta.x * 0.008;
            orbit.pitch = (orbit.pitch + motion.delta.y * 0.008).clamp(-1.45, 1.45);
        }
    }
    if map_takes_wheel(scroll.delta.y, egui_holds_pointer) {
        orbit.goal_distance = zoomed_goal(orbit.goal_distance, orbit.zoom_range, scroll.delta.y);
    }

    // Panning stops at the map's edge, so the map cannot be lost offscreen.
    let limit = Vec2::new(FLAT_WIDTH / 2.0, FLAT_HEIGHT / 2.0);
    orbit.pan = orbit.pan.clamp(-limit, limit);

    let ease = 1.0 - (-8.0 * time.delta_secs()).exp();
    orbit.distance += (orbit.goal_distance - orbit.distance) * ease;

    let (position, target) = if flat {
        let target = Vec3::new(orbit.pan.x, orbit.pan.y, 0.0);
        (target + Vec3::Z * orbit.distance, target)
    } else {
        let rotation = Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0);
        (rotation * Vec3::new(0.0, 0.0, orbit.distance), Vec3::ZERO)
    };
    for mut transform in &mut cameras {
        *transform = Transform::from_translation(position).looking_at(target, Vec3::Y);
    }
}

/// Whether the map should act on a wheel event this frame.
///
/// egui owns the pointer while it is over one of its surfaces or driving a
/// widget, and a wheel turned there belongs to that surface alone — a list
/// scrolled under the pointer must not also pull the map in behind it.
fn map_takes_wheel(scroll_y: f32, egui_holds_pointer: bool) -> bool {
    scroll_y.abs() > 0.0 && !egui_holds_pointer
}

/// The distance the camera should ease toward after a wheel turn, kept
/// inside the active view's zoom range.
fn zoomed_goal(goal: f32, range: (f32, f32), scroll_y: f32) -> f32 {
    let factor = 1.0 - scroll_y * 0.1;
    (goal * factor).clamp(range.0, range.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_over_the_map_zooms_it() {
        assert!(map_takes_wheel(1.0, false));
        assert!(map_takes_wheel(-1.0, false));
    }

    #[test]
    fn a_wheel_over_an_egui_surface_leaves_the_map_alone() {
        assert!(!map_takes_wheel(1.0, true));
        assert!(!map_takes_wheel(-1.0, true));
    }

    #[test]
    fn a_still_wheel_moves_nothing_either_way() {
        assert!(!map_takes_wheel(0.0, false));
        assert!(!map_takes_wheel(0.0, true));
    }

    #[test]
    fn zooming_moves_toward_the_wheel_and_stops_at_the_range() {
        let range = (10.0, 40.0);
        assert!(
            zoomed_goal(22.0, range, 1.0) < 22.0,
            "scrolling up draws in"
        );
        assert!(
            zoomed_goal(22.0, range, -1.0) > 22.0,
            "scrolling down pulls out"
        );
        assert_eq!(zoomed_goal(10.5, range, 50.0), range.0);
        assert_eq!(zoomed_goal(39.5, range, -50.0), range.1);
    }
}
