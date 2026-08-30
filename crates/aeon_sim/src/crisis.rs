//! The planetary succession crisis: paramountcy claims and Imperial
//! tithes.
//!
//! The MVP opens with a vacant, contested planetary paramountcy: the
//! previous paramount died without an accepted successor. Living independent
//! house heads may declare personal claims, then press one when their complete
//! realm holds strictly more of the planet than every rival realm.
//! Imperial tithes let the Consul's office extract wealth from the
//! houses, giving the Sanctora a lever over the whole field.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::model::OrgKind;
use bevy::app::App;
use bevy::prelude::{IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::assignments::{LogChannel, LogEntry};
use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::economy::OrgResources;
use crate::ids::{BodyId, CharacterId, OrgId, TitleId, WarId};
use crate::map::ProvinceRecord;
use crate::politics::{ADULT_AGE, PoliticsIndex, TitleHolder, TitleKind, TitleRecord};
use crate::text::TextDb;

/// The tithe rate as a divisor of an organisation's wealth (a twentieth).
pub const TITHE_DIVISOR: i64 = 20;

/// One living character's explicit personal claim to a Paramount title.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamountClaim {
    /// Contested Paramount title.
    pub title: TitleId,
    /// Character who personally declared the claim.
    pub claimant: CharacterId,
    /// Day the declaration took effect.
    pub declared: GameDate,
}

/// Every current personal Paramount claim, in structural identity order.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamountClaims {
    /// Claims keyed by `(title, claimant)`.
    pub entries: BTreeMap<(TitleId, CharacterId), ParamountClaim>,
}

/// Installs an empty personal-claim ledger for a fresh campaign.
pub fn init_paramount_claims(world: &mut World) {
    world.insert_resource(ParamountClaims::default());
}

/// Captures the personal-claim ledger for a campaign snapshot.
pub fn capture_paramount_claims(world: &World) -> ParamountClaims {
    world
        .get_resource::<ParamountClaims>()
        .cloned()
        .unwrap_or_default()
}

/// Restores a previously captured personal-claim ledger.
pub fn restore_paramount_claims(world: &mut World, claims: &ParamountClaims) {
    world.insert_resource(claims.clone());
}

/// Why a personal Paramount claim transition was refused.
#[derive(Copy, Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParamountClaimError {
    /// The title does not exist.
    #[error("unknown Paramount title {0}")]
    UnknownTitle(TitleId),
    /// The title is not a Paramountcy.
    #[error("title {0} is not a Paramountcy")]
    NotParamount(TitleId),
    /// Claims can only exist while the title is vacant.
    #[error("the Paramountcy is not vacant")]
    TitleNotVacant,
    /// The claimant does not exist.
    #[error("unknown claimant {0}")]
    UnknownCharacter(CharacterId),
    /// The claimant is dead or not yet an adult.
    #[error("the claimant must be a living adult")]
    NotLivingAdult,
    /// The claimant belongs to no organisation.
    #[error("the claimant belongs to no house")]
    NoOrganisation,
    /// Only dynastic-house heads may claim.
    #[error("only a dynastic house may claim the Paramountcy")]
    NotDynastic,
    /// A defunct house cannot sustain a claim.
    #[error("a defunct house cannot claim the Paramountcy")]
    DefunctHouse,
    /// The character is not their organisation's current head.
    #[error("only the current house head may claim the Paramountcy")]
    NotHouseHead,
    /// A claimant must be independent.
    #[error("a house with a liege cannot claim the Paramountcy")]
    HasLiege,
    /// The same character has already declared this claim.
    #[error("that character has already declared this claim")]
    AlreadyDeclared,
    /// Pressing and renouncing require an existing personal claim.
    #[error("that character has not declared a claim")]
    NoDeclaredClaim,
    /// The claimant's complete realm is not uniquely dominant.
    #[error("the claimant's realm does not strictly exceed every rival realm")]
    NotDominant,
    /// A war against another valid claimant must end before pressing.
    #[error("another claimant opposes this claim in active war {0}")]
    OpposingClaimantWar(WarId),
}

/// The planet's paramountcy title, if the scenario defines one.
pub fn paramountcy(world: &World) -> Option<(TitleId, BodyId)> {
    let index = world.resource::<PoliticsIndex>();
    index.titles.values().find_map(|entity| {
        let title = world.get::<TitleRecord>(*entity)?;
        match title.kind {
            TitleKind::Paramount(body) => Some((title.id, body)),
            _ => None,
        }
    })
}

/// How many provinces on `body` each organisation directly holds, in ID order.
pub fn province_counts_on(world: &World, body: BodyId) -> BTreeMap<OrgId, u32> {
    let index = world.resource::<PoliticsIndex>();
    let mut counts: BTreeMap<OrgId, u32> = BTreeMap::new();
    for entity in index.titles.values() {
        let Some(title) = world.get::<TitleRecord>(*entity) else {
            continue;
        };
        let (TitleKind::Province(province), TitleHolder::Org(org)) = (title.kind, title.holder)
        else {
            continue;
        };
        let on_body = world
            .resource::<crate::map::MapIndex>()
            .provinces
            .get(&province)
            .and_then(|e| world.get::<ProvinceRecord>(*e))
            .map(|r| r.body);
        if on_body == Some(body) {
            *counts.entry(org).or_default() += 1;
        }
    }
    counts
}

/// The independent root of an organisation's current liege chain.
pub fn realm_root(world: &World, start: OrgId) -> OrgId {
    let mut current = start;
    // Political content rejects cycles; the bound also prevents a malformed
    // live hierarchy from hanging a campaign.
    for _ in 0..16 {
        let Some(record) = crate::access::org(world, current) else {
            break;
        };
        match record.liege {
            Some(liege) => current = liege,
            None => break,
        }
    }
    current
}

/// Planetary holdings aggregated to each complete top-level realm.
pub fn realm_province_counts_on(world: &World, body: BodyId) -> BTreeMap<OrgId, u32> {
    let mut counts = BTreeMap::new();
    for (holder, held) in province_counts_on(world, body) {
        *counts.entry(realm_root(world, holder)).or_default() += held;
    }
    counts
}

/// Number of provinces held by the complete realm containing `org`.
pub fn realm_province_count_on(world: &World, body: BodyId, org: OrgId) -> u32 {
    realm_province_counts_on(world, body)
        .get(&realm_root(world, org))
        .copied()
        .unwrap_or_default()
}

/// The dominant realm on `body`: the independent organisation whose whole
/// branch holds strictly more provinces than every rival, if one exists.
pub fn dominant_claimant(world: &World, body: BodyId) -> Option<OrgId> {
    let counts = realm_province_counts_on(world, body);
    let mut ranked: Vec<(u32, OrgId)> = counts.iter().map(|(org, count)| (*count, *org)).collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    match ranked.as_slice() {
        [(top, org), rest @ ..] => {
            let contested = rest.iter().any(|(count, _)| count == top);
            (!contested).then_some(*org)
        }
        [] => None,
    }
}

fn paramount_body(world: &World, title: TitleId) -> Result<BodyId, ParamountClaimError> {
    let title_record =
        crate::access::title(world, title).ok_or(ParamountClaimError::UnknownTitle(title))?;
    match title_record.kind {
        TitleKind::Paramount(body) => Ok(body),
        _ => Err(ParamountClaimError::NotParamount(title)),
    }
}

/// Validates a character's present standing to hold a personal claim and
/// returns the independent house whose authority they carry.
pub fn claimant_eligibility(
    world: &World,
    title: TitleId,
    claimant: CharacterId,
) -> Result<OrgId, ParamountClaimError> {
    let _ = paramount_body(world, title)?;
    if crate::access::title(world, title).map(|record| record.holder) != Some(TitleHolder::Vacant) {
        return Err(ParamountClaimError::TitleNotVacant);
    }
    let character = crate::access::character(world, claimant)
        .ok_or(ParamountClaimError::UnknownCharacter(claimant))?;
    let date = world.resource::<CampaignClock>().date;
    if !character.alive() || character.age_years(date) < ADULT_AGE {
        return Err(ParamountClaimError::NotLivingAdult);
    }
    let org = character
        .organisation
        .ok_or(ParamountClaimError::NoOrganisation)?;
    let organisation = crate::access::org(world, org).ok_or(ParamountClaimError::NoOrganisation)?;
    if organisation.kind != OrgKind::DynasticHouse {
        return Err(ParamountClaimError::NotDynastic);
    }
    if organisation.defunct {
        return Err(ParamountClaimError::DefunctHouse);
    }
    if organisation.head != Some(claimant) {
        return Err(ParamountClaimError::NotHouseHead);
    }
    if organisation.liege.is_some() {
        return Err(ParamountClaimError::HasLiege);
    }
    Ok(org)
}

/// Current valid claims to one Paramount title, in claimant-ID order.
pub fn claims_for(world: &World, title: TitleId) -> Vec<ParamountClaim> {
    world
        .get_resource::<ParamountClaims>()
        .map(|claims| {
            claims
                .entries
                .values()
                .filter(|claim| claim.title == title)
                .copied()
                .filter(|claim| claimant_eligibility(world, title, claim.claimant).is_ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether this character currently has the explicit claim.
pub fn has_claim(world: &World, title: TitleId, claimant: CharacterId) -> bool {
    world
        .get_resource::<ParamountClaims>()
        .is_some_and(|claims| claims.entries.contains_key(&(title, claimant)))
        && claimant_eligibility(world, title, claimant).is_ok()
}

/// Records a living independent head's explicit personal declaration.
pub fn declare_claim(
    world: &mut World,
    title: TitleId,
    claimant: CharacterId,
) -> Result<(), ParamountClaimError> {
    let _ = claimant_eligibility(world, title, claimant)?;
    if world
        .resource::<ParamountClaims>()
        .entries
        .contains_key(&(title, claimant))
    {
        return Err(ParamountClaimError::AlreadyDeclared);
    }
    let declared = world.resource::<CampaignClock>().date;
    world.resource_mut::<ParamountClaims>().entries.insert(
        (title, claimant),
        ParamountClaim {
            title,
            claimant,
            declared,
        },
    );
    Ok(())
}

/// Renounces one character's claim without affecting any other claimant.
pub fn renounce_claim(
    world: &mut World,
    title: TitleId,
    claimant: CharacterId,
) -> Result<(), ParamountClaimError> {
    if world
        .resource_mut::<ParamountClaims>()
        .entries
        .remove(&(title, claimant))
        .is_some()
    {
        Ok(())
    } else {
        Err(ParamountClaimError::NoDeclaredClaim)
    }
}

/// Removes every personal claim made by a character, as on death.
pub fn remove_claims_by(world: &mut World, claimant: CharacterId) {
    if let Some(mut claims) = world.get_resource_mut::<ParamountClaims>() {
        claims
            .entries
            .retain(|(_, character), _| *character != claimant);
    }
}

/// Removes claims that no longer satisfy the same authoritative eligibility
/// predicate used by declaration and pressing.
pub fn cleanup_claims(world: &mut World) {
    let stale: Vec<(TitleId, CharacterId)> = world
        .get_resource::<ParamountClaims>()
        .map(|claims| {
            claims
                .entries
                .keys()
                .copied()
                .filter(|(title, claimant)| claimant_eligibility(world, *title, *claimant).is_err())
                .collect()
        })
        .unwrap_or_default();
    if let Some(mut claims) = world.get_resource_mut::<ParamountClaims>() {
        for key in stale {
            claims.entries.remove(&key);
        }
    }
}

/// Active war opposing this claimant to another currently valid claimant.
pub fn claimant_war_blocker(world: &World, title: TitleId, claimant: CharacterId) -> Option<WarId> {
    let claimant_org = claimant_eligibility(world, title, claimant).ok()?;
    claims_for(world, title)
        .into_iter()
        .filter(|other| other.claimant != claimant)
        .filter_map(|other| {
            let other_org = claimant_eligibility(world, title, other.claimant).ok()?;
            crate::wars::active_war_between(world, claimant_org, other_org)
        })
        .min()
}

fn log(world: &mut World, org: Option<OrgId>, text: String) {
    crate::access::log(world, LogEntry::line(text, LogChannel::Politics).by(org));
}

/// Presses one explicit personal claim, revalidating every prerequisite.
pub fn press_claim(
    world: &mut World,
    title: TitleId,
    claimant: CharacterId,
) -> Result<(), ParamountClaimError> {
    let claimant_org = claimant_eligibility(world, title, claimant)?;
    if !has_claim(world, title, claimant) {
        return Err(ParamountClaimError::NoDeclaredClaim);
    }
    let body = paramount_body(world, title)?;
    if dominant_claimant(world, body) != Some(claimant_org) {
        return Err(ParamountClaimError::NotDominant);
    }
    if let Some(war) = claimant_war_blocker(world, title, claimant) {
        return Err(ParamountClaimError::OpposingClaimantWar(war));
    }

    let entity = crate::access::title_entity(world, title).expect("indexed");
    world
        .get_mut::<TitleRecord>(entity)
        .expect("indexed")
        .holder = TitleHolder::Character(claimant);
    world
        .resource_mut::<ParamountClaims>()
        .entries
        .retain(|(claim_title, _), _| *claim_title != title);
    let name = crate::access::character_name(world, claimant);
    let line = world
        .resource::<TextDb>()
        .format("sim.crisis.paramountcy-claimed", &[("claimant", &name)]);
    log(world, Some(claimant_org), line);
    Ok(())
}

/// Compatibility boundary for the existing assignment effect. The effect's
/// organisation resolves to its current head, whose explicit claim is pressed.
pub fn claim_paramountcy(world: &mut World, claimant: OrgId) -> bool {
    let Some((title, _)) = paramountcy(world) else {
        return false;
    };
    let Some(head) = crate::access::org_head(world, claimant) else {
        return false;
    };
    press_claim(world, title, head).is_ok()
}

/// Collects Imperial tithes: every house pays a twentieth of its wealth
/// to `collector`. Valid only for the Sanctora Imperim.
pub fn collect_tithes(world: &mut World, collector: OrgId) -> bool {
    let is_sanctora = crate::access::org(world, collector)
        .is_some_and(|r| r.kind == aeon_data::model::OrgKind::SanctoraImperim);
    if !is_sanctora {
        return false;
    }

    let houses: Vec<OrgId> = crate::access::org_ids(world)
        .into_iter()
        .filter(|org| *org != collector)
        .filter(|org| {
            crate::access::org(world, *org)
                .is_some_and(|r| r.kind == aeon_data::model::OrgKind::DynasticHouse && !r.defunct)
        })
        .collect();

    let mut total = 0i64;
    for house in houses {
        let entity = crate::access::org_entity(world, house).expect("indexed");
        let paid = world
            .get_mut::<OrgResources>(entity)
            .map(|mut r| {
                let due = (r.wealth / TITHE_DIVISOR).max(0);
                r.wealth -= due;
                due
            })
            .unwrap_or(0);
        total += paid;
    }
    let entity = crate::access::org_entity(world, collector).expect("indexed");
    if let Some(mut r) = world.get_mut::<OrgResources>(entity) {
        r.wealth += total;
    }
    let line = world.resource::<TextDb>().format(
        "sim.crisis.tithes-collected",
        &[("amount", &total.to_string())],
    );
    log(world, Some(collector), line);
    true
}

pub(crate) fn install(app: &mut App) {
    app.add_systems(
        DailyTick,
        cleanup_claims
            .in_set(TickSet::Cleanup)
            .after(crate::wars::collapse_invalid_wars),
    );
}
