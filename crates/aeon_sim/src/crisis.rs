//! The planetary succession crisis: paramountcy claims and Imperial
//! tithes.
//!
//! The MVP opens with a vacant, contested planetary paramountcy: the
//! previous paramount died without an accepted successor. A house can
//! press its claim through a job when it holds strictly more of the
//! planet's provinces than any rival; the Consul can lend or withhold
//! endorsement, which shifts a claimant's standing without deciding it.
//! Imperial tithes let the Consul's office extract wealth from the
//! houses, giving the Sanctora a lever over the whole field.

use bevy::prelude::World;

use crate::economy::OrgResources;
use crate::ids::{BodyId, OrgId, TitleId};
use crate::jobs::{LogChannel, LogEntry};
use crate::map::ProvinceRecord;
use crate::politics::{PoliticsIndex, TitleHolder, TitleKind, TitleRecord};
use crate::text::TextDb;

/// The tithe rate as a divisor of an organisation's wealth (a twentieth).
pub const TITHE_DIVISOR: i64 = 20;

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

/// How many provinces on `body` each organisation holds, in ID order.
pub fn province_counts_on(world: &World, body: BodyId) -> std::collections::BTreeMap<OrgId, u32> {
    let index = world.resource::<PoliticsIndex>();
    let mut counts: std::collections::BTreeMap<OrgId, u32> = std::collections::BTreeMap::new();
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

/// The dominant claimant on `body`: the organisation holding strictly
/// more of its provinces than any other, if one exists.
pub fn dominant_claimant(world: &World, body: BodyId) -> Option<OrgId> {
    let counts = province_counts_on(world, body);
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

fn log(world: &mut World, org: Option<OrgId>, text: String) {
    crate::access::log(world, LogEntry::line(text, LogChannel::Politics).by(org));
}

/// Presses `claimant`'s claim to the paramountcy. Succeeds only when the
/// title is vacant and the claimant strictly dominates the planet.
pub fn claim_paramountcy(world: &mut World, claimant: OrgId) -> bool {
    let Some((title_id, body)) = paramountcy(world) else {
        return false;
    };
    let vacant =
        crate::access::title(world, title_id).is_some_and(|t| t.holder == TitleHolder::Vacant);
    if !vacant {
        return false;
    }
    if dominant_claimant(world, body) != Some(claimant) {
        return false;
    }

    let entity = crate::access::title_entity(world, title_id).expect("indexed");
    if let Some(mut title) = world.get_mut::<TitleRecord>(entity) {
        title.holder = TitleHolder::Org(claimant);
    }
    let name = crate::access::org_name(world, claimant);
    let line = world
        .resource::<TextDb>()
        .format("sim.crisis.paramountcy-claimed", &[("house", &name)]);
    log(world, Some(claimant), line);
    true
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
