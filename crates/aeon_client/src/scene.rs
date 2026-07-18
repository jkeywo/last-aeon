//! The 3D map scene: system view and per-body globe views.
//!
//! Visual entities are presentation-side mirrors of the simulation's map
//! entities, linked by stable ID. Programmer art on purpose: flat-shaded
//! spheres, clear political-ready colours, readable selection feedback.

use aeon_data::model::BodyKind;
use aeon_sim::map::{BodyRecord, DisplayName, GeoPosition, ProvinceRecord};
use aeon_sim::{BodyId, CampaignClock, ProvinceId};
use bevy::prelude::*;

use crate::view::{MapView, Selection, ViewState, geo_to_unit};

/// Radius of a body-view globe in render units.
pub const GLOBE_RADIUS: f32 = 2.5;

/// A body's marker in the system view.
#[derive(Component, Copy, Clone)]
pub struct SystemBodyVisual {
    /// The simulation body this visual mirrors.
    pub body: BodyId,
}

/// A body's globe in its body view.
#[derive(Component, Copy, Clone)]
pub struct GlobeVisual {
    /// The simulation body this globe shows.
    pub body: BodyId,
}

/// A province marker on a globe.
#[derive(Component, Copy, Clone)]
pub struct ProvinceMarker {
    /// The simulation province this marker mirrors.
    pub province: ProvinceId,
}

/// Base colour of a visual, so hover/selection tints can restore it.
#[derive(Component, Copy, Clone)]
pub struct BaseColor(pub Color);

fn body_display_radius(record: &BodyRecord) -> f32 {
    // Cube-root compression keeps the starbase visible next to the planet.
    (record.radius_km as f32).cbrt() / 8.0
}

fn body_orbit_display_radius(record: &BodyRecord) -> f32 {
    if record.orbit_radius_mm == 0 {
        0.0
    } else {
        4.0 + (record.orbit_radius_mm as f32).sqrt() / 2.0
    }
}

fn body_color(kind: BodyKind) -> Color {
    match kind {
        BodyKind::Planet => Color::srgb(0.45, 0.40, 0.32),
        BodyKind::Moon => Color::srgb(0.55, 0.56, 0.60),
        BodyKind::Starbase => Color::srgb(0.75, 0.62, 0.28),
    }
}

/// Spawns lights, camera scaffolding, and all map visuals from the
/// simulation's map entities. Runs once at startup, after the campaign
/// begins.
pub fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    bodies: Query<(&BodyRecord, &DisplayName)>,
    provinces: Query<(&ProvinceRecord, &DisplayName, &GeoPosition)>,
) {
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            ..Default::default()
        },
        Transform::from_xyz(30.0, 18.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // System-view body markers.
    for (record, name) in &bodies {
        let radius = body_display_radius(record);
        let color = body_color(record.kind);
        commands.spawn((
            SystemBodyVisual { body: record.id },
            BaseColor(color),
            Mesh3d(meshes.add(Sphere::new(radius).mesh().ico(4).unwrap())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.9,
                metallic: 0.0,
                ..Default::default()
            })),
            Transform::default(),
            Name::new(format!("system:{}", name.0)),
        ));
    }

    // Per-body globes with province markers, hidden until focused.
    for (record, name) in &bodies {
        let color = body_color(record.kind);
        let globe = commands
            .spawn((
                GlobeVisual { body: record.id },
                BaseColor(color),
                Mesh3d(meshes.add(Sphere::new(GLOBE_RADIUS).mesh().ico(5).unwrap())),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color.darker(0.05),
                    perceptual_roughness: 0.95,
                    metallic: 0.0,
                    ..Default::default()
                })),
                Transform::default(),
                Visibility::Hidden,
                Name::new(format!("globe:{}", name.0)),
            ))
            .id();

        for (province, province_name, geo) in &provinces {
            if province.body != record.id {
                continue;
            }
            let position = geo_to_unit(geo.latitude_mdeg, geo.longitude_mdeg) * GLOBE_RADIUS;
            let marker_color = Color::srgb(0.82, 0.78, 0.70);
            let marker = commands
                .spawn((
                    ProvinceMarker {
                        province: province.id,
                    },
                    BaseColor(marker_color),
                    Mesh3d(meshes.add(Sphere::new(0.07).mesh().ico(2).unwrap())),
                    MeshMaterial3d(materials.add(StandardMaterial {
                        base_color: marker_color,
                        perceptual_roughness: 0.6,
                        ..Default::default()
                    })),
                    Transform::from_translation(position),
                    Name::new(format!("province:{}", province_name.0)),
                ))
                .id();
            commands.entity(globe).add_child(marker);
        }
    }
}

/// Positions system-view bodies along their orbits from the campaign date.
pub fn update_system_positions(
    clock: Option<Res<CampaignClock>>,
    bodies: Query<&BodyRecord>,
    mut visuals: Query<(&SystemBodyVisual, &mut Transform)>,
) {
    let Some(clock) = clock else {
        return;
    };
    let day = clock.date.days_since_epoch() as f32;

    for (visual, mut transform) in &mut visuals {
        let Some(record) = bodies.iter().find(|r| r.id == visual.body) else {
            continue;
        };
        let position = if record.orbit_days == 0 {
            Vec3::ZERO
        } else {
            let angle =
                std::f32::consts::TAU * (day / record.orbit_days as f32) + visual.body.raw() as f32;
            let radius = body_orbit_display_radius(record);
            Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
        };
        transform.translation = position;
    }
}

/// Shows and hides visuals according to the active view.
pub fn apply_view_visibility(
    view: Res<ViewState>,
    mut system_visuals: Query<&mut Visibility, (With<SystemBodyVisual>, Without<GlobeVisual>)>,
    mut globes: Query<(&GlobeVisual, &mut Visibility), Without<SystemBodyVisual>>,
) {
    if !view.is_changed() {
        return;
    }
    let system_active = view.view == MapView::System;
    for mut visibility in &mut system_visuals {
        *visibility = if system_active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (globe, mut visibility) in &mut globes {
        *visibility = if view.view == MapView::Body(globe.body) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Recolours province markers by their holder's political colour.
///
/// Runs when politics may have changed hands; cheap at MVP scale.
pub fn apply_political_colors(
    clock: Option<Res<aeon_sim::CampaignClock>>,
    politics: Option<Res<aeon_sim::PoliticsIndex>>,
    content: Option<Res<aeon_sim::state::ContentDb>>,
    titles: Query<&aeon_sim::TitleRecord>,
    orgs: Query<&aeon_sim::OrgRecord>,
    mut markers: Query<(&ProvinceMarker, &mut BaseColor)>,
) {
    let (Some(clock), Some(politics), Some(content)) = (clock, politics, content) else {
        return;
    };
    // Refresh on the first frame of each day; holdings move slowly.
    if !clock.is_changed() {
        return;
    }

    for (marker, mut base) in &mut markers {
        let Some(title_id) = politics.province_titles.get(&marker.province) else {
            continue;
        };
        let Some(title_entity) = politics.titles.get(title_id) else {
            continue;
        };
        let Ok(title) = titles.get(*title_entity) else {
            continue;
        };
        let color = match title.holder {
            aeon_sim::TitleHolder::Org(org_id) => politics
                .orgs
                .get(&org_id)
                .and_then(|entity| orgs.get(*entity).ok())
                .and_then(|org| content.0.organisations.get(&org.key))
                .map(|def| Color::srgb_u8(def.color.0, def.color.1, def.color.2)),
            _ => None,
        };
        base.0 = color.unwrap_or(Color::srgb(0.35, 0.35, 0.38));
    }
}

/// Applies base political colours plus selection highlights to visuals.
///
/// Materials are only written when the target colour actually differs, so
/// asset change detection stays quiet on idle frames.
pub fn apply_selection_tint(
    view: Res<ViewState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    system_visuals: Query<(
        &SystemBodyVisual,
        &BaseColor,
        &MeshMaterial3d<StandardMaterial>,
    )>,
    markers: Query<(
        &ProvinceMarker,
        &BaseColor,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let mut apply =
        |selected: bool, base: &BaseColor, handle: &MeshMaterial3d<StandardMaterial>| {
            let target = if selected {
                Color::srgb(0.98, 0.88, 0.30)
            } else {
                base.0
            };
            let differs = materials
                .get(&handle.0)
                .is_some_and(|m| m.base_color != target);
            if differs && let Some(mut material) = materials.get_mut(&handle.0) {
                material.base_color = target;
            }
        };

    for (visual, base, handle) in &system_visuals {
        apply(
            view.selected == Some(Selection::Body(visual.body)),
            base,
            handle,
        );
    }
    for (marker, base, handle) in &markers {
        apply(
            view.selected == Some(Selection::Province(marker.province)),
            base,
            handle,
        );
    }
}
