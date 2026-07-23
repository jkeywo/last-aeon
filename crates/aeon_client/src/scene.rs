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
use aeon_sim::{BodyId, ProvinceId};
use bevy::asset::RenderAssetUsages;
use bevy::gltf::{Gltf, GltfMaterial, GltfMesh};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use serde::Deserialize;

use crate::map_modes::MapReadout;
use crate::view::{
    FLAT_HEIGHT, FLAT_WIDTH, GLOBE_RADIUS, MapMode, MapProjection, MapView, Selection, ViewState,
    geo_to_unit,
};

/// Baked political-texture resolution.
const TEX_W: usize = 768;
const TEX_H: usize = 384;

/// A body's marker in the system view.
#[derive(Component, Copy, Clone)]
pub struct SystemBodyVisual {
    /// The simulation body this visual mirrors.
    pub body: BodyId,
}

/// The uniform scale a system-view visual takes in its orbit, and the larger
/// scale it swells to when it is the focused body view. `focus` is `Some` only
/// for bodies whose system-view visual doubles as their zoomed-in view (the
/// starbase model); planets and moons hand off to a political globe instead.
#[derive(Component, Copy, Clone)]
pub struct SystemScale {
    orbit: f32,
    focus: Option<f32>,
}

/// Waits for the downloaded GLB to finish loading before spawning its meshes.
#[derive(Component)]
pub struct PendingStarbaseModel {
    asset: Handle<Gltf>,
    spawned: bool,
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
    /// Authored surface art, composited below the political map colours.
    pub surface_texture: Option<Handle<Image>>,
    /// Flat province ID image used for exact texture-colour picking.
    pub province_id_texture: Option<Handle<Image>>,
    /// Decoded RGB ID values, mapped to the live simulation province IDs.
    pub province_by_colour: BTreeMap<[u8; 3], ProvinceId>,
    /// The material extension that receives the active province RGB value.
    pub material: Handle<GlobeSurfaceMaterial>,
}

/// GPU data for the province-selection pulse. Binding slots start at 100 so
/// they cannot overlap with Bevy's standard PBR material bindings.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct GlobeMaterial {
    /// Selected ID encoded as sRGB RGB plus an enabled flag in alpha.
    #[uniform(100)]
    pub selection: Vec4,
    /// Exact, unfiltered province ID image.
    #[texture(101)]
    #[sampler(102)]
    pub province_ids: Handle<Image>,
}

impl MaterialExtension for GlobeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/globe_selection.wgsl".into()
    }
}

pub type GlobeSurfaceMaterial = ExtendedMaterial<StandardMaterial, GlobeMaterial>;

/// Base colour of a system-view body, so selection tint can restore it.
#[derive(Component, Copy, Clone)]
pub struct BaseColor(pub Color);

/// Tracks what the focused globe's texture was last baked from, so it is
/// rebuilt only when the body, map mode, or holdings actually change.
#[derive(Resource, Default)]
pub struct GlobeBake {
    baked: Option<(BodyId, MapMode, u64)>,
}

/// The small per-body manifest next to a province-ID texture.
#[derive(Deserialize)]
struct ProvinceIdManifest {
    provinces: BTreeMap<String, [u8; 3]>,
}

fn texture_asset_paths(body_key: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match body_key {
        "ashkarr" => Some((
            "textures/maps/ashkarr_surface.png",
            "textures/maps/ashkarr_province_ids.png",
            include_str!("../../../assets/textures/maps/ashkarr_province_ids.json"),
        )),
        "vesk" => Some((
            "textures/maps/vesk_surface.png",
            "textures/maps/vesk_province_ids.png",
            include_str!("../../../assets/textures/maps/vesk_province_ids.json"),
        )),
        _ => None,
    }
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

/// The downloaded starbase model is authored in large metre-like units; this
/// scales its roughly 95 km diameter to the display radius used by the system
/// map's other bodies.
fn starbase_display_scale(record: &BodyRecord) -> f32 {
    body_display_radius(record) / 65_000.0
}

/// The scale the starbase model swells to when it is the focused body view,
/// sized to fill the frame like a globe would under the same camera.
fn starbase_focus_scale() -> f32 {
    GLOBE_RADIUS / 65_000.0
}

fn imported_starbase_material(source: Option<&GltfMaterial>) -> StandardMaterial {
    let Some(source) = source else {
        return StandardMaterial {
            base_color: Color::srgb(0.45, 0.42, 0.34),
            metallic: 0.7,
            perceptual_roughness: 0.45,
            ..default()
        };
    };
    StandardMaterial {
        base_color: source.base_color,
        base_color_texture: source.base_color_texture.clone(),
        emissive: source.emissive,
        emissive_texture: source.emissive_texture.clone(),
        perceptual_roughness: source.perceptual_roughness,
        metallic: source.metallic,
        metallic_roughness_texture: source.metallic_roughness_texture.clone(),
        normal_map_texture: source.normal_map_texture.clone(),
        double_sided: source.double_sided,
        cull_mode: source.cull_mode,
        alpha_mode: source.alpha_mode,
        unlit: source.unlit,
        ..default()
    }
}

/// Spawns the downloaded model's primitives after Bevy has loaded its GLB
/// assets. Bevy 0.19 exposes glTF scenes as world assets, so the small static
/// station is expanded directly rather than using the previous scene-root API.
pub fn spawn_loaded_starbases(
    mut commands: Commands,
    mut pending: Query<(Entity, &mut PendingStarbaseModel)>,
    gltfs: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    gltf_materials: Res<Assets<GltfMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (root, mut pending) in &mut pending {
        if pending.spawned {
            continue;
        }
        let Some(gltf) = gltfs.get(&pending.asset) else {
            continue;
        };
        if gltf
            .meshes
            .iter()
            .any(|mesh| gltf_meshes.get(mesh).is_none())
        {
            continue;
        }

        let model_root = commands
            .spawn((
                Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::default(),
                Name::new("aurelian-spire-model"),
            ))
            .id();
        commands.entity(root).add_child(model_root);
        commands.entity(model_root).with_children(|parent| {
            for mesh in &gltf.meshes {
                let mesh = gltf_meshes
                    .get(mesh)
                    .expect("loaded glTF mesh checked above");
                for primitive in &mesh.primitives {
                    let source_material = primitive
                        .material
                        .as_ref()
                        .and_then(|material| gltf_materials.get(material));
                    parent.spawn((
                        Mesh3d(primitive.mesh.clone()),
                        MeshMaterial3d(materials.add(imported_starbase_material(source_material))),
                        Transform::default(),
                        Name::new(format!("aurelian-spire:{}", primitive.name)),
                    ));
                }
            }
        });
        pending.spawned = true;
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

/// Builds the flat map: one quad, because an equirectangular texture maps
/// linearly across a plane.
///
/// The sphere needs a grid of vertices to curve between its UVs; a plane
/// does not, so four corners carry the whole projection exactly.
fn build_flat_mesh(width: f32, height: f32) -> Mesh {
    let (hw, hh) = (width / 2.0, height / 2.0);
    let positions = vec![
        [-hw, -hh, 0.0],
        [hw, -hh, 0.0],
        [-hw, hh, 0.0],
        [hw, hh, 0.0],
    ];
    // v runs from the north pole down, matching `texel_unit` and the
    // sphere's own UVs, so one baked texture serves both meshes.
    let uvs = vec![[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]];
    let normals = vec![[0.0, 0.0, 1.0]; 4];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 2, 1, 3]));
    mesh
}

/// The two meshes a body's surface can be drawn as, kept so switching
/// projection swaps a handle rather than rebuilding geometry.
#[derive(Resource)]
pub struct SurfaceMeshes {
    /// The sphere.
    pub globe: Handle<Mesh>,
    /// The plane.
    pub flat: Handle<Mesh>,
}

impl SurfaceMeshes {
    /// The mesh for a projection.
    pub fn for_projection(&self, projection: MapProjection) -> Handle<Mesh> {
        match projection {
            MapProjection::Globe => self.globe.clone(),
            MapProjection::Flat => self.flat.clone(),
        }
    }
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

/// Resolves a clicked direction through the authored RGB province-ID image.
/// The image is sampled by texel (not filtered), so selection follows the
/// exact visible borders rather than a nearest-centroid approximation.
pub fn province_from_id_texture(
    dir: Vec3,
    globe: &GlobeVisual,
    images: &Assets<Image>,
) -> Option<ProvinceId> {
    let texture = globe.province_id_texture.as_ref()?;
    let image = images.get(texture)?;
    let size = image.texture_descriptor.size;
    let data = image.data.as_ref()?;
    let longitude = (-dir.z).atan2(dir.x);
    let latitude = dir.y.clamp(-1.0, 1.0).asin();
    let x = (((longitude / std::f32::consts::TAU + 0.5) * size.width as f32).floor() as u32)
        % size.width;
    let y = ((0.5 - latitude / std::f32::consts::PI) * size.height as f32)
        .floor()
        .clamp(0.0, (size.height - 1) as f32) as u32;
    let offset = ((y * size.width + x) * 4) as usize;
    let colour = [
        *data.get(offset)?,
        *data.get(offset + 1)?,
        *data.get(offset + 2)?,
    ];
    globe.province_by_colour.get(&colour).copied()
}

/// Updates only when the player changes selection; the shader itself drives
/// the ongoing pulse using Bevy's global time uniform.
pub fn update_globe_selection_glow(
    view: Res<ViewState>,
    globes: Query<&GlobeVisual>,
    mut materials: ResMut<Assets<GlobeSurfaceMaterial>>,
) {
    if !view.is_changed() {
        return;
    }
    for globe in &globes {
        let selected_colour = match view.selected {
            Some(Selection::Province(province)) if view.view == MapView::Body(globe.body) => globe
                .province_by_colour
                .iter()
                .find_map(|(colour, id)| (*id == province).then_some(*colour)),
            _ => None,
        };
        if let Some(mut material) = materials.get_mut(&globe.material) {
            material.extension.selection = selected_colour.map_or(Vec4::ZERO, |[r, g, b]| {
                Vec4::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
            });
        }
    }
}

/// Bakes the political texture: each texel takes the colour of its nearest
/// province, black where it borders a different province, neutral grey
/// where a province is unheld.
fn bake_texture(
    centroids: &[(ProvinceId, Vec3)],
    colours: &BTreeMap<ProvinceId, [u8; 3]>,
    surface: Option<&Image>,
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

    // Border where a 4-neighbour belongs to a different province.
    let mut is_border = vec![false; TEX_W * TEX_H];
    for y in 0..TEX_H {
        for x in 0..TEX_W {
            let i = y * TEX_W + x;
            let prov = grid[i];
            is_border[i] = [
                grid[y * TEX_W + x.wrapping_sub(1).min(TEX_W - 1)],
                grid[y * TEX_W + (x + 1).min(TEX_W - 1)],
                grid[y.saturating_sub(1) * TEX_W + x],
                grid[(y + 1).min(TEX_H - 1) * TEX_W + x],
            ]
            .iter()
            .any(|n| *n != prov);
        }
    }

    // Distance in texels from each texel to the nearest province border, so
    // the map-mode overlay can fade from strong at the border to faint in the
    // interior. The overlay is a reading aid, not a paint job: it names who
    // holds a border and then steps aside to let the authored world show.
    let distance = border_distance(&is_border);
    // Texels over which the overlay fades from its border peak to its floor.
    let fade = 16.0f32;

    let neutral = [90u8, 90, 96];
    let mut data = vec![0u8; TEX_W * TEX_H * 4];
    for y in 0..TEX_H {
        for x in 0..TEX_W {
            let i = y * TEX_W + x;
            let prov = grid[i];
            let rgb = if is_border[i] {
                surface_rgb(surface, x, y).unwrap_or([12, 12, 14])
            } else {
                let political = centroids
                    .get(prov as usize)
                    .and_then(|(id, _)| colours.get(id))
                    .copied()
                    .unwrap_or(neutral);
                match surface_rgb(surface, x, y) {
                    // Overlay weight: near the border the political colour
                    // dominates; deep inside a province the surface art does.
                    Some(base) => {
                        let edge = 1.0 - smoothstep01(distance[i] / fade);
                        blend(base, political, 0.15 + 0.75 * edge)
                    }
                    // No authored art to reveal: paint the mode solidly.
                    None => political,
                }
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

/// A two-pass chamfer transform: the approximate Euclidean distance in texels
/// from every texel to the nearest `true` (border) texel. Runs on rebake only.
fn border_distance(is_border: &[bool]) -> Vec<f32> {
    const DIAG: f32 = std::f32::consts::SQRT_2;
    let big = (TEX_W + TEX_H) as f32;
    let mut dist: Vec<f32> = is_border
        .iter()
        .map(|&b| if b { 0.0 } else { big })
        .collect();
    for y in 0..TEX_H {
        for x in 0..TEX_W {
            let i = y * TEX_W + x;
            let mut d = dist[i];
            if x > 0 {
                d = d.min(dist[i - 1] + 1.0);
            }
            if y > 0 {
                d = d.min(dist[i - TEX_W] + 1.0);
                if x > 0 {
                    d = d.min(dist[i - TEX_W - 1] + DIAG);
                }
                if x + 1 < TEX_W {
                    d = d.min(dist[i - TEX_W + 1] + DIAG);
                }
            }
            dist[i] = d;
        }
    }
    for y in (0..TEX_H).rev() {
        for x in (0..TEX_W).rev() {
            let i = y * TEX_W + x;
            let mut d = dist[i];
            if x + 1 < TEX_W {
                d = d.min(dist[i + 1] + 1.0);
            }
            if y + 1 < TEX_H {
                d = d.min(dist[i + TEX_W] + 1.0);
                if x + 1 < TEX_W {
                    d = d.min(dist[i + TEX_W + 1] + DIAG);
                }
                if x > 0 {
                    d = d.min(dist[i + TEX_W - 1] + DIAG);
                }
            }
            dist[i] = d;
        }
    }
    dist
}

/// Smoothstep on an already-normalised input, clamped to 0..1.
fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear blend of two colours: `weight` is how much of `overlay` shows.
fn blend(base: [u8; 3], overlay: [u8; 3], weight: f32) -> [u8; 3] {
    let w = weight.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - w) + f32::from(b) * w).round() as u8;
    [
        mix(base[0], overlay[0]),
        mix(base[1], overlay[1]),
        mix(base[2], overlay[2]),
    ]
}

/// Reads a source-art texel when it is the expected uncompressed map size.
/// Texture loading is asynchronous, so an unavailable asset simply delays a
/// bake rather than producing an incorrectly sized or filtered result.
fn surface_rgb(surface: Option<&Image>, x: usize, y: usize) -> Option<[u8; 3]> {
    let surface = surface?;
    let size = surface.texture_descriptor.size;
    if size.width != TEX_W as u32 || size.height != TEX_H as u32 {
        return None;
    }
    let data = surface.data.as_ref()?;
    let offset = (y * TEX_W + x) * 4;
    Some([
        *data.get(offset)?,
        *data.get(offset + 1)?,
        *data.get(offset + 2)?,
    ])
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

/// Spawns lights, system-view body markers, and per-body political globes.
#[allow(clippy::too_many_arguments)]
pub fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut globe_materials: ResMut<Assets<GlobeSurfaceMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
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
    let flat_mesh = meshes.add(build_flat_mesh(FLAT_WIDTH, FLAT_HEIGHT));
    commands.insert_resource(SurfaceMeshes {
        globe: globe_mesh.clone(),
        flat: flat_mesh,
    });

    // System-view body markers.
    for (record, name) in &bodies {
        let radius = body_display_radius(record);
        let color = body_color(record.kind);
        if record.kind == BodyKind::Starbase {
            commands.spawn((
                SystemBodyVisual { body: record.id },
                SystemScale {
                    orbit: starbase_display_scale(record),
                    focus: Some(starbase_focus_scale()),
                },
                BaseColor(color),
                PendingStarbaseModel {
                    asset: asset_server.load("models/aurelian_spire.glb"),
                    spawned: false,
                },
                Transform::from_scale(Vec3::splat(starbase_display_scale(record))),
                Visibility::default(),
                Name::new(format!("system:{}", name.0)),
            ));
        } else {
            // Planets and moons show their surface art, lit, so the system
            // view reads as real worlds rather than coloured dots. A body's
            // equirectangular UV sphere maps the same art the globe uses;
            // bodies without authored art fall back to a flat-shaded sphere.
            let surface = texture_asset_paths(record.key.as_str())
                .map(|(surface_path, _, _)| asset_server.load(surface_path));
            let (mesh, base_color) = match &surface {
                Some(_) => (meshes.add(build_globe_mesh(radius, 64, 32)), Color::WHITE),
                None => (
                    meshes.add(Sphere::new(radius).mesh().ico(4).unwrap()),
                    color,
                ),
            };
            commands.spawn((
                SystemBodyVisual { body: record.id },
                SystemScale {
                    orbit: 1.0,
                    focus: None,
                },
                // The tint the selection highlight restores to: white keeps a
                // textured world true-coloured, a flat body keeps its colour.
                BaseColor(base_color),
                Mesh3d(mesh),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color,
                    base_color_texture: surface,
                    perceptual_roughness: 0.9,
                    metallic: 0.0,
                    ..Default::default()
                })),
                Transform::default(),
                Name::new(format!("system:{}", name.0)),
            ));
        }
    }

    // Per-body political globes, hidden until focused. The starbase has no
    // province surface; its zoomed-in view is the station model instead.
    for (record, name) in &bodies {
        if record.kind == BodyKind::Starbase {
            continue;
        }
        let body_provinces: Vec<(&ProvinceRecord, &DisplayName, &GeoPosition)> = provinces
            .iter()
            .filter(|(p, _, _)| p.body == record.id)
            .collect();
        let centroids = body_provinces
            .iter()
            .map(|(p, _, geo)| (p.id, geo_to_unit(geo.latitude_mdeg, geo.longitude_mdeg)))
            .collect();
        let (surface_texture, province_id_texture, province_by_colour) =
            texture_asset_paths(record.key.as_str()).map_or_else(
                || (None, None, BTreeMap::new()),
                |(surface_path, id_path, manifest)| {
                    let manifest: ProvinceIdManifest =
                        serde_json::from_str(manifest.strip_prefix('\u{FEFF}').unwrap_or(manifest))
                            .expect("bundled province ID manifests must be valid JSON");
                    let province_by_colour = body_provinces
                        .iter()
                        .filter_map(|(province, _, _)| {
                            manifest
                                .provinces
                                .get(province.key.as_str())
                                .map(|colour| (*colour, province.id))
                        })
                        .collect();
                    (
                        Some(asset_server.load(surface_path)),
                        Some(asset_server.load(id_path)),
                        province_by_colour,
                    )
                },
            );
        // Blank texture; the refresh system paints it on first focus.
        let texture = images.add(blank_texture());
        let province_ids_for_shader = province_id_texture
            .clone()
            .unwrap_or_else(|| texture.clone());
        let material = globe_materials.add(GlobeSurfaceMaterial {
            base: StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(texture.clone()),
                unlit: true,
                ..Default::default()
            },
            extension: GlobeMaterial {
                selection: Vec4::ZERO,
                province_ids: province_ids_for_shader,
            },
        });
        commands.spawn((
            GlobeVisual {
                body: record.id,
                centroids,
                texture: texture.clone(),
                surface_texture,
                province_id_texture,
                province_by_colour,
                material: material.clone(),
            },
            Mesh3d(globe_mesh.clone()),
            // Lit evenly: this is a political map, not a lit body. Shading
            // it would darken the limb exactly where province colours and
            // their labels still need to be read. The system view keeps its
            // directional light — those materials are separate.
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::Hidden,
            Name::new(format!("globe:{}", name.0)),
        ));
    }
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

/// Places system-view bodies at fixed, static positions: each sits at its
/// orbit radius but at an angle fixed by its id, so the system reads as a
/// stable diagram rather than an animated orrery. A body whose visual doubles
/// as its zoomed-in view (the starbase) is instead centred and enlarged while
/// it is focused.
pub fn update_system_positions(
    view: Res<ViewState>,
    bodies: Query<&BodyRecord>,
    mut visuals: Query<(&SystemBodyVisual, &SystemScale, &mut Transform)>,
) {
    for (visual, scale, mut transform) in &mut visuals {
        let Some(record) = bodies.iter().find(|r| r.id == visual.body) else {
            continue;
        };
        if let (MapView::Body(body), Some(focus)) = (view.view, scale.focus)
            && body == visual.body
        {
            transform.translation = Vec3::ZERO;
            transform.scale = Vec3::splat(focus);
            continue;
        }
        let radius = body_orbit_display_radius(record);
        let position = if radius == 0.0 {
            Vec3::ZERO
        } else {
            let angle = visual.body.raw() as f32;
            Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius)
        };
        transform.translation = position;
        transform.scale = Vec3::splat(scale.orbit);
    }
}

/// Shows and hides visuals according to the active view.
pub fn apply_view_visibility(
    view: Res<ViewState>,
    mut system_visuals: Query<
        (&SystemBodyVisual, &SystemScale, &mut Visibility),
        Without<GlobeVisual>,
    >,
    mut globes: Query<(&GlobeVisual, &mut Visibility), Without<SystemBodyVisual>>,
) {
    if !view.is_changed() {
        return;
    }
    let system_active = view.view == MapView::System;
    for (visual, scale, mut visibility) in &mut system_visuals {
        // A body whose visual is also its zoomed-in view stays lit while it is
        // the focused body; every other marker hides outside the system view.
        let focused = view.view == MapView::Body(visual.body) && scale.focus.is_some();
        *visibility = if system_active || focused {
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
    let surface = match &globe.surface_texture {
        Some(handle) => {
            let Some(image) = images.get(handle) else {
                return;
            };
            Some(image)
        }
        None => None,
    };
    let colours: BTreeMap<ProvinceId, [u8; 3]> = readout
        .provinces
        .iter()
        .map(|(id, entry)| (*id, entry.colour))
        .collect();
    let data = bake_texture(&globe.centroids, &colours, surface);
    if let Some(mut image) = images.get_mut(&globe.texture) {
        image.data = Some(data);
    }
    bake.baked = Some((body, *mode, fingerprint));
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

/// Swaps every body's surface mesh when the projection changes.
///
/// One entity per body either way: the material, the baked texture and the
/// click observer all stay put, and only the geometry is exchanged.
pub fn apply_projection(
    view: Res<ViewState>,
    surfaces: Option<Res<SurfaceMeshes>>,
    mut globes: Query<&mut Mesh3d, With<GlobeVisual>>,
) {
    if !view.is_changed() {
        return;
    }
    let Some(surfaces) = surfaces else {
        return;
    };
    let wanted = surfaces.for_projection(view.projection);
    for mut mesh in &mut globes {
        if mesh.0 != wanted {
            mesh.0 = wanted.clone();
        }
    }
}
