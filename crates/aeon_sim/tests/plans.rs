//! Plans: adoption under pressure, ordered steps through the shared
//! gate, skip conditions, retry-exhaustion, leader death, and snapshot
//! round-trips.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::map::MapIndex;
use aeon_sim::order::adjust_order;
use aeon_sim::plans::Plans;
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
// Household members below the head, for the ambition tests.
define_character(#{
    id: "yeva-birch", gender: "female",
    birth_year: 380, organisation: "birch",
    skills: #{ command: 4, diplomacy: 7, intrigue: 6, stewardship: 8 },
});
define_character(#{
    id: "brakk-birch", gender: "male",
    birth_year: 382, organisation: "birch",
    skills: #{ command: 7, diplomacy: 5, intrigue: 5, stewardship: 6 },
});

// The reactive scorer may only reach these two.
define_assignment(#{
    id: "settle-the-shire",
    ai_intent: "order", category: "consequential", duration_days: 15,
    skill: "diplomacy", difficulty: 5, ai_available: true,
    results: #{
        success: #{ weight: 999 },
        failure: #{ weight: 1 },
    },
});
define_assignment(#{
    id: "collect-a-tithe",
    ai_intent: "resources", category: "consequential", duration_days: 10,
    skill: "stewardship", difficulty: 5, ai_available: true,
    results: #{
        success: #{ weight: 999 },
        failure: #{ weight: 1 },
    },
});

// Reserved for plans: the scorer cannot start these. raise-a-fund
// carries a token cost so the peace plan is a head's plan, not a
// household ambition.
define_assignment(#{
    id: "raise-a-fund",
    category: "consequential", duration_days: 10, wealth_cost: 10,
    skill: "stewardship", difficulty: 5, ai_available: false,
    results: #{
        success: #{ weight: 999 },
        failure: #{ weight: 1 },
    },
});
define_assignment(#{
    id: "doomed-errand",
    category: "consequential", duration_days: 5,
    skill: "intrigue", difficulty: 40, ai_available: false,
    results: #{
        success: #{ weight: 1 },
        failure: #{ weight: 999 },
    },
});

define_plan(#{
    id: "restore-the-peace",
    goal: "order",
    cooldown_days: 60,
    max_days: 300,
    max_step_retries: 3,
    methods: [
        #{ id: "firm-hand",
           steps: [
               #{ start: "raise-a-fund", skip_if: #{ min_wealth: 5000 } },
               #{ start: "settle-the-shire" },
           ] },
    ],
});
define_plan(#{
    id: "fill-the-coffers",
    goal: "resources",
    cooldown_days: 90,
    max_days: 100,
    max_step_retries: 1,
    methods: [
        #{ id: "desperate", steps: [ #{ start: "doomed-errand" } ] },
    ],
});

// An army for the orders step to point at, and a doctrine to point it to.
define_army(#{
    id: "birch-levy", owner: "birch", general: "bela-birch",
    province: "beta", manpower: 400, supplies: 100,
});
define_assignment(#{
    id: "stand-guard",
    category: "consequential", duration_days: 10,
    skill: "command", difficulty: 5, target: "own-army",
    ai_available: false,
    results: #{
        success: #{ weight: 999 },
        failure: #{ weight: 1 },
    },
});
define_plan(#{
    id: "call-to-arms",
    goal: "muster",
    max_days: 60,
    methods: [
        #{ id: "ready", steps: [ #{ orders: ["stand-guard"], army: "own" } ] },
    ],
});

// A province-target assignment and a plan that aims it at whichever
// holding is worst off when the step starts.
define_assignment(#{
    id: "hold-the-ground",
    category: "consequential", duration_days: 10,
    skill: "diplomacy", difficulty: 5, target: "province",
    ai_available: false,
    results: #{
        success: #{ weight: 999 },
        failure: #{ weight: 1 },
    },
});
define_plan(#{
    id: "calm-the-worst",
    goal: "obligation",
    max_days: 90,
    methods: [
        #{ id: "in-person",
           steps: [ #{ start: "hold-the-ground", target: "worst-holding" } ] },
    ],
});

// A grievance held by the player, and a campaign aimed back at them,
// so the rumour path has something to whisper about.
define_obligation(#{
    id: "birch-resented-by-ash",
    kind: "grievance", debtor: "birch", creditor: "ash", weight: 30,
    origin: "an old slight at court",
});
define_assignment(#{
    id: "make-amends",
    ai_intent: "standing", category: "consequential", duration_days: 20,
    skill: "diplomacy", difficulty: 5, target: "organisation",
    ai_available: true,
    results: #{
        success: #{ weight: 999 },
        failure: #{ weight: 1 },
    },
});
define_plan(#{
    id: "mend-the-rift",
    goal: "standing",
    target: "organisation",
    score_bonus: 40,
    cooldown_days: 120,
    max_days: 200,
    max_step_retries: 1,
    methods: [
        #{ id: "gently", steps: [ #{ start: "make-amends", target: "plan" } ] },
    ],
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
            name: "Plan Trial".to_owned(),
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

/// Puts Birch's holding into disorder, the pressure the peace plan answers.
fn press_birch(h: &mut SimHost) {
    let beta = h.world_mut().resource::<MapIndex>().province_keys[&key("beta")];
    adjust_order(h.world_mut(), beta, -600);
}

/// Holds Birch's holding just unsettled enough to keep the pressure
/// above the plan threshold, without tipping it into unrest — a province
/// pressed into unrest for long enough revolts, and a lost holding is no
/// longer anyone's pressure.
fn hold_birch_pressed(h: &mut SimHost) {
    let beta = h.world_mut().resource::<MapIndex>().province_keys[&key("beta")];
    let current = aeon_sim::order::province_order(h.world_mut(), beta).order;
    if current > 250 {
        adjust_order(h.world_mut(), beta, 250 - current);
    }
}

/// The definition key of the assignment `id` currently points at, if it
/// is still running.
fn running_def(h: &mut SimHost, id: aeon_sim::AssignmentId) -> Option<ContentKey> {
    let world = h.world_mut();
    let entity = world
        .resource::<aeon_sim::AssignmentsIndex>()
        .assignments
        .get(&id)
        .copied()?;
    world
        .get::<aeon_sim::assignments::ActiveAssignment>(entity)
        .map(|a| a.def.clone())
}

#[test]
fn a_pressed_head_adopts_the_plan_and_walks_its_steps_in_order() {
    let mut h = host(21);
    press_birch(&mut h);
    let head = bela(&mut h);

    // Walk day by day so each stage of the plan can be observed.
    let mut saw_first_step = false;
    let mut adopted = false;
    for _ in 0..400 {
        h.advance_days(1);
        let plans = h.world_mut().resource::<Plans>().clone();
        if let Some(plan) = plans.active.get(&head) {
            adopted = true;
            assert_eq!(plan.def.as_str(), "restore-the-peace");
            assert_eq!(plan.method, "firm-hand");
            if let Some(id) = plan.current_assignment {
                let def = running_def(&mut h, id).expect("current assignment is live");
                match plan.step {
                    0 => {
                        // The first step is an assignment the reactive
                        // scorer may not touch: ai_available gates the
                        // scorer, not plans.
                        assert_eq!(def.as_str(), "raise-a-fund");
                        saw_first_step = true;
                    }
                    1 => assert_eq!(def.as_str(), "settle-the-shire"),
                    other => panic!("the plan has no step {other}"),
                }
            }
        } else if adopted {
            // The plan ended; it must have completed, not vanished.
            break;
        }
    }
    assert!(adopted, "a head under heavy disorder should adopt the plan");
    assert!(
        saw_first_step,
        "the first step should run before the second"
    );

    let plans = h.world_mut().resource::<Plans>().clone();
    assert!(
        !plans.active.contains_key(&head),
        "the plan should have completed"
    );
    assert!(
        plans
            .cooldowns
            .contains_key(&(head, key("restore-the-peace"))),
        "completion starts the cooldown"
    );
}

#[test]
fn a_step_already_satisfied_is_skipped() {
    let mut h = host(22);
    press_birch(&mut h);
    let head = bela(&mut h);

    // A full treasury satisfies the first step's skip condition.
    let birch = org(&mut h, "birch");
    {
        let world = h.world_mut();
        let entity = aeon_sim::access::org_entity(world, birch).expect("birch exists");
        world
            .get_mut::<aeon_sim::OrgResources>(entity)
            .expect("resources")
            .wealth = 10_000;
    }

    for _ in 0..400 {
        h.advance_days(1);
        let plans = h.world_mut().resource::<Plans>().clone();
        if !plans.active.contains_key(&head) {
            // Keep the pressure on until the head answers it; order
            // recovers daily and a slow pacing roll can outwait it.
            hold_birch_pressed(&mut h);
        }
        if let Some(plan) = plans.active.get(&head)
            && let Some(id) = plan.current_assignment
        {
            let def = running_def(&mut h, id).expect("current assignment is live");
            assert_eq!(
                def.as_str(),
                "settle-the-shire",
                "the funded step should be skipped, not run"
            );
            assert_eq!(plan.step, 1, "the plan should stand on its second step");
            return;
        }
    }
    panic!("the plan never started a step");
}

#[test]
fn a_step_that_keeps_failing_abandons_the_plan() {
    let mut h = host(23);
    let head = bela(&mut h);

    // An empty treasury is the pressure the doomed plan answers.
    let birch = org(&mut h, "birch");
    {
        let world = h.world_mut();
        let entity = aeon_sim::access::org_entity(world, birch).expect("birch exists");
        world
            .get_mut::<aeon_sim::OrgResources>(entity)
            .expect("resources")
            .wealth = 0;
    }

    let mut adopted = false;
    for _ in 0..400 {
        h.advance_days(1);
        let plans = h.world_mut().resource::<Plans>().clone();
        if plans.active.contains_key(&head) {
            adopted = true;
        }
        if adopted && !plans.active.contains_key(&head) {
            assert!(
                plans
                    .cooldowns
                    .contains_key(&(head, key("fill-the-coffers"))),
                "abandonment starts the cooldown like completion does"
            );
            return;
        }
    }
    panic!("the doomed plan was never adopted and abandoned; adopted={adopted}");
}

#[test]
fn a_dead_leader_abandons_the_plan() {
    let mut h = host(24);
    press_birch(&mut h);
    let head = bela(&mut h);

    for _ in 0..400 {
        h.advance_days(1);
        let active = h.world_mut().resource::<Plans>().active.contains_key(&head);
        if active {
            let date = {
                let world = h.world_mut();
                world.resource::<aeon_sim::CampaignClock>().date
            };
            let world = h.world_mut();
            let entity = world.resource::<PoliticsIndex>().characters[&head];
            world
                .get_mut::<aeon_sim::CharacterRecord>(entity)
                .expect("record")
                .death = Some(date);
            h.advance_days(2);
            assert!(
                !h.world_mut().resource::<Plans>().active.contains_key(&head),
                "a plan does not outlive the person pursuing it"
            );
            return;
        }
    }
    panic!("the plan was never adopted");
}

#[test]
fn plans_replay_identically_and_survive_snapshots() {
    let run = || {
        let mut h = host(25);
        let head = bela(&mut h);
        // Hold the pressure until the plan is adopted, so the snapshot
        // is taken with a plan genuinely in flight.
        for _ in 0..400 {
            h.advance_days(1);
            if h.world_mut().resource::<Plans>().active.contains_key(&head) {
                break;
            }
            hold_birch_pressed(&mut h);
        }
        assert!(
            h.world_mut().resource::<Plans>().active.contains_key(&head),
            "the fixture should adopt a plan within 400 days"
        );
        h
    };
    let mut a = run();
    let b = run();
    assert_eq!(
        a.state_hash(),
        b.state_hash(),
        "the same seed must pursue the same plans"
    );

    let snapshot = a.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "plan state must round-trip through a snapshot"
    );

    a.advance_days(300);
    restored.advance_days(300);
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "and continue identically afterwards"
    );
}

#[test]
fn a_plan_aimed_at_the_player_reaches_them_as_a_rumour() {
    use aeon_sim::{LogSubject, MessageLog};

    let mut h = host(26);
    let head = bela(&mut h);
    let ash = org(&mut h, "ash");

    // With nothing else pressing, Birch's heaviest pressure is the
    // grievance Ash holds, and the plan answering it is aimed at Ash —
    // the player's own house.
    for _ in 0..400 {
        h.advance_days(1);
        if h.world_mut().resource::<Plans>().active.contains_key(&head) {
            let plan = h.world_mut().resource::<Plans>().active[&head].clone();
            assert_eq!(plan.def.as_str(), "mend-the-rift");
            let whispered = h
                .world_mut()
                .resource::<MessageLog>()
                .entries
                .iter()
                .any(|entry| {
                    entry.org == Some(ash) && entry.subject == Some(LogSubject::Character(head))
                });
            assert!(
                whispered,
                "adopting a plan against the player should leave them a rumour"
            );
            return;
        }
    }
    panic!("the grievance plan was never adopted");
}

/// A pressure built by hand, for driving adoption directly in tests
/// whose subject is what happens after.
fn pressure(intent: aeon_data::model::AiIntent) -> aeon_sim::agency::ScoredIntent {
    aeon_sim::agency::ScoredIntent {
        intent,
        assignment: key("settle-the-shire"),
        target: aeon_sim::AssignmentTarget::None,
        score: 100,
        reason: String::new(),
        subject: None,
        explains: false,
    }
}

#[test]
fn an_orders_step_points_the_actors_own_army_at_the_doctrine() {
    use aeon_data::model::AiIntent;

    let mut h = host(27);
    let head = bela(&mut h);
    let birch = org(&mut h, "birch");

    assert!(
        aeon_sim::plans::try_adopt(h.world_mut(), head, birch, &[pressure(AiIntent::Muster)]),
        "the muster pressure should adopt call-to-arms"
    );
    h.advance_days(1);

    let world = h.world_mut();
    let orders: Vec<Vec<ContentKey>> = {
        let forces = world.resource::<aeon_sim::ForcesIndex>().clone();
        forces
            .armies
            .values()
            .filter_map(|e| world.get::<aeon_sim::ArmyRecord>(*e))
            .filter(|a| a.owner == birch)
            .map(|a| a.standing_order.0.clone())
            .collect()
    };
    assert_eq!(
        orders,
        vec![vec![key("stand-guard")]],
        "the general's own army should carry the plan's doctrine"
    );
    assert!(
        !world.resource::<Plans>().active.contains_key(&head),
        "a single instant step completes the plan the same day"
    );
}

#[test]
fn an_orders_step_with_no_army_to_order_waits() {
    use aeon_data::model::AiIntent;

    let mut h = host(28);
    let aron = h.world_mut().resource::<PoliticsIndex>().character_keys[&key("aron-ash")];
    let ash = org(&mut h, "ash");

    // Ash fields no army; the step has nothing to order and waits.
    assert!(aeon_sim::plans::try_adopt(
        h.world_mut(),
        aron,
        ash,
        &[pressure(AiIntent::Muster)]
    ));
    h.advance_days(5);
    let plans = h.world_mut().resource::<Plans>().clone();
    let plan = plans.active.get(&aron).expect("the plan waits, not dies");
    assert_eq!(plan.step, 0, "a blocked orders step stays where it is");
}

#[test]
fn a_worst_holding_selector_aims_at_the_most_disordered_province() {
    use aeon_data::model::AiIntent;

    let mut h = host(29);
    let head = bela(&mut h);
    let birch = org(&mut h, "birch");
    press_birch(&mut h);

    assert!(aeon_sim::plans::try_adopt(
        h.world_mut(),
        head,
        birch,
        &[pressure(AiIntent::Obligation)]
    ));
    h.advance_days(1);

    let beta = h.world_mut().resource::<MapIndex>().province_keys[&key("beta")];
    let plans = h.world_mut().resource::<Plans>().clone();
    let id = plans.active[&head]
        .current_assignment
        .expect("the step should have started");
    let world = h.world_mut();
    let entity = world.resource::<aeon_sim::AssignmentsIndex>().assignments[&id];
    let active = world
        .get::<aeon_sim::assignments::ActiveAssignment>(entity)
        .expect("live");
    assert_eq!(active.def.as_str(), "hold-the-ground");
    assert_eq!(
        active.target,
        aeon_sim::AssignmentTarget::Province(beta),
        "the selector should have named the disordered holding"
    );
}

#[test]
fn a_courtier_adopts_only_what_spends_nothing() {
    use aeon_data::model::AiIntent;

    let mut h = host(30);
    let yeva = h.world_mut().resource::<PoliticsIndex>().character_keys[&key("yeva-birch")];
    let birch = org(&mut h, "birch");

    // The peace plan's first step costs the house wealth; a courtier may
    // not reach for it, however heavy the pressure.
    assert!(
        !aeon_sim::plans::try_adopt(h.world_mut(), yeva, birch, &[pressure(AiIntent::Order)]),
        "a costed plan is the head's to adopt, not the household's"
    );

    // The coffers plan spends nothing, so the ambition is hers to have.
    assert!(aeon_sim::plans::try_adopt(
        h.world_mut(),
        yeva,
        birch,
        &[pressure(AiIntent::Resources)]
    ));
    let plans = h.world_mut().resource::<Plans>().clone();
    assert_eq!(plans.active[&yeva].def.as_str(), "fill-the-coffers");
}

#[test]
fn an_ambition_the_house_already_has_in_hand_is_not_duplicated() {
    use aeon_data::model::AiIntent;

    let mut h = host(31);
    let yeva = h.world_mut().resource::<PoliticsIndex>().character_keys[&key("yeva-birch")];
    let brakk = h.world_mut().resource::<PoliticsIndex>().character_keys[&key("brakk-birch")];
    let birch = org(&mut h, "birch");

    assert!(aeon_sim::plans::try_adopt(
        h.world_mut(),
        yeva,
        birch,
        &[pressure(AiIntent::Resources)]
    ));
    assert!(
        !aeon_sim::plans::try_adopt(
            h.world_mut(),
            brakk,
            birch,
            &[pressure(AiIntent::Resources)]
        ),
        "one member already pursues it; a second ambition adds nothing but noise"
    );
}

#[test]
fn a_spectator_campaign_leaves_no_house_idle() {
    let mut h = host(32);
    aeon_sim::state::become_spectator(h.world_mut());
    let aron = h.world_mut().resource::<PoliticsIndex>().character_keys[&key("aron-ash")];
    let alpha = h.world_mut().resource::<MapIndex>().province_keys[&key("alpha")];

    // Ash is the fixture's player house; with no player, its head is one
    // more autonomous character. The peace plan's first step costs the
    // house wealth, so adopting it takes an authority no player-house
    // head is ever offered by the agency pass.
    for _ in 0..400 {
        h.advance_days(1);
        if h.world_mut().resource::<Plans>().active.contains_key(&aron) {
            let plan = h.world_mut().resource::<Plans>().active[&aron].clone();
            assert_eq!(plan.def.as_str(), "restore-the-peace");

            // And spectatorship is campaign state, not session state.
            let snapshot = h.snapshot();
            let restored = SimHost::restore_with_content(snapshot, content()).unwrap();
            let mut restored = restored;
            assert_eq!(
                restored.world_mut().resource::<aeon_sim::PlayerHouse>().0,
                None,
                "a spectator campaign restores as a spectator campaign"
            );
            return;
        }
        let current = aeon_sim::order::province_order(h.world_mut(), alpha).order;
        if current > 250 {
            adjust_order(h.world_mut(), alpha, 250 - current);
        }
    }
    panic!("the erstwhile player house never acted on its own pressures");
}
