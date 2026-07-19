//! What the panels read, and what they write.
//!
//! Bevy caps a system at sixteen parameters, and the panels legitimately
//! need more than that — so the queries and resources they use are bundled
//! here into a few named groups. The grouping is not only an arithmetic
//! convenience: [`PanelData`] is everything the interface *reads*, and
//! [`JobUi`] and [`MapUi`] are the small amounts of state it *writes*, so
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
use aeon_sim::jobs::CharacterCondition;
use aeon_sim::map::{BodyRecord, DisplayName, GeoPosition, ProvinceRecord};
use aeon_sim::obligations::Obligations;
use aeon_sim::order::ProvincialOrder;
use aeon_sim::politics::{CharacterSkills, CharacterTraits, Lineage, OpinionLedger};
use aeon_sim::presence::CharacterLocation;
use aeon_sim::{ActiveJob, CharacterId, CharacterRecord, OrgId, OrgRecord, TitleRecord};

use crate::forecast_view::{AvailabilityView, ForecastCache};
use crate::jobs_ui::JobForm;
use crate::map_modes::MapReadout;
use crate::ui::dock::DockState;
use crate::ui::picker::PickerState;
use crate::ui::theme::UiTheme;
use crate::view::MapMode;

/// The two resources an in-progress action writes to.
///
/// They belong together: the form holds the choice being made, and the
/// picker is how one of its slots gets filled in.
#[derive(SystemParam)]
pub struct JobUi<'w> {
    pub form: ResMut<'w, JobForm>,
    pub picker: ResMut<'w, PickerState>,
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
    pub conditions: Query<'w, 's, &'static CharacterCondition>,
    pub active_jobs: Query<'w, 's, &'static ActiveJob>,
    pub order: Query<'w, 's, (&'static ProvinceRecord, &'static ProvincialOrder)>,
    pub obligations: Option<Res<'w, Obligations>>,
    pub availability: Res<'w, AvailabilityView>,
    pub province_records: Query<'w, 's, &'static ProvinceRecord>,
    pub cache: Res<'w, ForecastCache>,
    pub readout: Res<'w, MapReadout>,
    pub theme: Res<'w, UiTheme>,
}
