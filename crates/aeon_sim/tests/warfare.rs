//! Operational warfare: marches, engagements, conquest, raids,
//! blockades, standing orders, and determinism.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::economy::OrgResources;
use aeon_sim::forces::{ArmyRecord, ForcesIndex, ShipRecord, form_army};
use aeon_sim::warfare::{StandingOrder, province_holder};
use aeon_sim::{
    ArmyId, AssignmentTarget, CampaignConfig, CharacterId, MessageLog, PlayerCommand,
    PoliticsIndex, ProvinceId, SimHost,
};

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
                   latitude_mdeg: 10000, longitude_mdeg: 10000,
                   wealth_output: 40 });
define_province(#{ id: "gamma", body: "world",
                   latitude_mdeg: -10000, longitude_mdeg: -10000 });

define_house(#{
    id: "ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha"],
    wealth: 500, manpower: 5000, supplies: 800, legitimacy: 60,
});
define_house(#{
    id: "birch", tier: "great",
    head: "bela-birch", color: [60, 60, 200], provinces: ["beta", "gamma"],
    wealth: 400, manpower: 2000, supplies: 400, legitimacy: 50,
});

define_ship(#{
    id: "ash-sloop", class: "patrol",
    owner: "ash", location: "alpha",
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

// Engine-op assignments with certain rolls so tests isolate op semantics.
define_assignment(#{
    id: "march-the-army", 
    category: "consequential", duration_days: 2,
    skill: "command", difficulty: 0,
    target: "own-army-and-province", military_op: "move", ai_available: false,
    results: #{ success: #{ weight: 1000000 }, failure: #{ weight: 1 } },
});
define_assignment(#{
    id: "lay-siege", 
    category: "consequential", duration_days: 20,
    skill: "command", difficulty: 0,
    target: "own-army-and-province", military_op: "besiege", ai_available: false,
    results: #{
        success: #{ weight: 1000000, log: true, },
        failure: #{ weight: 1, log: true, },
    },
});
define_assignment(#{
    id: "raid-the-province", 
    category: "consequential", duration_days: 3,
    skill: "command", difficulty: 0,
    target: "own-army-and-province", military_op: "raid", ai_available: false,
    results: #{ success: #{ weight: 1000000 }, failure: #{ weight: 1 } },
});
define_assignment(#{
    id: "blockade-the-port", 
    category: "consequential", duration_days: 2,
    skill: "command", difficulty: 0,
    target: "own-ship-and-province", military_op: "blockade", ai_available: false,
    results: #{ success: #{ weight: 1000000 }, failure: #{ weight: 1 } },
});
define_assignment(#{
    id: "answer-the-alarm", 
    category: "consequential", duration_days: 2,
    skill: "command", difficulty: 0,
    target: "own-army-and-province", military_op: "move", ai_available: false,
    results: #{ success: #{ weight: 1000000 }, failure: #{ weight: 1 } },
});
"#;

/// The blank table, plus the two siege lines these tests read.
///
/// Blank answers every other key, so the fixture does not have to author
/// prose for definitions no test looks at.
fn strings() -> aeon_data::StringTable {
    let mut table = aeon_data::StringTable::blank();
    table.extend(&[
        ("assignment.lay-siege.success.log-text", "{target} fell."),
        (
            "assignment.lay-siege.failure.log-text",
            "The siege of {target} broke.",
        ),
    ]);
    table
}

fn content() -> Arc<aeon_data::ContentSet> {
    let (set, report) = load_content(
        &[ContentSource {
            path: "fixture.rhai".to_owned(),
            source: FIXTURE.to_owned(),
        }],
        &strings(),
    );
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    Arc::new(set.unwrap())
}

fn host(seed: u64) -> SimHost {
    SimHost::new_with_content(
        CampaignConfig {
            name: "Warfare Trial".to_owned(),
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

fn province(h: &mut SimHost, name: &str) -> ProvinceId {
    h.world_mut().resource::<aeon_sim::MapIndex>().province_keys[&key(name)]
}

fn org(h: &mut SimHost, name: &str) -> aeon_sim::OrgId {
    h.world_mut().resource::<PoliticsIndex>().org_keys[&key(name)]
}

/// Directly musters an army for a side (bypassing the muster assignment).
fn muster(h: &mut SimHost, owner: &str, general: &str, men: i64, at: &str) -> ArmyId {
    let owner = org(h, owner);
    let general = char_id(h, general);
    let at = province(h, at);
    form_army(h.world_mut(), owner, general, men, men / 5, at)
}

#[test]
fn marches_move_armies_and_take_road_time() {
    let mut h = host(1);
    let aron = char_id(&mut h, "aron-ash");
    let army = muster(&mut h, "ash", "aron-ash", 1000, "alpha");
    let beta = province(&mut h, "beta");

    let envelope = h
        .submit(PlayerCommand::StartAssignment {
            assignment: key("march-the-army"),
            leader: aron,
            target: AssignmentTarget::ArmyToProvince(army, beta),
        })
        .unwrap();
    // March duration is at least twice the liner time (3 days locally).
    h.advance_days(2);
    {
        let world = h.world_mut();
        let forces = world.resource::<ForcesIndex>();
        let record = world.get::<ArmyRecord>(forces.armies[&army]).unwrap();
        assert_ne!(record.location, beta, "still marching");
    }
    h.advance_days(8);
    let world = h.world_mut();
    let forces = world.resource::<ForcesIndex>();
    let record = world.get::<ArmyRecord>(forces.armies[&army]).unwrap();
    assert_eq!(record.location, beta);
    let _ = envelope;
}

#[test]
fn sieges_take_provinces_after_beating_the_garrison() {
    let mut h = host(2);
    let aron = char_id(&mut h, "aron-ash");
    let attacker = muster(&mut h, "ash", "aron-ash", 3000, "alpha");
    let _defender = muster(&mut h, "birch", "bela-birch", 800, "beta");
    let beta = province(&mut h, "beta");
    let ash = org(&mut h, "ash");
    let birch = org(&mut h, "birch");

    assert_eq!(province_holder(h.world_mut(), beta), Some(birch));
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("lay-siege"),
        leader: aron,
        target: AssignmentTarget::ArmyToProvince(attacker, beta),
    })
    .unwrap();
    h.advance_days(25);

    let world = h.world_mut();
    assert_eq!(
        province_holder(world, beta),
        Some(ash),
        "conquest transfers the title"
    );
    // The outnumbered garrison lost men and fell back to Gamma or broke.
    let log = world.resource::<MessageLog>();
    assert!(
        log.entries.iter().any(|e| e.text.contains("fell")),
        "log: {:?}",
        log.entries
    );
}

#[test]
fn a_strong_garrison_breaks_a_weak_siege() {
    let mut h = host(3);
    let aron = char_id(&mut h, "aron-ash");
    let attacker = muster(&mut h, "ash", "aron-ash", 300, "alpha");
    let _defender = muster(&mut h, "birch", "bela-birch", 4000, "beta");
    let beta = province(&mut h, "beta");
    let birch = org(&mut h, "birch");

    h.submit(PlayerCommand::StartAssignment {
        assignment: key("lay-siege"),
        leader: aron,
        target: AssignmentTarget::ArmyToProvince(attacker, beta),
    })
    .unwrap();
    h.advance_days(25);

    let world = h.world_mut();
    assert_eq!(
        province_holder(world, beta),
        Some(birch),
        "the defended province holds"
    );
    let log = world.resource::<MessageLog>();
    assert!(log.entries.iter().any(|e| e.text.contains("broke")));
}

#[test]
fn raids_loot_wealth_from_the_holder() {
    let mut h = host(4);
    let aron = char_id(&mut h, "aron-ash");
    let army = muster(&mut h, "ash", "aron-ash", 1500, "alpha");
    let beta = province(&mut h, "beta");
    let (ash, birch) = (org(&mut h, "ash"), org(&mut h, "birch"));

    let (ash_before, birch_before) = {
        let world = h.world_mut();
        let index = world.resource::<PoliticsIndex>().clone();
        (
            world.get::<OrgResources>(index.orgs[&ash]).unwrap().wealth,
            world
                .get::<OrgResources>(index.orgs[&birch])
                .unwrap()
                .wealth,
        )
    };
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("raid-the-province"),
        leader: aron,
        target: AssignmentTarget::ArmyToProvince(army, beta),
    })
    .unwrap();
    h.advance_days(10);

    let world = h.world_mut();
    let index = world.resource::<PoliticsIndex>().clone();
    let ash_after = world.get::<OrgResources>(index.orgs[&ash]).unwrap().wealth;
    let birch_after = world
        .get::<OrgResources>(index.orgs[&birch])
        .unwrap()
        .wealth;
    assert!(ash_after > ash_before, "raider gains loot");
    assert!(birch_after < birch_before, "holder loses wealth");
}

#[test]
fn blockades_halve_wealth_output() {
    let mut h = host(5);
    let aron = char_id(&mut h, "aron-ash");
    let beta = province(&mut h, "beta");
    let sloop = {
        let world = h.world_mut();
        world.resource::<ForcesIndex>().ship_keys[&key("ash-sloop")]
    };

    // A ship is ordered by the officer who commands it, so one must be
    // aboard before it can be sent anywhere.
    h.submit(PlayerCommand::SetShipCaptain {
        ship: sloop,
        captain: Some(aron),
    })
    .unwrap();
    h.advance_days(2);
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("blockade-the-port"),
        leader: aron,
        target: AssignmentTarget::ShipToProvince(sloop, beta),
    })
    .unwrap();
    h.advance_days(4);
    {
        let world = h.world_mut();
        let forces = world.resource::<ForcesIndex>();
        let ship = world.get::<ShipRecord>(forces.ships[&sloop]).unwrap();
        assert_eq!(ship.blockading, Some(beta));
    }

    // Compare a blockaded month's wealth with the unblockaded baseline.
    let birch = org(&mut h, "birch");
    let before = {
        let world = h.world_mut();
        let index = world.resource::<PoliticsIndex>().clone();
        world
            .get::<OrgResources>(index.orgs[&birch])
            .unwrap()
            .wealth
    };
    h.advance_days(30);
    let world = h.world_mut();
    let index = world.resource::<PoliticsIndex>().clone();
    let after = world
        .get::<OrgResources>(index.orgs[&birch])
        .unwrap()
        .wealth;
    // Beta authored at 40 wealth, halved to 20 by the blockade, then
    // scaled again by the order the blockade has been eroding all month;
    // Gamma is untouched at its default 10.
    let (beta_order, gamma_order) = {
        let map = world.resource::<aeon_sim::MapIndex>().clone();
        let read = |province| {
            aeon_sim::order::output_factor_permille(
                aeon_sim::order::province_order(world, province).order,
            )
        };
        (read(beta), read(map.province_keys[&key("gamma")]))
    };
    assert_eq!(
        after - before,
        20 * beta_order / 1000 + 10 * gamma_order / 1000
    );
    assert!(
        beta_order < 1000,
        "a month under blockade should have cost Beta some order"
    );
    assert!(
        after - before < 30,
        "a blockade should bite through both output and order"
    );
}

#[test]
fn standing_orders_answer_threats_and_yield_to_bespoke_assignments() {
    let mut h = host(6);
    let aron = char_id(&mut h, "aron-ash");
    let attacker = muster(&mut h, "ash", "aron-ash", 1000, "alpha");
    let defender = muster(&mut h, "birch", "bela-birch", 3000, "gamma");
    let beta = province(&mut h, "beta");

    // The defender guards Birch holdings from Gamma.
    {
        let world = h.world_mut();
        let forces = world.resource::<ForcesIndex>().clone();
        world
            .get_mut::<ArmyRecord>(forces.armies[&defender])
            .unwrap()
            .standing_order = StandingOrder::DefendHoldings;
    }

    // A siege on Beta triggers the alarm; the defender marches.
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("lay-siege"),
        leader: aron,
        target: AssignmentTarget::ArmyToProvince(attacker, beta),
    })
    .unwrap();
    h.advance_days(3);

    {
        let world = h.world_mut();
        let assignments_index = world.resource::<aeon_sim::AssignmentsIndex>();
        let reactive = assignments_index.assignments.values().any(|entity| {
            world
                .get::<aeon_sim::ActiveAssignment>(*entity)
                .is_some_and(|assignment| {
                    assignment.def.as_str() == "answer-the-alarm"
                        && matches!(assignment.target, AssignmentTarget::ArmyToProvince(a, p)
                            if a == defender && p == beta)
                })
        });
        assert!(reactive, "standing order created a reactive assignment");
        let log = world.resource::<MessageLog>();
        assert!(log.entries.iter().any(|e| e.text.contains("alarm")));
    }

    // The defender arrives to hold Beta; the siege meets it in the field.
    h.advance_days(20);
    let world = h.world_mut();
    let birch = world.resource::<PoliticsIndex>().org_keys[&key("birch")];
    assert_eq!(
        province_holder(world, beta),
        Some(birch),
        "the relieved province holds against the outnumbered siege"
    );
}

#[test]
fn idle_armies_without_orders_do_not_react() {
    let mut h = host(7);
    let aron = char_id(&mut h, "aron-ash");
    let attacker = muster(&mut h, "ash", "aron-ash", 1000, "alpha");
    let _defender = muster(&mut h, "birch", "bela-birch", 3000, "gamma");
    let beta = province(&mut h, "beta");

    // No standing order set: HoldFast is the default.
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("lay-siege"),
        leader: aron,
        target: AssignmentTarget::ArmyToProvince(attacker, beta),
    })
    .unwrap();
    h.advance_days(3);

    let world = h.world_mut();
    let assignments_index = world.resource::<aeon_sim::AssignmentsIndex>();
    let reactive = assignments_index.assignments.values().any(|entity| {
        world
            .get::<aeon_sim::ActiveAssignment>(*entity)
            .is_some_and(|assignment| assignment.def.as_str() == "answer-the-alarm")
    });
    assert!(!reactive, "HoldFast armies do not answer alarms");
}

#[test]
fn warfare_is_deterministic_and_survives_snapshots() {
    let run = |seed: u64| {
        let mut h = host(seed);
        let aron = char_id(&mut h, "aron-ash");
        let attacker = muster(&mut h, "ash", "aron-ash", 2500, "alpha");
        let defender = muster(&mut h, "birch", "bela-birch", 2000, "beta");
        {
            let world = h.world_mut();
            let forces = world.resource::<ForcesIndex>().clone();
            world
                .get_mut::<ArmyRecord>(forces.armies[&defender])
                .unwrap()
                .standing_order = StandingOrder::DefendHoldings;
        }
        let beta = province(&mut h, "beta");
        h.submit(PlayerCommand::StartAssignment {
            assignment: key("lay-siege"),
            leader: aron,
            target: AssignmentTarget::ArmyToProvince(attacker, beta),
        })
        .unwrap();
        h.advance_days(60);
        h
    };
    let mut a = run(21);
    let b = run(21);
    assert_eq!(a.state_hash(), b.state_hash());

    let snapshot = a.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(restored.state_hash(), a.state_hash());
    a.advance_days(60);
    restored.advance_days(60);
    assert_eq!(restored.state_hash(), a.state_hash());
}

// ---------------------------------------------------------------------------
// Ship captains
// ---------------------------------------------------------------------------

#[test]
fn a_ship_is_ordered_by_its_captain_and_nobody_else() {
    let mut h = host(31);
    let aron = char_id(&mut h, "aron-ash");
    let beta = province(&mut h, "beta");
    let ship = {
        let key = key("ash-sloop");
        h.world_mut().resource::<ForcesIndex>().ship_keys[&key]
    };

    // The fixture's sloop has no captain, so nobody can order it.
    let refused = h.submit(PlayerCommand::StartAssignment {
        assignment: key("blockade-the-port"),
        leader: aron,
        target: AssignmentTarget::ShipToProvince(ship, beta),
    });
    assert!(
        refused.is_err(),
        "a ship without a captain has no order to give"
    );

    // Put Aron aboard, and the same order stands.
    h.submit(PlayerCommand::SetShipCaptain {
        ship,
        captain: Some(aron),
    })
    .unwrap();
    h.advance_days(2);
    {
        let world = h.world_mut();
        let forces = world.resource::<ForcesIndex>().clone();
        let record = world.get::<ShipRecord>(forces.ships[&ship]).unwrap();
        assert_eq!(record.captain, Some(aron), "the command was taken up");
    }
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("blockade-the-port"),
        leader: aron,
        target: AssignmentTarget::ShipToProvince(ship, beta),
    })
    .expect("the captain may order their own ship");
}

#[test]
fn an_officer_cannot_hold_two_commands_at_once() {
    let mut h = host(32);
    let aron = char_id(&mut h, "aron-ash");
    let ship = {
        let key = key("ash-sloop");
        h.world_mut().resource::<ForcesIndex>().ship_keys[&key]
    };
    // Aron already commands an army.
    let _army = muster(&mut h, "ash", "aron-ash", 800, "alpha");

    let refused = h.submit(PlayerCommand::SetShipCaptain {
        ship,
        captain: Some(aron),
    });
    assert!(
        refused.is_err(),
        "a general cannot also take a ship's command"
    );
}

#[test]
fn a_captains_death_leaves_the_ship_without_one() {
    let mut h = host(33);
    let aron = char_id(&mut h, "aron-ash");
    let ship = {
        let key = key("ash-sloop");
        h.world_mut().resource::<ForcesIndex>().ship_keys[&key]
    };
    h.submit(PlayerCommand::SetShipCaptain {
        ship,
        captain: Some(aron),
    })
    .unwrap();
    h.advance_days(2);

    let date = h.world_mut().resource::<aeon_sim::CampaignClock>().date;
    aeon_sim::politics::process_death(h.world_mut(), aron, date);

    let world = h.world_mut();
    let forces = world.resource::<ForcesIndex>().clone();
    let record = world.get::<ShipRecord>(forces.ships[&ship]).unwrap();
    assert_eq!(record.captain, None, "the command falls vacant");
    assert!(
        world
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|e| e.text.contains("without a captain")),
        "and the fleet is told"
    );
}

#[test]
fn captain_assignment_survives_a_snapshot() {
    let mut h = host(34);
    let aron = char_id(&mut h, "aron-ash");
    let ship = {
        let key = key("ash-sloop");
        h.world_mut().resource::<ForcesIndex>().ship_keys[&key]
    };
    h.submit(PlayerCommand::SetShipCaptain {
        ship,
        captain: Some(aron),
    })
    .unwrap();
    h.advance_days(3);

    let snapshot = h.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(
        restored.state_hash(),
        h.state_hash(),
        "a ship's command round-trips through a snapshot"
    );

    let world = restored.world_mut();
    let forces = world.resource::<ForcesIndex>().clone();
    let record = world.get::<ShipRecord>(forces.ships[&ship]).unwrap();
    assert_eq!(record.captain, Some(aron));
}
