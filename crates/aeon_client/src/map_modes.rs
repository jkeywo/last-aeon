//! What each map mode says about each province.
//!
//! One exclusive system asks the simulation for everything the focused
//! globe needs, and publishes a readout per province: the colour to bake,
//! a numeric value to print on the map, and a hover explanation. The
//! texture bake, the map labels, and the legend all read this same
//! readout, so they can never disagree about what is being shown.
//!
//! Every graded mode pairs its colour with a printed value, so no reading
//! of the map depends on distinguishing colours.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::model::HouseTier;
use aeon_sim::forces::ArmyRecord;
use aeon_sim::map::ProvinceRecord;
use aeon_sim::order::{ORDER_MAX, ProvincialOrder, pressures, province_order};
use aeon_sim::state::ContentDb;
use aeon_sim::{
    BodyId, CampaignClock, OrgId, OrgRecord, PlayerHouse, PoliticsIndex, ProvinceId, TitleHolder,
    TitleRecord, answers_to, opinion_between,
};
use bevy::prelude::*;

use crate::view::{MapMode, MapView, ViewState};

/// Neutral grey for provinces a mode has nothing to say about.
///
/// Mirrors `semantics.map_neutral` in the theme; kept as a constant here
/// because the bake runs before any egui context exists.
const NEUTRAL: [u8; 3] = [90, 90, 96];

/// What one mode says about one province.
#[derive(Clone, Debug)]
pub struct ProvinceReadout {
    /// The colour to paint on the globe.
    pub colour: [u8; 3],
    /// A short value printed beside the province name, so the map can be
    /// read without relying on colour.
    pub value: Option<String>,
    /// A fuller explanation for hovering.
    pub hint: String,
    /// Something here needs the player's attention.
    pub alert: bool,
}

impl Default for ProvinceReadout {
    fn default() -> Self {
        Self {
            colour: NEUTRAL,
            value: None,
            hint: String::new(),
            alert: false,
        }
    }
}

/// One entry in the situation strip: something demanding attention.
#[derive(Clone, Debug)]
pub struct SituationItem {
    /// The province concerned.
    pub province: ProvinceId,
    /// The body it sits on, so the view can focus it.
    pub body: BodyId,
    /// Short headline.
    pub headline: String,
    /// Fuller explanation on hover.
    pub detail: String,
    /// Whether this is urgent rather than merely notable.
    pub urgent: bool,
}

/// The readouts for the focused globe, plus the current threat list.
#[derive(Resource, Default)]
pub struct MapReadout {
    key: Option<(BodyId, MapMode, GameDate)>,
    /// Per-province readout for the focused body.
    pub provinces: BTreeMap<ProvinceId, ProvinceReadout>,
    /// The legend for the active mode, from worst to best.
    pub legend: Vec<(String, [u8; 3])>,
    /// Everything currently demanding the player's attention.
    pub situation: Vec<SituationItem>,
}

/// Blends between two colours, `t` in 0..=1000.
fn mix(low: [u8; 3], high: [u8; 3], t: i32) -> [u8; 3] {
    let t = t.clamp(0, 1000);
    let channel = |a: u8, b: u8| -> u8 {
        let a = i32::from(a);
        let b = i32::from(b);
        (a + (b - a) * t / 1000) as u8
    };
    [
        channel(low[0], high[0]),
        channel(low[1], high[1]),
        channel(low[2], high[2]),
    ]
}

/// Washes a house colour toward neutral, `fade` in 0..=1000.
fn mine_faded(colour: [u8; 3], fade: i32) -> [u8; 3] {
    mix(colour, NEUTRAL, fade)
}

/// A red-amber-green ramp, `t` in 0..=1000.
fn ramp(t: i32) -> [u8; 3] {
    if t < 500 {
        mix([190, 60, 55], [214, 170, 60], t * 2)
    } else {
        mix([214, 170, 60], [90, 175, 95], (t - 500) * 2)
    }
}

/// Recomputes the readout when the focused body, the mode, or the day
/// changes.
pub fn refresh_map_readout(world: &mut World) {
    let (Some(view), Some(mode)) = (
        world.get_resource::<ViewState>().copied(),
        world.get_resource::<MapMode>().copied(),
    ) else {
        return;
    };
    let Some(date) = world.get_resource::<CampaignClock>().map(|c| c.date) else {
        return;
    };
    let MapView::Body(body) = view.view else {
        return;
    };
    let key = (body, mode, date);
    if world
        .get_resource::<MapReadout>()
        .is_some_and(|readout| readout.key == Some(key))
    {
        return;
    }

    let player = world.get_resource::<PlayerHouse>().and_then(|p| p.0);
    let provinces: Vec<(ProvinceId, ProvinceRecord)> = world
        .iter_entities()
        .filter_map(|entity| entity.get::<ProvinceRecord>().cloned())
        .filter(|record| record.body == body)
        .map(|record| (record.id, record))
        .collect();

    let mut readouts = BTreeMap::new();
    for (id, record) in &provinces {
        readouts.insert(
            *id,
            readout_for(world, mode, *id, record, player, &provinces),
        );
    }

    let legend = legend_for(world, mode, &provinces);
    let situation = situation_for(world, player, body);

    let mut readout = world.resource_mut::<MapReadout>();
    readout.key = Some(key);
    readout.provinces = readouts;
    readout.legend = legend;
    readout.situation = situation;
}

/// The organisation holding a province, if any.
fn holder_of(world: &World, province: ProvinceId) -> Option<OrgId> {
    let index = world.get_resource::<PoliticsIndex>()?;
    let title = index.province_titles.get(&province)?;
    let entity = index.titles.get(title)?;
    match world.get::<TitleRecord>(*entity)?.holder {
        TitleHolder::Org(org) => Some(org),
        _ => None,
    }
}

/// The great house at the top of an organisation's liege chain.
fn great_house_of(world: &World, start: OrgId) -> OrgId {
    let Some(index) = world.get_resource::<PoliticsIndex>() else {
        return start;
    };
    let mut current = start;
    for _ in 0..16 {
        let Some(record) = index
            .orgs
            .get(&current)
            .and_then(|entity| world.get::<OrgRecord>(*entity))
        else {
            break;
        };
        match (record.tier, record.liege) {
            (Some(HouseTier::Vassal), Some(liege)) => current = liege,
            _ => break,
        }
    }
    current
}

/// An organisation's authored colour.
fn org_colour(world: &World, org: OrgId) -> [u8; 3] {
    let Some(content) = world.get_resource::<ContentDb>() else {
        return NEUTRAL;
    };
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.orgs.get(&org).copied())
        .and_then(|entity| world.get::<OrgRecord>(entity))
        .and_then(|record| content.0.organisations.get(&record.key))
        .map(|def| [def.color.0, def.color.1, def.color.2])
        .unwrap_or(NEUTRAL)
}

/// An organisation's display name.
fn org_name(world: &World, org: OrgId) -> String {
    let Some(content) = world.get_resource::<ContentDb>() else {
        return String::new();
    };
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.orgs.get(&org).copied())
        .and_then(|entity| world.get::<OrgRecord>(entity))
        .and_then(|record| content.0.organisations.get(&record.key))
        .map(|def| def.name.clone())
        .unwrap_or_default()
}

/// The head of an organisation.
fn head_of(world: &World, org: OrgId) -> Option<aeon_sim::CharacterId> {
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.orgs.get(&org).copied())
        .and_then(|entity| world.get::<OrgRecord>(entity))
        .and_then(|record| record.head)
}

/// Total manpower standing in a province, and whose it mostly is.
fn garrison_in(world: &World, province: ProvinceId) -> (i64, Option<OrgId>) {
    let mut total = 0;
    let mut strongest: Option<(i64, OrgId)> = None;
    for entity in world.iter_entities() {
        let Some(army) = entity.get::<ArmyRecord>() else {
            continue;
        };
        if army.location != province {
            continue;
        }
        total += army.manpower;
        if strongest.is_none_or(|(men, _)| army.manpower > men) {
            strongest = Some((army.manpower, army.owner));
        }
    }
    (total, strongest.map(|(_, org)| org))
}

fn readout_for(
    world: &World,
    mode: MapMode,
    province: ProvinceId,
    record: &ProvinceRecord,
    player: Option<OrgId>,
    all: &[(ProvinceId, ProvinceRecord)],
) -> ProvinceReadout {
    let holder = holder_of(world, province);
    match mode {
        MapMode::Holder | MapMode::GreatHouse => {
            let painted = holder.map(|org| {
                if mode == MapMode::GreatHouse {
                    great_house_of(world, org)
                } else {
                    org
                }
            });
            ProvinceReadout {
                colour: painted.map(|org| org_colour(world, org)).unwrap_or(NEUTRAL),
                value: None,
                hint: match painted {
                    Some(org) => format!("Held by {}", org_name(world, org)),
                    None => "Unclaimed — in revolt or never held".to_owned(),
                },
                alert: holder.is_none(),
            }
        }
        MapMode::MyControl => {
            // Hop distance up the chain of vassalage, not the great house at
            // the top of it: a vassal player's own ground is its own, and
            // its liege's ground is not the player's at all.
            let hops = match (holder, player) {
                (Some(holder), Some(player)) => answers_to(world, holder, player),
                _ => None,
            };
            let mine = player.map(|org| org_colour(world, org)).unwrap_or(NEUTRAL);
            match (hops, holder) {
                (Some(0), _) => ProvinceReadout {
                    colour: mine,
                    value: Some("direct".to_owned()),
                    hint: "Held directly by your house".to_owned(),
                    alert: false,
                },
                (Some(hops), Some(holder)) => {
                    // Each step away washes the colour further toward neutral.
                    let fade = (hops.min(3) * 300).min(750) as i32;
                    ProvinceReadout {
                        colour: mine_faded(mine, fade),
                        value: Some(format!("via {}", org_name(world, holder))),
                        hint: format!(
                            "Held by {}, {} step{} down your chain of vassalage",
                            org_name(world, holder),
                            hops,
                            if hops == 1 { "" } else { "s" }
                        ),
                        alert: false,
                    }
                }
                (_, Some(holder)) => ProvinceReadout {
                    colour: NEUTRAL,
                    value: None,
                    hint: format!("{} — outside your realm", org_name(world, holder)),
                    alert: false,
                },
                (_, None) => ProvinceReadout {
                    colour: NEUTRAL,
                    value: None,
                    hint: "Unclaimed — answers to nobody".to_owned(),
                    alert: false,
                },
            }
        }
        MapMode::Order => {
            let state: ProvincialOrder = province_order(world, province);
            let pressure = pressures(world, province);
            let percent = state.order * 100 / ORDER_MAX;
            let mut hint = format!("Order {percent}% — {}", pressure.describe());
            if let Some(days) = state.days_to_revolt() {
                hint.push_str(&format!("\nIn unrest: revolts in {days} days"));
            }
            ProvinceReadout {
                colour: if state.in_unrest() {
                    [150, 40, 40]
                } else {
                    ramp(state.order * 1000 / ORDER_MAX)
                },
                value: Some(format!("{percent}%")),
                hint,
                alert: state.in_unrest(),
            }
        }
        MapMode::Wealth => {
            let content = world.get_resource::<ContentDb>();
            let output = content
                .and_then(|db| db.0.provinces.get(&record.key).map(|def| def.wealth_output))
                .unwrap_or(0);
            let best = all
                .iter()
                .filter_map(|(_, other)| {
                    content
                        .and_then(|db| db.0.provinces.get(&other.key).map(|def| def.wealth_output))
                })
                .max()
                .unwrap_or(1)
                .max(1);
            ProvinceReadout {
                colour: mix([60, 55, 45], [235, 200, 90], (output * 1000 / best) as i32),
                value: Some(output.to_string()),
                hint: format!("Worth {output} wealth a month at full order"),
                alert: false,
            }
        }
        MapMode::Military => {
            let (men, owner) = garrison_in(world, province);
            let best = all
                .iter()
                .map(|(id, _)| garrison_in(world, *id).0)
                .max()
                .unwrap_or(1)
                .max(1);
            let friendly = owner.is_some() && owner == player;
            let base = if men == 0 {
                NEUTRAL
            } else if friendly {
                mix([50, 70, 110], [110, 170, 240], (men * 1000 / best) as i32)
            } else {
                mix([110, 60, 55], [240, 120, 100], (men * 1000 / best) as i32)
            };
            ProvinceReadout {
                colour: base,
                value: (men > 0).then(|| men.to_string()),
                hint: match owner {
                    Some(org) if men > 0 => {
                        format!(
                            "{men} men here, the strongest under {}",
                            org_name(world, org)
                        )
                    }
                    _ => "No troops standing here".to_owned(),
                },
                alert: !friendly && men > 0 && holder == player,
            }
        }
        MapMode::PlayerRelations => {
            let player_head = player.and_then(|org| head_of(world, org));
            match (holder, player) {
                (Some(holder), Some(player)) if holder == player => ProvinceReadout {
                    colour: [80, 130, 200],
                    value: Some("you".to_owned()),
                    hint: "Your own holding".to_owned(),
                    alert: false,
                },
                (Some(holder), _) => {
                    let opinion = match (head_of(world, holder), player_head) {
                        (Some(them), Some(you)) => opinion_between(world, them, you),
                        _ => 0,
                    };
                    ProvinceReadout {
                        colour: ramp((opinion.clamp(-100, 100) + 100) * 5),
                        value: Some(format!("{opinion:+}")),
                        hint: format!(
                            "{} regards your house at {opinion:+}",
                            org_name(world, holder)
                        ),
                        alert: opinion <= -50,
                    }
                }
                _ => ProvinceReadout {
                    hint: "Unclaimed".to_owned(),
                    alert: true,
                    ..Default::default()
                },
            }
        }
        MapMode::ClaimPressure => {
            // A province's weight in the paramountcy race is its holder's
            // share of the body: whoever holds the most leads it.
            let mut counts: BTreeMap<OrgId, usize> = BTreeMap::new();
            for (id, _) in all {
                if let Some(org) = holder_of(world, *id) {
                    *counts.entry(org).or_default() += 1;
                }
            }
            let leader = counts
                .iter()
                .max_by_key(|(_, count)| **count)
                .map(|(org, _)| *org);
            let best = counts.values().copied().max().unwrap_or(0);
            match holder {
                Some(org) => {
                    let held = counts.get(&org).copied().unwrap_or(0);
                    let leading = Some(org) == leader;
                    ProvinceReadout {
                        colour: if leading {
                            [230, 190, 70]
                        } else {
                            mix(
                                [70, 70, 80],
                                [190, 150, 90],
                                (held * 1000 / best.max(1)) as i32,
                            )
                        },
                        value: Some(held.to_string()),
                        hint: format!(
                            "{} holds {held} of {} provinces here{}",
                            org_name(world, org),
                            all.len(),
                            if leading {
                                " — currently leading the claim"
                            } else {
                                ""
                            }
                        ),
                        alert: leading && Some(org) != player,
                    }
                }
                None => ProvinceReadout {
                    value: Some("—".to_owned()),
                    hint: "Unclaimed: it counts for nobody's claim".to_owned(),
                    alert: false,
                    ..Default::default()
                },
            }
        }
    }
}

fn legend_for(
    world: &World,
    mode: MapMode,
    provinces: &[(ProvinceId, ProvinceRecord)],
) -> Vec<(String, [u8; 3])> {
    match mode {
        MapMode::Holder | MapMode::GreatHouse => {
            // Political modes legend themselves: list the houses on show.
            let mut seen: BTreeMap<OrgId, [u8; 3]> = BTreeMap::new();
            for (id, _) in provinces {
                if let Some(org) = holder_of(world, *id) {
                    let painted = if mode == MapMode::GreatHouse {
                        great_house_of(world, org)
                    } else {
                        org
                    };
                    seen.entry(painted)
                        .or_insert_with(|| org_colour(world, painted));
                }
            }
            let mut legend: Vec<(String, [u8; 3])> = seen
                .into_iter()
                .map(|(org, colour)| (org_name(world, org), colour))
                .collect();
            legend.sort_by(|a, b| a.0.cmp(&b.0));
            legend
        }
        MapMode::MyControl => {
            let mine = world
                .get_resource::<PlayerHouse>()
                .and_then(|p| p.0)
                .map(|org| org_colour(world, org))
                .unwrap_or(NEUTRAL);
            vec![
                ("Held directly".to_owned(), mine),
                ("Through a vassal".to_owned(), mine_faded(mine, 300)),
                ("Further down".to_owned(), mine_faded(mine, 600)),
                ("Not your realm".to_owned(), NEUTRAL),
            ]
        }
        MapMode::Order => vec![
            ("In unrest".to_owned(), [150, 40, 40]),
            ("Restive".to_owned(), ramp(250)),
            ("Settled".to_owned(), ramp(750)),
            ("Loyal".to_owned(), ramp(1000)),
        ],
        MapMode::Wealth => vec![
            ("Poor".to_owned(), mix([60, 55, 45], [235, 200, 90], 0)),
            (
                "Middling".to_owned(),
                mix([60, 55, 45], [235, 200, 90], 500),
            ),
            ("Rich".to_owned(), mix([60, 55, 45], [235, 200, 90], 1000)),
        ],
        MapMode::Military => vec![
            ("Empty".to_owned(), NEUTRAL),
            ("Yours".to_owned(), mix([50, 70, 110], [110, 170, 240], 900)),
            (
                "Others".to_owned(),
                mix([110, 60, 55], [240, 120, 100], 900),
            ),
        ],
        MapMode::PlayerRelations => vec![
            ("Hostile".to_owned(), ramp(0)),
            ("Indifferent".to_owned(), ramp(500)),
            ("Friendly".to_owned(), ramp(1000)),
            ("Yours".to_owned(), [80, 130, 200]),
        ],
        MapMode::ClaimPressure => vec![
            ("Out of it".to_owned(), [70, 70, 80]),
            (
                "Contending".to_owned(),
                mix([70, 70, 80], [190, 150, 90], 700),
            ),
            ("Leading".to_owned(), [230, 190, 70]),
        ],
    }
}

/// The owner of the strongest force in a province, when it is not the
/// player's own.
fn owner_of_hostile_force(world: &World, province: ProvinceId, player: OrgId) -> Option<OrgId> {
    let (men, owner) = garrison_in(world, province);
    match owner {
        Some(owner) if men > 0 && owner != player => Some(owner),
        _ => None,
    }
}

/// Everything currently demanding the player's attention, worst first.
fn situation_for(world: &World, player: Option<OrgId>, body: BodyId) -> Vec<SituationItem> {
    let Some(player) = player else {
        return Vec::new();
    };
    let mut items = Vec::new();

    let provinces: Vec<ProvinceRecord> = world
        .iter_entities()
        .filter_map(|entity| entity.get::<ProvinceRecord>().cloned())
        .collect();

    for record in &provinces {
        let province = record.id;
        let holder = holder_of(world, province);
        let name = world
            .get_resource::<aeon_sim::MapIndex>()
            .and_then(|index| index.provinces.get(&province).copied())
            .and_then(|entity| world.get::<aeon_sim::DisplayName>(entity))
            .map(|display| display.0.clone())
            .unwrap_or_default();

        // Our own ground slipping out of our hands.
        if holder == Some(player) {
            let state = province_order(world, province);
            if let Some(days) = state.days_to_revolt() {
                items.push(SituationItem {
                    province,
                    body: record.body,
                    headline: format!("{name} revolts in {days}d"),
                    detail: format!(
                        "{name} is in open unrest. Garrison it, go there in person, \
                         or hold court to restore order before it throws you off."
                    ),
                    urgent: days <= 30,
                });
            } else if state.order < aeon_sim::order::ORDER_START / 2 {
                items.push(SituationItem {
                    province,
                    body: record.body,
                    headline: format!("{name} is restive"),
                    detail: format!(
                        "Order in {name} has fallen to {}%. It will not recover \
                         unless you attend to it.",
                        state.order * 100 / ORDER_MAX
                    ),
                    urgent: false,
                });
            }

            // Someone else's troops on our ground.
            if let Some(occupier) = owner_of_hostile_force(world, province, player) {
                let (men, _) = garrison_in(world, province);
                items.push(SituationItem {
                    province,
                    body: record.body,
                    headline: format!("{name} occupied"),
                    detail: format!(
                        "{} has {men} men standing in {name}, and its order is \
                         falling while they remain.",
                        org_name(world, occupier)
                    ),
                    urgent: true,
                });
            }
        }
    }

    // Urgent first, then by province, and keep the strip readable.
    items.sort_by(|a, b| {
        b.urgent
            .cmp(&a.urgent)
            .then_with(|| a.province.cmp(&b.province))
    });
    let _ = body;
    items.truncate(12);
    items
}
