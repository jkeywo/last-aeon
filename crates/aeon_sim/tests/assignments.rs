//! Assignment-system guarantees: command-driven initiation, delegation rules,
//! skill-shifted graded results, routine retries, script effects, popups
//! with choices, personal risks, the notable-result log, AI agency, and
//! snapshot fidelity.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::model::{OutcomeKind, RiskTag};
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::assignments::{CharacterCondition, apply_risk};
use aeon_sim::politics::process_death;
use aeon_sim::{
    AssignmentTarget, AssignmentsIndex, CampaignConfig, CharacterId, CommandRejection, MessageLog,
    PendingPopups, PlayerCommand, PoliticsIndex, SimHost, opinion_between,
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
                   latitude_mdeg: 10000, longitude_mdeg: 10000 });

define_house(#{
    id: "ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha"],
});
define_house(#{
    id: "birch", tier: "great",
    head: "bela-birch", color: [60, 60, 200], provinces: ["beta"],
});

define_character(#{
    id: "aron-ash", gender: "male",
    birth_year: 370, organisation: "ash",
    skills: #{ command: 8, diplomacy: 20, intrigue: 4, stewardship: 7 },
});
define_character(#{
    id: "cera-ash", gender: "female",
    birth_year: 380, organisation: "ash",
    skills: #{ command: 4, diplomacy: 6, intrigue: 5, stewardship: 8 },
});
define_character(#{
    id: "bela-birch", gender: "female",
    birth_year: 372, organisation: "birch",
    skills: #{ command: 6, diplomacy: 9, intrigue: 8, stewardship: 5 },
});

// Always succeeds for a competent leader; carries a courting effect.
define_assignment(#{
    id: "sure-court", 
    category: "consequential", duration_days: 10,
    skill: "diplomacy", difficulty: 0, target: "organisation",
    ai_available: false,
    results: #{
        success: #{
            weight: 1000000, log: true,
            effect_fn: "court_win",
        },
        failure: #{ weight: 1 },
    },
});
fn court_win(ctx) {
    [#{ kind: "opinion", from: "target-head", toward: "leader",
        amount: 20, days: 720, reason: "courted" }]
}

// Practically always fails; routine, so it retries.
define_assignment(#{
    id: "doomed-chore", 
    category: "routine", duration_days: 5,
    skill: "stewardship", difficulty: 40, ai_available: false,
    results: #{
        success: #{ weight: 1 },
        failure: #{ weight: 1000000 },
    },
});

// Popup with two choices on success.
define_assignment(#{
    id: "momentous-find", 
    category: "consequential", duration_days: 7,
    skill: "stewardship", difficulty: 0, ai_available: false,
    results: #{
        success: #{
            weight: 1000000, popup: true,
            choices: [
                #{ id: "keep-quiet", },
                #{ id: "share-it", effect_fn: "share_find" },
            ],
        },
        failure: #{ weight: 1 },
    },
});
fn share_find(ctx) {
    [#{ kind: "log", message_key: "assignment.momentous-find.shared.log" }]
}

// A spread of outcomes, each logged distinctly, so a forecast can be
// checked against what actually happens. Difficulty matches Cera's
// stewardship, so effectiveness is zero and the authored weights apply
// unshifted: 100 / 300 / 400 / 200 permille.
define_assignment(#{
    id: "even-gamble", 
    category: "consequential", duration_days: 5,
    skill: "stewardship", difficulty: 8, ai_available: false,
    risks: ["injury"],
    results: #{
        critical_success: #{ weight: 100, log: true, },
        success: #{ weight: 300, log: true, },
        failure: #{ weight: 400, log: true, },
        disaster: #{ weight: 200, log: true, },
    },
});

// AI-available assignment so autonomous organisations act.
define_assignment(#{
    id: "ai-errand", 
    category: "consequential", duration_days: 15,
    skill: "diplomacy", difficulty: 5, ai_available: true,
    results: #{
        success: #{ weight: 700, log: true, },
        failure: #{ weight: 300 },
    },
});
// One per requirement, so each gate is proved on its own rather than
// through some other assignment that happens to share it.
define_assignment(#{
    id: "gated-raid",
    category: "consequential", duration_days: 5,
    skill: "command", difficulty: 0, target: "province",
    requires: #{ target_holder: "other" },
    ai_available: false,
    results: #{ success: #{ weight: 1 }, failure: #{ weight: 1 } },
});
define_assignment(#{
    id: "gated-alarm",
    category: "consequential", duration_days: 5,
    skill: "command", difficulty: 0, target: "province",
    requires: #{ owner_threatened: true },
    ai_available: false,
    results: #{ success: #{ weight: 1 }, failure: #{ weight: 1 } },
});
define_assignment(#{
    id: "gated-petition",
    category: "consequential", duration_days: 5,
    skill: "diplomacy", difficulty: 0, target: "character",
    requires: #{ target_holds_title: "consul" },
    ai_available: false,
    results: #{ success: #{ weight: 1 }, failure: #{ weight: 1 } },
});
define_assignment(#{
    id: "gated-favour",
    category: "consequential", duration_days: 5,
    skill: "diplomacy", difficulty: 0, target: "organisation",
    requires: #{ target_owes_favour: true },
    ai_available: false,
    results: #{ success: #{ weight: 1 }, failure: #{ weight: 1 } },
});
define_assignment(#{
    id: "gated-whisper",
    category: "consequential", duration_days: 5,
    skill: "intrigue", difficulty: 0, target: "character",
    requires: #{ target_house: "other" },
    ai_available: false,
    results: #{ success: #{ weight: 1 }, failure: #{ weight: 1 } },
});
// Two phases: the road, which can be turned back from, and the work
// itself, which cannot.
define_assignment(#{
    id: "staged-work",
    category: "consequential", duration_days: 20,
    skill: "command", difficulty: 0, target: "none",
    ai_available: false,
    stages: [
        #{ id: "approach", days: 14, interruptible: true },
        #{ id: "commit", days: 6, interruptible: false },
    ],
    results: #{ success: #{ weight: 1 }, failure: #{ weight: 1 } },
});
"#;

/// The prose behind the fixture's IDs.
///
/// These tests are about text reaching the player — a popup naming who it
/// is about, an outcome distinguishable in the log — so they need real
/// rows rather than the blank table fixtures use when prose is beside the
/// point.
fn strings() -> aeon_data::StringTable {
    // Starts from the shipped table so the simulation's own rows — the
    // lines it writes into the log itself — are still there.
    let mut table = aeon_sim::TextDb::embedded().0.as_ref().clone();
    table.extend(&[
        ("character.aron-ash.name", "Aron Ash"),
        ("character.cera-ash.name", "Cera Ash"),
        ("character.bela-birch.name", "Bela Birch"),
        ("organisation.ash.name", "House Ash"),
        ("organisation.birch.name", "House Birch"),
        ("body.world.name", "World"),
        ("province.alpha.name", "Alpha"),
        ("province.beta.name", "Beta"),
        ("scenario.fixture.name", "Fixture"),
        ("assignment.sure-court.title", "Court a Rival"),
        ("assignment.sure-court.summary", "Send an envoy."),
        (
            "assignment.sure-court.success.log-text",
            "{leader} charmed {target}.",
        ),
        ("assignment.doomed-chore.title", "A Doomed Chore"),
        ("assignment.doomed-chore.summary", "It will not go well."),
        ("assignment.momentous-find.title", "A Momentous Find"),
        ("assignment.momentous-find.summary", "Something turned up."),
        (
            "assignment.momentous-find.success.popup-text",
            "{leader} found it. Cera Ash was there.",
        ),
        (
            "assignment.momentous-find.success.choice.keep-quiet",
            "Keep quiet",
        ),
        (
            "assignment.momentous-find.success.choice.share-it",
            "Share it",
        ),
        (
            "assignment.momentous-find.shared.log",
            "The find was shared with the court.",
        ),
        ("assignment.even-gamble.title", "An Even Gamble"),
        ("assignment.even-gamble.summary", "Could go either way."),
        (
            "assignment.even-gamble.critical-success.log-text",
            "OUTCOME-CRIT",
        ),
        ("assignment.even-gamble.success.log-text", "OUTCOME-SUCCESS"),
        ("assignment.even-gamble.failure.log-text", "OUTCOME-FAILURE"),
        (
            "assignment.even-gamble.disaster.log-text",
            "OUTCOME-DISASTER",
        ),
        ("assignment.staged-work.title", "Staged Work"),
        ("assignment.staged-work.summary", "It has phases."),
        ("assignment.gated-raid.title", "Raid"),
        ("assignment.gated-raid.summary", "Take what is theirs."),
        ("assignment.gated-alarm.title", "Respond"),
        ("assignment.gated-alarm.summary", "Answer the alarm."),
        ("assignment.gated-petition.title", "Petition"),
        ("assignment.gated-petition.summary", "Ask the Consul."),
        ("assignment.gated-favour.title", "Call In Favour"),
        ("assignment.gated-favour.summary", "Collect what is owed."),
        ("assignment.gated-whisper.title", "Discredit"),
        ("assignment.gated-whisper.summary", "Spread a word."),
        ("assignment.ai-errand.title", "An AI errand"),
        ("assignment.ai-errand.summary", "Ordinary business."),
        (
            "assignment.ai-errand.success.log-text",
            "the errand was run",
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
    let mut host = SimHost::new_with_content(
        CampaignConfig {
            name: "Assignments Trial".to_owned(),
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
    );
    // The plugin installs the shipped table; this fixture has its own
    // prose, and the lines a script asks for by key are resolved as they
    // are written, so the simulation must read the same table the content
    // was loaded against.
    host.world_mut()
        .insert_resource(aeon_sim::TextDb(Arc::new(strings())));
    host
}

fn char_id(h: &mut SimHost, key: &str) -> CharacterId {
    let key = ContentKey::new(key).unwrap();
    h.world_mut().resource::<PoliticsIndex>().character_keys[&key]
}

fn org_id(h: &mut SimHost, key: &str) -> aeon_sim::OrgId {
    let key = ContentKey::new(key).unwrap();
    h.world_mut().resource::<PoliticsIndex>().org_keys[&key]
}

fn key(text: &str) -> ContentKey {
    ContentKey::new(text).unwrap()
}

#[test]
fn assignments_resolve_with_effects_and_logs() {
    let mut h = host(1);
    let aron = char_id(&mut h, "aron-ash");
    let bela = char_id(&mut h, "bela-birch");
    let birch = org_id(&mut h, "birch");

    let before = opinion_between(h.world_mut(), bela, aron);
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("sure-court"),
        leader: aron,
        target: AssignmentTarget::Org(birch),
    })
    .unwrap();
    h.advance_days(12);

    let world = h.world_mut();
    assert!(world.resource::<AssignmentsIndex>().assignments.is_empty());
    let after = opinion_between(world, bela, aron);
    assert_eq!(after, before + 20, "courting effect applies");
    let log = world.resource::<MessageLog>();
    assert!(
        log.entries.iter().any(|e| e.text.contains("charmed")),
        "log: {:?}",
        log.entries
    );
}

#[test]
fn one_character_leads_one_assignment_and_delegation_works() {
    let mut h = host(2);
    let aron = char_id(&mut h, "aron-ash");
    let cera = char_id(&mut h, "cera-ash");
    let bela = char_id(&mut h, "bela-birch");
    let birch = org_id(&mut h, "birch");

    h.submit(PlayerCommand::StartAssignment {
        assignment: key("sure-court"),
        leader: aron,
        target: AssignmentTarget::Org(birch),
    })
    .unwrap();
    h.advance_days(1);

    // The head is now busy.
    let refused = h.submit(PlayerCommand::StartAssignment {
        assignment: key("doomed-chore"),
        leader: aron,
        target: AssignmentTarget::None,
    });
    assert!(matches!(refused, Err(CommandRejection::Assignment(_))));

    // Delegation to another member works.
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("doomed-chore"),
        leader: cera,
        target: AssignmentTarget::None,
    })
    .unwrap();
    h.advance_days(1);
    assert_eq!(
        h.world_mut()
            .resource::<AssignmentsIndex>()
            .assignments
            .len(),
        2
    );

    // A character from another organisation cannot lead for the player.
    let refused = h.submit(PlayerCommand::StartAssignment {
        assignment: key("sure-court"),
        leader: bela,
        target: AssignmentTarget::Org(birch),
    });
    assert!(matches!(refused, Err(CommandRejection::Assignment(_))));
}

#[test]
fn routine_failures_retry_automatically() {
    let mut h = host(3);
    let cera = char_id(&mut h, "cera-ash");
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("doomed-chore"),
        leader: cera,
        target: AssignmentTarget::None,
    })
    .unwrap();

    // Across several failure cycles the assignment keeps restarting.
    h.advance_days(23);
    let world = h.world_mut();
    let index = world.resource::<AssignmentsIndex>();
    assert_eq!(
        index.assignments.len(),
        1,
        "routine assignment restarted after failure"
    );
}

#[test]
fn popups_open_for_the_player_and_choices_apply() {
    let mut h = host(4);
    let cera = char_id(&mut h, "cera-ash");
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("momentous-find"),
        leader: cera,
        target: AssignmentTarget::None,
    })
    .unwrap();
    h.advance_days(9);

    let popup_id = {
        let world = h.world_mut();
        let popups = world.resource::<PendingPopups>();
        assert_eq!(popups.popups.len(), 1, "popup opened");
        assert!(popups.popups[0].text.contains("Cera Ash"));
        assert_eq!(popups.popups[0].choices.len(), 2);
        popups.popups[0].id
    };

    // A bad answer is rejected at validation.
    let refused = h.submit(PlayerCommand::AnswerPopup {
        popup: popup_id,
        choice: key("no-such-choice"),
    });
    assert!(matches!(refused, Err(CommandRejection::Assignment(_))));

    h.submit(PlayerCommand::AnswerPopup {
        popup: popup_id,
        choice: key("share-it"),
    })
    .unwrap();
    h.advance_days(1);

    let world = h.world_mut();
    assert!(world.resource::<PendingPopups>().popups.is_empty());
    assert!(
        world
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|e| e.text.contains("shared with the court"))
    );
}

#[test]
fn risks_have_their_stated_consequences() {
    let mut h = host(5);
    let cera = char_id(&mut h, "cera-ash");
    let date = h.date();

    let world = h.world_mut();
    apply_risk(world, cera, RiskTag::Injury, date);
    let entity = world.resource::<PoliticsIndex>().characters[&cera];
    let condition = *world.get::<CharacterCondition>(entity).unwrap();
    assert!(condition.injured_until.is_some());
    assert!(!condition.can_lead(date));

    // An injured character cannot take a assignment.
    let refused = h.submit(PlayerCommand::StartAssignment {
        assignment: key("doomed-chore"),
        leader: cera,
        target: AssignmentTarget::None,
    });
    assert!(matches!(refused, Err(CommandRejection::Assignment(_))));

    // Death through risk flows into succession machinery.
    let aron = char_id(&mut h, "aron-ash");
    let world = h.world_mut();
    apply_risk(world, aron, RiskTag::Death, date);
    let entity = world.resource::<PoliticsIndex>().characters[&aron];
    assert!(
        world
            .get::<aeon_sim::CharacterRecord>(entity)
            .unwrap()
            .death
            .is_some()
    );
}

#[test]
fn dead_leaders_abandon_their_assignments() {
    let mut h = host(6);
    let cera = char_id(&mut h, "cera-ash");
    let birch = org_id(&mut h, "birch");
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("sure-court"),
        leader: cera,
        target: AssignmentTarget::Org(birch),
    })
    .unwrap();
    h.advance_days(2);

    let date = h.date();
    process_death(h.world_mut(), cera, date);
    h.advance_days(10);

    let world = h.world_mut();
    assert!(world.resource::<AssignmentsIndex>().assignments.is_empty());
    assert!(
        world
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|e| e.text.contains("abandoned"))
    );
}

#[test]
fn ai_organisations_act_and_their_results_reach_the_log() {
    let mut h = host(7);
    // A year of monthly AI planning with an ai_available errand.
    h.advance_days(400);
    let world = h.world_mut();
    let log = world.resource::<MessageLog>();
    let birch = world.resource::<PoliticsIndex>().org_keys[&key("birch")];
    assert!(
        log.entries
            .iter()
            .any(|e| e.org == Some(birch) && e.text.contains("errand")),
        "AI-run assignments should reach the shared log: {:?}",
        log.entries
    );
}

#[test]
fn assignment_world_is_deterministic_and_survives_snapshots() {
    let run = |seed: u64| {
        let mut h = host(seed);
        let aron = char_id(&mut h, "aron-ash");
        let birch = org_id(&mut h, "birch");
        h.submit(PlayerCommand::StartAssignment {
            assignment: key("sure-court"),
            leader: aron,
            target: AssignmentTarget::Org(birch),
        })
        .unwrap();
        h.advance_days(200);
        h
    };
    let mut a = run(11);
    let b = run(11);
    assert_eq!(a.state_hash(), b.state_hash());

    let snapshot = a.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(restored.state_hash(), a.state_hash());

    a.advance_days(100);
    restored.advance_days(100);
    assert_eq!(restored.state_hash(), a.state_hash());
}

// ---------------------------------------------------------------------------
// Forecasts
// ---------------------------------------------------------------------------

/// Runs `even-gamble` once per seed and tallies which outcome actually
/// occurred, read back from the distinct log lines each result writes.
fn sample_outcomes(trials: u64) -> [u32; 4] {
    let shared = content();
    let mut tally = [0u32; 4];
    for seed in 0..trials {
        let mut h = SimHost::new_with_content(
            CampaignConfig {
                name: "Forecast Trial".to_owned(),
                seed,
                start_date: CalendarDate {
                    year: 411,
                    month: 1,
                    day: 1,
                }
                .to_date()
                .unwrap(),
            },
            Arc::clone(&shared),
        );
        let cera = char_id(&mut h, "cera-ash");
        h.submit(PlayerCommand::StartAssignment {
            assignment: key("even-gamble"),
            leader: cera,
            target: AssignmentTarget::None,
        })
        .unwrap();
        h.advance_days(12);
        let log = h.world_mut().resource::<MessageLog>().clone();
        let seen = |needle: &str| log.entries.iter().any(|e| e.text.contains(needle));
        if seen("OUTCOME-CRIT") {
            tally[0] += 1;
        } else if seen("OUTCOME-SUCCESS") {
            tally[1] += 1;
        } else if seen("OUTCOME-FAILURE") {
            tally[2] += 1;
        } else if seen("OUTCOME-DISASTER") {
            tally[3] += 1;
        } else {
            panic!("seed {seed} produced no recognisable outcome");
        }
    }
    tally
}

#[test]
fn a_forecast_reports_the_odds_that_actually_resolve() {
    let mut h = host(1);
    let cera = char_id(&mut h, "cera-ash");
    let ash = org_id(&mut h, "ash");
    let forecast = aeon_sim::forecast::forecast(
        h.world_mut(),
        ash,
        &key("even-gamble"),
        cera,
        AssignmentTarget::None,
    )
    .expect("assignment is defined");

    // Difficulty equals the leader's skill, so the authored weights stand.
    assert_eq!(forecast.effectiveness, 0);
    assert_eq!(forecast.skill_value, 8);
    assert_eq!(forecast.difficulty, 8);
    let chance = |kind: OutcomeKind| -> u32 {
        forecast
            .results
            .iter()
            .find(|r| r.kind == kind)
            .map(|r| r.chance)
            .expect("kind present")
    };
    assert_eq!(chance(OutcomeKind::CriticalSuccess), 100);
    assert_eq!(chance(OutcomeKind::Success), 300);
    assert_eq!(chance(OutcomeKind::Failure), 400);
    assert_eq!(chance(OutcomeKind::Disaster), 200);
    assert_eq!(forecast.success_chance(), 400);

    // What the player was promised is what the simulation delivers. The
    // tolerance is wide enough never to flake but far tighter than any
    // real divergence between the forecast and resolution would produce.
    const TRIALS: u64 = 1500;
    let tally = sample_outcomes(TRIALS);
    let observed: Vec<u32> = tally
        .iter()
        .map(|count| (u64::from(*count) * 1000 / TRIALS) as u32)
        .collect();
    for (index, kind) in OutcomeKind::ALL.iter().enumerate() {
        let forecast_chance = chance(*kind);
        let seen = observed[index];
        let drift = forecast_chance.abs_diff(seen);
        assert!(
            drift <= 60,
            "{kind:?}: forecast {forecast_chance}permille but observed {seen}permille",
        );
    }
}

#[test]
fn a_forecast_quotes_costs_duration_delay_and_risks() {
    let mut h = host(3);
    let cera = char_id(&mut h, "cera-ash");
    let ash = org_id(&mut h, "ash");
    let forecast = aeon_sim::forecast::forecast(
        h.world_mut(),
        ash,
        &key("even-gamble"),
        cera,
        AssignmentTarget::None,
    )
    .expect("assignment is defined");

    assert_eq!(forecast.duration_days, 5, "authored duration is quoted");
    assert!(
        forecast.order_delay_days >= 0,
        "an order delay is always reported"
    );
    assert!(forecast.startable(), "an eligible leader is not blocked");
    assert_eq!(forecast.risks.len(), 1, "the authored risk is surfaced");
    let injury = forecast.risks[0];
    assert_eq!(injury.tag, RiskTag::Injury);
    assert!(
        injury.on_disaster > injury.on_failure,
        "a disaster must be the more dangerous outcome"
    );
    assert!(
        forecast.military_op.is_none(),
        "a civil assignment has no conditional field contest"
    );
}

#[test]
fn a_forecast_explains_why_a_assignment_cannot_be_started() {
    let mut h = host(4);
    let cera = char_id(&mut h, "cera-ash");
    let ash = org_id(&mut h, "ash");
    // Put Cera to work, then forecast a second assignment for her.
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("even-gamble"),
        leader: cera,
        target: AssignmentTarget::None,
    })
    .unwrap();
    h.advance_days(1);

    let forecast = aeon_sim::forecast::forecast(
        h.world_mut(),
        ash,
        &key("even-gamble"),
        cera,
        AssignmentTarget::None,
    )
    .expect("assignment is defined");
    assert!(
        !forecast.startable(),
        "a busy leader cannot start a assignment"
    );
    assert_eq!(
        forecast.blocked,
        Some(aeon_sim::assignments::AssignmentRejection::LeaderBusy)
    );
    // The forecast still describes the assignment, so the player can plan ahead.
    assert_eq!(forecast.duration_days, 5);
    assert_eq!(forecast.success_chance(), 400);
}

// ---------------------------------------------------------------------------
// Leader availability
// ---------------------------------------------------------------------------

#[test]
fn availability_names_the_specific_reason_a_character_cannot_lead() {
    use aeon_sim::{LeaderAvailability, Post, leader_availability};

    let mut h = host(21);
    let aron = char_id(&mut h, "aron-ash");
    let cera = char_id(&mut h, "cera-ash");
    let bela = char_id(&mut h, "bela-birch");
    let ash = org_id(&mut h, "ash");
    let birch = org_id(&mut h, "birch");
    let date = h.world_mut().resource::<aeon_sim::CampaignClock>().date;

    // Free.
    assert_eq!(
        leader_availability(h.world_mut(), ash, aron, date),
        LeaderAvailability::Available
    );

    // A member of another house is not ours to command.
    assert!(matches!(
        leader_availability(h.world_mut(), ash, bela, date),
        LeaderAvailability::Ineligible(_)
    ));
    assert!(matches!(
        leader_availability(h.world_mut(), birch, aron, date),
        LeaderAvailability::Ineligible(_)
    ));

    // Busy names the assignment and when it ends, not merely "unavailable".
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("sure-court"),
        leader: aron,
        target: AssignmentTarget::Org(birch),
    })
    .unwrap();
    h.advance_days(1);
    match leader_availability(h.world_mut(), ash, aron, date) {
        LeaderAvailability::Busy { def, .. } => assert_eq!(def, key("sure-court")),
        other => panic!("expected Busy, got {other:?}"),
    }

    // A standing command is reported as such, and names the force.
    let alpha = {
        let key = ContentKey::new("alpha").unwrap();
        h.world_mut().resource::<aeon_sim::MapIndex>().province_keys[&key]
    };
    let army = aeon_sim::forces::form_army(h.world_mut(), ash, cera, 500, 100, alpha);
    match leader_availability(h.world_mut(), ash, cera, date) {
        LeaderAvailability::Posted(Post::General { army: id, .. }) => {
            assert_eq!(id, army)
        }
        other => panic!("expected Assigned, got {other:?}"),
    }
}

#[test]
fn indisposition_reports_when_it_clears() {
    use aeon_sim::{LeaderAvailability, leader_availability};

    let mut h = host(22);
    let cera = char_id(&mut h, "cera-ash");
    let ash = org_id(&mut h, "ash");
    let date = h.world_mut().resource::<aeon_sim::CampaignClock>().date;

    apply_risk(h.world_mut(), cera, RiskTag::Injury, date);
    match leader_availability(h.world_mut(), ash, cera, date) {
        LeaderAvailability::Indisposed { until: Some(until) } => {
            assert!(until > date, "recovery must lie in the future");
        }
        other => panic!("expected Indisposed with a date, got {other:?}"),
    }
}

#[test]
fn a_standing_command_bars_a_second_one_but_not_ordinary_work() {
    use aeon_sim::{AssignmentRejection, LeaderAvailability, Post};

    let commanding = LeaderAvailability::Posted(Post::General {
        army: aeon_sim::ArmyId::from_raw(7).unwrap(),
        name: "First Levy".to_owned(),
    });
    let other_army = aeon_sim::ArmyId::from_raw(9).unwrap();
    let own_army = aeon_sim::ArmyId::from_raw(7).unwrap();

    // A general may still court, scheme, and administer.
    assert_eq!(commanding.blocks_assignment(AssignmentTarget::None), None);
    assert_eq!(
        commanding.blocks_assignment(AssignmentTarget::Org(aeon_sim::OrgId::from_raw(3).unwrap())),
        None
    );

    // And may order the force they actually command.
    assert_eq!(
        commanding.blocks_assignment(AssignmentTarget::OwnArmy(own_army)),
        None
    );

    // But not somebody else's.
    assert_eq!(
        commanding.blocks_assignment(AssignmentTarget::OwnArmy(other_army)),
        Some(AssignmentRejection::AlreadyAssigned)
    );
}

#[test]
fn the_simulation_and_its_availability_report_never_disagree() {
    // The bug this guards against: three separate eligibility checks that
    // drifted apart, so the interface offered leaders the simulation then
    // refused. Every character, every target kind, one answer.
    use aeon_sim::{PoliticsIndex, leader_availability};

    let mut h = host(23);
    let ash = org_id(&mut h, "ash");
    let birch = org_id(&mut h, "birch");
    let aron = char_id(&mut h, "aron-ash");

    // Put one character to work so the busy path is covered too.
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("sure-court"),
        leader: aron,
        target: AssignmentTarget::Org(birch),
    })
    .unwrap();
    h.advance_days(1);

    let date = h.world_mut().resource::<aeon_sim::CampaignClock>().date;
    let everyone: Vec<_> = h
        .world_mut()
        .resource::<PoliticsIndex>()
        .characters
        .keys()
        .copied()
        .collect();

    for character in everyone {
        for target in [AssignmentTarget::None, AssignmentTarget::Org(birch)] {
            let reported = leader_availability(h.world_mut(), ash, character, date)
                .blocks_assignment(target)
                .is_none();
            let accepted = aeon_sim::command::validate_command(
                h.world_mut(),
                &PlayerCommand::StartAssignment {
                    assignment: key("sure-court"),
                    leader: character,
                    target,
                },
            );
            // The command pipeline may refuse for reasons beyond the
            // leader (a bad target for the definition); it must never
            // accept a leader the report calls unavailable.
            if !reported {
                assert!(
                    accepted.is_err(),
                    "availability says no but the simulation accepted {character:?}"
                );
            }
        }
    }
}

/// The authored requirements, one test per gate.
///
/// These are the defects playtesting reported: buttons to raid your own
/// holdings, and an alarm to answer with nothing sounding. Before the
/// requirements existed, `target_valid` asked only whether the target
/// *existed*, so a province was a legal raid target because it was a
/// province.
mod requirements {
    use super::*;
    use aeon_sim::{AssignmentTarget, ProvinceId, target_allowed};

    fn province(h: &mut SimHost, name: &str) -> ProvinceId {
        h.world_mut()
            .resource::<aeon_sim::map::MapIndex>()
            .province_keys[&key(name)]
    }

    #[test]
    fn you_cannot_raid_your_own_holding() {
        let mut h = host(1);
        let ash = org_id(&mut h, "ash");
        let ours = province(&mut h, "alpha");
        let theirs = province(&mut h, "beta");

        assert!(
            !target_allowed(
                h.world_mut(),
                &key("gated-raid"),
                ash,
                AssignmentTarget::Province(ours)
            ),
            "Alpha is ours, so raiding it must not be on offer"
        );
        assert!(
            target_allowed(
                h.world_mut(),
                &key("gated-raid"),
                ash,
                AssignmentTarget::Province(theirs)
            ),
            "Beta is Birch's, so raiding it is a legal thing to want"
        );
    }

    #[test]
    fn an_alarm_needs_something_to_answer() {
        let mut h = host(2);
        let ash = org_id(&mut h, "ash");
        let ours = province(&mut h, "alpha");
        // Nobody is standing in Ash's holdings at the start.
        assert!(
            !target_allowed(
                h.world_mut(),
                &key("gated-alarm"),
                ash,
                AssignmentTarget::Province(ours)
            ),
            "with no hostile force anywhere of ours, there is no alarm"
        );
    }

    #[test]
    fn only_the_consul_can_be_petitioned() {
        let mut h = host(3);
        let ash = org_id(&mut h, "ash");
        let bela = char_id(&mut h, "bela-birch");
        // The fixture appoints no Consul, so nobody qualifies — which is
        // the case the client used to get wrong by naming the assignment
        // in its own source.
        assert!(
            !target_allowed(
                h.world_mut(),
                &key("gated-petition"),
                ash,
                AssignmentTarget::Character(bela)
            ),
            "Bela holds no Consulship, so there is nothing to petition"
        );
    }

    #[test]
    fn a_favour_can_only_be_called_in_from_someone_who_owes_one() {
        let mut h = host(4);
        let ash = org_id(&mut h, "ash");
        let birch = org_id(&mut h, "birch");
        assert!(
            !target_allowed(
                h.world_mut(),
                &key("gated-favour"),
                ash,
                AssignmentTarget::Org(birch)
            ),
            "Birch owes Ash nothing, so there is nothing to call in"
        );

        aeon_sim::obligations::create(
            h.world_mut(),
            aeon_sim::obligations::ObligationKind::Favour,
            birch,
            ash,
            "a debt",
            10,
            None,
        );
        assert!(
            target_allowed(
                h.world_mut(),
                &key("gated-favour"),
                ash,
                AssignmentTarget::Org(birch)
            ),
            "and once they owe one, it can be called in"
        );
    }

    #[test]
    fn whispers_are_spread_about_other_peoples_people() {
        let mut h = host(5);
        let ash = org_id(&mut h, "ash");
        let ours = char_id(&mut h, "cera-ash");
        let theirs = char_id(&mut h, "bela-birch");

        assert!(
            !target_allowed(
                h.world_mut(),
                &key("gated-whisper"),
                ash,
                AssignmentTarget::Character(ours)
            ),
            "Cera is ours"
        );
        assert!(
            target_allowed(
                h.world_mut(),
                &key("gated-whisper"),
                ash,
                AssignmentTarget::Character(theirs)
            ),
            "Bela is not"
        );
    }

    #[test]
    fn an_assignment_that_declares_nothing_is_offered_as_widely_as_before() {
        // The gate must be opt-in: content that says nothing about its
        // targets keeps whatever reach it had.
        let mut h = host(6);
        let ash = org_id(&mut h, "ash");
        let birch = org_id(&mut h, "birch");
        assert!(target_allowed(
            h.world_mut(),
            &key("sure-court"),
            ash,
            AssignmentTarget::Org(birch)
        ));
    }
}

/// Phases, and what being called off means during each.
///
/// An assignment that authors no phases is one phase long and can be
/// called off at any time, which is how every assignment behaved before
/// phases existed — so the default has to keep working exactly as it did.
mod stages {
    use super::*;
    use aeon_sim::AssignmentTarget;

    fn start(h: &mut SimHost, who: &str, what: &str) -> aeon_sim::AssignmentId {
        let org = org_id(h, "ash");
        let leader = char_id(h, who);
        aeon_sim::start_assignment(
            h.world_mut(),
            org,
            &key(what),
            leader,
            AssignmentTarget::None,
        )
    }

    fn live(h: &mut SimHost, id: aeon_sim::AssignmentId) -> bool {
        aeon_sim::access::assignment(h.world_mut(), id).is_some()
    }

    #[test]
    fn an_assignment_with_no_phases_can_be_called_off_at_any_time() {
        let mut h = host(1);
        let id = start(&mut h, "aron-ash", "sure-court");
        h.advance_days(3);
        aeon_sim::request_cancel(h.world_mut(), id);
        assert!(!live(&mut h, id), "one phase, always interruptible");
    }

    #[test]
    fn calling_off_during_an_interruptible_phase_ends_it_at_once() {
        let mut h = host(2);
        let id = start(&mut h, "aron-ash", "staged-work");
        h.advance_days(5); // still on the road
        aeon_sim::request_cancel(h.world_mut(), id);
        assert!(!live(&mut h, id), "the approach can be turned back");
    }

    #[test]
    fn calling_off_during_a_committed_phase_waits_rather_than_refusing() {
        let mut h = host(3);
        let id = start(&mut h, "aron-ash", "staged-work");
        h.advance_days(16); // past day 14: committed

        aeon_sim::request_cancel(h.world_mut(), id);
        assert!(
            live(&mut h, id),
            "an army mid-assault does not turn round because a message arrived"
        );
        let asked = aeon_sim::access::assignment(h.world_mut(), id)
            .is_some_and(|active| active.cancel_requested);
        assert!(
            asked,
            "but the request is recorded, so the click is not silently lost"
        );
    }

    #[test]
    fn a_deferred_cancellation_is_honoured_when_the_work_ends() {
        let mut h = host(4);
        let id = start(&mut h, "aron-ash", "staged-work");
        h.advance_days(16);
        aeon_sim::request_cancel(h.world_mut(), id);
        // Run past the end of the committed phase.
        h.advance_days(10);
        assert!(!live(&mut h, id), "it ends once it can");
    }

    #[test]
    fn the_point_of_no_return_is_the_start_of_the_first_committed_phase() {
        // What the interface shows before the player commits: a deadline
        // they cannot see is not a decision they get to make.
        let set = content();
        let staged = &set.assignments[&key("staged-work")];
        assert_eq!(staged.point_of_no_return(), Some(14));

        let plain = &set.assignments[&key("sure-court")];
        assert_eq!(
            plain.point_of_no_return(),
            None,
            "an assignment that can always be called off has no such day"
        );
    }

    #[test]
    fn phases_are_walked_in_order_and_do_not_run_off_the_end() {
        let set = content();
        let staged = &set.assignments[&key("staged-work")];
        assert_eq!(staged.stage_at(0), 0);
        assert_eq!(staged.stage_at(13), 0, "the last day of the approach");
        assert_eq!(staged.stage_at(14), 1, "the first day of the work");
        assert_eq!(
            staged.stage_at(999),
            1,
            "a late tick saturates rather than indexing past the work"
        );
    }
}
