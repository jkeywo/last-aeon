//! Provincial order: how it moves, what it changes, and how a province
//! that stays in unrest eventually throws off its ruler.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::economy::OrgResources;
use aeon_sim::forces::form_army;
use aeon_sim::map::MapIndex;
use aeon_sim::order::{
    ORDER_CRITICAL, ORDER_MAX, ORDER_START, ProvincialOrder, REVOLT_DAYS, adjust_order,
    output_factor_permille, province_order,
};
use aeon_sim::warfare::province_holder;
use aeon_sim::{
    CampaignConfig, CharacterId, MessageLog, PoliticsIndex, ProvinceId, SimHost, TitleHolder,
};

const FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_province(#{ id: "alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0, wealth_output: 100 });
define_province(#{ id: "beta", body: "world",
                   latitude_mdeg: 10000, longitude_mdeg: 10000 });
// A second Ash holding, far from the seat, where nobody stands.
define_province(#{ id: "gamma", body: "world",
                   latitude_mdeg: -20000, longitude_mdeg: -20000, wealth_output: 100 });

define_house(#{
    id: "ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha", "gamma"],
    wealth: 500, manpower: 5000, supplies: 800, legitimacy: 60,
});
define_house(#{
    id: "birch", tier: "great",
    head: "bela-birch", color: [60, 60, 200], provinces: ["beta"],
    wealth: 400, manpower: 2000, supplies: 400, legitimacy: 50,
});

define_character(#{
    id: "aron-ash", gender: "male",
    birth_year: 370, organisation: "ash",
    skills: #{ command: 14, diplomacy: 8, intrigue: 4, stewardship: 7 },
});
define_character(#{
    id: "bela-birch", gender: "female",
    birth_year: 372, organisation: "birch",
    skills: #{ command: 4, diplomacy: 9, intrigue: 8, stewardship: 5 },
});
"#;

fn content() -> Arc<aeon_data::ContentSet> {
    let (set, report) = load_content(&[ContentSource {
        path: "fixture.rhai".to_owned(),
        source: FIXTURE.to_owned(),
    }], &aeon_data::StringTable::blank());
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    Arc::new(set.unwrap())
}

fn host(seed: u64) -> SimHost {
    SimHost::new_with_content(
        CampaignConfig {
            name: "Order Trial".to_owned(),
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

fn province(h: &mut SimHost, name: &str) -> ProvinceId {
    h.world_mut().resource::<MapIndex>().province_keys[&key(name)]
}

fn char_id(h: &mut SimHost, name: &str) -> CharacterId {
    h.world_mut().resource::<PoliticsIndex>().character_keys[&key(name)]
}

fn order_of(h: &mut SimHost, name: &str) -> ProvincialOrder {
    let id = province(h, name);
    province_order(h.world_mut(), id)
}

#[test]
fn provinces_begin_settled_and_stay_settled_when_left_alone() {
    let mut h = host(1);
    assert_eq!(order_of(&mut h, "beta").order, ORDER_START);
    h.advance_days(365);
    assert_eq!(
        order_of(&mut h, "beta").order,
        ORDER_START,
        "a quiet province should neither drift up nor rot away"
    );
}

#[test]
fn a_garrison_restores_damaged_ground() {
    let mut h = host(2);
    let gamma = province(&mut h, "gamma");
    adjust_order(h.world_mut(), gamma, -400);
    let damaged = province_order(h.world_mut(), gamma).order;

    // Without attention, damage simply persists.
    h.advance_days(60);
    assert_eq!(
        province_order(h.world_mut(), gamma).order,
        damaged,
        "unattended damage should not mend itself"
    );

    // A garrison of the holder's own troops repairs it over time.
    let aron = char_id(&mut h, "aron-ash");
    let ash = h.world_mut().resource::<PoliticsIndex>().org_keys[&key("ash")];
    form_army(h.world_mut(), ash, aron, 500, 100, gamma);
    h.advance_days(60);
    let mended = province_order(h.world_mut(), gamma).order;
    assert!(
        mended > damaged,
        "a garrison should restore order: {damaged} -> {mended}"
    );
}

#[test]
fn order_scales_what_a_province_pays() {
    let base = |order: i32| -> i64 { 100 * output_factor_permille(order) / 1000 };
    assert_eq!(base(ORDER_START), 100, "settled ground pays its full worth");
    assert!(
        base(ORDER_MAX) > base(ORDER_START),
        "loyalty pays a premium"
    );
    assert!(base(0) < base(ORDER_START) / 2, "collapse is ruinous");
}

#[test]
fn a_province_left_in_unrest_revolts_and_can_be_retaken() {
    let mut h = host(3);
    let alpha = province(&mut h, "gamma");
    let ash = h.world_mut().resource::<PoliticsIndex>().org_keys[&key("ash")];
    assert_eq!(province_holder(h.world_mut(), alpha), Some(ash));

    // Drive the province into open unrest.
    adjust_order(h.world_mut(), alpha, -(ORDER_START - ORDER_CRITICAL));
    h.advance_days(1);
    let state = province_order(h.world_mut(), alpha);
    assert!(state.in_unrest(), "should be in unrest");
    assert!(
        state.days_to_revolt().is_some_and(|days| days > 0),
        "the revolt clock should be visible while it runs"
    );

    // The warning is given the day unrest begins, not after the fact.
    let warned = h
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .any(|entry| entry.text.contains("open unrest"));
    assert!(warned, "unrest must be telegraphed");

    // Left alone, it throws off its ruler.
    h.advance_days(REVOLT_DAYS as u32 + 2);
    assert_eq!(
        province_holder(h.world_mut(), alpha),
        None,
        "a revolted province should answer to nobody"
    );
    let revolted = h
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .any(|entry| entry.text.contains("revolted"));
    assert!(revolted, "the revolt must be reported");

    // It is vacant, not destroyed: the title still exists to be taken.
    let index = h.world_mut().resource::<PoliticsIndex>().clone();
    let title = index.province_titles[&alpha];
    let holder = h
        .world_mut()
        .get::<aeon_sim::TitleRecord>(index.titles[&title])
        .map(|record| record.holder);
    assert_eq!(holder, Some(TitleHolder::Vacant));

    // And a revolted province stops paying its old ruler.
    let ash_entity = index.orgs[&ash];
    let before = h
        .world_mut()
        .get::<OrgResources>(ash_entity)
        .map(|r| r.wealth)
        .unwrap();
    h.advance_days(30);
    let after = h
        .world_mut()
        .get::<OrgResources>(ash_entity)
        .map(|r| r.wealth)
        .unwrap();
    // Alpha, still loyal and settled, pays its full 100. Gamma, in
    // revolt, pays nothing at all — the month's income is exactly one
    // province's worth.
    assert_eq!(
        after - before,
        100,
        "a province in revolt should pay its former ruler nothing"
    );
}

#[test]
fn unrest_that_is_answered_never_becomes_a_revolt() {
    let mut h = host(4);
    let alpha = province(&mut h, "gamma");
    let ash = h.world_mut().resource::<PoliticsIndex>().org_keys[&key("ash")];

    adjust_order(h.world_mut(), alpha, -(ORDER_START - ORDER_CRITICAL));
    h.advance_days(30);
    assert!(province_order(h.world_mut(), alpha).unrest_days > 0);

    // Answer it with troops before the clock runs out.
    let aron = char_id(&mut h, "aron-ash");
    form_army(h.world_mut(), ash, aron, 500, 100, alpha);
    h.advance_days(REVOLT_DAYS as u32 + 30);

    assert_eq!(
        province_holder(h.world_mut(), alpha),
        Some(ash),
        "a province brought back from the brink should stay loyal"
    );
    let state = province_order(h.world_mut(), alpha);
    assert!(!state.in_unrest(), "order should have recovered");
    assert_eq!(state.unrest_days, 0, "the clock should have reset");
}

#[test]
fn order_survives_snapshots_and_replays_identically() {
    let run = |seed: u64| {
        let mut h = host(seed);
        let alpha = province(&mut h, "gamma");
        adjust_order(h.world_mut(), alpha, -350);
        h.advance_days(200);
        h
    };
    let mut a = run(7);
    let b = run(7);
    assert_eq!(a.state_hash(), b.state_hash());

    let snapshot = a.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "order must round-trip through a snapshot"
    );

    a.advance_days(120);
    restored.advance_days(120);
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "a restored campaign must keep evolving identically"
    );
}
