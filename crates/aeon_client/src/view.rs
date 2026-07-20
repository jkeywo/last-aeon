//! Presentation view state: which map view is active and what is selected.
//!
//! Pure presentation — none of this is authoritative simulation state, and
//! none of it appears in snapshots.

use aeon_sim::{ArmyId, BodyId, CharacterId, OrgId, ProvinceId, ShipId};
use bevy::math::Vec3;
use bevy::prelude::Resource;

/// Radius of the globe in world units.
pub const GLOBE_RADIUS: f32 = 2.5;

/// Width of the flat map, chosen as the globe's equator unrolled so that
/// switching projection does not change how much ground a degree covers.
pub const FLAT_WIDTH: f32 = std::f32::consts::TAU * GLOBE_RADIUS;

/// Height of the flat map. Equirectangular, so exactly half the width.
pub const FLAT_HEIGHT: f32 = FLAT_WIDTH / 2.0;

/// Which map the player is looking at.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MapView {
    /// The local-system view: all bodies in their orbits.
    System,
    /// A single body's strategic view (rotatable globe with provinces).
    Body(BodyId),
}

/// How a body's surface is laid out.
///
/// Every position on a body is held as a unit direction, never as a world
/// point — so the projection is the only thing that knows where a place
/// ends up on screen, and switching it moves the map, the labels, the
/// selection pin and the click targets together.
///
/// The baked texture is projection-agnostic already: it is equirectangular
/// in latitude and longitude, which is exactly what both the sphere's UVs
/// and the flat quad's UVs ask for.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum MapProjection {
    /// A sphere. Truthful about a world, hides half of it.
    #[default]
    Globe,
    /// Equirectangular. Distorts the poles, shows everything at once.
    Flat,
}

impl MapProjection {
    /// The other one.
    pub fn toggled(self) -> Self {
        match self {
            MapProjection::Globe => MapProjection::Flat,
            MapProjection::Flat => MapProjection::Globe,
        }
    }

    /// A short label key for the toggle.
    pub fn label_key(self) -> &'static str {
        match self {
            MapProjection::Globe => "ui.projection.globe",
            MapProjection::Flat => "ui.projection.flat",
        }
    }

    /// Where a surface direction sits in world space.
    pub fn place(self, dir: Vec3) -> Vec3 {
        match self {
            MapProjection::Globe => dir * GLOBE_RADIUS,
            MapProjection::Flat => {
                let (lat, lon) = unit_to_radians(dir);
                Vec3::new(
                    lon / std::f32::consts::PI * (FLAT_WIDTH / 2.0),
                    lat / std::f32::consts::FRAC_PI_2 * (FLAT_HEIGHT / 2.0),
                    0.0,
                )
            }
        }
    }

    /// The surface direction at a world point, the inverse of `place`,
    /// used to turn a click into a place.
    pub fn direction_at(self, world: Vec3) -> Vec3 {
        match self {
            MapProjection::Globe => world.normalize_or_zero(),
            MapProjection::Flat => {
                let lon = world.x / (FLAT_WIDTH / 2.0) * std::f32::consts::PI;
                let lat = world.y / (FLAT_HEIGHT / 2.0) * std::f32::consts::FRAC_PI_2;
                Vec3::new(lat.cos() * lon.cos(), lat.sin(), -(lat.cos() * lon.sin()))
            }
        }
    }

    /// Whether a place is on the side of the body facing the camera.
    ///
    /// A globe hides half its surface and its far side must not be drawn
    /// over its near side; a flat map hides nothing, so the question does
    /// not arise.
    pub fn faces_camera(self, dir: Vec3, camera: Vec3) -> bool {
        match self {
            MapProjection::Globe => (camera - self.place(dir)).dot(dir) > 0.0,
            MapProjection::Flat => true,
        }
    }

    /// How far the selection pin stands off the surface.
    pub fn pin_offset(self, dir: Vec3) -> Vec3 {
        match self {
            MapProjection::Globe => dir * (GLOBE_RADIUS * 0.03),
            MapProjection::Flat => Vec3::Z * 0.12,
        }
    }
}

/// A unit direction as latitude and longitude in radians.
fn unit_to_radians(dir: Vec3) -> (f32, f32) {
    let dir = dir.normalize_or_zero();
    (dir.y.clamp(-1.0, 1.0).asin(), (-dir.z).atan2(dir.x))
}

/// What the player has selected for inspection.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Selection {
    /// A celestial body.
    Body(BodyId),
    /// A province.
    Province(ProvinceId),
    /// An organisation.
    Org(OrgId),
    /// A character.
    Character(CharacterId),
    /// An army. Selectable because an army is a thing that is given
    /// orders, and orders belong under the thing they are given to.
    Army(ArmyId),
    /// A ship, for the same reason.
    Ship(ShipId),
}

/// The active view and selection.
#[derive(Resource, Copy, Clone, Debug)]
pub struct ViewState {
    /// The active map view.
    pub view: MapView,
    /// How the focused body's surface is laid out.
    pub projection: MapProjection,
    /// The current inspection selection, if any.
    pub selected: Option<Selection>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            view: MapView::System,
            projection: MapProjection::default(),
            selected: None,
        }
    }
}

/// The global search box's current query.
#[derive(Resource, Clone, Debug, Default)]
pub struct SearchState {
    /// The text the player has typed; empty hides the results.
    pub query: String,
}

/// How the political globe colours provinces.
///
/// Each mode answers one strategic question. The political modes paint
/// house colours; the rest paint a graded scale, and always pair it with a
/// numeric value on the map so the answer never depends on colour alone.
#[derive(Resource, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum MapMode {
    /// Colour each province by the house that directly holds it.
    #[default]
    Holder,
    /// Colour each province by the great house at the top of its holder's
    /// liege chain.
    GreatHouse,
    /// Shade by how much of the map answers to the player, directly or
    /// through vassals.
    MyControl,
    /// Shade by provincial order, marking unrest.
    Order,
    /// Shade by what the province is worth.
    Wealth,
    /// Shade by the fighting strength standing in the province.
    Military,
    /// Shade by how its holder regards the player's house.
    PlayerRelations,
    /// Shade by each province's weight in the race for the paramountcy.
    ClaimPressure,
}

impl MapMode {
    /// Every mode, in display order.
    ///
    /// A slice rather than a fixed-size array so adding a mode never means
    /// editing a length.
    pub const ALL: &'static [MapMode] = &[
        MapMode::Holder,
        MapMode::GreatHouse,
        MapMode::MyControl,
        MapMode::Order,
        MapMode::Wealth,
        MapMode::Military,
        MapMode::PlayerRelations,
        MapMode::ClaimPressure,
    ];

    /// The stem of this mode's rows in the string table.
    fn text_stem(self) -> &'static str {
        match self {
            MapMode::Holder => "holder",
            MapMode::GreatHouse => "great-house",
            MapMode::MyControl => "my-control",
            MapMode::Order => "order",
            MapMode::Wealth => "wealth",
            MapMode::Military => "military",
            MapMode::PlayerRelations => "relations",
            MapMode::ClaimPressure => "claim",
        }
    }

    /// The key of a short label for the mode selector.
    pub fn label_key(self) -> String {
        format!("ui.map-mode.{}.label", self.text_stem())
    }

    /// The key of the strategic question this mode answers.
    pub fn description_key(self) -> String {
        format!("ui.map-mode.{}.description", self.text_stem())
    }

    /// Whether this mode paints house colours rather than a graded scale.
    pub fn is_political(self) -> bool {
        matches!(self, MapMode::Holder | MapMode::GreatHouse)
    }
}

/// Converts a latitude/longitude in millidegrees to a unit vector on the
/// globe (Y up, longitude zero on +X, east positive toward -Z).
pub fn geo_to_unit(latitude_mdeg: i32, longitude_mdeg: i32) -> bevy::math::Vec3 {
    let lat = (latitude_mdeg as f32 / 1000.0).to_radians();
    let lon = (longitude_mdeg as f32 / 1000.0).to_radians();
    bevy::math::Vec3::new(lat.cos() * lon.cos(), lat.sin(), -(lat.cos() * lon.sin()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_reaches_the_bar_exactly_once() {
        // The icon bar draws `ALL` and nothing else, so a mode missing from
        // it is a mode the player cannot select, and a duplicated one is a
        // button that does nothing new. Neither shows up as a compile error.
        let mut seen: Vec<MapMode> = Vec::new();
        for mode in MapMode::ALL {
            assert!(!seen.contains(mode), "{mode:?} is listed twice");
            seen.push(*mode);
        }
    }

    #[test]
    fn every_mode_has_a_label_and_a_description_to_show() {
        // A mode whose rows are missing would hover blank, which is not a
        // compile error — but the table can be asked directly.
        let strings = aeon_sim::TextDb::embedded();
        for mode in MapMode::ALL {
            assert!(
                strings.0.get(&mode.label_key()).is_some(),
                "{mode:?} has no label row"
            );
            assert!(
                strings.0.get(&mode.description_key()).is_some(),
                "{mode:?} has no description row, so its button would hover blank"
            );
        }
    }

    /// Both projections must be able to answer "where is this" and "what
    /// is here" consistently, or a click lands on a different province
    /// from the one under the pointer.
    #[test]
    fn placing_a_direction_and_reading_it_back_returns_the_same_place() {
        for projection in [MapProjection::Globe, MapProjection::Flat] {
            for (lat, lon) in [
                (0, 0),
                (12_345, -67_890),
                (-45_000, 179_000),
                (60_000, 90_000),
                (-80_000, -10_000),
            ] {
                let dir = geo_to_unit(lat, lon);
                let world = projection.place(dir);
                let back = projection.direction_at(world);
                assert!(dir.distance(back) < 1e-4, "{projection:?} lost {lat},{lon}");
            }
        }
    }

    #[test]
    fn the_flat_map_fills_exactly_its_declared_extent() {
        // The quad is built to these dimensions and its texture stretched
        // across them, so a projection placing a pole anywhere but the top
        // edge would slide every label off the map it belongs to.
        let north = MapProjection::Flat.place(geo_to_unit(90_000, 0));
        let south = MapProjection::Flat.place(geo_to_unit(-90_000, 0));
        assert!(
            (north.y - FLAT_HEIGHT / 2.0).abs() < 1e-3,
            "north pole on the top edge"
        );
        assert!(
            (south.y + FLAT_HEIGHT / 2.0).abs() < 1e-3,
            "south pole on the bottom edge"
        );

        let east = MapProjection::Flat.place(geo_to_unit(0, 180_000));
        assert!(
            (east.x.abs() - FLAT_WIDTH / 2.0).abs() < 1e-3,
            "the date line is an edge"
        );
        assert!(north.z.abs() < 1e-6, "the flat map is flat");
    }

    #[test]
    fn only_the_globe_hides_its_far_side() {
        // Longitude zero is +X, so a camera out along +X sees it and not
        // the date line behind the body.
        let camera = Vec3::new(20.0, 0.0, 0.0);
        let towards = geo_to_unit(0, 0);
        let away = geo_to_unit(0, 180_000);

        assert!(MapProjection::Globe.faces_camera(towards, camera));
        assert!(
            !MapProjection::Globe.faces_camera(away, camera),
            "the far side of a globe must not draw over the near side"
        );
        // A flat map has no far side, so nothing is ever culled from it.
        assert!(MapProjection::Flat.faces_camera(towards, camera));
        assert!(MapProjection::Flat.faces_camera(away, camera));
    }

    #[test]
    fn geo_conversion_hits_the_cardinal_points() {
        let north = geo_to_unit(90_000, 0);
        assert!((north.y - 1.0).abs() < 1e-5);

        let equator_prime = geo_to_unit(0, 0);
        assert!((equator_prime.x - 1.0).abs() < 1e-5);

        let east_90 = geo_to_unit(0, 90_000);
        assert!((east_90.z - (-1.0)).abs() < 1e-5);

        for (lat, lon) in [(12_345, -67_890), (-45_000, 179_000)] {
            let v = geo_to_unit(lat, lon);
            assert!((v.length() - 1.0).abs() < 1e-5);
        }
    }
}
