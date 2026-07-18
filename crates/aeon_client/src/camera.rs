//! Orbit camera for the system and globe views.
//!
//! Right-drag rotates, wheel zooms. Switching views retargets the camera
//! and eases toward the new framing.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::view::{MapView, ViewState};

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
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.45,
            distance: 26.0,
            goal_distance: 22.0,
            zoom_range: (10.0, 40.0),
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
    mut previous: Local<Option<MapView>>,
    mut orbit: ResMut<OrbitCamera>,
) {
    if *previous == Some(view.view) {
        return;
    }
    *previous = Some(view.view);
    match view.view {
        MapView::System => {
            orbit.goal_distance = 22.0;
            orbit.zoom_range = (10.0, 40.0);
        }
        MapView::Body(_) => {
            orbit.goal_distance = 9.5;
            orbit.zoom_range = (4.0, 16.0);
        }
    }
}

/// Applies drag rotation, wheel zoom, and easing, then writes the camera
/// transform. Both views orbit the origin: the system centres on the
/// primary, the globe sits at the origin of its own view.
pub fn drive_camera(
    time: Res<Time>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut orbit: ResMut<OrbitCamera>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
) {
    if buttons.pressed(MouseButton::Right) {
        orbit.yaw -= motion.delta.x * 0.008;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.008).clamp(-1.45, 1.45);
    }
    if scroll.delta.y.abs() > 0.0 {
        let factor = 1.0 - scroll.delta.y * 0.1;
        orbit.goal_distance =
            (orbit.goal_distance * factor).clamp(orbit.zoom_range.0, orbit.zoom_range.1);
    }

    let ease = 1.0 - (-8.0 * time.delta_secs()).exp();
    orbit.distance += (orbit.goal_distance - orbit.distance) * ease;

    let rotation = Quat::from_euler(EulerRot::YXZ, orbit.yaw, -orbit.pitch, 0.0);
    let position = rotation * Vec3::new(0.0, 0.0, orbit.distance);
    for mut transform in &mut cameras {
        *transform = Transform::from_translation(position).looking_at(Vec3::ZERO, Vec3::Y);
    }
}
