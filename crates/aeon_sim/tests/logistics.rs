//! Economy, presence, travel, ships, and armies: production and influence
//! recharge, job costs, army formation, transit and order delay, upkeep,
//! and snapshot fidelity.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::economy::{OrgResources, effective_legitimacy};
use aeon_sim::forces::{ArmyRecord, ForcesIndex, ShipLocation, ShipRecord};
use aeon_sim::politics::{TitleHolder, TitleRecord};
use aeon_sim::presence::{Location, character_location, travel_days};
use aeon_sim::{
    CampaignConfig, CharacterId, CommandRejection, JobTarget, PlayerCommand, PoliticsIndex, SimHost,
};

const FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", name: "Fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", name: "World", kind: "planet", radius_km: 6000 });
define_body(#{ id: "moon", name: "Moon", kind: "moon", radius_km: 1500,
               parent: "world", orbit_radius_mm: 384, orbit_days: 27 });
define_province(#{ id: "alpha", name: "Alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0,
                   wealth_output: 20, manpower_output: 30, supplies_output: 10 });
define_province(#{ id: "beta", name: "Beta", body: "world",
                   latitude_mdeg: 10000, longitude_mdeg: 10000 });
define_province(#{ id: "luna-port", name: "Luna Port", body: "moon",
                   latitude_mdeg: 0, longitude_mdeg: 0 });

define_house(#{
    id: "ash", name: "House Ash", surname: "Ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60],
    provinces: ["alpha", "luna-port"],
    wealth: 100, manpower: 2000, supplies: 300, legitimacy: 60,
});
define_house(#{
    id: "birch", name: "House Birch", surname: "Birch", tier: "great",
    head: "bela-birch", color: [60, 60, 200], provinces: ["beta"],
    wealth: 50, manpower: 500, supplies: 100, legitimacy: 40,
});

define_title(#{ id: "paramountcy", name: "Paramount", kind: "paramount", body: "world" });

define_ship(#{
    id: "runner", name: "Runner", class: "transport",
    owner: "ash", location: "alpha",
});

define_character(#{
    id: "aron-ash", name: "Aron Ash", gender: "male",
    birth_year: 370, organisation: "ash",
    skills: #{ command: 20, diplomacy: 8, intrigue: 4, stewardship: 7 },
});
define_character(#{
    id: "cera-ash", name: "Cera Ash", gender: "female",
    birth_year: 380, organisation: "ash",
    skills: #{ command: 6, diplomacy: 6, intrigue: 5, stewardship: 8 },
});
define_character(#{
    id: "bela-birch", name: "Bela Birch", gender: "female",
    birth_year: 372, organisation: "birch",
    skills: #{ command: 6, diplomacy: 9, intrigue: 8, stewardship: 5 },
});

define_job(#{
    id: "sure-muster", title: "Sure Muster", summary: "s",
    category: "consequential", duration_days: 10,
    skill: "command", difficulty: 0,
    wealth_cost: 40, influence_cost: 5, ai_available: false,
    results: #{
        success: #{ weight: 1000000, effect_fn: "muster" },
        failure: #{ weight: 1 },
    },
});
fn muster(ctx) {
    [#{ kind: "form-army", manpower: 800, supplies: 120 }]
}

define_job(#{
    id: "pricey-rite", title: "Pricey Rite", summary: "s",
    category: "consequential", duration_days: 5,
    skill: "stewardship", difficulty: 0,
    wealth_cost: 100000, ai_available: false,
    results: #{
        success: #{ weight: 1 },
        failure: #{ weight: 1 },
    },
});
"#;

fn content() -> Arc<aeon_data::ContentSet> {
    let (set, report) = load_content(&[ContentSource {
        path: "fixture.rhai".to_owned(),
        source: FIXTURE.to_owned(),
    }]);
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    Arc::new(set.unwrap())
}

fn host(seed: u64) -> SimHost {
    SimHost::new_with_content(
        CampaignConfig {
            name: "Logistics Trial".to_owned(),
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

fn char_id(h: &mut SimHost, name: &str) -> CharacterId {
    h.world_mut().resource::<PoliticsIndex>().character_keys[&key(name)]
}

fn ash_resources(h: &mut SimHost) -> OrgResources {
    let world = h.world_mut();
    let index = world.resource::<PoliticsIndex>();
    let ash = index.org_keys[&key("ash")];
    *world.get::<OrgResources>(index.orgs[&ash]).unwrap()
}

#[test]
fn provinces_produce_and_influence_recharges_to_cap() {
    let mut h = host(1);
    let start = ash_resources(&mut h);
    assert_eq!((start.wealth, start.influence), (100, 60));

    // Influence starts at cap; spend some so recharge is observable.
    {
        let world = h.world_mut();
        let index = world.resource::<PoliticsIndex>().clone();
        let ash = index.org_keys[&key("ash")];
        world
            .get_mut::<OrgResources>(index.orgs[&ash])
            .unwrap()
            .influence = 0;
    }

    h.advance_days(30); // one monthly pulse
    let after = ash_resources(&mut h);
    // Alpha (20/30/10) plus Luna Port defaults (10/10/10); ship upkeep -1.
    assert_eq!(after.wealth, 100 + 30);
    assert_eq!(after.manpower, 2000 + 40);
    assert_eq!(after.supplies, 300 + 20 - 1);
    assert_eq!(after.influence, 6); // legitimacy 60 / 10
}

#[test]
fn paramount_title_raises_effective_legitimacy() {
    let mut h = host(2);
    let world = h.world_mut();
    let index = world.resource::<PoliticsIndex>().clone();
    let ash = index.org_keys[&key("ash")];
    assert_eq!(effective_legitimacy(world, ash), 60);

    let paramountcy = index.title_keys[&key("paramountcy")];
    world
        .get_mut::<TitleRecord>(index.titles[&paramountcy])
        .unwrap()
        .holder = TitleHolder::Org(ash);
    assert_eq!(effective_legitimacy(world, ash), 80);
}

#[test]
fn jobs_cost_resources_and_reject_the_unaffordable() {
    let mut h = host(3);
    let aron = char_id(&mut h, "aron-ash");

    let refused = h.submit(PlayerCommand::StartJob {
        job: key("pricey-rite"),
        leader: aron,
        target: JobTarget::None,
    });
    assert!(matches!(refused, Err(CommandRejection::Job(_))));

    let before = ash_resources(&mut h);
    h.submit(PlayerCommand::StartJob {
        job: key("sure-muster"),
        leader: aron,
        target: JobTarget::None,
    })
    .unwrap();
    h.advance_days(1);
    let after = ash_resources(&mut h);
    assert_eq!(after.wealth, before.wealth - 40);
    assert_eq!(after.influence, before.influence - 5);
}

#[test]
fn muster_jobs_form_armies_at_the_generals_province() {
    let mut h = host(4);
    let aron = char_id(&mut h, "aron-ash");
    h.submit(PlayerCommand::StartJob {
        job: key("sure-muster"),
        leader: aron,
        target: JobTarget::None,
    })
    .unwrap();
    let manpower_before = ash_resources(&mut h).manpower;
    h.advance_days(12);

    let world = h.world_mut();
    let forces = world.resource::<ForcesIndex>().clone();
    assert_eq!(forces.armies.len(), 1);
    let army = world
        .get::<ArmyRecord>(*forces.armies.values().next().unwrap())
        .unwrap()
        .clone();
    assert_eq!(army.general, aron);
    assert_eq!(army.manpower, 800);
    assert_eq!(army.supplies, 120);
    assert!(army.name.contains("House Ash"));
    // Mustered at Aron's location (House Ash's first holding, Alpha).
    let alpha = world.resource::<aeon_sim::MapIndex>().province_keys[&key("alpha")];
    assert_eq!(army.location, alpha);

    let after = ash_resources(&mut h);
    assert_eq!(after.manpower, manpower_before - 800);

    // Disbanding returns the soldiers.
    let army_id = army.id;
    h.submit(PlayerCommand::DisbandArmy { army: army_id })
        .unwrap();
    h.advance_days(1);
    assert_eq!(ash_resources(&mut h).manpower, manpower_before);
}

#[test]
fn travel_crosses_bodies_and_lands_on_schedule() {
    let mut h = host(5);
    let cera = char_id(&mut h, "cera-ash");
    let (alpha, beta, luna) = {
        let world = h.world_mut();
        let map = world.resource::<aeon_sim::MapIndex>();
        (
            map.province_keys[&key("alpha")],
            map.province_keys[&key("beta")],
            map.province_keys[&key("luna-port")],
        )
    };

    // Same-body travel is quick; cross-body takes the orbital lag.
    {
        let world = h.world_mut();
        assert_eq!(travel_days(world, alpha, beta), 3);
        assert_eq!(travel_days(world, alpha, luna), 4 + 384 / 50);
    }

    h.submit(PlayerCommand::Travel {
        character: cera,
        destination: luna,
    })
    .unwrap();
    h.advance_days(2);
    assert!(matches!(
        character_location(h.world_mut(), cera),
        Some(Location::Transit { .. })
    ));
    h.advance_days(12);
    assert_eq!(
        character_location(h.world_mut(), cera),
        Some(Location::Province(luna))
    );
}

#[test]
fn orders_across_distance_and_in_transit_are_delayed() {
    let mut h = host(6);
    let aron = char_id(&mut h, "aron-ash");
    let cera = char_id(&mut h, "cera-ash");
    let luna = {
        let world = h.world_mut();
        world.resource::<aeon_sim::MapIndex>().province_keys[&key("luna-port")]
    };

    // Send Cera to the moon and let her land.
    h.submit(PlayerCommand::Travel {
        character: cera,
        destination: luna,
    })
    .unwrap();
    h.advance_days(15);

    // A job led by Cera (on the moon) is delayed; one led by Aron
    // (co-located with himself) is not.
    let near = h
        .submit(PlayerCommand::StartJob {
            job: key("sure-muster"),
            leader: aron,
            target: JobTarget::None,
        })
        .unwrap();
    let far = h
        .submit(PlayerCommand::StartJob {
            job: key("sure-muster"),
            leader: cera,
            target: JobTarget::None,
        })
        .unwrap();
    assert_eq!(h.date().days_until(near.day), 1);
    let lag = h.date().days_until(far.day);
    assert!(lag > 1, "cross-body orders lag, got {lag}");

    // With the head himself in transit, everything is delayed.
    h.advance_days(30); // let the earlier jobs resolve
    h.submit(PlayerCommand::Travel {
        character: aron,
        destination: luna,
    })
    .unwrap();
    h.advance_days(2);
    let during_transit = h.submit(PlayerCommand::Noop).unwrap();
    assert!(
        h.date().days_until(during_transit.day) > 1,
        "orders while the head is in space are delayed"
    );
}

#[test]
fn ships_move_between_bodies_and_dock() {
    let mut h = host(7);
    let (runner, luna) = {
        let world = h.world_mut();
        let forces = world.resource::<ForcesIndex>();
        let map = world.resource::<aeon_sim::MapIndex>();
        (
            forces.ship_keys[&key("runner")],
            map.province_keys[&key("luna-port")],
        )
    };

    h.submit(PlayerCommand::MoveShip {
        ship: runner,
        destination: luna,
    })
    .unwrap();
    h.advance_days(3);
    {
        let world = h.world_mut();
        let forces = world.resource::<ForcesIndex>();
        let ship = world.get::<ShipRecord>(forces.ships[&runner]).unwrap();
        assert!(matches!(ship.location, ShipLocation::Transit { .. }));
    }
    h.advance_days(10);
    let world = h.world_mut();
    let forces = world.resource::<ForcesIndex>();
    let ship = world.get::<ShipRecord>(forces.ships[&runner]).unwrap();
    assert_eq!(ship.location, ShipLocation::Docked(luna));
}

#[test]
fn logistics_survive_snapshots_and_stay_deterministic() {
    let run = |seed: u64| {
        let mut h = host(seed);
        let aron = char_id(&mut h, "aron-ash");
        h.submit(PlayerCommand::StartJob {
            job: key("sure-muster"),
            leader: aron,
            target: JobTarget::None,
        })
        .unwrap();
        h.advance_days(120);
        h
    };
    let mut a = run(9);
    let b = run(9);
    assert_eq!(a.state_hash(), b.state_hash());

    let snapshot = a.snapshot();
    assert_eq!(snapshot.state.forces.armies.len(), 1);
    assert_eq!(snapshot.state.forces.ships.len(), 1);
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(restored.state_hash(), a.state_hash());

    a.advance_days(200);
    restored.advance_days(200);
    assert_eq!(restored.state_hash(), a.state_hash());
}
