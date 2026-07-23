//! Space background shared by every 3D view.

use bevy::light::Skybox;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureViewDescriptor, TextureViewDimension};

const SKYBOX_PATH: &str = "skybox/phoenix_space_cubemap.png";

/// The vertically stacked source image and whether it has been converted to
/// the cubemap texture view that Bevy's skybox renderer requires.
#[derive(Resource)]
pub struct SpaceSkybox {
    image: Handle<Image>,
    prepared: bool,
}

impl FromWorld for SpaceSkybox {
    fn from_world(world: &mut World) -> Self {
        Self {
            image: world.resource::<AssetServer>().load(SKYBOX_PATH),
            prepared: false,
        }
    }
}

/// Loads the Phoenix six-face starfield and converts it once it is available.
pub struct SpaceSkyboxPlugin;

impl Plugin for SpaceSkyboxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpaceSkybox>()
            .add_systems(Update, prepare_skybox);
    }
}

/// A skybox component for the main 3D camera.
pub fn space_skybox(skybox: &SpaceSkybox) -> Skybox {
    Skybox {
        image: Some(skybox.image.clone()),
        brightness: 450.0,
        ..default()
    }
}

fn prepare_skybox(
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut skybox: ResMut<SpaceSkybox>,
    mut cameras: Query<&mut Skybox>,
) {
    if skybox.prepared || !asset_server.load_state(&skybox.image).is_loaded() {
        return;
    }
    let Some(mut image) = images.get_mut(&skybox.image) else {
        return;
    };
    let layers = image.height() / image.width();
    if layers != 6 {
        bevy::log::error!(
            "skybox must be a vertical six-face cubemap, got {}x{}",
            image.width(),
            image.height()
        );
        skybox.prepared = true;
        return;
    }
    if let Err(error) = image.reinterpret_stacked_2d_as_array(layers) {
        bevy::log::error!("could not prepare skybox cubemap: {error}");
        skybox.prepared = true;
        return;
    }
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    for mut camera in &mut cameras {
        camera.image = Some(skybox.image.clone());
    }
    skybox.prepared = true;
}
