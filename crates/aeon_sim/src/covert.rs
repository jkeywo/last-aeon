//! Covert provenance: the read behind every simulation-side surface
//! that must not name who is behind authored covert work, and the durable
//! record of what investigation has found out.
//!
//! Covertness itself is derived from content — a `covert: true` flag on an
//! authored plan, goal, or assignment. What changes is who that work
//! confides in. Before anyone has found the owner out, an authored covert
//! line is written for its owning organisation alone; once a house has
//! discovered the owner through an ordinary investigation, later lines are
//! written for the owner *and* every house that knows, and the ordinary
//! player surfaces of those houses may name the hand.
//!
//! Two rules keep this honest.
//!
//! * **A line's audience is stamped once, at write time, and never
//!   re-widened.** Discovery therefore reveals by writing *new* history —
//!   the revelation line, the result lines the operation goes on to
//!   produce, and the live projection — never by reopening lines already
//!   written. [`audience`] is the write-time decision; it reads the
//!   exposure record but only ever affects the line being written now.
//! * **Discovery is per knower.** The record names a culprit *and* the
//!   house that found them out, so a third house learns nothing from
//!   someone else's investigation.
//!
//! Spectators and replay verification see everything at every stage,
//! because [`LogAudience::visible_to`] treats the absent player as
//! omniscient — secrecy is an audience, not a second history.
//!
//! [`Exposure`] is snapshotted campaign state: it must outlive the
//! Situation lifecycle that discovered it, so it cannot live in
//! [`crate::situations::SituationState`], whose per-occurrence bookkeeping
//! is pruned when a lifecycle ends.

use std::collections::BTreeSet;

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use bevy::app::App;
use bevy::prelude::{Resource, World};
use serde::{Deserialize, Serialize};

use crate::assignments::{ActiveAssignment, AssignmentTarget, AssignmentsIndex, LogAudience};
use crate::ids::{OrgId, ProvinceId};
use crate::situations::SituationOccurrence;
use crate::state::ContentDb;

/// One discovered attribution: a house proved whose hand moved against it.
///
/// The occurrence is kept so the discovery stays tied to the exact
/// Situation activation that produced it — durable evidence, long after
/// that lifecycle has ended and its card become a resolution notice.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ExposureRecord {
    /// The organisation whose covert work stands proved.
    pub culprit: OrgId,
    /// The organisation that found them out.
    pub knower: OrgId,
    /// The exact Situation activation the discovery was made through.
    pub occurrence: SituationOccurrence,
    /// The settled day it was discovered.
    pub discovered: GameDate,
}

/// Durable record of every covert attribution investigation has proved.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exposure {
    /// Discoveries in deterministic record order. At most one per
    /// culprit-and-knower pair: the first discovery stands, so repeating an
    /// investigation cannot rewrite when the hand was proved.
    pub records: BTreeSet<ExposureRecord>,
}

impl Exposure {
    /// Whether `knower` has proved `culprit` behind its covert work.
    pub fn knows(&self, knower: OrgId, culprit: OrgId) -> bool {
        self.records
            .iter()
            .any(|record| record.knower == knower && record.culprit == culprit)
    }

    /// Every house that has proved `culprit`, in stable ID order.
    pub fn knowers_of(&self, culprit: OrgId) -> BTreeSet<OrgId> {
        self.records
            .iter()
            .filter(|record| record.culprit == culprit)
            .map(|record| record.knower)
            .collect()
    }
}

/// Whether `viewer` may name `culprit` as the hand behind its covert work.
///
/// The one seam every surface asks. A spectator (no player organisation)
/// always may — spectator omniscience is an observation rule, not
/// knowledge. The culprit always may, of itself. Everyone else must have
/// found out, which today means an ordinary investigation recorded a
/// durable [`ExposureRecord`].
pub fn is_exposed(world: &World, culprit: OrgId, viewer: Option<OrgId>) -> bool {
    exposed_to(world.get_resource::<Exposure>(), culprit, viewer)
}

/// [`is_exposed`] without a [`World`], for surfaces that hold the record
/// itself rather than the simulation — chiefly the client, whose panel
/// context carries the projected [`Exposure`] resource and no world. One
/// rule, two callers, so the client can never drift from the simulation.
pub fn exposed_to(exposure: Option<&Exposure>, culprit: OrgId, viewer: Option<OrgId>) -> bool {
    let Some(viewer) = viewer else {
        return true;
    };
    viewer == culprit || exposure.is_some_and(|exposure| exposure.knows(viewer, culprit))
}

/// Whether an authored plan is covert.
pub fn plan_is_covert(world: &World, def: &ContentKey) -> bool {
    world
        .get_resource::<ContentDb>()
        .and_then(|content| content.0.plans.get(def).map(|plan| plan.covert))
        .unwrap_or(false)
}

/// Whether an authored goal is covert.
pub fn goal_is_covert(world: &World, def: &ContentKey) -> bool {
    world
        .get_resource::<ContentDb>()
        .and_then(|content| content.0.goals.get(def).map(|goal| goal.covert))
        .unwrap_or(false)
}

/// Whether an authored assignment is covert.
pub fn assignment_is_covert(world: &World, def: &ContentKey) -> bool {
    world
        .get_resource::<ContentDb>()
        .and_then(|content| {
            content
                .0
                .assignments
                .get(def)
                .map(|assignment| assignment.covert)
        })
        .unwrap_or(false)
}

/// Whether somebody other than `viewer` has covert work running against
/// `province` that `viewer` has not yet proved: a live covert
/// province-aimed assignment owned by another organisation, for whose
/// owner `viewer` holds no [`ExposureRecord`].
///
/// The simulation-side twin of the Unquiet Holdings card's read, so an
/// assignment gated on `target_under_covert_work` is offered exactly while
/// the card offers its own investigate action, and withdraws with it. The
/// card is raised on three facts — live, covert, aimed at this province,
/// owned by somebody other than the holder — and drops the action once
/// the holder has proved that owner (`unquiet_hand_known`, which reads the
/// same [`Exposure`] records through the world view's `exposures`). This
/// predicate is those same facts judged against the same authored flag
/// and the same record, with `viewer` standing where the card's bound
/// holder stands. Keeping the two in step is what stops an ordinary
/// enquiry starting where no live card would launch it: an enquiry into a
/// hand already proved has nothing left to prove, and were it accepted it
/// would start without the lifecycle its effect reads. A house's own
/// covert work on its own ground raises no alarm, so it does not count.
///
/// A pure read of the live assignments and the exposure record; it rolls
/// nothing.
pub fn under_covert_work(world: &World, viewer: OrgId, province: ProvinceId) -> bool {
    let Some(index) = world.get_resource::<AssignmentsIndex>() else {
        return false;
    };
    let exposure = world.get_resource::<Exposure>();
    index.assignments.values().any(|entity| {
        world.get::<ActiveAssignment>(*entity).is_some_and(|work| {
            work.owner != viewer
                && work.target == AssignmentTarget::Province(province)
                && assignment_is_covert(world, &work.def)
                && !exposure.is_some_and(|exposure| exposure.knows(viewer, work.owner))
        })
    })
}

/// The audience covert work confides in *at this moment*: its owner, plus
/// every house that has already proved the owner behind it.
///
/// This is a write-time decision. A line stamped with it keeps that
/// audience forever, so a later discovery never reopens earlier history —
/// it only widens the lines written after it. Spectators and replay still
/// read every line, because [`LogAudience::visible_to`] with no player
/// admits everything.
pub fn audience(world: &World, owner: OrgId) -> LogAudience {
    let mut organisations = world
        .get_resource::<Exposure>()
        .map(|exposure| exposure.knowers_of(owner))
        .unwrap_or_default();
    organisations.insert(owner);
    LogAudience::organisations(organisations)
}

/// Records that `knower` has proved `culprit` behind its covert work.
///
/// Idempotent: the first discovery of a pair stands.
pub fn expose(
    world: &mut World,
    culprit: OrgId,
    knower: OrgId,
    occurrence: SituationOccurrence,
    discovered: GameDate,
) {
    if knower == culprit {
        return;
    }
    // A world that never installed the resource still records its
    // discoveries: exposure is campaign state, not an optional feature.
    if world.get_resource::<Exposure>().is_none() {
        world.insert_resource(Exposure::default());
    }
    let mut exposure = world.resource_mut::<Exposure>();
    if exposure.knows(knower, culprit) {
        return;
    }
    exposure.records.insert(ExposureRecord {
        culprit,
        knower,
        occurrence,
        discovered,
    });
}

/// Whether a client surface may name a covert plan to this viewer.
///
/// The client owns no visibility rule of its own: the inspector's pursuing
/// line asks this, over the projected [`Exposure`] resource, exactly as the
/// simulation-side surfaces ask [`is_exposed`]. Ordinary plans are named
/// openly — AI reasons are visible. A covert plan is named to a spectator
/// (no player organisation), to the house pursuing it, and to any house
/// that has proved it; to every other ordinary player the line is omitted
/// entirely, so no secondary interface leaks the culprit.
pub fn plan_named_to_viewer(
    covert: bool,
    exposure: Option<&Exposure>,
    owner: Option<OrgId>,
    viewer: Option<OrgId>,
) -> bool {
    if !covert {
        return true;
    }
    match owner {
        Some(owner) => exposed_to(exposure, owner, viewer),
        // A pursuer belonging to no organisation has no owner to prove;
        // only the spectator reads that line.
        None => viewer.is_none(),
    }
}

/// Captures exposure persistence for the campaign snapshot.
pub fn capture(world: &World) -> Exposure {
    world
        .get_resource::<Exposure>()
        .cloned()
        .unwrap_or_default()
}

/// Restores exposure persistence before the first post-restore evaluation.
pub fn restore(world: &mut World, state: &Exposure) {
    world.insert_resource(state.clone());
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<Exposure>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::situations::{SituationInstanceKey, SituationSource};
    use aeon_data::model::SituationSubjectKind;
    use std::collections::BTreeMap;

    fn occurrence() -> SituationOccurrence {
        SituationOccurrence {
            situation: SituationInstanceKey {
                definition: ContentKey::new("unquiet-holdings").unwrap(),
                source: SituationSource {
                    kind: SituationSubjectKind::Scenario,
                    key: ContentKey::new("ashkarr-succession").unwrap(),
                    id: None,
                },
                bindings: BTreeMap::new(),
            },
            activated: GameDate::from_days(180),
        }
    }

    fn record(culprit: u64, knower: u64) -> ExposureRecord {
        ExposureRecord {
            culprit: OrgId::from_raw(culprit).unwrap(),
            knower: OrgId::from_raw(knower).unwrap(),
            occurrence: occurrence(),
            discovered: GameDate::from_days(200),
        }
    }

    #[test]
    fn discovery_is_per_knower_and_never_shared_with_bystanders() {
        let culprit = OrgId::from_raw(7).unwrap();
        let knower = OrgId::from_raw(4).unwrap();
        let bystander = OrgId::from_raw(9).unwrap();
        let mut exposure = Exposure::default();
        assert!(!exposure.knows(knower, culprit));
        exposure.records.insert(record(7, 4));
        assert!(exposure.knows(knower, culprit));
        assert!(
            !exposure.knows(bystander, culprit),
            "one house's investigation teaches no other house"
        );
        assert_eq!(exposure.knowers_of(culprit), BTreeSet::from([knower]));
    }

    #[test]
    fn the_client_predicate_names_covert_plans_only_to_those_who_may_know() {
        let vantar = OrgId::from_raw(7).unwrap();
        let harrow = OrgId::from_raw(4).unwrap();
        let draksha = OrgId::from_raw(9).unwrap();
        let mut exposure = Exposure::default();

        // Ordinary plans stay openly named to everyone.
        assert!(plan_named_to_viewer(
            false,
            Some(&exposure),
            Some(vantar),
            Some(harrow)
        ));
        assert!(plan_named_to_viewer(false, None, Some(vantar), None));

        // Before discovery a covert plan is named only to the spectator and
        // to the house pursuing it.
        assert!(!plan_named_to_viewer(
            true,
            Some(&exposure),
            Some(vantar),
            Some(harrow)
        ));
        assert!(plan_named_to_viewer(
            true,
            Some(&exposure),
            Some(vantar),
            None
        ));
        assert!(plan_named_to_viewer(
            true,
            Some(&exposure),
            Some(vantar),
            Some(vantar)
        ));

        // After Harrow proves Vantar, Harrow reads the pursuing line and
        // an uninvolved house still does not.
        exposure.records.insert(record(7, 4));
        assert!(plan_named_to_viewer(
            true,
            Some(&exposure),
            Some(vantar),
            Some(harrow)
        ));
        assert!(!plan_named_to_viewer(
            true,
            Some(&exposure),
            Some(vantar),
            Some(draksha)
        ));

        // A campaign with no exposure state projected yet reads as no
        // discovery, never as disclosure.
        assert!(!plan_named_to_viewer(
            true,
            None,
            Some(vantar),
            Some(harrow)
        ));
        // A plan whose pursuer belongs to no organisation names nobody.
        assert!(!plan_named_to_viewer(
            true,
            Some(&exposure),
            None,
            Some(harrow)
        ));
    }

    #[test]
    fn a_discovery_is_recorded_once_however_often_it_is_proved() {
        let mut exposure = Exposure::default();
        exposure.records.insert(record(7, 4));
        assert_eq!(exposure.records.len(), 1);
        assert!(exposure.knows(OrgId::from_raw(4).unwrap(), OrgId::from_raw(7).unwrap()));
    }
}
