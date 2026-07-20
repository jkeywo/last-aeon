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
