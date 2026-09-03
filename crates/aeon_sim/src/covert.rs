//! Covert provenance: the read behind every simulation-side surface
//! that must not name who is behind authored covert work.
//!
//! Covertness is derived from content — a `covert: true` flag on an
//! authored plan, goal, or assignment — never from new snapshot state.
//! Before exposure, the ordinary player surfaces narrow such work's
//! provenance to the owning organisation: log lines carry an owner-only
//! [`LogAudience`], and no rumour is whispered. The client's inspector
//! also names no covert plan, but it gates on the authored flag rather
//! than asking here — its panel context holds no [`World`] — so issue
//! #18 must rewire that surface when it projects exposure to the client.
//! Spectators and replay verification see everything, because
//! [`LogAudience::visible_to`] treats the absent player as omniscient —
//! secrecy is an audience, not a second history.
//!
//! [`is_exposed`] is the deliberate seam for the investigation issue
//! (#18): today nothing is ever exposed, so it returns `false`; when
//! investigation lands it will read authoritative exposure state, and
//! every surface that asks through this module inherits the answer.

use aeon_data::ContentKey;
use bevy::prelude::World;

use crate::assignments::LogAudience;
use crate::ids::OrgId;
use crate::state::ContentDb;

/// Whether covert work by `_owner` has been exposed.
///
/// The investigation hook: deliberately a stub returning `false` until
/// issue #18 gives exposure authoritative state. Every simulation-side
/// covertness read goes through this one seam, so exposure flips those
/// together; the client's inspector gate reads the authored flag and
/// must be rewired to ask exposure state when #18 lands.
pub fn is_exposed(_world: &World, _owner: OrgId) -> bool {
    false
}

/// Whether an authored plan is covert and not yet exposed.
pub fn plan_is_covert(world: &World, def: &ContentKey, owner: OrgId) -> bool {
    let authored = world
        .get_resource::<ContentDb>()
        .and_then(|content| content.0.plans.get(def).map(|plan| plan.covert))
        .unwrap_or(false);
    authored && !is_exposed(world, owner)
}

/// Whether an authored goal is covert and not yet exposed.
pub fn goal_is_covert(world: &World, def: &ContentKey, owner: OrgId) -> bool {
    let authored = world
        .get_resource::<ContentDb>()
        .and_then(|content| content.0.goals.get(def).map(|goal| goal.covert))
        .unwrap_or(false);
    authored && !is_exposed(world, owner)
}

/// Whether an authored assignment is covert and not yet exposed.
pub fn assignment_is_covert(world: &World, def: &ContentKey, owner: OrgId) -> bool {
    let authored = world
        .get_resource::<ContentDb>()
        .and_then(|content| {
            content
                .0
                .assignments
                .get(def)
                .map(|assignment| assignment.covert)
        })
        .unwrap_or(false);
    authored && !is_exposed(world, owner)
}

/// The audience covert work confides in: its owner alone.
///
/// Spectators and replay still read the line — [`LogAudience::visible_to`]
/// with no player admits everything — so authoritative provenance is kept,
/// not rewritten.
pub fn owner_only(owner: OrgId) -> LogAudience {
    LogAudience::organisations([owner])
}
