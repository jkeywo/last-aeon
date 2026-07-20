//! What may be offered against whatever is currently selected.
//!
//! The requirements on an assignment are relational — whose the province
//! is, whether anyone owes you a favour, whether an alarm is actually
//! sounding — and answering them needs the whole world. Panels do not have
//! the world; they have queries. So one exclusive system asks the
//! simulation once per selection and publishes the answer, exactly as
//! `AvailabilityView` does for "can this character act".
//!
//! The interface therefore never decides for itself what to offer. Before
//! this existed it did, and the client had a content key spelled out in
//! its source to decide whether the Consul could be petitioned.

use std::collections::BTreeSet;

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use aeon_sim::state::ContentDb;
use aeon_sim::{AssignmentTarget, CampaignClock, PlayerHouse, target_allowed};
use bevy::prelude::*;

use crate::view::{Selection, ViewState};

/// Which assignments the current selection is a legal target for.
#[derive(Resource, Default)]
pub struct OfferView {
    key: Option<(Option<Selection>, GameDate)>,
    offerable: BTreeSet<ContentKey>,
}

impl OfferView {
    /// Whether this assignment may be aimed at the current selection.
    pub fn allows(&self, assignment: &ContentKey) -> bool {
        self.offerable.contains(assignment)
    }
}

/// Recomputes the offerable set when the selection or the day changes.
pub fn refresh_offers(world: &mut World) {
    let (Some(date), Some(org)) = (
        world
            .get_resource::<CampaignClock>()
            .map(|clock| clock.date),
        world
            .get_resource::<PlayerHouse>()
            .and_then(|player| player.0),
    ) else {
        return;
    };
    let selected = world
        .get_resource::<ViewState>()
        .and_then(|view| view.selected);
    let key = (selected, date);
    if world
        .get_resource::<OfferView>()
        .is_some_and(|view| view.key == Some(key))
    {
        return;
    }

    // The target each assignment would act on, given what is selected.
    let target_for = |selected: Option<Selection>| -> Option<AssignmentTarget> {
        match selected {
            Some(Selection::Character(id)) => Some(AssignmentTarget::Character(id)),
            Some(Selection::Org(id)) => Some(AssignmentTarget::Org(id)),
            Some(Selection::Province(id)) => Some(AssignmentTarget::Province(id)),
            // A force's own orders: the target is the force itself, which
            // is what lets `army_present` be answered before a
            // destination has been chosen.
            Some(Selection::Army(id)) => Some(AssignmentTarget::OwnArmy(id)),
            _ => None,
        }
    };

    let keys: Vec<ContentKey> = world
        .get_resource::<ContentDb>()
        .map(|content| content.0.assignments.keys().cloned().collect())
        .unwrap_or_default();

    let mut offerable = BTreeSet::new();
    if let Some(target) = target_for(selected) {
        for assignment in keys {
            if target_allowed(world, &assignment, org, target) {
                offerable.insert(assignment);
            }
        }
    }

    let mut view = world.resource_mut::<OfferView>();
    view.key = Some(key);
    view.offerable = offerable;
}
