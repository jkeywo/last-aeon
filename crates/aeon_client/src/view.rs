//! Presentation view state: which map view is active and what is selected.
//!
//! Pure presentation — none of this is authoritative simulation state, and
//! none of it appears in snapshots.

use aeon_sim::{BodyId, CharacterId, OrgId, ProvinceId};
use bevy::prelude::Resource;

/// Which map the player is looking at.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MapView {
    /// The local-system view: all bodies in their orbits.
    System,
    /// A single body's strategic view (rotatable globe with provinces).
    Body(BodyId),
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
}

/// The active view and selection.
#[derive(Resource, Copy, Clone, Debug)]
pub struct ViewState {
    /// The active map view.
    pub view: MapView,
    /// The current inspection selection, if any.
    pub selected: Option<Selection>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            view: MapView::System,
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
#[derive(Resource, Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum MapMode {
    /// Colour each province by the house that directly holds it.
    #[default]
    Holder,
    /// Colour each province by the great house at the top of its holder's
    /// liege chain.
    GreatHouse,
}

impl MapMode {
    /// The next mode in the toggle cycle.
    pub fn toggled(self) -> Self {
        match self {
            MapMode::Holder => MapMode::GreatHouse,
            MapMode::GreatHouse => MapMode::Holder,
        }
    }

    /// A short label for the toggle button.
    pub fn label(self) -> &'static str {
        match self {
            MapMode::Holder => "Map: Holder",
            MapMode::GreatHouse => "Map: Great House",
        }
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
