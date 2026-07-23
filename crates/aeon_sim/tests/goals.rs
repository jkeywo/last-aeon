//! Grand-strategy goals: a head forms an ambition, it lifts the pressures
//! that serve it, it outlives the reign that formed it, and it survives a
//! snapshot.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::model::AiIntent;
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::goals::Goals;
use aeon_sim::{CampaignConfig, OrgId, PoliticsIndex, SimHost};

const FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_province(#{ id: "alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0 });
define_province(#{ id: "beta", body: "world",
                   latitude_mdeg: 10000, longitude_mdeg: 10000 });

define_house(#{
    id: "ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha"],
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
    skills: #{ command: 8, diplomacy: 12, intrigue: 4, stewardship: 7 },
});
define_character(#{
    id: "bela-birch", gender: "female",
    birth_year: 372, organisation: "birch",
    skills: #{ command: 6, diplomacy: 9, intrigue: 8, stewardship: 5 },
});

// A grand ambition Birch's head can weigh: it favours standing, and its
// trigger is met by any solvent great house that is nobody's vassal.
define_goal(#{
    id: "become-consul",
    favours: ["standing"],
    favour_bonus: 40,
    max_days: 3600,
    cooldown_days: 720,
    trigger: #{ min_legitimacy: 40, is_vassal: false },
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
            name: "Goal Trial".to_owned(),
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

fn bela(h: &mut SimHost) -> aeon_sim::CharacterId {
    h.world_mut().resource::<PoliticsIndex>().character_keys[&key("bela-birch")]
}

/// Advances until Birch holds an ambition, or panics after a year.
fn advance_to_ambition(h: &mut SimHost, birch: OrgId) {
    for _ in 0..400 {
        h.advance_days(1);
        if h.world_mut()
            .resource::<Goals>()
            .active
            .contains_key(&birch)
        {
            return;
        }
    }
    panic!("Birch never formed a grand ambition");
}

#[test]
fn a_head_forms_an_ambition_its_house_then_holds() {
    let mut h = host(41);
    let birch = org(&mut h, "birch");
    advance_to_ambition(&mut h, birch);

    let goals = h.world_mut().resource::<Goals>().clone();
    let active = &goals.active[&birch];
    assert_eq!(active.def.as_str(), "become-consul");
    assert_eq!(
        active.adopted_by,
        bela(&mut h),
        "the head who set it is recorded"
    );

    // The player house is never handed a grand ambition by the pass.
    let ash = org(&mut h, "ash");
    assert!(
        !h.world_mut().resource::<Goals>().active.contains_key(&ash),
        "the player's own house forms no ambition on its own"
    );
}

#[test]
fn an_ambition_lifts_the_pressure_it_favours() {
    let mut h = host(42);
    let birch = org(&mut h, "birch");
    advance_to_ambition(&mut h, birch);

    let world = h.world_mut();
    assert_eq!(
        aeon_sim::goals::favour_bonus(world, birch, AiIntent::Standing),
        40,
        "the favoured pressure is lifted by the goal's bonus"
    );
    assert_eq!(
        aeon_sim::goals::favour_bonus(world, birch, AiIntent::Muster),
        0,
        "a pressure the goal does not favour is untouched"
    );
}

#[test]
fn an_ambition_outlives_the_head_who_formed_it() {
    let mut h = host(43);
    let birch = org(&mut h, "birch");
    advance_to_ambition(&mut h, birch);
    let head = bela(&mut h);

    // The head who set the house on it dies; the ambition is the house's,
    // not the person's, so it stands.
    let date = h.world_mut().resource::<aeon_sim::CampaignClock>().date;
    {
        let world = h.world_mut();
        let entity = world.resource::<PoliticsIndex>().characters[&head];
        world
            .get_mut::<aeon_sim::CharacterRecord>(entity)
            .expect("record")
            .death = Some(date);
    }
    h.advance_days(40);

    assert!(
        h.world_mut()
            .resource::<Goals>()
            .active
            .contains_key(&birch),
        "a plan dies with its leader, but a house's ambition does not"
    );
}

#[test]
fn goals_replay_identically_and_survive_snapshots() {
    let run = || {
        let mut h = host(44);
        let birch = org(&mut h, "birch");
        advance_to_ambition(&mut h, birch);
        h
    };
    let mut a = run();
    let b = run();
    assert_eq!(
        a.state_hash(),
        b.state_hash(),
        "the same seed must form the same ambitions"
    );

    let snapshot = a.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "goal state must round-trip through a snapshot"
    );

    a.advance_days(300);
    restored.advance_days(300);
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "and continue identically afterwards"
    );
}
