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
