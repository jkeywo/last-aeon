//! What the panels read, and what they write.
//!
//! Bevy caps a system at sixteen parameters, and the panels legitimately
//! need more than that — so the queries and resources they use are bundled
//! here into a few named groups. The grouping is not only an arithmetic
//! convenience: [`PanelData`] is everything the interface *reads*, and
//! [`AssignmentUi`] and [`MapUi`] are the small amounts of state it *writes*, so
//! the split says which is which.
//!
//! These live apart from the panels themselves because every panel module
//! needs them, and a shared type that lives inside one of its consumers
//! makes that consumer impossible to extract from.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use std::collections::BTreeMap;

use aeon_sim::economy::OrgResources;
use aeon_sim::forces::{ArmyRecord, ShipRecord};
use aeon_sim::map::{BodyRecord, DisplayName, GeoPosition, ProvinceRecord};
use aeon_sim::obligations::Obligations;
use aeon_sim::order::ProvincialOrder;
use aeon_sim::politics::{CharacterSkills, CharacterTraits, Lineage, OpinionLedger};
use aeon_sim::presence::CharacterLocation;
use aeon_sim::{
    ActiveAssignment, CharacterId, CharacterRecord, OrgId, OrgRecord, TextDb, TitleRecord,
};

use crate::assignment_ui::AssignmentForm;
use crate::forecast_view::{AvailabilityView, ForecastCache};
use crate::map_modes::MapReadout;
use crate::offer_view::OfferView;
use crate::ui::assignment_popup::AssignmentPopup;
use crate::ui::dock::DockState;
use crate::ui::theme::UiTheme;
use crate::view::MapMode;

/// The two resources the panels write while an action is being started.
///
/// The picker is not here: it belongs to the popup that contains it, and
/// the panels only ever open that.
#[derive(SystemParam)]
pub struct AssignmentUi<'w> {
    pub form: ResMut<'w, AssignmentForm>,
    pub popup: ResMut<'w, AssignmentPopup>,
}

/// How the player is looking at things: what the map shows, and where
/// the panels are.
#[derive(SystemParam)]
pub struct MapUi<'w> {
    pub mode: ResMut<'w, MapMode>,
    pub dock: ResMut<'w, DockState>,
}

/// Character lookup shared across the panel helpers.
pub type CharMap<'a> = BTreeMap<CharacterId, CharacterParts<'a>>;
/// Organisation lookup shared across the panel helpers.
pub type OrgMap<'a> = BTreeMap<OrgId, (&'a OrgRecord, Option<&'a OrgResources>)>;

/// The components every character row carries.
pub type CharacterQuery = (
    &'static CharacterRecord,
    &'static CharacterSkills,
    &'static CharacterTraits,
    &'static Lineage,
    &'static OpinionLedger,
);

/// One character row, borrowed.
pub type CharacterParts<'a> = (
    &'a CharacterRecord,
    &'a CharacterSkills,
    &'a CharacterTraits,
    &'a Lineage,
    &'a OpinionLedger,
);

/// Every world query the panels read, bundled to stay within system
/// parameter limits.
#[derive(SystemParam)]
pub struct PanelData<'w, 's> {
    pub bodies: Query<'w, 's, (&'static BodyRecord, &'static DisplayName)>,
    pub provinces: Query<
        'w,
        's,
        (
            &'static ProvinceRecord,
            &'static DisplayName,
            &'static GeoPosition,
        ),
    >,
    pub orgs: Query<'w, 's, (&'static OrgRecord, Option<&'static OrgResources>)>,
    pub characters: Query<'w, 's, CharacterQuery>,
    pub locations: Query<'w, 's, &'static CharacterLocation>,
    pub titles: Query<'w, 's, &'static TitleRecord>,
    pub ships: Query<'w, 's, &'static ShipRecord>,
    pub armies: Query<'w, 's, &'static ArmyRecord>,
    pub active_assignments: Query<'w, 's, &'static ActiveAssignment>,
    pub order: Query<'w, 's, (&'static ProvinceRecord, &'static ProvincialOrder)>,
    pub obligations: Option<Res<'w, Obligations>>,
    pub availability: Res<'w, AvailabilityView>,
    pub offers: Res<'w, OfferView>,
    pub province_records: Query<'w, 's, &'static ProvinceRecord>,
    pub cache: Res<'w, ForecastCache>,
    pub readout: Res<'w, MapReadout>,
    pub theme: Res<'w, UiTheme>,
    /// Every string the panels draw. Absent until a campaign starts.
    pub strings: Option<Res<'w, TextDb>>,
    /// Plans autonomous characters are pursuing. Absent until a
    /// campaign starts.
    pub plans: Option<Res<'w, aeon_sim::plans::Plans>>,
}
