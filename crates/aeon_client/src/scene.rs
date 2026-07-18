//! The 3D map scene: system view and per-body political globes.
//!
//! The system view shows each body in its orbit. A body's globe paints its
//! provinces as coloured regions with black borders onto the sphere,
//! baked into an equirectangular texture from the simulation's holdings.
//! Two map modes colour by the direct holder or by the top-liege great
//! house. Programmer art, but a real political map rather than marker dots.

use std::collections::BTreeMap;

use aeon_data::model::BodyKind;
use aeon_sim::map::{BodyRecord, DisplayName, GeoPosition, ProvinceRecord};
use aeon_sim::{BodyId, CampaignClock, ProvinceId};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::map_modes::MapReadout;
use crate::view::{MapMode, MapView, Selection, ViewState, geo_to_unit};

/// Radius of a body-view globe in render units.
pub const GLOBE_RADIUS: f32 = 2.5;
/// Baked political-texture resolution.
const TEX_W: usize = 768;
const TEX_H: usize = 384;

/// A body's marker in the system view.
#[derive(Component, Copy, Clone)]
pub struct SystemBodyVisual {
    /// The simulation body this visual mirrors.
    pub body: BodyId,
}

/// A body's political globe in its body view.
#[derive(Component, Clone)]
pub struct GlobeVisual {
    /// The simulation body this globe shows.
    pub body: BodyId,
    /// Unit-sphere centroids of the body's provinces, for picking, the
    /// texture bake, and label projection.
    pub centroids: Vec<(ProvinceId, Vec3)>,
    /// The political texture this globe paints (mutated on rebake).
    pub texture: Handle<Image>,
}

/// The bright pin marking the selected province on a globe.
#[derive(Component)]
pub struct SelectionPin;

/// Base colour of a system-view body, so selection tint can restore it.
#[derive(Component, Copy, Clone)]
pub struct BaseColor(pub Color);

/// Tracks what the focused globe's texture was last baked from, so it is
/// rebuilt only when the body, map mode, or holdings actually change.
#[derive(Resource, Default)]
pub struct GlobeBake {
    baked: Option<(BodyId, MapMode, u64)>,
}

fn body_display_radius(record: &BodyRecord) -> f32 {
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

/// The texel's latitude/longitude in millidegrees, matching the globe
/// mesh's equirectangular UVs (u: longitude -180..180, v: latitude
/// +90 (top) .. -90 (bottom)).
fn texel_unit(x: usize, y: usize) -> Vec3 {
    let u = (x as f32 + 0.5) / TEX_W as f32;
    let v = (y as f32 + 0.5) / TEX_H as f32;
    let lon = (u * 360.0 - 180.0).to_radians();
    let lat = (90.0 - v * 180.0).to_radians();
    Vec3::new(lat.cos() * lon.cos(), lat.sin(), -(lat.cos() * lon.sin()))
}

/// Builds a UV sphere whose vertex UVs match [`texel_unit`], so a baked
/// equirectangular texture lands exactly where its texels say.
fn build_globe_mesh(radius: f32, sectors: usize, stacks: usize) -> Mesh {
    let mut positions = Vec::with_capacity((sectors + 1) * (stacks + 1));
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uvs = Vec::with_capacity(positions.capacity());
    for stack in 0..=stacks {
        let v = stack as f32 / stacks as f32;
        let lat = (90.0 - v * 180.0).to_radians();
        for sector in 0..=sectors {
            let u = sector as f32 / sectors as f32;
            let lon = (u * 360.0 - 180.0).to_radians();
            let dir = Vec3::new(lat.cos() * lon.cos(), lat.sin(), -(lat.cos() * lon.sin()));
            positions.push((dir * radius).to_array());
            normals.push(dir.to_array());
            uvs.push([u, v]);
        }
    }
    let mut indices = Vec::with_capacity(sectors * stacks * 6);
    let row = sectors + 1;
    for stack in 0..stacks {
        for sector in 0..sectors {
            let a = (stack * row + sector) as u32;
            let b = a + row as u32;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// The nearest province centroid to a unit direction, by great-circle
/// proximity (max dot product).
pub fn nearest_province(dir: Vec3, centroids: &[(ProvinceId, Vec3)]) -> Option<ProvinceId> {
    centroids
        .iter()
        .map(|(id, c)| (*id, c.dot(dir)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(id, _)| id)
}

/// Bakes the political texture: each texel takes the colour of its nearest
/// province, black where it borders a different province, neutral grey
/// where a province is unheld.
fn bake_texture(
    centroids: &[(ProvinceId, Vec3)],
    colours: &BTreeMap<ProvinceId, [u8; 3]>,
) -> Vec<u8> {
    // Grid of nearest-province indices (into `centroids`).
    let mut grid = vec![u16::MAX; TEX_W * TEX_H];
    for y in 0..TEX_H {
        for x in 0..TEX_W {
            let dir = texel_unit(x, y);
            let mut best = 0usize;
            let mut best_dot = f32::MIN;
            for (i, (_, c)) in centroids.iter().enumerate() {
                let d = c.dot(dir);
                if d > best_dot {
                    best_dot = d;
                    best = i;
                }
            }
            grid[y * TEX_W + x] = best as u16;
        }
    }

    let neutral = [90u8, 90, 96];
    let mut data = vec![0u8; TEX_W * TEX_H * 4];
    for y in 0..TEX_H {
        for x in 0..TEX_W {
            let i = y * TEX_W + x;
            let prov = grid[i];
            // Border where a 4-neighbour belongs to a different province.
            let border = [
                grid[y * TEX_W + x.wrapping_sub(1).min(TEX_W - 1)],
                grid[y * TEX_W + (x + 1).min(TEX_W - 1)],
                grid[y.saturating_sub(1) * TEX_W + x],
                grid[(y + 1).min(TEX_H - 1) * TEX_W + x],
            ]
            .iter()
            .any(|n| *n != prov);
            let rgb = if border {
                [12u8, 12, 14]
            } else {
                centroids
                    .get(prov as usize)
                    .and_then(|(id, _)| colours.get(id))
                    .copied()
                    .unwrap_or(neutral)
            };
            let o = i * 4;
            data[o] = rgb[0];
            data[o + 1] = rgb[1];
            data[o + 2] = rgb[2];
            data[o + 3] = 255;
        }
    }
    data
}

/// A stable fingerprint of what the focused globe is painting, so the
/// texture rebakes only when the picture would actually change.
fn readout_fingerprint(readout: &MapReadout) -> u64 {
    // Accumulated in province order rather than xor-ed: xor would cancel
    // identical contributions, so a map where every province shares one
    // colour would fingerprint the same whatever that colour was.
    let mut acc = 0xcbf2_9ce4_8422_2325u64;
    for (province, entry) in &readout.provinces {
        let colour = u64::from(entry.colour[0]) << 16
            | u64::from(entry.colour[1]) << 8
            | u64::from(entry.colour[2]);
        for value in [province.raw(), colour] {
            acc ^= value;
            acc = acc.wrapping_mul(0x100_0000_01b3);
        }
    }
    acc
}

/// Spawns lights, system-view body markers, per-body political globes, and
/// the selection pin.
pub fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
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

    let globe_mesh = meshes.add(build_globe_mesh(GLOBE_RADIUS, 96, 48));

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

    // Per-body political globes, hidden until focused.
    for (record, name) in &bodies {
        let centroids: Vec<(ProvinceId, Vec3)> = provinces
            .iter()
            .filter(|(p, _, _)| p.body == record.id)
            .map(|(p, _, geo)| (p.id, geo_to_unit(geo.latitude_mdeg, geo.longitude_mdeg)))
            .collect();
        // Blank texture; the refresh system paints it on first focus.
        let texture = images.add(blank_texture());
        commands.spawn((
            GlobeVisual {
                body: record.id,
                centroids,
                texture: texture.clone(),
            },
            Mesh3d(globe_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(texture),
                perceptual_roughness: 1.0,
                metallic: 0.0,
                ..Default::default()
            })),
            Transform::default(),
            Visibility::Hidden,
            Name::new(format!("globe:{}", name.0)),
        ));
    }

    // A single reusable selection pin.
    commands.spawn((
        SelectionPin,
        Mesh3d(meshes.add(Sphere::new(0.09).mesh().ico(2).unwrap())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.98, 0.88, 0.30),
            unlit: true,
            ..Default::default()
        })),
        Transform::default(),
        Visibility::Hidden,
        Name::new("selection-pin"),
    ));
}

fn blank_texture() -> Image {
    let data = vec![80u8; TEX_W * TEX_H * 4];
    Image::new(
        Extent3d {
            width: TEX_W as u32,
            height: TEX_H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
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

/// Rebakes the focused globe's texture when the body, the map mode, or
/// what the mode is showing actually changes.
pub fn refresh_globe_texture(
    view: Res<ViewState>,
    mode: Res<MapMode>,
    readout: Res<MapReadout>,
    mut bake: ResMut<GlobeBake>,
    mut images: ResMut<Assets<Image>>,
    globes: Query<&GlobeVisual>,
) {
    let MapView::Body(body) = view.view else {
        return;
    };
    if readout.provinces.is_empty() {
        return;
    }
    let fingerprint = readout_fingerprint(&readout);
    if bake.baked == Some((body, *mode, fingerprint)) {
        return;
    }
    let Some(globe) = globes.iter().find(|g| g.body == body) else {
        return;
    };
    let colours: BTreeMap<ProvinceId, [u8; 3]> = readout
        .provinces
        .iter()
        .map(|(id, entry)| (*id, entry.colour))
        .collect();
    let data = bake_texture(&globe.centroids, &colours);
    if let Some(mut image) = images.get_mut(&globe.texture) {
        image.data = Some(data);
    }
    bake.baked = Some((body, *mode, fingerprint));
}

/// Moves the selection pin to the selected province on the focused globe,
/// or hides it.
pub fn update_selection_pin(
    view: Res<ViewState>,
    provinces: Query<(&ProvinceRecord, &DisplayName, &GeoPosition)>,
    mut pins: Query<(&mut Transform, &mut Visibility), With<SelectionPin>>,
) {
    let Ok((mut transform, mut visibility)) = pins.single_mut() else {
        return;
    };
    let target = match (view.view, view.selected) {
        (MapView::Body(body), Some(Selection::Province(id))) => provinces
            .iter()
            .find(|(p, _, _)| p.id == id && p.body == body)
            .map(|(_, _, geo)| geo_to_unit(geo.latitude_mdeg, geo.longitude_mdeg)),
        _ => None,
    };
    match target {
        Some(dir) => {
            transform.translation = dir * (GLOBE_RADIUS * 1.03);
            *visibility = Visibility::Inherited;
        }
        None => *visibility = Visibility::Hidden,
    }
}

/// Tints hovered/selected system-view bodies so feedback is unmissable.
pub fn apply_selection_tint(
    view: Res<ViewState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    system_visuals: Query<(
        &SystemBodyVisual,
        &BaseColor,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (visual, base, handle) in &system_visuals {
        let selected = view.selected == Some(Selection::Body(visual.body));
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
    }
}
