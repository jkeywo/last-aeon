//! The authored Ashkarr Succession scenario: structural integrity, the
//! contested paramountcy, Imperial tithes, and a deterministic
//! multi-year playthrough on the real repository content.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSet, load_content};
use aeon_sim::crisis::{
    claim_paramountcy, collect_tithes, dominant_claimant, paramountcy, province_counts_on,
};
use aeon_sim::economy::OrgResources;
use aeon_sim::politics::{TitleHolder, TitleRecord};
use aeon_sim::{CampaignConfig, CampaignOver, OrgId, PoliticsIndex, SimHost, TitleKind};

fn repository_content() -> Arc<ContentSet> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (set, report) = load_content(&sources);
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
    assert_eq!(content.ships.len(), 6, "ships");
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

    // The three great houses each hold five planetary provinces, so no
    // house dominates and none can yet claim.
    let counts = province_counts_on(world, body);
    let veyrin = index.org_keys[&key("veyrin")];
    let draksha = index.org_keys[&key("draksha")];
    let meloch = index.org_keys[&key("meloch")];
    assert_eq!(counts[&veyrin], 5);
    assert_eq!(counts[&draksha], 5);
    assert_eq!(counts[&meloch], 5);
    assert_eq!(dominant_claimant(world, body), None, "contested at start");
}

#[test]
fn a_dominant_house_can_claim_the_paramountcy() {
    let mut h = scenario_host(2);
    let (title_id, body) = paramountcy(h.world_mut()).unwrap();

    // Hand the player house (Harrow) enough planetary provinces to strictly
    // dominate: transfer a rival great house's holdings to it.
    let harrow = org(&mut h, "harrow");
    let veyrin = org(&mut h, "veyrin");
    {
        let world = h.world_mut();
        let index = world.resource::<PoliticsIndex>().clone();
        // Move every Veyrin planetary province to Harrow.
        let veyrin_titles: Vec<_> = index
            .titles
            .values()
            .filter(|e| {
                world
                    .get::<TitleRecord>(**e)
                    .is_some_and(|t| t.holder == TitleHolder::Org(veyrin))
            })
            .copied()
            .collect();
        for entity in veyrin_titles {
            world.get_mut::<TitleRecord>(entity).unwrap().holder = TitleHolder::Org(harrow);
        }
    }

    // Harrow now dominates the planet and the claim succeeds.
    assert_eq!(dominant_claimant(h.world_mut(), body), Some(harrow));
    assert!(claim_paramountcy(h.world_mut(), harrow));

    let world = h.world_mut();
    let index = world.resource::<PoliticsIndex>();
    let record = world.get::<TitleRecord>(index.titles[&title_id]).unwrap();
    assert_eq!(record.holder, TitleHolder::Org(harrow));

    // A second claim on the now-held title does nothing.
    assert!(!claim_paramountcy(h.world_mut(), harrow));
}

#[test]
fn a_non_dominant_house_cannot_claim() {
    let mut h = scenario_host(3);
    let veyrin = org(&mut h, "veyrin");
    // Great houses are tied 5-5-5, so even a great house cannot claim.
    assert!(!claim_paramountcy(h.world_mut(), veyrin));
    let (title_id, _) = paramountcy(h.world_mut()).unwrap();
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
