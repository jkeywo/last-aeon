//! The authored Ashkarr Succession scenario: structural integrity, the
//! contested paramountcy, Imperial tithes, and a deterministic
//! multi-year playthrough on the real repository content.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSet, load_content};
use aeon_sim::crisis::{
    ParamountClaimError, collect_tithes, declare_claim, dominant_claimant, paramountcy,
    press_claim, province_counts_on, realm_province_counts_on,
};
use aeon_sim::economy::OrgResources;
use aeon_sim::politics::{TitleHolder, TitleRecord};
use aeon_sim::{CampaignConfig, CampaignOver, OrgId, PoliticsIndex, SimHost, TitleKind};

fn repository_content() -> Arc<ContentSet> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (set, report) = load_content(&sources, &aeon_data::StringTable::blank());
    assert!(
        set.is_some(),
        "repository content must load: {:?}",
        report.findings
    );
    Arc::new(set.unwrap())
}

fn scenario_host(seed: u64) -> SimHost {
    let content = repository_content();
    let scenario = content.scenario.clone().expect("scenario defined");
    let start = CalendarDate {
        year: scenario.start_year,
        month: scenario.start_month,
        day: scenario.start_day,
    }
    .to_date()
    .unwrap();
    SimHost::new_with_content(
        CampaignConfig {
            name: scenario.name,
            seed,
            start_date: start,
        },
        content,
    )
}

fn key(text: &str) -> ContentKey {
    ContentKey::new(text).unwrap()
}

fn org(h: &mut SimHost, name: &str) -> OrgId {
    h.world_mut().resource::<PoliticsIndex>().org_keys[&key(name)]
}

#[test]
fn scenario_has_the_full_authored_field() {
    let content = repository_content();
    // Fourteen organisations: 3 great + 8 vassal + 2 independent + Sanctora.
    assert_eq!(content.organisations.len(), 14, "organisation count");
    assert_eq!(content.provinces.len(), 41, "province count");
    assert_eq!(content.bodies.len(), 3, "bodies");
    assert_eq!(content.ships.len(), 7, "ships");
    assert_eq!(content.armies.len(), 17, "starting armies");
    assert_eq!(
        content
            .scenario
            .as_ref()
            .unwrap()
            .player_house
            .as_ref()
            .unwrap()
            .as_str(),
        "harrow"
    );

    // Every province is held by exactly one organisation at start.
    let mut held: std::collections::BTreeSet<&ContentKey> = std::collections::BTreeSet::new();
    for org in content.organisations.values() {
        for province in &org.provinces {
            assert!(held.insert(province), "province {province} double-held");
        }
    }
    assert_eq!(held.len(), 41, "every province is allocated");

    // Every starting army has a general who belongs to the owning house.
    for army in content.armies.values() {
        let general = content
            .characters
            .get(army.general.as_ref().expect("starting army has a general"))
            .unwrap_or_else(|| panic!("army {} general defined", army.key));
        assert_eq!(
            general.organisation.as_ref(),
            Some(&army.owner),
            "army {} general belongs to its owner",
            army.key
        );
    }
}

#[test]
fn authored_routes_starports_and_capacities_are_complete() {
    let content = repository_content();
    assert_eq!(content.routes.len(), 80);
    assert_eq!(
        content
            .routes
            .values()
            .filter(|route| route.kind == aeon_data::model::RouteKind::Surface)
            .count(),
        65
    );
    assert_eq!(
        content
            .routes
            .values()
            .filter(|route| route.kind == aeon_data::model::RouteKind::Space)
            .count(),
        15
    );
    let starports: std::collections::BTreeSet<_> = content
        .provinces
        .values()
        .filter(|province| province.starport)
        .map(|province| province.key.as_str())
        .collect();
    assert_eq!(
        starports,
        std::collections::BTreeSet::from([
            "karvessa",
            "old-anchorage",
            "port-vesk",
            "redwater",
            "spire-decks",
            "tolmaz",
        ])
    );
    assert_eq!(content.ships[&key("redwater-runner")].troop_capacity, 1200);
    assert_eq!(content.ships[&key("karvess-hauler")].troop_capacity, 1200);
    assert!(
        content
            .ships
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "redwater-runner" | "karvess-hauler"))
            .all(|(_, ship)| ship.troop_capacity == 0)
    );
}

#[test]
fn route_selection_is_stable_and_uses_authored_space_time() {
    let mut h = scenario_host(77);
    let world = h.world_mut();
    let map = world.resource::<aeon_sim::MapIndex>();
    let from = map.province_keys[&key("redwater")];
    let to = map.province_keys[&key("port-vesk")];
    let graph = world.resource::<aeon_sim::routes::RouteGraph>();
    let first = graph
        .fastest_path(aeon_data::model::RouteKind::Space, from, to)
        .unwrap();
    let second = graph
        .fastest_path(aeon_data::model::RouteKind::Space, from, to)
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(aeon_sim::routes::RouteGraph::path_days(&first), 7);
}

#[test]
fn starting_armies_spawn_deterministically_at_their_provinces() {
    let mut a = scenario_host(9);
    let b = scenario_host(9);
    assert_eq!(a.state_hash(), b.state_hash());

    let world = a.world_mut();
    let forces = world.resource::<aeon_sim::ForcesIndex>().clone();
    assert_eq!(forces.armies.len(), 17, "all starting armies spawned");
    // Each army stands in a real province with positive manpower and a
    // living general.
    let politics = world.resource::<PoliticsIndex>().clone();
    for entity in forces.armies.values() {
        let army = world.get::<aeon_sim::ArmyRecord>(*entity).unwrap();
        assert!(army.manpower > 0);
        assert!(
            politics
                .characters
                .contains_key(&army.general.expect("starting general"))
        );
    }
}

#[test]
fn the_paramountcy_starts_vacant_and_contested() {
    let mut h = scenario_host(1);
    let world = h.world_mut();
    let (title_id, body) = paramountcy(world).expect("scenario defines a paramountcy");

    let index = world.resource::<PoliticsIndex>();
    let record = world.get::<TitleRecord>(index.titles[&title_id]).unwrap();
    assert_eq!(record.holder, TitleHolder::Vacant, "vacant at start");
    assert!(matches!(record.kind, TitleKind::Paramount(_)));

    // The three great houses each directly hold five planetary provinces,
    // while the approved dominance test also counts transitive vassals.
    let counts = province_counts_on(world, body);
    let veyrin = index.org_keys[&key("veyrin")];
    let draksha = index.org_keys[&key("draksha")];
    let meloch = index.org_keys[&key("meloch")];
    assert_eq!(counts[&veyrin], 5);
    assert_eq!(counts[&draksha], 5);
    assert_eq!(counts[&meloch], 5);
    let realm_counts = realm_province_counts_on(world, body);
    assert_eq!(realm_counts[&veyrin], 11);
    assert_eq!(realm_counts[&draksha], 9);
    assert_eq!(realm_counts[&meloch], 9);
    assert_eq!(
        dominant_claimant(world, body),
        Some(veyrin),
        "Veyrin's whole realm leads at the opening"
    );
}

#[test]
fn a_dominant_head_can_declare_and_press_the_paramountcy() {
    let mut h = scenario_host(2);
    let (title_id, body) = paramountcy(h.world_mut()).unwrap();
    let veyrin = org(&mut h, "veyrin");
    let claimant = aeon_sim::access::org_head(h.world_mut(), veyrin).unwrap();

    assert_eq!(dominant_claimant(h.world_mut(), body), Some(veyrin));
    declare_claim(h.world_mut(), title_id, claimant).unwrap();
    press_claim(h.world_mut(), title_id, claimant).unwrap();

    let world = h.world_mut();
    let index = world.resource::<PoliticsIndex>();
    let record = world.get::<TitleRecord>(index.titles[&title_id]).unwrap();
    assert_eq!(record.holder, TitleHolder::Character(claimant));
}

#[test]
fn a_non_dominant_head_can_declare_but_cannot_press() {
    let mut h = scenario_host(3);
    let pell = org(&mut h, "pell");
    let claimant = aeon_sim::access::org_head(h.world_mut(), pell).unwrap();
    let (title_id, _) = paramountcy(h.world_mut()).unwrap();
    declare_claim(h.world_mut(), title_id, claimant).unwrap();
    assert_eq!(
        press_claim(h.world_mut(), title_id, claimant),
        Err(ParamountClaimError::NotDominant)
    );
    let world = h.world_mut();
    let index = world.resource::<PoliticsIndex>();
    assert_eq!(
        world
            .get::<TitleRecord>(index.titles[&title_id])
            .unwrap()
            .holder,
        TitleHolder::Vacant
    );
}

#[test]
fn imperial_tithes_move_wealth_from_houses_to_the_sanctora() {
    let mut h = scenario_host(4);
    let sanctora = org(&mut h, "sanctora-imperim");
    let harrow = org(&mut h, "harrow");

    let (sanctora_before, harrow_before) = {
        let world = h.world_mut();
        let index = world.resource::<PoliticsIndex>().clone();
        (
            world
                .get::<OrgResources>(index.orgs[&sanctora])
                .unwrap()
                .wealth,
            world
                .get::<OrgResources>(index.orgs[&harrow])
                .unwrap()
                .wealth,
        )
    };

    assert!(collect_tithes(h.world_mut(), sanctora));

    let world = h.world_mut();
    let index = world.resource::<PoliticsIndex>().clone();
    let sanctora_after = world
        .get::<OrgResources>(index.orgs[&sanctora])
        .unwrap()
        .wealth;
    let harrow_after = world
        .get::<OrgResources>(index.orgs[&harrow])
        .unwrap()
        .wealth;
    assert!(
        sanctora_after > sanctora_before,
        "the Sanctora gains tithes"
    );
    assert_eq!(
        harrow_after,
        harrow_before - harrow_before / 20,
        "Harrow pays a twentieth"
    );

    // Only the Sanctora may collect tithes.
    assert!(!collect_tithes(h.world_mut(), harrow));
}

#[test]
fn the_scenario_runs_a_deterministic_decade() {
    let mut a = scenario_host(0xA301);
    let mut b = scenario_host(0xA301);
    a.advance_days(360 * 10);
    b.advance_days(360 * 10);
    assert_eq!(a.state_hash(), b.state_hash(), "deterministic decade");

    // A decade of autonomous politics leaves the world alive: the founding
    // generation thins, new characters are born, and the player house
    // survives.
    let world = a.world_mut();
    assert!(
        world.get_resource::<CampaignOver>().is_none(),
        "the player house survives the decade"
    );
    let index = world.resource::<PoliticsIndex>().clone();
    assert!(
        index.characters.len() > 38,
        "births occurred over the decade"
    );
    let deaths = index
        .characters
        .values()
        .filter(|e| {
            world
                .get::<aeon_sim::CharacterRecord>(**e)
                .is_some_and(|r| r.death.is_some())
        })
        .count();
    assert!(deaths > 0, "deaths occurred over the decade");
}

#[test]
fn the_scenario_survives_a_snapshot_mid_campaign() {
    let content = repository_content();
    let mut original = scenario_host(55);
    original.advance_days(360 * 6);
    let hash = original.state_hash();

    let snapshot = original.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content).unwrap();
    assert_eq!(restored.state_hash(), hash);

    original.advance_days(360 * 4);
    restored.advance_days(360 * 4);
    assert_eq!(restored.state_hash(), original.state_hash());
}

#[test]
fn realm_questions_are_answered_by_the_simulation() {
    let mut h = scenario_host(21);
    let harrow = org(&mut h, "harrow");
    let veyrin = org(&mut h, "veyrin");
    let sanctora = org(&mut h, "sanctora-imperim");
    let world = h.world_mut();

    // The Great House map paints a vassal's ground under its great house;
    // a great house and the Sanctora stand under their own banner.
    assert_eq!(aeon_sim::politics::great_house_of(world, harrow), veyrin);
    assert_eq!(aeon_sim::politics::great_house_of(world, veyrin), veyrin);
    assert_eq!(
        aeon_sim::politics::great_house_of(world, sanctora),
        sanctora
    );
}

#[test]
fn garrisons_are_counted_in_stable_order() {
    let mut h = scenario_host(22);
    let world = h.world_mut();

    // Every authored army stands somewhere; the garrison count at its
    // province must include it and name an owner.
    let forces = world.resource::<aeon_sim::ForcesIndex>().clone();
    for entity in forces.armies.values() {
        let army = world
            .get::<aeon_sim::ArmyRecord>(*entity)
            .expect("indexed")
            .clone();
        let province = army.location.province().expect("starting army is ashore");
        let (men, owner) = aeon_sim::forces::garrison_in(world, province);
        assert!(men >= army.manpower, "garrison misses an army's men");
        assert!(owner.is_some(), "a garrisoned province names an owner");
    }
    // An empty province reports an empty garrison.
    let garrisoned: std::collections::BTreeSet<_> = forces
        .armies
        .values()
        .filter_map(|e| world.get::<aeon_sim::ArmyRecord>(*e))
        .filter_map(|a| a.location.province())
        .collect();
    let map = world.resource::<aeon_sim::MapIndex>().clone();
    if let Some(empty) = map.provinces.keys().find(|p| !garrisoned.contains(p)) {
        assert_eq!(aeon_sim::forces::garrison_in(world, *empty), (0, None));
    }
}
