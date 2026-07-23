//! The goods economy: a world that cannot feed itself falls into want,
//! and a world with a surplus enriches the house that holds it.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::economy::OrgResources;
use aeon_sim::map::MapIndex;
use aeon_sim::order::{ORDER_START, province_order};
use aeon_sim::{CampaignConfig, OrgId, PoliticsIndex, ProvinceId, SimHost};

// Ash holds a farming world and a hungry moon. The world makes far more
// grain than it eats; the moon eats grain and makes none.
const FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_body(#{ id: "moon", kind: "moon", radius_km: 1700,
              parent: "world", orbit_radius_mm: 384, orbit_days: 27 });

define_good(#{ id: "grain", value: 2 });

// Zero the plain outputs so the only wealth in motion is trade.
define_province(#{ id: "alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0,
                   wealth_output: 0, manpower_output: 0, supplies_output: 0,
                   produces: #{ grain: 40 } });
define_province(#{ id: "luna", body: "moon",
                   latitude_mdeg: 0, longitude_mdeg: 0,
                   wealth_output: 0, manpower_output: 0, supplies_output: 0,
                   consumes: #{ grain: 30 } });

define_house(#{
    id: "ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha", "luna"],
    wealth: 500, manpower: 5000, supplies: 800, legitimacy: 60,
});
define_character(#{
    id: "aron-ash", gender: "male",
    birth_year: 370, organisation: "ash",
    skills: #{ command: 8, diplomacy: 12, intrigue: 4, stewardship: 7 },
});
"#;

fn content() -> Arc<aeon_data::ContentSet> {
    let (set, report) = load_content(
        &[ContentSource {
            path: "fixture.rhai".to_owned(),
            source: FIXTURE.to_owned(),
        }],
        &aeon_data::StringTable::blank(),
    );
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    Arc::new(set.unwrap())
}

fn host(seed: u64) -> SimHost {
    SimHost::new_with_content(
        CampaignConfig {
            name: "Trade Trial".to_owned(),
            seed,
            start_date: CalendarDate {
                year: 411,
                month: 1,
                day: 1,
            }
            .to_date()
            .unwrap(),
        },
        content(),
    )
}

fn key(text: &str) -> ContentKey {
    ContentKey::new(text).unwrap()
}

fn org(h: &mut SimHost, name: &str) -> OrgId {
    h.world_mut().resource::<PoliticsIndex>().org_keys[&key(name)]
}

fn province(h: &mut SimHost, name: &str) -> ProvinceId {
    h.world_mut().resource::<MapIndex>().province_keys[&key(name)]
}

fn wealth(h: &mut SimHost, house: OrgId) -> i64 {
    let entity = aeon_sim::access::org_entity(h.world_mut(), house).unwrap();
    h.world_mut().get::<OrgResources>(entity).unwrap().wealth
}

#[test]
fn a_body_nets_what_its_provinces_make_and_want() {
    let mut h = host(61);
    let world = h.world_mut().resource::<MapIndex>().body_keys[&key("world")];
    let moon = h.world_mut().resource::<MapIndex>().body_keys[&key("moon")];

    assert_eq!(
        aeon_sim::trade::body_balance(h.world_mut(), world)[&key("grain")],
        40,
        "the farming world nets a grain surplus"
    );
    assert_eq!(
        aeon_sim::trade::body_balance(h.world_mut(), moon)[&key("grain")],
        -30,
        "the hungry moon nets a grain deficit"
    );
    assert!(!aeon_sim::trade::body_in_want(h.world_mut(), world));
    assert!(aeon_sim::trade::body_in_want(h.world_mut(), moon));
}

#[test]
fn a_world_in_want_loses_order() {
    let mut h = host(62);
    let luna = province(&mut h, "luna");
    let alpha = province(&mut h, "alpha");
    let before = province_order(h.world_mut(), luna).order;

    h.advance_days(30);

    assert!(
        province_order(h.world_mut(), luna).order < before,
        "a province on a world that cannot feed itself slips into disorder"
    );
    assert_eq!(
        province_order(h.world_mut(), alpha).order,
        ORDER_START,
        "a province on a self-sufficient world feels no such want"
    );
}

#[test]
fn a_world_in_surplus_enriches_its_holder() {
    let mut h = host(63);
    let ash = org(&mut h, "ash");
    let before = wealth(&mut h, ash);

    h.advance_days(30);

    // The world's grain surplus (40) at value 2 sells for 80, split among
    // its one held province — and the plain outputs are zeroed, so trade
    // is the only wealth in motion.
    assert_eq!(
        wealth(&mut h, ash) - before,
        80,
        "the surplus of a prosperous world reaches the house that holds it"
    );
}

#[test]
fn the_goods_economy_replays_and_survives_a_snapshot() {
    let run = || {
        let mut h = host(64);
        h.advance_days(90);
        h
    };
    let mut a = run();
    let b = run();
    assert_eq!(
        a.state_hash(),
        b.state_hash(),
        "goods flows replay identically"
    );

    let snapshot = a.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(restored.state_hash(), a.state_hash());
    a.advance_days(60);
    restored.advance_days(60);
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "and continue identically afterwards"
    );
}

// ---------------------------------------------------------------------------
// Buildings
// ---------------------------------------------------------------------------

const BUILD_FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_good(#{ id: "grain", value: 2 });

// Alpha eats grain and grows none: the world is in want until a granary
// is raised.
define_province(#{ id: "alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0,
                   wealth_output: 0, manpower_output: 0, supplies_output: 0,
                   consumes: #{ grain: 10 } });

define_building(#{
    id: "granary", build_days: 20,
    wealth_cost: 0, supplies_cost: 0,
    adds_wealth: 5, produces: #{ grain: 15 },
});
define_assignment(#{
    id: "raise-a-granary", category: "consequential",
    duration_days: 20, skill: "stewardship", difficulty: 2, target: "province",
    requires: #{ target_holder: "own" },
    results: #{
        success: #{ weight: 999, effect_fn: "built" },
        failure: #{ weight: 1 },
    },
});
fn built(ctx) { [#{ kind: "construct", building: "granary" }] }

define_house(#{
    id: "ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha"],
    wealth: 500, manpower: 5000, supplies: 800, legitimacy: 60,
});
define_character(#{
    id: "aron-ash", gender: "male",
    birth_year: 370, organisation: "ash",
    skills: #{ command: 8, diplomacy: 6, intrigue: 4, stewardship: 16 },
});
"#;

fn build_host(seed: u64) -> SimHost {
    let (set, report) = load_content(
        &[ContentSource {
            path: "fixture.rhai".to_owned(),
            source: BUILD_FIXTURE.to_owned(),
        }],
        &aeon_data::StringTable::blank(),
    );
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    SimHost::new_with_content(
        CampaignConfig {
            name: "Build Trial".to_owned(),
            seed,
            start_date: CalendarDate {
                year: 411,
                month: 1,
                day: 1,
            }
            .to_date()
            .unwrap(),
        },
        Arc::new(set.unwrap()),
    )
}

#[test]
fn a_raised_building_changes_the_balance_and_the_wealth() {
    use aeon_sim::{AssignmentTarget, PlayerCommand};

    let mut h = build_host(71);
    let world = h.world_mut().resource::<MapIndex>().body_keys[&key("world")];
    let alpha = province(&mut h, "alpha");
    let aron = h.world_mut().resource::<PoliticsIndex>().character_keys
        [&ContentKey::new("aron-ash").unwrap()];

    assert!(
        aeon_sim::trade::body_in_want(h.world_mut(), world),
        "the world starts hungry, with no grain grown"
    );

    // The player raises a granary on Alpha; it may take a couple of tries
    // if the work happens to fail, so keep asking until it stands.
    let built = ContentKey::new("granary").unwrap();
    for _ in 0..6 {
        let has = {
            let entity = h.world_mut().resource::<MapIndex>().provinces[&alpha];
            h.world_mut()
                .get::<aeon_sim::trade::Buildings>(entity)
                .is_some_and(|b| b.0.contains(&built))
        };
        if has {
            break;
        }
        let _ = h.submit(PlayerCommand::StartAssignment {
            assignment: ContentKey::new("raise-a-granary").unwrap(),
            leader: aron,
            target: AssignmentTarget::Province(alpha),
        });
        h.advance_days(22);
    }

    let entity = h.world_mut().resource::<MapIndex>().provinces[&alpha];
    assert!(
        h.world_mut()
            .get::<aeon_sim::trade::Buildings>(entity)
            .is_some_and(|b| b.0.contains(&built)),
        "the granary should stand after the work is seen through"
    );
    // Grain now nets +5 (15 grown less 10 eaten), so the want is answered,
    // and the province's wealth output has risen with it.
    assert_eq!(
        aeon_sim::trade::body_balance(h.world_mut(), world)[&key("grain")],
        5
    );
    assert!(!aeon_sim::trade::body_in_want(h.world_mut(), world));
    assert_eq!(
        aeon_sim::trade::building_wealth_bonus(h.world_mut(), alpha),
        5
    );
}

#[test]
fn a_raised_building_survives_a_snapshot() {
    let mut h = build_host(72);
    let alpha = province(&mut h, "alpha");
    let entity = h.world_mut().resource::<MapIndex>().provinces[&alpha];
    // Plant the building directly, then round-trip.
    h.world_mut()
        .get_mut::<aeon_sim::trade::Buildings>(entity)
        .unwrap()
        .0
        .push(ContentKey::new("granary").unwrap());
    let content = h
        .world_mut()
        .resource::<aeon_sim::state::ContentDb>()
        .0
        .clone();

    let snapshot = h.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content).unwrap();
    let restored_entity = restored.world_mut().resource::<MapIndex>().provinces[&alpha];
    assert!(
        restored
            .world_mut()
            .get::<aeon_sim::trade::Buildings>(restored_entity)
            .is_some_and(|b| b.0.contains(&ContentKey::new("granary").unwrap())),
        "a raised building is campaign state and rides the snapshot"
    );
}

// ---------------------------------------------------------------------------
// Trade routes
// ---------------------------------------------------------------------------

const ROUTE_FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_body(#{ id: "moon", kind: "moon", radius_km: 1700,
              parent: "world", orbit_radius_mm: 384, orbit_days: 27 });
define_good(#{ id: "grain", value: 2 });

define_province(#{ id: "alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0,
                   wealth_output: 0, manpower_output: 0, supplies_output: 0,
                   produces: #{ grain: 30 } });
define_province(#{ id: "luna", body: "moon",
                   latitude_mdeg: 0, longitude_mdeg: 0,
                   wealth_output: 0, manpower_output: 0, supplies_output: 0,
                   consumes: #{ grain: 20 } });

define_house(#{
    id: "ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha", "luna"],
    wealth: 500, manpower: 5000, supplies: 800, legitimacy: 60,
});
define_character(#{
    id: "aron-ash", gender: "male",
    birth_year: 370, organisation: "ash",
    skills: #{ command: 8, diplomacy: 12, intrigue: 4, stewardship: 7 },
});

// A transport to ply the route, and a patrol boat to blockade with.
define_ship(#{ id: "hauler", class: "transport", owner: "ash", location: "alpha" });
define_ship(#{ id: "picket", class: "patrol", owner: "ash", location: "luna" });
"#;

fn route_host(seed: u64) -> SimHost {
    let (set, report) = load_content(
        &[ContentSource {
            path: "fixture.rhai".to_owned(),
            source: ROUTE_FIXTURE.to_owned(),
        }],
        &aeon_data::StringTable::blank(),
    );
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    SimHost::new_with_content(
        CampaignConfig {
            name: "Route Trial".to_owned(),
            seed,
            start_date: CalendarDate {
                year: 411,
                month: 1,
                day: 1,
            }
            .to_date()
            .unwrap(),
        },
        Arc::new(set.unwrap()),
    )
}

fn ship(h: &mut SimHost, name: &str) -> aeon_sim::ShipId {
    h.world_mut().resource::<aeon_sim::ForcesIndex>().ship_keys[&key(name)]
}

fn grain_route(h: &mut SimHost) -> aeon_sim::trade::TradeRoute {
    aeon_sim::trade::TradeRoute {
        good: key("grain"),
        source: province(h, "alpha"),
        sink: province(h, "luna"),
    }
}

#[test]
fn a_route_answers_a_hungry_world() {
    let mut h = route_host(81);
    let moon = h.world_mut().resource::<MapIndex>().body_keys[&key("moon")];
    let hauler = ship(&mut h, "hauler");

    assert!(
        aeon_sim::trade::body_in_want(h.world_mut(), moon),
        "the moon starts hungry"
    );

    let route = grain_route(&mut h);
    assert!(aeon_sim::trade::set_route(h.world_mut(), hauler, route));

    assert_eq!(
        aeon_sim::trade::route_relief(h.world_mut(), moon, &key("grain")),
        20
    );
    assert!(
        !aeon_sim::trade::body_in_want(h.world_mut(), moon),
        "the route brings enough grain to answer the want"
    );
}

#[test]
fn a_blockade_cuts_the_line() {
    let mut h = route_host(82);
    let moon = h.world_mut().resource::<MapIndex>().body_keys[&key("moon")];
    let luna = province(&mut h, "luna");
    let hauler = ship(&mut h, "hauler");
    let picket = ship(&mut h, "picket");
    let route = grain_route(&mut h);
    aeon_sim::trade::set_route(h.world_mut(), hauler, route);
    assert!(!aeon_sim::trade::body_in_want(h.world_mut(), moon));

    // The picket blockades the delivery dock; the line is cut and the
    // want returns.
    {
        let entity = h.world_mut().resource::<aeon_sim::ForcesIndex>().ships[&picket];
        h.world_mut()
            .get_mut::<aeon_sim::ShipRecord>(entity)
            .unwrap()
            .blockading = Some(luna);
    }
    assert!(
        aeon_sim::trade::body_in_want(h.world_mut(), moon),
        "a blockade at either dock stops the goods"
    );
}

#[test]
fn a_route_earns_the_carrier_the_trade_margin() {
    let mut h = route_host(83);
    let ash = org(&mut h, "ash");
    let hauler = ship(&mut h, "hauler");
    let route = grain_route(&mut h);
    aeon_sim::trade::set_route(h.world_mut(), hauler, route);

    let before = wealth(&mut h, ash);
    aeon_sim::trade::monthly_trade_profit(h.world_mut());
    // 20 grain delivered into a deficit of 20, at value 2, is 40 wealth.
    assert_eq!(
        wealth(&mut h, ash) - before,
        40,
        "the carrier profits on what it sells into scarcity"
    );
}

#[test]
fn the_player_may_route_a_transport_and_it_survives_a_snapshot() {
    use aeon_sim::PlayerCommand;

    let mut h = route_host(84);
    let moon = h.world_mut().resource::<MapIndex>().body_keys[&key("moon")];
    let hauler = ship(&mut h, "hauler");
    let route = grain_route(&mut h);
    h.submit(PlayerCommand::SetTradeRoute {
        ship: hauler,
        route,
    })
    .expect("the player may route their own transport");
    h.advance_days(2);
    assert!(!aeon_sim::trade::body_in_want(h.world_mut(), moon));

    let content = h
        .world_mut()
        .resource::<aeon_sim::state::ContentDb>()
        .0
        .clone();
    let snapshot = h.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content).unwrap();
    assert!(
        !aeon_sim::trade::body_in_want(restored.world_mut(), moon),
        "the route is campaign state and rides the snapshot"
    );
}
