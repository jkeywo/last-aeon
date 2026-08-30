//! Formal wars: occurrence-stable conflicts between frozen political branches.
//!
//! A war records who joined each side when it was declared. Later changes to
//! the liege tree do not silently rewrite that membership; the sole explicit
//! exception is a liege adopting a side that contains one of its vassals.

use std::collections::{BTreeMap, BTreeSet};

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use bevy::app::App;
use bevy::prelude::{IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::ids::{OrgId, WarId};
use crate::state::CampaignIds;

/// One of a war's two stable sides.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarSideId {
    /// The branch that initiated the war.
    Attacker,
    /// The branch against which it was declared.
    Defender,
}

impl WarSideId {
    /// Both sides in deterministic order.
    pub const ALL: [Self; 2] = [Self::Attacker, Self::Defender];

    fn index(self) -> usize {
        match self {
            Self::Attacker => 0,
            Self::Defender => 1,
        }
    }

    /// The other side of the war.
    pub fn opposite(self) -> Self {
        match self {
            Self::Attacker => Self::Defender,
            Self::Defender => Self::Attacker,
        }
    }
}

/// One frozen side of a formal war.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarSide {
    /// Organisation with authority to negotiate for the whole side.
    pub leader: OrgId,
    /// Organisations committed to this side, in stable-ID order.
    pub members: BTreeSet<OrgId>,
}

/// A liege's explicit adoption of a side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarAdoption {
    /// Day the adoption took effect.
    pub date: GameDate,
    /// Liege organisation that adopted the conflict.
    pub adopter: OrgId,
    /// Side it adopted.
    pub side: WarSideId,
}

/// Why an active war ended.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WarConclusionKind {
    /// A side leader secured peace for the complete war.
    NegotiatedPeace,
    /// A side ceased to have valid political authority.
    InvalidSide(WarSideId),
}

/// The frozen conclusion of a formal war.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarConclusion {
    /// Day hostilities ended.
    pub date: GameDate,
    /// Authoritative reason they ended.
    pub kind: WarConclusionKind,
}

/// One occurrence of a formal war.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarRecord {
    /// Occurrence-stable identity.
    pub id: WarId,
    /// Authored cause used by presentation and Situation content.
    pub cause: ContentKey,
    /// Day the war began.
    pub declared: GameDate,
    /// Attacker then defender; membership remains frozen after declaration.
    pub sides: [WarSide; 2],
    /// Explicit liege adoptions, oldest first.
    pub adoption_history: Vec<WarAdoption>,
    /// How and when the war ended; absent while active.
    pub conclusion: Option<WarConclusion>,
}

impl WarRecord {
    /// Whether this occurrence still authorises hostility.
    pub fn active(&self) -> bool {
        self.conclusion.is_none()
    }

    /// Reads one of the two sides.
    pub fn side(&self, side: WarSideId) -> &WarSide {
        &self.sides[side.index()]
    }

    /// Which side contains an organisation, if either.
    pub fn side_of(&self, org: OrgId) -> Option<WarSideId> {
        WarSideId::ALL
            .into_iter()
            .find(|side| self.side(*side).members.contains(&org))
    }
}

/// Every formal war, including concluded occurrences retained for history.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wars {
    /// Wars in occurrence-ID order.
    pub records: BTreeMap<WarId, WarRecord>,
}

/// Installs an empty formal-war ledger for a fresh campaign.
pub fn init_wars(world: &mut World) {
    world.insert_resource(Wars::default());
}

/// Captures the complete formal-war ledger for a campaign snapshot.
pub fn capture_wars(world: &World) -> Wars {
    world.get_resource::<Wars>().cloned().unwrap_or_default()
}

/// Restores a previously captured formal-war ledger.
pub fn restore_wars(world: &mut World, wars: &Wars) {
    world.insert_resource(wars.clone());
}

impl Wars {
    /// Active wars in stable occurrence order.
    pub fn active(&self) -> impl Iterator<Item = &WarRecord> {
        self.records.values().filter(|war| war.active())
    }
}

/// Why a formal-war state transition was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WarError {
    /// The named war does not exist.
    #[error("unknown war {0}")]
    UnknownWar(WarId),
    /// A concluded war cannot be changed again.
    #[error("war {0} has already ended")]
    WarConcluded(WarId),
    /// An organisation is missing, defunct, or otherwise cannot lead a side.
    #[error("organisation {0} cannot participate in a formal war")]
    InvalidOrganisation(OrgId),
    /// A house cannot declare war on itself.
    #[error("an organisation cannot declare war on itself")]
    SameOrganisation,
    /// The two requested leaders already oppose one another in this war.
    #[error("the organisations already oppose one another in war {0}")]
    AlreadyOpposed(WarId),
    /// A liege already belongs to one of this war's sides.
    #[error("organisation {0} already participates in war {1}")]
    AlreadyParticipating(OrgId, WarId),
    /// The requested side contains no vassal of the adopting liege.
    #[error("organisation {0} has no vassal on that side of war {1}")]
    NoVassalOnSide(OrgId, WarId),
    /// The live hierarchy was malformed and could not form disjoint sides.
    #[error("the political branches for this declaration overlap")]
    OverlappingBranches,
    /// Only a current side leader may settle the complete war.
    #[error("organisation {0} has no authority to settle war {1}")]
    NotSideLeader(OrgId, WarId),
}

fn organisation_stands(world: &World, org: OrgId) -> bool {
    crate::access::org(world, org).is_some_and(|record| !record.defunct)
}

/// The current organisation and every transitive vassal beneath it.
pub fn vassal_branch(world: &World, leader: OrgId) -> BTreeSet<OrgId> {
    let mut branch = BTreeSet::new();
    let mut pending = BTreeSet::from([leader]);
    while let Some(org) = pending.pop_first() {
        if !branch.insert(org) {
            continue;
        }
        pending.extend(crate::politics::vassals_of(world, org));
    }
    branch
}

fn partition_branches(
    attacker: OrgId,
    defender: OrgId,
    mut attacker_branch: BTreeSet<OrgId>,
    mut defender_branch: BTreeSet<OrgId>,
) -> Result<[WarSide; 2], WarError> {
    if defender_branch.contains(&attacker) {
        // Rebellion: the descendant retains its complete branch and that
        // branch is cut out of the ancestor's side.
        defender_branch.retain(|org| !attacker_branch.contains(org));
    } else if attacker_branch.contains(&defender) {
        // A liege attacking down its own tree follows the same rule with
        // the roles reversed.
        attacker_branch.retain(|org| !defender_branch.contains(org));
    } else if !attacker_branch.is_disjoint(&defender_branch) {
        return Err(WarError::OverlappingBranches);
    }

    if !attacker_branch.contains(&attacker)
        || !defender_branch.contains(&defender)
        || attacker_branch.is_empty()
        || defender_branch.is_empty()
        || !attacker_branch.is_disjoint(&defender_branch)
    {
        return Err(WarError::OverlappingBranches);
    }

    Ok([
        WarSide {
            leader: attacker,
            members: attacker_branch,
        },
        WarSide {
            leader: defender,
            members: defender_branch,
        },
    ])
}

/// Declares a new formal war between the leaders' current political branches.
pub fn declare_war(
    world: &mut World,
    attacker: OrgId,
    defender: OrgId,
    cause: ContentKey,
) -> Result<WarId, WarError> {
    if attacker == defender {
        return Err(WarError::SameOrganisation);
    }
    for org in [attacker, defender] {
        if !organisation_stands(world, org) {
            return Err(WarError::InvalidOrganisation(org));
        }
    }
    if let Some(war) = active_war_between(world, attacker, defender) {
        return Err(WarError::AlreadyOpposed(war));
    }

    let sides = partition_branches(
        attacker,
        defender,
        vassal_branch(world, attacker),
        vassal_branch(world, defender),
    )?;
    let id: WarId = world.resource_mut::<CampaignIds>().0.allocate();
    let declared = world.resource::<CampaignClock>().date;
    world.resource_mut::<Wars>().records.insert(
        id,
        WarRecord {
            id,
            cause,
            declared,
            sides,
            adoption_history: Vec::new(),
            conclusion: None,
        },
    );
    Ok(id)
}

/// Whether a nonparticipant liege may adopt the requested side.
pub fn can_adopt_side(world: &World, war: WarId, adopter: OrgId, side: WarSideId) -> bool {
    let Some(record) = world
        .get_resource::<Wars>()
        .and_then(|wars| wars.records.get(&war))
    else {
        return false;
    };
    record.active()
        && organisation_stands(world, adopter)
        && record.side_of(adopter).is_none()
        && record.side(side).members.iter().any(|member| {
            crate::politics::answers_to(world, *member, adopter).is_some_and(|hops| hops > 0)
        })
}

/// Has a nonparticipating liege explicitly adopt one side of an active war.
pub fn adopt_side(
    world: &mut World,
    war: WarId,
    adopter: OrgId,
    side: WarSideId,
) -> Result<(), WarError> {
    let record = world
        .get_resource::<Wars>()
        .and_then(|wars| wars.records.get(&war))
        .cloned()
        .ok_or(WarError::UnknownWar(war))?;
    if !record.active() {
        return Err(WarError::WarConcluded(war));
    }
    if !organisation_stands(world, adopter) {
        return Err(WarError::InvalidOrganisation(adopter));
    }
    if record.side_of(adopter).is_some() {
        return Err(WarError::AlreadyParticipating(adopter, war));
    }
    if !can_adopt_side(world, war, adopter, side) {
        return Err(WarError::NoVassalOnSide(adopter, war));
    }

    let opposing = record.side(side.opposite()).members.clone();
    let mut joining = vassal_branch(world, adopter);
    joining.retain(|org| !opposing.contains(org));
    let date = world.resource::<CampaignClock>().date;
    let mut wars = world.resource_mut::<Wars>();
    let war_record = wars.records.get_mut(&war).expect("war was read above");
    let chosen = &mut war_record.sides[side.index()];
    chosen.members.extend(joining);
    chosen.leader = adopter;
    war_record.adoption_history.push(WarAdoption {
        date,
        adopter,
        side,
    });
    Ok(())
}

/// Reads a formal-war occurrence, active or concluded.
pub fn war(world: &World, id: WarId) -> Option<&WarRecord> {
    world.get_resource::<Wars>()?.records.get(&id)
}

/// Formal-war occurrences in stable ID order.
pub fn war_ids(world: &World) -> Vec<WarId> {
    world
        .get_resource::<Wars>()
        .map(|wars| wars.records.keys().copied().collect())
        .unwrap_or_default()
}

/// Whether the named occurrence is still active.
pub fn is_active_war(world: &World, id: WarId) -> bool {
    war(world, id).is_some_and(WarRecord::active)
}

/// Which side currently contains an organisation in the named occurrence.
pub fn side_of(world: &World, id: WarId, org: OrgId) -> Option<WarSideId> {
    war(world, id)?.side_of(org)
}

/// Whether `org` presently has whole-war peace authority.
pub fn can_negotiate(world: &World, id: WarId, org: OrgId) -> bool {
    war(world, id).is_some_and(|war| {
        war.active()
            && WarSideId::ALL
                .into_iter()
                .any(|side| war.side(side).leader == org)
    })
}

/// Ends a whole formal war while retaining its record for history and replay.
pub fn conclude_war(
    world: &mut World,
    war: WarId,
    kind: WarConclusionKind,
) -> Result<(), WarError> {
    let date = world.resource::<CampaignClock>().date;
    {
        let mut wars = world.resource_mut::<Wars>();
        let Some(record) = wars.records.get_mut(&war) else {
            return Err(WarError::UnknownWar(war));
        };
        if !record.active() {
            return Err(WarError::WarConcluded(war));
        }
        record.conclusion = Some(WarConclusion { date, kind });
    }
    clear_blockades_for(world, war);
    crate::assignments::abort_assignments_for_war(world, war);
    Ok(())
}

fn clear_blockades_for(world: &mut World, war: WarId) {
    let ships: Vec<_> = world
        .get_resource::<crate::forces::ForcesIndex>()
        .map(|forces| forces.ships.values().copied().collect())
        .unwrap_or_default();
    for entity in ships {
        let belongs_to_war = world
            .get::<crate::forces::ShipRecord>(entity)
            .and_then(|ship| ship.blockading)
            .is_some_and(|blockade| blockade.war == war);
        if belongs_to_war && let Some(mut ship) = world.get_mut::<crate::forces::ShipRecord>(entity)
        {
            ship.blockading = None;
        }
    }
}

/// Settles the whole war after revalidating the negotiating side leader.
pub fn negotiate_peace(world: &mut World, war: WarId, negotiator: OrgId) -> Result<(), WarError> {
    if !can_negotiate(world, war, negotiator) {
        return match self::war(world, war) {
            Some(record) if !record.active() => Err(WarError::WarConcluded(war)),
            Some(_) => Err(WarError::NotSideLeader(negotiator, war)),
            None => Err(WarError::UnknownWar(war)),
        };
    }
    conclude_war(world, war, WarConclusionKind::NegotiatedPeace)
}

/// The active war places `a` and `b` on opposing sides.
pub fn opposed_in(world: &World, war: WarId, a: OrgId, b: OrgId) -> bool {
    let Some(record) = world
        .get_resource::<Wars>()
        .and_then(|wars| wars.records.get(&war))
    else {
        return false;
    };
    record.active()
        && matches!(
            (record.side_of(a), record.side_of(b)),
            (Some(left), Some(right)) if left != right
        )
}

/// Lowest stable active WarId that places `a` and `b` on opposing sides.
pub fn active_war_between(world: &World, a: OrgId, b: OrgId) -> Option<WarId> {
    world.get_resource::<Wars>()?.active().find_map(|war| {
        matches!(
            (war.side_of(a), war.side_of(b)),
            (Some(left), Some(right)) if left != right
        )
        .then_some(war.id)
    })
}

/// Active formal wars involving an organisation, in occurrence order.
pub fn active_wars_for(world: &World, org: OrgId) -> Vec<WarId> {
    world
        .get_resource::<Wars>()
        .map(|wars| {
            wars.active()
                .filter(|war| war.side_of(org).is_some())
                .map(|war| war.id)
                .collect()
        })
        .unwrap_or_default()
}

/// Ends wars whose frozen side no longer has valid political authority.
pub fn collapse_invalid_wars(world: &mut World) {
    let invalid: Vec<(WarId, WarSideId)> = world
        .get_resource::<Wars>()
        .map(|wars| {
            wars.active()
                .filter_map(|war| {
                    WarSideId::ALL.into_iter().find_map(|side_id| {
                        let side = war.side(side_id);
                        let valid = side.members.contains(&side.leader)
                            && organisation_stands(world, side.leader)
                            && side
                                .members
                                .iter()
                                .any(|member| organisation_stands(world, *member));
                        (!valid).then_some((war.id, side_id))
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    for (war, side) in invalid {
        let _ = conclude_war(world, war, WarConclusionKind::InvalidSide(side));
    }
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(DailyTick, collapse_invalid_wars.in_set(TickSet::Cleanup));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org(raw: u64) -> OrgId {
        OrgId::from_raw(raw).expect("non-zero test ID")
    }

    #[test]
    fn descendant_branch_is_cut_out_of_ancestor_side() {
        let ancestor = org(1);
        let sibling = org(2);
        let rebel = org(3);
        let rebel_vassal = org(4);
        let sides = partition_branches(
            rebel,
            ancestor,
            BTreeSet::from([rebel, rebel_vassal]),
            BTreeSet::from([ancestor, sibling, rebel, rebel_vassal]),
        )
        .unwrap();
        assert_eq!(sides[0].members, BTreeSet::from([rebel, rebel_vassal]));
        assert_eq!(sides[1].members, BTreeSet::from([ancestor, sibling]));
    }

    #[test]
    fn attacking_down_the_tree_uses_the_same_partition_rule() {
        let ancestor = org(1);
        let sibling = org(2);
        let rebel = org(3);
        let rebel_vassal = org(4);
        let sides = partition_branches(
            ancestor,
            rebel,
            BTreeSet::from([ancestor, sibling, rebel, rebel_vassal]),
            BTreeSet::from([rebel, rebel_vassal]),
        )
        .unwrap();
        assert_eq!(sides[0].members, BTreeSet::from([ancestor, sibling]));
        assert_eq!(sides[1].members, BTreeSet::from([rebel, rebel_vassal]));
    }
}
