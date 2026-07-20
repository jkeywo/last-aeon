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
use aeon_sim::crisis::{dominant_claimant, province_counts_on};
use aeon_sim::forces::garrison_in;
use aeon_sim::map::ProvinceRecord;
use aeon_sim::order::{ORDER_MAX, ProvincialOrder, pressures, province_order};
use aeon_sim::politics::great_house_of;
use aeon_sim::state::ContentDb;
use aeon_sim::warfare::province_holder;
use aeon_sim::{
    BodyId, CampaignClock, OrgId, OrgRecord, PlayerHouse, PoliticsIndex, ProvinceId, TextDb,
    answers_to, opinion_between,
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

fn readout_for(
    world: &World,
    mode: MapMode,
    province: ProvinceId,
    record: &ProvinceRecord,
    player: Option<OrgId>,
    all: &[(ProvinceId, ProvinceRecord)],
) -> ProvinceReadout {
    let holder = province_holder(world, province);
    let strings = world.resource::<TextDb>();
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
                    Some(org) => strings.format(
                        "ui.map-mode.holder.hint.held",
                        &[("holder", &org_name(world, org))],
                    ),
                    None => strings.text("ui.map-mode.holder.hint.lost").to_owned(),
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
                    value: Some(strings.text("ui.map-mode.holder.value.direct").to_owned()),
                    hint: strings.text("ui.map-mode.holder.hint.direct").to_owned(),
                    alert: false,
                },
                (Some(hops), Some(holder)) => {
                    // Each step away washes the colour further toward neutral.
                    let fade = (hops.min(3) * 300).min(750) as i32;
                    let name = org_name(world, holder);
                    ProvinceReadout {
                        colour: mine_faded(mine, fade),
                        value: Some(
                            strings.format("ui.map-mode.holder.value.vassal", &[("holder", &name)]),
                        ),
                        hint: strings.format_plural(
                            "ui.map-mode.holder.hint.vassal",
                            i64::from(hops),
                            &[("holder", &name), ("hops", &hops.to_string())],
                        ),
                        alert: false,
                    }
                }
                (_, Some(holder)) => ProvinceReadout {
                    colour: NEUTRAL,
                    value: None,
                    hint: strings.format(
                        "ui.map-mode.holder.hint.outside",
                        &[("holder", &org_name(world, holder))],
                    ),
                    alert: false,
                },
                (_, None) => ProvinceReadout {
                    colour: NEUTRAL,
                    value: None,
                    hint: strings.text("ui.map-mode.holder.hint.unclaimed").to_owned(),
                    alert: false,
                },
            }
        }
        MapMode::Order => {
            let state: ProvincialOrder = province_order(world, province);
            let pressure = pressures(world, province);
            let percent = state.order * 100 / ORDER_MAX;
            let mut hint = strings.format(
                "ui.map-mode.order.hint",
                &[
                    ("percent", &percent.to_string()),
                    ("pressure", &pressure.describe(strings)),
                ],
            );
            if let Some(days) = state.days_to_revolt() {
                hint.push('\n');
                hint.push_str(&strings.format(
                    "ui.map-mode.order.hint.revolt",
                    &[("days", &days.to_string())],
                ));
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
                hint: strings.format(
                    "ui.map-mode.wealth.hint",
                    &[("output", &output.to_string())],
                ),
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
                    Some(org) if men > 0 => strings.format(
                        "ui.map-mode.military.hint.garrison",
                        &[("men", &men.to_string()), ("owner", &org_name(world, org))],
                    ),
                    _ => strings.text("ui.map-mode.military.hint.empty").to_owned(),
                },
                alert: !friendly && men > 0 && holder == player,
            }
        }
        MapMode::PlayerRelations => {
            let player_head = player.and_then(|org| aeon_sim::access::org_head(world, org));
            match (holder, player) {
                (Some(holder), Some(player)) if holder == player => ProvinceReadout {
                    colour: [80, 130, 200],
                    value: Some(strings.text("ui.map-mode.relations.value.you").to_owned()),
                    hint: strings.text("ui.map-mode.relations.hint.you").to_owned(),
                    alert: false,
                },
                (Some(holder), _) => {
                    let opinion = match (aeon_sim::access::org_head(world, holder), player_head) {
                        (Some(them), Some(you)) => opinion_between(world, them, you),
                        _ => 0,
                    };
                    ProvinceReadout {
                        colour: ramp((opinion.clamp(-100, 100) + 100) * 5),
                        value: Some(format!("{opinion:+}")),
                        hint: strings.format(
                            "ui.map-mode.relations.hint.opinion",
                            &[
                                ("holder", &org_name(world, holder)),
                                ("opinion", &format!("{opinion:+}")),
                            ],
                        ),
                        alert: opinion <= -50,
                    }
                }
                _ => ProvinceReadout {
                    hint: strings
                        .text("ui.map-mode.relations.hint.unclaimed")
                        .to_owned(),
                    alert: true,
                    ..Default::default()
                },
            }
        }
        MapMode::ClaimPressure => {
            // A province's weight in the paramountcy race is its holder's
            // share of the body. The simulation's own dominance test says
            // who is leading; with the counts tied nobody is — exactly as
            // the claim job would refuse a tied claimant.
            let counts = province_counts_on(world, record.body);
            let leader = dominant_claimant(world, record.body);
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
                        hint: strings.format(
                            if leading {
                                "ui.map-mode.claim.hint.leading"
                            } else {
                                "ui.map-mode.claim.hint.held"
                            },
                            &[
                                ("holder", &org_name(world, org)),
                                ("held", &held.to_string()),
                                ("total", &all.len().to_string()),
                            ],
                        ),
                        alert: leading && Some(org) != player,
                    }
                }
                None => ProvinceReadout {
                    value: Some(strings.text("ui.map-mode.claim.value.unclaimed").to_owned()),
                    hint: strings.text("ui.map-mode.claim.hint.unclaimed").to_owned(),
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
    let strings = world.resource::<TextDb>();
    let key = |suffix: &str| strings.text(suffix).to_owned();
    match mode {
        MapMode::Holder | MapMode::GreatHouse => {
            // Political modes legend themselves: list the houses on show.
            let mut seen: BTreeMap<OrgId, [u8; 3]> = BTreeMap::new();
            for (id, _) in provinces {
                if let Some(org) = province_holder(world, *id) {
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
                (key("ui.legend.my-control.direct"), mine),
                (key("ui.legend.my-control.vassal"), mine_faded(mine, 300)),
                (key("ui.legend.my-control.further"), mine_faded(mine, 600)),
                (key("ui.legend.my-control.outside"), NEUTRAL),
            ]
        }
        MapMode::Order => vec![
            (key("ui.legend.order.unrest"), [150, 40, 40]),
            (key("ui.legend.order.restive"), ramp(250)),
            (key("ui.legend.order.settled"), ramp(750)),
            (key("ui.legend.order.loyal"), ramp(1000)),
        ],
        MapMode::Wealth => vec![
            (
                key("ui.legend.wealth.poor"),
                mix([60, 55, 45], [235, 200, 90], 0),
            ),
            (
                key("ui.legend.wealth.middling"),
                mix([60, 55, 45], [235, 200, 90], 500),
            ),
            (
                key("ui.legend.wealth.rich"),
                mix([60, 55, 45], [235, 200, 90], 1000),
            ),
        ],
        MapMode::Military => vec![
            (key("ui.legend.military.empty"), NEUTRAL),
            (
                key("ui.legend.military.yours"),
                mix([50, 70, 110], [110, 170, 240], 900),
            ),
            (
                key("ui.legend.military.others"),
                mix([110, 60, 55], [240, 120, 100], 900),
            ),
        ],
        MapMode::PlayerRelations => vec![
            (key("ui.legend.relations.hostile"), ramp(0)),
            (key("ui.legend.relations.indifferent"), ramp(500)),
            (key("ui.legend.relations.friendly"), ramp(1000)),
            (key("ui.legend.relations.yours"), [80, 130, 200]),
        ],
        MapMode::ClaimPressure => vec![
            (key("ui.legend.claim.out"), [70, 70, 80]),
            (
                key("ui.legend.claim.contending"),
                mix([70, 70, 80], [190, 150, 90], 700),
            ),
            (key("ui.legend.claim.leading"), [230, 190, 70]),
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
    let strings = world.resource::<TextDb>();
    let mut items = Vec::new();

    let provinces: Vec<ProvinceRecord> = world
        .iter_entities()
        .filter_map(|entity| entity.get::<ProvinceRecord>().cloned())
        .collect();

    for record in &provinces {
        let province = record.id;
        let holder = province_holder(world, province);
        let name = aeon_sim::access::province_name(world, province);

        // Our own ground slipping out of our hands.
        if holder == Some(player) {
            let state = province_order(world, province);
            if let Some(days) = state.days_to_revolt() {
                items.push(SituationItem {
                    province,
                    body: record.body,
                    headline: strings.format(
                        "ui.situation.revolt.headline",
                        &[("province", &name), ("days", &days.to_string())],
                    ),
                    detail: strings.format("ui.situation.revolt.detail", &[("province", &name)]),
                    urgent: days <= 30,
                });
            } else if state.order < aeon_sim::order::ORDER_START / 2 {
                items.push(SituationItem {
                    province,
                    body: record.body,
                    headline: strings
                        .format("ui.situation.restive.headline", &[("province", &name)]),
                    detail: strings.format(
                        "ui.situation.restive.detail",
                        &[
                            ("province", &name),
                            ("percent", &(state.order * 100 / ORDER_MAX).to_string()),
                        ],
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
                    headline: strings
                        .format("ui.situation.occupied.headline", &[("province", &name)]),
                    detail: strings.format(
                        "ui.situation.occupied.detail",
                        &[
                            ("province", &name),
                            ("occupier", &org_name(world, occupier)),
                            ("men", &men.to_string()),
                        ],
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
