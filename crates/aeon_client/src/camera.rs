//! Orbit camera for the system and globe views.
//!
//! Right-drag rotates, wheel zooms. Switching views retargets the camera
//! and eases toward the new framing.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

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

pub fn spawn_camera(mut commands: Commands) {
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
pub fn drive_camera(
    time: Res<Time>,
    view: Res<ViewState>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut orbit: ResMut<OrbitCamera>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
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
    if scroll.delta.y.abs() > 0.0 {
        let factor = 1.0 - scroll.delta.y * 0.1;
        orbit.goal_distance =
            (orbit.goal_distance * factor).clamp(orbit.zoom_range.0, orbit.zoom_range.1);
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
