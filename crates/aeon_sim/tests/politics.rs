//! Political-model guarantees: deterministic spawning, opinion
//! derivation, succession, the contested Consular appointment, office
//! appointment, life-cycle simulation, and snapshot fidelity.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentSource, load_content};
use aeon_sim::politics::{
    ADULT_AGE, COMMANDER_VACANCY_DAYS, CONSUL_CONTEST_DAYS, CampaignOver, ConsulContest,
    OpinionEntry, OpinionLedger, process_death,
};
use aeon_sim::{
    CampaignConfig, CharacterId, CharacterRecord, OfficeRecord, OrgRecord, PoliticsIndex, SimHost,
    TitleHolder, TitleRecord, answers_to, opinion_between,
};

const FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", name: "Fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram", "Tolv"], female: ["Yeva", "Sanna"] });
define_trait(#{ id: "devout", name: "Devout", summary: "s",
                opinion_same: 15, opinion_opposed: 15, opposites: ["profane"] });
define_trait(#{ id: "profane", name: "Profane", summary: "s",
                opinion_same: 5, opinion_opposed: 15, opposites: ["devout"] });

define_body(#{ id: "world", name: "World", kind: "planet", radius_km: 6000 });
define_province(#{ id: "alpha", name: "Alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0 });
define_province(#{ id: "beta", name: "Beta", body: "world",
                   latitude_mdeg: 10000, longitude_mdeg: 10000 });
define_province(#{ id: "gamma", name: "Gamma", body: "world",
                   latitude_mdeg: -10000, longitude_mdeg: -10000 });

define_house(#{
    id: "ash", name: "House Ash", surname: "Ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha"],
});
define_house(#{
    id: "birch", name: "House Birch", surname: "Birch", tier: "great",
    head: "bela-birch", color: [60, 60, 200], provinces: ["beta"],
});
define_organisation(#{
    id: "sanctora-imperim", name: "Sanctora", kind: "sanctora-imperim",
    head: "consul-vex", color: [212, 175, 55], provinces: ["gamma"],
});

define_title(#{ id: "paramountcy", name: "Paramount", kind: "paramount", body: "world" });
define_title(#{ id: "consulate", name: "Consul", kind: "consul",
                holder_character: "consul-vex" });
define_office(#{ id: "commander", name: "Commander", organisation: "sanctora-imperim",
                 province: "gamma", holder: "prefect-hale" });

define_character(#{
    id: "aron-ash", name: "Aron Ash", gender: "male",
    birth_year: 370, organisation: "ash", spouse: "yeva-ash", traits: ["devout"],
    skills: #{ command: 8, diplomacy: 6, intrigue: 4, stewardship: 7 },
});
define_character(#{
    id: "yeva-ash", name: "Yeva Ash", gender: "female",
    birth_year: 374, organisation: "ash", traits: ["devout"],
    skills: #{ command: 2, diplomacy: 8, intrigue: 6, stewardship: 9 },
});
define_character(#{
    id: "cera-ash", name: "Cera Ash", gender: "female",
    birth_year: 395, organisation: "ash", parents: ["aron-ash", "yeva-ash"],
    traits: ["profane"],
    skills: #{ command: 4, diplomacy: 3, intrigue: 5, stewardship: 2 },
});
define_character(#{
    id: "doran-ash", name: "Doran Ash", gender: "male",
    birth_year: 398, organisation: "ash", parents: ["aron-ash", "yeva-ash"],
    traits: ["devout"],
    skills: #{ command: 3, diplomacy: 2, intrigue: 2, stewardship: 3 },
});
define_character(#{
    id: "bela-birch", name: "Bela Birch", gender: "female",
    birth_year: 372, organisation: "birch", traits: ["profane"],
    skills: #{ command: 6, diplomacy: 9, intrigue: 8, stewardship: 5 },
});
define_character(#{
    id: "consul-vex", name: "Consul Vex", gender: "male",
    birth_year: 365, organisation: "sanctora-imperim", traits: ["devout"],
    skills: #{ command: 3, diplomacy: 12, intrigue: 9, stewardship: 11 },
});
define_character(#{
    id: "prefect-hale", name: "Prefect Hale", gender: "female",
    birth_year: 380, organisation: "sanctora-imperim", traits: ["devout"],
    skills: #{ command: 9, diplomacy: 6, intrigue: 5, stewardship: 8 },
});
define_character(#{
    id: "adept-rho", name: "Adept Rho", gender: "male",
    birth_year: 384, organisation: "sanctora-imperim", traits: ["devout"],
    skills: #{ command: 5, diplomacy: 10, intrigue: 6, stewardship: 9 },
});
"#;

fn fixture_content() -> Arc<aeon_data::ContentSet> {
    let (set, report) = load_content(&[ContentSource {
        path: "fixture.rhai".to_owned(),
        source: FIXTURE.to_owned(),
    }]);
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    Arc::new(set.unwrap())
}

fn start_date() -> aeon_core::calendar::GameDate {
    CalendarDate {
        year: 411,
        month: 1,
        day: 1,
    }
    .to_date()
    .unwrap()
}

fn fixture_host(seed: u64) -> SimHost {
    SimHost::new_with_content(
        CampaignConfig {
            name: "Politics Trial".to_owned(),
            seed,
            start_date: start_date(),
        },
        fixture_content(),
    )
}

fn char_id(host: &mut SimHost, key: &str) -> CharacterId {
    let key = aeon_data::ContentKey::new(key).unwrap();
    host.world_mut().resource::<PoliticsIndex>().character_keys[&key]
}

#[test]
fn political_world_spawns_deterministically() {
    let mut a = fixture_host(5);
    let b = fixture_host(5);
    assert_eq!(a.state_hash(), b.state_hash());

    let world = a.world_mut();
    let index = world.resource::<PoliticsIndex>().clone();
    assert_eq!(index.characters.len(), 8);
    assert_eq!(index.orgs.len(), 3);
    // 3 province titles + paramountcy + consulate.
    assert_eq!(index.titles.len(), 5);
    assert_eq!(index.offices.len(), 1);

    // The paramountcy starts vacant; province titles are held.
    let paramount_key = aeon_data::ContentKey::new("paramountcy").unwrap();
    let paramount = index.title_keys[&paramount_key];
    let record = world.get::<TitleRecord>(index.titles[&paramount]).unwrap();
    assert_eq!(record.holder, TitleHolder::Vacant);

    let _ = b;
}

#[test]
fn opinion_derives_from_traits_bonds_and_modifiers() {
    let mut host = fixture_host(1);
    let aron = char_id(&mut host, "aron-ash");
    let yeva = char_id(&mut host, "yeva-ash");
    let cera = char_id(&mut host, "cera-ash");
    let bela = char_id(&mut host, "bela-birch");

    let world = host.world_mut();
    // Spouse (+30), same org (+10), shared devout (+15) = 55.
    assert_eq!(opinion_between(world, aron, yeva), 55);
    // Marriage is symmetric even though the content declared it on one
    // side only.
    assert_eq!(opinion_between(world, yeva, aron), 55);
    // Parent/child (+25), same org (+10), devout vs profane (-15) = 20.
    assert_eq!(opinion_between(world, aron, cera), 20);
    // Different orgs, devout vs profane = -15.
    assert_eq!(opinion_between(world, aron, bela), -15);

    // A stored directional modifier shifts one direction only.
    let entity = world.resource::<PoliticsIndex>().characters[&bela];
    world
        .get_mut::<OpinionLedger>(entity)
        .unwrap()
        .set(OpinionEntry {
            target: aron,
            amount: 40,
            reason: "test-favour".to_owned(),
            expires: None,
        });
    assert_eq!(opinion_between(world, bela, aron), 25);
    assert_eq!(opinion_between(world, aron, bela), -15);
}

#[test]
fn expired_modifiers_stop_counting_and_are_cleaned_up() {
    let mut host = fixture_host(2);
    let aron = char_id(&mut host, "aron-ash");
    let bela = char_id(&mut host, "bela-birch");

    let expiry = start_date().add_days(10);
    {
        let world = host.world_mut();
        let entity = world.resource::<PoliticsIndex>().characters[&bela];
        world
            .get_mut::<OpinionLedger>(entity)
            .unwrap()
            .set(OpinionEntry {
                target: aron,
                amount: 40,
                reason: "fleeting".to_owned(),
                expires: Some(expiry),
            });
        assert_eq!(opinion_between(world, bela, aron), 25);
    }

    host.advance_days(40); // past expiry and a monthly cleanup
    let world = host.world_mut();
    assert_eq!(opinion_between(world, bela, aron), -15);
    let entity = world.resource::<PoliticsIndex>().characters[&bela];
    assert!(world.get::<OpinionLedger>(entity).unwrap().0.is_empty());
}

#[test]
fn house_succession_prefers_the_eldest_child() {
    let mut host = fixture_host(3);
    let aron = char_id(&mut host, "aron-ash");
    let cera = char_id(&mut host, "cera-ash");

    let date = start_date();
    process_death(host.world_mut(), aron, date);

    let world = host.world_mut();
    let index = world.resource::<PoliticsIndex>().clone();
    let ash_key = aeon_data::ContentKey::new("ash").unwrap();
    let ash = index.org_keys[&ash_key];
    let record = world.get::<OrgRecord>(index.orgs[&ash]).unwrap();
    // Cera (b. 395) precedes Doran (b. 398); gender does not matter.
    assert_eq!(record.head, Some(cera));
    assert!(!record.defunct);
    assert!(world.get_resource::<CampaignOver>().is_none());
}

#[test]
fn a_house_without_heirs_ends_the_player_campaign() {
    let mut host = fixture_host(4);
    let date = start_date();
    for key in ["cera-ash", "doran-ash", "yeva-ash", "aron-ash"] {
        let id = char_id(&mut host, key);
        process_death(host.world_mut(), id, date);
    }

    let world = host.world_mut();
    let over = world.get_resource::<CampaignOver>().expect("campaign ends");
    assert!(over.reason.contains("House Ash"), "reason: {}", over.reason);
}

#[test]
fn consul_vacancy_opens_a_contest_and_the_tsar_appoints() {
    let mut host = fixture_host(6);
    let vex = char_id(&mut host, "consul-vex");

    let date = start_date();
    process_death(host.world_mut(), vex, date);

    {
        let world = host.world_mut();
        let contest = world
            .get_resource::<ConsulContest>()
            .expect("contest opens");
        // Candidates: living adult org heads (Aron, Bela) and adult
        // Sanctora members (Hale, Rho). Vex is dead.
        assert_eq!(contest.candidates.len(), 4);
    }

    host.advance_days(CONSUL_CONTEST_DAYS as u32 + 1);

    let world = host.world_mut();
    assert!(world.get_resource::<ConsulContest>().is_none());
    let index = world.resource::<PoliticsIndex>().clone();
    let consulate_key = aeon_data::ContentKey::new("consulate").unwrap();
    let consulate = index.title_keys[&consulate_key];
    let title = world.get::<TitleRecord>(index.titles[&consulate]).unwrap();
    let TitleHolder::Character(winner) = title.holder else {
        panic!("consulate should be held after the appointment");
    };
    // The new Consul heads the Sanctora.
    let sanctora_key = aeon_data::ContentKey::new("sanctora-imperim").unwrap();
    let sanctora = index.org_keys[&sanctora_key];
    let org = world.get::<OrgRecord>(index.orgs[&sanctora]).unwrap();
    assert_eq!(org.head, Some(winner));
    // The winner is alive and adult.
    let record = world
        .get::<CharacterRecord>(index.characters[&winner])
        .unwrap();
    assert!(record.alive());
    assert!(record.age_years(world.resource::<aeon_sim::CampaignClock>().date) >= ADULT_AGE);
}

#[test]
fn the_consul_fills_a_vacant_command() {
    let mut host = fixture_host(7);
    let hale = char_id(&mut host, "prefect-hale");
    let rho = char_id(&mut host, "adept-rho");

    process_death(host.world_mut(), hale, start_date());
    host.advance_days(COMMANDER_VACANCY_DAYS as u32 + 1);

    let world = host.world_mut();
    let index = world.resource::<PoliticsIndex>().clone();
    let commander_key = aeon_data::ContentKey::new("commander").unwrap();
    let commander = index.office_keys[&commander_key];
    let office = world
        .get::<OfficeRecord>(index.offices[&commander])
        .unwrap();
    // Best living Sanctora member by command + stewardship:
    // Vex 3+11=14, Rho 5+9=14 — tie resolves to the lower stable ID.
    // Vex was defined before Rho, so Vex wins the tie.
    let vex = index.character_keys[&aeon_data::ContentKey::new("consul-vex").unwrap()];
    let expected = if vex < rho { vex } else { rho };
    assert_eq!(office.holder, Some(expected));
    assert!(office.vacant_since.is_none());
}

#[test]
fn decades_of_life_simulation_stay_deterministic() {
    let mut a = fixture_host(11);
    let mut b = fixture_host(11);
    a.advance_days(360 * 40);
    b.advance_days(360 * 40);
    assert_eq!(a.state_hash(), b.state_hash());

    // Forty years later the founding generation is gone and life went on.
    let world = a.world_mut();
    let index = world.resource::<PoliticsIndex>().clone();
    let deaths = index
        .characters
        .values()
        .filter(|e| {
            world
                .get::<CharacterRecord>(**e)
                .is_some_and(|r| r.death.is_some())
        })
        .count();
    assert!(deaths > 0, "four decades should see deaths");
    assert!(index.characters.len() > 8, "four decades should see births");
}

#[test]
fn politics_survive_snapshot_restore_identically() {
    let content = fixture_content();
    let mut host = SimHost::new_with_content(
        CampaignConfig {
            name: "Politics Snapshot".to_owned(),
            seed: 13,
            start_date: start_date(),
        },
        content.clone(),
    );
    host.advance_days(360 * 10);
    let hash = host.state_hash();

    let snapshot = host.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content).unwrap();
    assert_eq!(restored.state_hash(), hash);

    // Restored campaigns continue identically.
    host.advance_days(360 * 5);
    restored.advance_days(360 * 5);
    assert_eq!(restored.state_hash(), host.state_hash());
}

#[test]
fn repository_content_runs_a_political_decade() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets readable");
    let (set, report) = load_content(&sources);
    assert!(
        set.is_some(),
        "repository content loads: {:?}",
        report.findings
    );
    let content = Arc::new(set.unwrap());

    let scenario = content.scenario.clone().expect("scenario defined");
    let start = CalendarDate {
        year: scenario.start_year,
        month: scenario.start_month,
        day: scenario.start_day,
    }
    .to_date()
    .unwrap();

    let mut host = SimHost::new_with_content(
        CampaignConfig {
            name: scenario.name.clone(),
            seed: 0xA301,
            start_date: start,
        },
        content,
    );
    host.advance_days(3600);
    let world = host.world_mut();
    assert!(world.get_resource::<CampaignOver>().is_none());
    let index = world.resource::<PoliticsIndex>().clone();
    // 3 great + 8 vassal + 2 independent houses + the Sanctora Imperim.
    assert_eq!(index.orgs.len(), 14);
    assert_eq!(index.titles.len(), 43); // 41 provinces + paramountcy + consulate
}

/// A minimal realm with one great house, its vassal, a rival great house
/// and the rival's vassal — kept separate from the shared fixture so that
/// testing vassalage does not perturb unrelated counts.
const VASSAL_FIXTURE: &str = r#"
define_scenario(#{
    id: "vassals", name: "Vassals", start_year: 411, start_month: 1, start_day: 1,
    player_house: "cedar",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });
define_body(#{ id: "world", name: "World", kind: "planet", radius_km: 6000 });
define_province(#{ id: "alpha", name: "Alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0 });
define_province(#{ id: "beta", name: "Beta", body: "world",
                   latitude_mdeg: 20000, longitude_mdeg: 20000 });

define_house(#{
    id: "ash", name: "House Ash", surname: "Ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha"],
});
define_house(#{
    id: "birch", name: "House Birch", surname: "Birch", tier: "great",
    head: "bela-birch", color: [60, 60, 200], provinces: ["beta"],
});
define_house(#{
    id: "cedar", name: "House Cedar", surname: "Cedar", tier: "vassal",
    liege: "ash", head: "cera-cedar", color: [90, 140, 90],
});
define_house(#{
    id: "dogwood", name: "House Dogwood", surname: "Dogwood", tier: "vassal",
    liege: "birch", head: "dorn-dogwood", color: [140, 120, 90],
});

define_character(#{ id: "aron-ash", name: "Aron Ash", gender: "male",
    birth_year: 370, organisation: "ash" });
define_character(#{ id: "bela-birch", name: "Bela Birch", gender: "female",
    birth_year: 372, organisation: "birch" });
define_character(#{ id: "cera-cedar", name: "Cera Cedar", gender: "female",
    birth_year: 378, organisation: "cedar" });
define_character(#{ id: "dorn-dogwood", name: "Dorn Dogwood", gender: "male",
    birth_year: 381, organisation: "dogwood" });
"#;

fn vassal_host(seed: u64) -> SimHost {
    let (set, report) = load_content(&[ContentSource {
        path: "vassals.rhai".to_owned(),
        source: VASSAL_FIXTURE.to_owned(),
    }]);
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    SimHost::new_with_content(
        CampaignConfig {
            name: "Vassal Trial".to_owned(),
            seed,
            start_date: start_date(),
        },
        Arc::new(set.unwrap()),
    )
}

fn org_id(h: &mut SimHost, name: &str) -> aeon_sim::OrgId {
    let key = aeon_data::ContentKey::new(name).unwrap();
    h.world_mut().resource::<PoliticsIndex>().org_keys[&key]
}

#[test]
fn vassalage_is_measured_in_hops_not_by_the_top_of_the_chain() {
    // Content rules cap the chain at one level — a liege must be a great
    // house — so one hop is the deepest valid arrangement, though
    // `answers_to` counts further should that rule ever relax.
    let mut h = vassal_host(21);
    let ash = org_id(&mut h, "ash");
    let cedar = org_id(&mut h, "cedar");
    let dogwood = org_id(&mut h, "dogwood");
    let birch = org_id(&mut h, "birch");
    let world = h.world_mut();

    // A house holds its own ground directly.
    assert_eq!(answers_to(world, ash, ash), Some(0));
    assert_eq!(answers_to(world, cedar, cedar), Some(0));

    // Distance is counted in steps up the chain.
    assert_eq!(answers_to(world, cedar, ash), Some(1));
    assert_eq!(answers_to(world, dogwood, birch), Some(1));

    // Vassalage runs one way only, and does not reach across the map.
    assert_eq!(
        answers_to(world, ash, cedar),
        None,
        "a liege does not answer to its own vassal"
    );
    assert_eq!(
        answers_to(world, cedar, birch),
        None,
        "nor to a rival great house"
    );
    assert_eq!(
        answers_to(world, cedar, dogwood),
        None,
        "nor to another liege's vassal"
    );
}

#[test]
fn a_vassals_own_ground_is_its_own_not_its_lieges() {
    // The distinction the "my realm" map mode rests on: walking to the top
    // of the chain would tell Cedar that Ash's holdings are Cedar's, and
    // that Cedar's own holdings belong to Ash.
    let mut h = vassal_host(22);
    let ash = org_id(&mut h, "ash");
    let cedar = org_id(&mut h, "cedar");
    let world = h.world_mut();

    assert_eq!(
        answers_to(world, cedar, cedar),
        Some(0),
        "Cedar holds its own ground directly, whoever its liege is"
    );
    assert_eq!(
        answers_to(world, ash, cedar),
        None,
        "and Ash's ground is not Cedar's at any distance"
    );
}
