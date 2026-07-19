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

    /// The next mode in the cycle.
    pub fn toggled(self) -> Self {
        let index = MapMode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        MapMode::ALL[(index + 1) % MapMode::ALL.len()]
    }

    /// A short label for the mode selector.
    pub fn label(self) -> &'static str {
        match self {
            MapMode::Holder => "Holder",
            MapMode::GreatHouse => "Great House",
            MapMode::MyControl => "My Realm",
            MapMode::Order => "Order",
            MapMode::Wealth => "Wealth",
            MapMode::Military => "Military",
            MapMode::PlayerRelations => "Relations",
            MapMode::ClaimPressure => "Claim",
        }
    }

    /// The strategic question this mode answers.
    pub fn description(self) -> &'static str {
        match self {
            MapMode::Holder => "Who directly holds each province.",
            MapMode::GreatHouse => {
                "Which great house each province ultimately answers to, \
                 following its holder's liege chain."
            }
            MapMode::MyControl => {
                "What answers to you: ground you hold yourself, ground held \
                 through your vassals, and ground that is not yours at all."
            }
            MapMode::Order => {
                "How governable each province is. Provinces in unrest are \
                 marked, and will throw off their ruler if left."
            }
            MapMode::Wealth => "What each province is worth in monthly output.",
            MapMode::Military => "Where the fighting strength stands.",
            MapMode::PlayerRelations => "How each province's holder regards your house.",
            MapMode::ClaimPressure => {
                "Each province's weight in the race for the Paramountcy: who \
                 leads, who is contesting, and who is out of it."
            }
        }
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
