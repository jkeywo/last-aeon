//! Stable semantic world facts exposed to authored Rhai functions.
//!
//! This is intentionally a copied, read-only value tree. Scripts see game
//! concepts in stable-ID order, never ECS entities or component reflection,
//! and therefore cannot couple content to Bevy storage or mutate the world.

use aeon_data::model::{BodyKind, Gender, HouseTier, ObligationKind, OrgKind};
use bevy::prelude::World;
use rhai::{Array, Dynamic, Map};

use crate::assignments::{ActiveAssignment, AssignmentTarget};
use crate::clock::CampaignClock;
use crate::forces::{ArmyRecord, ForcesIndex, ShipLocation, ShipRecord};
use crate::map::{DisplayName, MapIndex, ProvinceRecord};
use crate::obligations::{ObligationStatus, Obligations};
use crate::politics::{
    CharacterRecord, CharacterSkills, ConsulContest, OrgRecord, PoliticsIndex, TitleHolder,
    TitleKind, TitleRecord,
};

fn integer(value: u64) -> i64 {
    i64::try_from(value).expect("stable IDs fit Rhai's signed integer range")
}

fn optional_id(value: Option<u64>) -> Dynamic {
    value
        .map(integer)
        .map(Dynamic::from)
        .unwrap_or(Dynamic::UNIT)
}

fn array(values: impl IntoIterator<Item = Dynamic>) -> Dynamic {
    Array::from_iter(values).into()
}

fn map(entries: impl IntoIterator<Item = (&'static str, Dynamic)>) -> Map {
    entries
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}

fn body_kind(kind: BodyKind) -> &'static str {
    match kind {
        BodyKind::Planet => "planet",
        BodyKind::Moon => "moon",
        BodyKind::Starbase => "starbase",
    }
}

fn gender(kind: Gender) -> &'static str {
    match kind {
        Gender::Male => "male",
        Gender::Female => "female",
    }
}

fn org_kind(kind: OrgKind) -> &'static str {
    match kind {
        OrgKind::DynasticHouse => "dynastic-house",
        OrgKind::SanctoraImperim => "sanctora-imperim",
    }
}

fn house_tier(tier: Option<HouseTier>) -> Dynamic {
    tier.map(|tier| match tier {
        HouseTier::Great => "great",
        HouseTier::Vassal => "vassal",
        HouseTier::Independent => "independent",
    })
    .map(Dynamic::from)
    .unwrap_or(Dynamic::UNIT)
}

fn obligation_kind(kind: ObligationKind) -> &'static str {
    match kind {
        ObligationKind::Favour => "favour",
        ObligationKind::Promise => "promise",
        ObligationKind::Grievance => "grievance",
    }
}

fn obligation_status(status: ObligationStatus) -> &'static str {
    match status {
        ObligationStatus::Open => "open",
        ObligationStatus::Fulfilled => "fulfilled",
        ObligationStatus::Broken => "broken",
        ObligationStatus::Expired => "expired",
    }
}

fn assignment_target(target: AssignmentTarget) -> Map {
    match target {
        AssignmentTarget::None => map([("kind", "none".into())]),
        AssignmentTarget::Character(id) => map([
            ("kind", "character".into()),
            ("a", integer(id.raw()).into()),
        ]),
        AssignmentTarget::Org(id) => map([
            ("kind", "organisation".into()),
            ("a", integer(id.raw()).into()),
        ]),
        AssignmentTarget::Province(id) => {
            map([("kind", "province".into()), ("a", integer(id.raw()).into())])
        }
        AssignmentTarget::War(id) => map([("kind", "war".into()), ("a", integer(id.raw()).into())]),
        AssignmentTarget::WarSide(id, side) => map([
            ("kind", "war-side".into()),
            ("a", integer(id.raw()).into()),
            (
                "side",
                match side {
                    crate::wars::WarSideId::Attacker => "attacker",
                    crate::wars::WarSideId::Defender => "defender",
                }
                .into(),
            ),
        ]),
        AssignmentTarget::OwnArmy(id) => {
            map([("kind", "own-army".into()), ("a", integer(id.raw()).into())])
        }
        AssignmentTarget::ArmyToProvince(army, province) => map([
            ("kind", "army-to-province".into()),
            ("a", integer(army.raw()).into()),
            ("b", integer(province.raw()).into()),
        ]),
        AssignmentTarget::ShipToProvince(ship, province) => map([
            ("kind", "ship-to-province".into()),
            ("a", integer(ship.raw()).into()),
            ("b", integer(province.raw()).into()),
        ]),
        #[allow(unreachable_patterns)]
        _ => map([("kind", "unsupported".into())]),
    }
}

/// Builds the `ctx.world` value shared by every authored Rhai invocation.
///
/// Arrays follow stable-ID order. Maps contain only semantic, serialisable
/// values; missing optional IDs are Rhai unit values.
pub fn context_value(world: &World) -> Map {
    let clock = world.resource::<CampaignClock>();
    let date = clock.date;
    let start_date = clock.start_date;
    let mut view = Map::new();
    view.insert("date".into(), date.days_since_epoch().into());
    view.insert("start_date".into(), start_date.days_since_epoch().into());
    view.insert(
        "player_org".into(),
        optional_id(
            world
                .get_resource::<crate::politics::PlayerHouse>()
                .and_then(|player| player.0)
                .map(|value| value.raw()),
        ),
    );

    if let Some(content) = world.get_resource::<crate::state::ContentDb>()
        && let Some(scenario) = &content.0.scenario
    {
        view.insert("scenario".into(), scenario.key.as_str().to_owned().into());
    }

    let mut characters = Array::new();
    let mut organisations = Array::new();
    let mut titles = Array::new();
    let mut offices = Array::new();
    let mut opinions = Array::new();
    if let Some(index) = world.get_resource::<PoliticsIndex>() {
        for (id, entity) in &index.characters {
            let Some(record) = world.get::<CharacterRecord>(*entity) else {
                continue;
            };
            let skills = world
                .get::<CharacterSkills>(*entity)
                .copied()
                .unwrap_or_default()
                .0;
            let is_head = index.orgs.values().any(|org_entity| {
                world
                    .get::<OrgRecord>(*org_entity)
                    .is_some_and(|org| org.head == Some(*id))
            });
            let held_titles = index.titles.values().filter_map(|title_entity| {
                let title = world.get::<TitleRecord>(*title_entity)?;
                (title.holder == TitleHolder::Character(*id))
                    .then_some(Dynamic::from(integer(title.id.raw())))
            });
            characters.push(
                map([
                    ("id", integer(id.raw()).into()),
                    (
                        "key",
                        record
                            .key
                            .as_ref()
                            .map(|key| Dynamic::from(key.as_str().to_owned()))
                            .unwrap_or(Dynamic::UNIT),
                    ),
                    ("name", record.name.clone().into()),
                    ("gender", gender(record.gender).into()),
                    ("alive", record.alive().into()),
                    (
                        "adult",
                        (record.alive() && record.age_years(date) >= 18).into(),
                    ),
                    ("age", record.age_years(date).into()),
                    (
                        "organisation",
                        optional_id(record.organisation.map(|value| value.raw())),
                    ),
                    ("is_head", is_head.into()),
                    ("command", i64::from(skills.command).into()),
                    ("diplomacy", i64::from(skills.diplomacy).into()),
                    ("intrigue", i64::from(skills.intrigue).into()),
                    ("stewardship", i64::from(skills.stewardship).into()),
                    ("held_titles", array(held_titles)),
                    (
                        "location_kind",
                        match world
                            .get::<crate::presence::CharacterLocation>(*entity)
                            .map(|location| location.0)
                        {
                            Some(crate::presence::Location::Province(_)) => "province".into(),
                            Some(crate::presence::Location::Aboard(_)) => "aboard".into(),
                            None => Dynamic::UNIT,
                        },
                    ),
                    (
                        "location",
                        match world
                            .get::<crate::presence::CharacterLocation>(*entity)
                            .map(|location| location.0)
                        {
                            Some(crate::presence::Location::Province(province)) => {
                                integer(province.raw()).into()
                            }
                            Some(crate::presence::Location::Aboard(ship)) => {
                                integer(ship.raw()).into()
                            }
                            None => Dynamic::UNIT,
                        },
                    ),
                ])
                .into(),
            );
        }

        for (id, entity) in &index.orgs {
            let Some(record) = world.get::<OrgRecord>(*entity) else {
                continue;
            };
            let resources = world
                .get::<crate::economy::OrgResources>(*entity)
                .copied()
                .unwrap_or_default();
            organisations.push(
                map([
                    ("id", integer(id.raw()).into()),
                    ("key", record.key.as_str().to_owned().into()),
                    ("name", crate::access::org_name(world, *id).into()),
                    ("kind", org_kind(record.kind).into()),
                    ("tier", house_tier(record.tier)),
                    ("liege", optional_id(record.liege.map(|value| value.raw()))),
                    ("head", optional_id(record.head.map(|value| value.raw()))),
                    ("defunct", record.defunct.into()),
                    ("wealth", resources.wealth.into()),
                    ("manpower", resources.manpower.into()),
                    ("supplies", resources.supplies.into()),
                    ("influence", resources.influence.into()),
                    ("legitimacy", i64::from(resources.legitimacy).into()),
                    (
                        "effective_legitimacy",
                        i64::from(crate::economy::effective_legitimacy(world, *id)).into(),
                    ),
                ])
                .into(),
            );
        }

        for (id, entity) in &index.titles {
            let Some(record) = world.get::<TitleRecord>(*entity) else {
                continue;
            };
            let (kind, scope) = match record.kind {
                TitleKind::Province(value) => ("province", Some(value.raw())),
                TitleKind::Paramount(value) => ("paramount", Some(value.raw())),
                TitleKind::Consul => ("consul", None),
            };
            let (holder_kind, holder) = match record.holder {
                TitleHolder::Org(value) => ("organisation", Some(value.raw())),
                TitleHolder::Character(value) => ("character", Some(value.raw())),
                TitleHolder::Vacant => ("vacant", None),
            };
            titles.push(
                map([
                    ("id", integer(id.raw()).into()),
                    (
                        "key",
                        record
                            .key
                            .as_ref()
                            .map(|key| Dynamic::from(key.as_str().to_owned()))
                            .unwrap_or(Dynamic::UNIT),
                    ),
                    ("name", record.name.clone().into()),
                    ("kind", kind.into()),
                    ("scope", optional_id(scope)),
                    ("holder_kind", holder_kind.into()),
                    ("holder", optional_id(holder)),
                ])
                .into(),
            );
        }

        for (id, entity) in &index.offices {
            let Some(record) = world.get::<crate::politics::OfficeRecord>(*entity) else {
                continue;
            };
            offices.push(
                map([
                    ("id", integer(id.raw()).into()),
                    ("key", record.key.as_str().to_owned().into()),
                    ("name", record.name.clone().into()),
                    ("organisation", integer(record.organisation.raw()).into()),
                    (
                        "province",
                        optional_id(record.province.map(|value| value.raw())),
                    ),
                    (
                        "holder",
                        optional_id(record.holder.map(|value| value.raw())),
                    ),
                ])
                .into(),
            );
        }

        // Opinion is an authoritative derived fact, not a copy of the stored
        // modifier ledger. Pair order is stable (from, then to).
        for from in index.characters.keys() {
            for to in index.characters.keys() {
                if from == to {
                    continue;
                }
                opinions.push(
                    map([
                        ("from", integer(from.raw()).into()),
                        ("to", integer(to.raw()).into()),
                        (
                            "value",
                            i64::from(crate::politics::opinion_between(world, *from, *to)).into(),
                        ),
                    ])
                    .into(),
                );
            }
        }
    }
    view.insert("characters".into(), characters.into());
    view.insert("organisations".into(), organisations.into());
    view.insert("titles".into(), titles.into());
    view.insert("offices".into(), offices.into());
    view.insert("opinions".into(), opinions.into());

    let mut bodies = Array::new();
    let mut provinces = Array::new();
    if let Some(index) = world.get_resource::<MapIndex>() {
        for (id, entity) in &index.bodies {
            let Some(record) = world.get::<crate::map::BodyRecord>(*entity) else {
                continue;
            };
            let name = world
                .get::<DisplayName>(*entity)
                .map(|name| name.0.clone())
                .unwrap_or_default();
            bodies.push(
                map([
                    ("id", integer(id.raw()).into()),
                    ("key", record.key.as_str().to_owned().into()),
                    ("name", name.into()),
                    ("kind", body_kind(record.kind).into()),
                    (
                        "parent",
                        optional_id(record.parent.map(|value| value.raw())),
                    ),
                ])
                .into(),
            );
        }
        for (id, entity) in &index.provinces {
            let Some(record) = world.get::<ProvinceRecord>(*entity) else {
                continue;
            };
            let name = world
                .get::<DisplayName>(*entity)
                .map(|name| name.0.clone())
                .unwrap_or_default();
            provinces.push(
                map([
                    ("id", integer(id.raw()).into()),
                    ("key", record.key.as_str().to_owned().into()),
                    ("name", name.into()),
                    ("body", integer(record.body.raw()).into()),
                    ("starport", record.starport.into()),
                    (
                        "holder",
                        optional_id(
                            crate::warfare::province_holder(world, *id).map(|value| value.raw()),
                        ),
                    ),
                    (
                        "order",
                        i64::from(crate::order::province_order(world, *id).order).into(),
                    ),
                ])
                .into(),
            );
        }
    }
    view.insert("bodies".into(), bodies.into());
    view.insert("provinces".into(), provinces.into());

    let routes = world
        .get_resource::<crate::routes::RouteGraph>()
        .map(|graph| {
            graph
                .routes()
                .into_iter()
                .map(|leg| {
                    map([
                        ("key", leg.route.as_str().to_owned().into()),
                        (
                            "kind",
                            match leg.kind {
                                aeon_data::model::RouteKind::Surface => "surface".into(),
                                aeon_data::model::RouteKind::Space => "space".into(),
                            },
                        ),
                        ("a", integer(leg.from.raw()).into()),
                        ("b", integer(leg.to.raw()).into()),
                        ("travel_days", i64::from(leg.travel_days).into()),
                        ("risk", i64::from(leg.risk).into()),
                    ])
                    .into()
                })
                .collect::<Array>()
        })
        .unwrap_or_default();
    view.insert("routes".into(), routes.into());

    let mut armies = Array::new();
    let mut ships = Array::new();
    if let Some(index) = world.get_resource::<ForcesIndex>() {
        for (id, entity) in &index.armies {
            let Some(record) = world.get::<ArmyRecord>(*entity) else {
                continue;
            };
            armies.push(
                map([
                    ("id", integer(id.raw()).into()),
                    ("name", record.name.clone().into()),
                    ("owner", integer(record.owner.raw()).into()),
                    (
                        "general",
                        optional_id(record.general.map(|value| value.raw())),
                    ),
                    (
                        "lieutenant",
                        optional_id(record.lieutenant.map(|value| value.raw())),
                    ),
                    ("manpower", record.manpower.into()),
                    ("supplies", record.supplies.into()),
                    (
                        "location",
                        record
                            .location
                            .province()
                            .map_or(Dynamic::UNIT, |province| integer(province.raw()).into()),
                    ),
                    (
                        "location_kind",
                        match record.location {
                            crate::forces::ArmyLocation::Province(_) => "province".into(),
                            crate::forces::ArmyLocation::Embarked(_) => "aboard".into(),
                        },
                    ),
                    (
                        "aboard_ship",
                        match record.location {
                            crate::forces::ArmyLocation::Embarked(ship) => {
                                integer(ship.raw()).into()
                            }
                            crate::forces::ArmyLocation::Province(_) => Dynamic::UNIT,
                        },
                    ),
                    ("orders_suspended", record.orders_suspended.into()),
                    (
                        "retreat_destination",
                        optional_id(record.retreat_destination.map(|value| value.raw())),
                    ),
                ])
                .into(),
            );
        }
        for (id, entity) in &index.ships {
            let Some(record) = world.get::<ShipRecord>(*entity) else {
                continue;
            };
            let (location_kind, location, arrives) = match record.location {
                ShipLocation::Docked(at) => ("docked", at.raw(), Dynamic::UNIT),
                ShipLocation::OnRoute { to, arrives, .. } => {
                    ("route", to.raw(), Dynamic::from(arrives.days_since_epoch()))
                }
            };
            ships.push(
                map([
                    ("id", integer(id.raw()).into()),
                    ("key", record.key.as_str().to_owned().into()),
                    ("name", record.name.clone().into()),
                    ("owner", integer(record.owner.raw()).into()),
                    (
                        "captain",
                        optional_id(record.captain.map(|value| value.raw())),
                    ),
                    ("first_officer", optional_id(record.first_officer.map(|value| value.raw()))),
                    ("troop_capacity", record.troop_capacity.into()),
                    ("personal_transport", record.personal_transport.into()),
                    ("orders_suspended", record.orders_suspended.into()),
                    ("retreat_destination", optional_id(record.retreat_destination.map(|value| value.raw()))),
                    ("occupant_characters", array(world.resource::<crate::politics::PoliticsIndex>().characters.iter().filter_map(|(character, entity)| {
                        matches!(world.get::<crate::presence::CharacterLocation>(*entity).map(|location| location.0), Some(crate::presence::Location::Aboard(aboard)) if aboard == *id)
                            .then_some(integer(character.raw()).into())
                    }))),
                    ("occupant_armies", array(index.armies.iter().filter_map(|(army, entity)| {
                        matches!(world.get::<ArmyRecord>(*entity).map(|record| record.location), Some(crate::forces::ArmyLocation::Embarked(aboard)) if aboard == *id)
                            .then_some(integer(army.raw()).into())
                    }))),
                    ("location_kind", location_kind.into()),
                    ("location", integer(location).into()),
                    ("arrives", arrives),
                    (
                        "blockading",
                        optional_id(record.blockading.map(|value| value.province.raw())),
                    ),
                    (
                        "blockade_war",
                        optional_id(record.blockading.map(|value| value.war.raw())),
                    ),
                ])
                .into(),
            );
        }
    }
    view.insert("armies".into(), armies.into());
    view.insert("ships".into(), ships.into());

    let obligations = world
        .get_resource::<Obligations>()
        .map(|ledger| {
            ledger.entries.iter().map(|entry| {
                map([
                    ("id", integer(entry.id).into()),
                    (
                        "source",
                        entry
                            .source
                            .as_ref()
                            .map(|key| Dynamic::from(key.as_str().to_owned()))
                            .unwrap_or(Dynamic::UNIT),
                    ),
                    ("kind", obligation_kind(entry.kind).into()),
                    ("debtor", integer(entry.debtor.raw()).into()),
                    ("creditor", integer(entry.creditor.raw()).into()),
                    ("created", entry.created.days_since_epoch().into()),
                    (
                        "expires",
                        entry
                            .expires
                            .map(|date| Dynamic::from(date.days_since_epoch()))
                            .unwrap_or(Dynamic::UNIT),
                    ),
                    ("weight", i64::from(entry.weight).into()),
                    ("status", obligation_status(entry.status).into()),
                ])
                .into()
            })
        })
        .map(Array::from_iter)
        .unwrap_or_default();
    view.insert("obligations".into(), obligations.into());

    let assignments = world
        .get_resource::<crate::assignments::AssignmentsIndex>()
        .map(|index| {
            index.assignments.values().filter_map(|entity| {
                let record = world.get::<ActiveAssignment>(*entity)?;
                // Covertness and the live Order resistance are read from
                // the same authored definition and the same shared
                // calculation the resolution roll will use, so a
                // projection can quote the exact number without owning a
                // second copy of the rule. `covert` is the authored kind of
                // the work — deniable work stays deniable work after it is
                // found out — so content that keys off it (the Unquiet
                // Holdings trigger) keeps recognising the same operation.
                // Who now knows the hand is the separate `exposures` view.
                let covert = crate::covert::assignment_is_covert(world, &record.def);
                let order_shift = world
                    .get_resource::<crate::state::ContentDb>()
                    .and_then(|content| content.0.assignments.get(&record.def).cloned())
                    .and_then(|def| {
                        crate::forecast::order_modifier_reading(world, record.target, &def)
                    })
                    .map(|(_, shift)| i64::from(shift))
                    .unwrap_or(0);
                Some(
                    map([
                        ("id", integer(record.id.raw()).into()),
                        ("key", record.def.as_str().to_owned().into()),
                        ("owner", integer(record.owner.raw()).into()),
                        ("leader", integer(record.leader.raw()).into()),
                        ("target", assignment_target(record.target).into()),
                        ("war", optional_id(record.war.map(|war| war.raw()))),
                        ("started", record.started.days_since_epoch().into()),
                        ("completes", record.completes.days_since_epoch().into()),
                        ("cancel_requested", record.cancel_requested.into()),
                        ("covert", covert.into()),
                        ("order_shift", order_shift.into()),
                    ])
                    .into(),
                )
            })
        })
        .map(Array::from_iter)
        .unwrap_or_default();
    view.insert("assignments".into(), assignments.into());

    // What investigation has proved, in durable record order. Content asks
    // this — never the culprit binding alone — before a projection may name
    // the hand behind covert work, so a card discloses to the house that
    // found the culprit out and to nobody else.
    let exposures = world
        .get_resource::<crate::covert::Exposure>()
        .map(|exposure| {
            exposure.records.iter().map(|record| {
                map([
                    ("culprit", integer(record.culprit.raw()).into()),
                    ("knower", integer(record.knower.raw()).into()),
                    ("discovered", record.discovered.days_since_epoch().into()),
                ])
                .into()
            })
        })
        .map(Array::from_iter)
        .unwrap_or_default();
    view.insert("exposures".into(), exposures.into());

    let contest = world
        .get_resource::<ConsulContest>()
        .map(|contest| {
            map([
                ("title", integer(contest.title.raw()).into()),
                ("opened", contest.opened.days_since_epoch().into()),
                (
                    "deadline",
                    contest
                        .opened
                        .add_days(crate::politics::CONSUL_CONTEST_DAYS)
                        .days_since_epoch()
                        .into(),
                ),
                (
                    "candidates",
                    array(
                        contest
                            .candidates
                            .iter()
                            .map(|id| Dynamic::from(integer(id.raw()))),
                    ),
                ),
                (
                    "scores",
                    array(contest.candidates.iter().map(|id| {
                        map([
                            ("character", integer(id.raw()).into()),
                            (
                                "score",
                                crate::politics::consul_score(world, contest.title, *id).into(),
                            ),
                        ])
                        .into()
                    })),
                ),
            ])
        })
        .map(Dynamic::from)
        .unwrap_or(Dynamic::UNIT);
    view.insert("consul_contest".into(), contest);

    let mut paramount_claims = Array::new();
    let mut realm_totals = Array::new();
    if world.get_resource::<PoliticsIndex>().is_some()
        && let Some((title, body)) = crate::crisis::paramountcy(world)
    {
        let dominant = crate::crisis::dominant_claimant(world, body);
        for claim in crate::crisis::claims_for(world, title) {
            let Some(organisation) =
                crate::crisis::claimant_eligibility(world, title, claim.claimant).ok()
            else {
                continue;
            };
            paramount_claims.push(
                map([
                    ("title", integer(title.raw()).into()),
                    ("character", integer(claim.claimant.raw()).into()),
                    ("organisation", integer(organisation.raw()).into()),
                    ("declared", claim.declared.days_since_epoch().into()),
                    (
                        "can_press",
                        (dominant == Some(organisation)
                            && crate::crisis::claimant_war_blocker(world, title, claim.claimant)
                                .is_none())
                        .into(),
                    ),
                ])
                .into(),
            );
        }

        for (organisation, provinces) in crate::crisis::realm_province_counts_on(world, body) {
            let army_manpower = world
                .get_resource::<ForcesIndex>()
                .map(|forces| {
                    forces
                        .armies
                        .values()
                        .filter_map(|entity| world.get::<ArmyRecord>(*entity))
                        .filter(|army| crate::crisis::realm_root(world, army.owner) == organisation)
                        .map(|army| army.manpower)
                        .sum::<i64>()
                })
                .unwrap_or_default();
            realm_totals.push(
                map([
                    ("organisation", integer(organisation.raw()).into()),
                    ("provinces", i64::from(provinces).into()),
                    ("army_manpower", army_manpower.into()),
                ])
                .into(),
            );
        }
    }
    view.insert("paramount_claims".into(), paramount_claims.into());
    view.insert("realm_totals".into(), realm_totals.into());

    let wars = crate::wars::war_ids(world)
        .into_iter()
        .filter_map(|id| {
            let record = crate::wars::war(world, id)?;
            let sides = crate::wars::WarSideId::ALL.into_iter().map(|side| {
                let side = record.side(side);
                map([
                    ("leader", integer(side.leader.raw()).into()),
                    (
                        "members",
                        array(
                            side.members
                                .iter()
                                .map(|member| Dynamic::from(integer(member.raw()))),
                        ),
                    ),
                ])
                .into()
            });
            let (concluded, conclusion_kind) = match record.conclusion {
                None => (Dynamic::UNIT, Dynamic::UNIT),
                Some(conclusion) => {
                    let kind = match conclusion.kind {
                        crate::wars::WarConclusionKind::NegotiatedPeace => "negotiated-peace",
                        crate::wars::WarConclusionKind::InvalidSide(
                            crate::wars::WarSideId::Attacker,
                        ) => "invalid-attacker",
                        crate::wars::WarConclusionKind::InvalidSide(
                            crate::wars::WarSideId::Defender,
                        ) => "invalid-defender",
                    };
                    (
                        Dynamic::from(conclusion.date.days_since_epoch()),
                        Dynamic::from(kind),
                    )
                }
            };
            Some(
                map([
                    ("id", integer(id.raw()).into()),
                    ("cause", record.cause.as_str().to_owned().into()),
                    ("declared", record.declared.days_since_epoch().into()),
                    ("active", record.active().into()),
                    ("sides", array(sides)),
                    ("concluded", concluded),
                    ("conclusion_kind", conclusion_kind),
                ])
                .into(),
            )
        })
        .collect::<Array>();
    view.insert("wars".into(), wars.into());

    view
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CampaignConfig, SimHost};
    use aeon_core::calendar::GameDate;

    #[test]
    fn content_free_view_keeps_the_stable_topology() {
        let mut host = SimHost::new(CampaignConfig {
            name: "view".to_owned(),
            seed: 1,
            start_date: GameDate::from_days(7),
        });
        let view = context_value(host.world_mut());
        assert_eq!(view["date"].as_int().unwrap(), 7);
        assert_eq!(view["start_date"].as_int().unwrap(), 7);
        for key in [
            "characters",
            "organisations",
            "titles",
            "provinces",
            "obligations",
            "assignments",
            "exposures",
            "paramount_claims",
            "wars",
        ] {
            assert!(view[key].is_array(), "{key} is always an array");
        }
    }
}
