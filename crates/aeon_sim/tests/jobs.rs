//! Job-system guarantees: command-driven initiation, delegation rules,
//! skill-shifted graded results, routine retries, script effects, popups
//! with choices, personal risks, the notable-result log, AI agency, and
//! snapshot fidelity.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::model::{JobResultKind, RiskTag};
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::jobs::{CharacterCondition, apply_risk};
use aeon_sim::politics::process_death;
use aeon_sim::{
    CampaignConfig, CharacterId, CommandRejection, JobTarget, JobsIndex, MessageLog, PendingPopups,
    PlayerCommand, PoliticsIndex, SimHost, opinion_between,
};

const FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", name: "Fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", name: "World", kind: "planet", radius_km: 6000 });
define_province(#{ id: "alpha", name: "Alpha", body: "world",
                   latitude_mdeg: 0, longitude_mdeg: 0 });
define_province(#{ id: "beta", name: "Beta", body: "world",
                   latitude_mdeg: 10000, longitude_mdeg: 10000 });

define_house(#{
    id: "ash", name: "House Ash", surname: "Ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha"],
});
define_house(#{
    id: "birch", name: "House Birch", surname: "Birch", tier: "great",
    head: "bela-birch", color: [60, 60, 200], provinces: ["beta"],
});

define_character(#{
    id: "aron-ash", name: "Aron Ash", gender: "male",
    birth_year: 370, organisation: "ash",
    skills: #{ command: 8, diplomacy: 20, intrigue: 4, stewardship: 7 },
});
define_character(#{
    id: "cera-ash", name: "Cera Ash", gender: "female",
    birth_year: 380, organisation: "ash",
    skills: #{ command: 4, diplomacy: 6, intrigue: 5, stewardship: 8 },
});
define_character(#{
    id: "bela-birch", name: "Bela Birch", gender: "female",
    birth_year: 372, organisation: "birch",
    skills: #{ command: 6, diplomacy: 9, intrigue: 8, stewardship: 5 },
});

// Always succeeds for a competent leader; carries a courting effect.
define_job(#{
    id: "sure-court", title: "Sure Courting", summary: "s",
    category: "consequential", duration_days: 10,
    skill: "diplomacy", difficulty: 0, target: "organisation",
    ai_available: false,
    results: #{
        success: #{
            weight: 1000000, log: true,
            log_text: "{leader} charmed {target}.",
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
define_job(#{
    id: "doomed-chore", title: "Doomed Chore", summary: "s",
    category: "routine", duration_days: 5,
    skill: "stewardship", difficulty: 40, ai_available: false,
    results: #{
        success: #{ weight: 1 },
        failure: #{ weight: 1000000 },
    },
});

// Popup with two choices on success.
define_job(#{
    id: "momentous-find", title: "Momentous Find", summary: "s",
    category: "consequential", duration_days: 7,
    skill: "stewardship", difficulty: 0, ai_available: false,
    results: #{
        success: #{
            weight: 1000000, popup: true,
            popup_text: "{leader} uncovered something in the archives.",
            choices: [
                #{ id: "keep-quiet", label: "Keep it quiet" },
                #{ id: "share-it", label: "Share it", effect_fn: "share_find" },
            ],
        },
        failure: #{ weight: 1 },
    },
});
fn share_find(ctx) {
    [#{ kind: "log", message: "The find was shared with the court." }]
}

// A spread of outcomes, each logged distinctly, so a forecast can be
// checked against what actually happens. Difficulty matches Cera's
// stewardship, so effectiveness is zero and the authored weights apply
// unshifted: 100 / 300 / 400 / 200 permille.
define_job(#{
    id: "even-gamble", title: "Even Gamble", summary: "s",
    category: "consequential", duration_days: 5,
    skill: "stewardship", difficulty: 8, ai_available: false,
    risks: ["injury"],
    results: #{
        critical_success: #{ weight: 100, log: true, log_text: "OUTCOME-CRIT" },
        success: #{ weight: 300, log: true, log_text: "OUTCOME-SUCCESS" },
        failure: #{ weight: 400, log: true, log_text: "OUTCOME-FAILURE" },
        disaster: #{ weight: 200, log: true, log_text: "OUTCOME-DISASTER" },
    },
});

// AI-available job so autonomous organisations act.
define_job(#{
    id: "ai-errand", title: "Errand", summary: "s",
    category: "consequential", duration_days: 15,
    skill: "diplomacy", difficulty: 5, ai_available: true,
    results: #{
        success: #{ weight: 700, log: true, log_text: "{leader} ran an errand." },
        failure: #{ weight: 300 },
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
            name: "Jobs Trial".to_owned(),
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
fn jobs_resolve_with_effects_and_logs() {
    let mut h = host(1);
    let aron = char_id(&mut h, "aron-ash");
    let bela = char_id(&mut h, "bela-birch");
    let birch = org_id(&mut h, "birch");

    let before = opinion_between(h.world_mut(), bela, aron);
    h.submit(PlayerCommand::StartJob {
        job: key("sure-court"),
        leader: aron,
        target: JobTarget::Org(birch),
    })
    .unwrap();
    h.advance_days(12);

    let world = h.world_mut();
    assert!(world.resource::<JobsIndex>().jobs.is_empty());
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
fn one_character_leads_one_job_and_delegation_works() {
    let mut h = host(2);
    let aron = char_id(&mut h, "aron-ash");
    let cera = char_id(&mut h, "cera-ash");
    let bela = char_id(&mut h, "bela-birch");
    let birch = org_id(&mut h, "birch");

    h.submit(PlayerCommand::StartJob {
        job: key("sure-court"),
        leader: aron,
        target: JobTarget::Org(birch),
    })
    .unwrap();
    h.advance_days(1);

    // The head is now busy.
    let refused = h.submit(PlayerCommand::StartJob {
        job: key("doomed-chore"),
        leader: aron,
        target: JobTarget::None,
    });
    assert!(matches!(refused, Err(CommandRejection::Job(_))));

    // Delegation to another member works.
    h.submit(PlayerCommand::StartJob {
        job: key("doomed-chore"),
        leader: cera,
        target: JobTarget::None,
    })
    .unwrap();
    h.advance_days(1);
    assert_eq!(h.world_mut().resource::<JobsIndex>().jobs.len(), 2);

    // A character from another organisation cannot lead for the player.
    let refused = h.submit(PlayerCommand::StartJob {
        job: key("sure-court"),
        leader: bela,
        target: JobTarget::Org(birch),
    });
    assert!(matches!(refused, Err(CommandRejection::Job(_))));
}

#[test]
fn routine_failures_retry_automatically() {
    let mut h = host(3);
    let cera = char_id(&mut h, "cera-ash");
    h.submit(PlayerCommand::StartJob {
        job: key("doomed-chore"),
        leader: cera,
        target: JobTarget::None,
    })
    .unwrap();

    // Across several failure cycles the job keeps restarting.
    h.advance_days(23);
    let world = h.world_mut();
    let index = world.resource::<JobsIndex>();
    assert_eq!(index.jobs.len(), 1, "routine job restarted after failure");
}

#[test]
fn popups_open_for_the_player_and_choices_apply() {
    let mut h = host(4);
    let cera = char_id(&mut h, "cera-ash");
    h.submit(PlayerCommand::StartJob {
        job: key("momentous-find"),
        leader: cera,
        target: JobTarget::None,
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
    assert!(matches!(refused, Err(CommandRejection::Job(_))));

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

    // An injured character cannot take a job.
    let refused = h.submit(PlayerCommand::StartJob {
        job: key("doomed-chore"),
        leader: cera,
        target: JobTarget::None,
    });
    assert!(matches!(refused, Err(CommandRejection::Job(_))));

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
fn dead_leaders_abandon_their_jobs() {
    let mut h = host(6);
    let cera = char_id(&mut h, "cera-ash");
    let birch = org_id(&mut h, "birch");
    h.submit(PlayerCommand::StartJob {
        job: key("sure-court"),
        leader: cera,
        target: JobTarget::Org(birch),
    })
    .unwrap();
    h.advance_days(2);

    let date = h.date();
    process_death(h.world_mut(), cera, date);
    h.advance_days(10);

    let world = h.world_mut();
    assert!(world.resource::<JobsIndex>().jobs.is_empty());
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
        "AI-run jobs should reach the shared log: {:?}",
        log.entries
    );
}

#[test]
fn job_world_is_deterministic_and_survives_snapshots() {
    let run = |seed: u64| {
        let mut h = host(seed);
        let aron = char_id(&mut h, "aron-ash");
        let birch = org_id(&mut h, "birch");
        h.submit(PlayerCommand::StartJob {
            job: key("sure-court"),
            leader: aron,
            target: JobTarget::Org(birch),
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
        h.submit(PlayerCommand::StartJob {
            job: key("even-gamble"),
            leader: cera,
            target: JobTarget::None,
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
        JobTarget::None,
    )
    .expect("job is defined");

    // Difficulty equals the leader's skill, so the authored weights stand.
    assert_eq!(forecast.effectiveness, 0);
    assert_eq!(forecast.skill_value, 8);
    assert_eq!(forecast.difficulty, 8);
    let chance = |kind: JobResultKind| -> u32 {
        forecast
            .results
            .iter()
            .find(|r| r.kind == kind)
            .map(|r| r.chance)
            .expect("kind present")
    };
    assert_eq!(chance(JobResultKind::CriticalSuccess), 100);
    assert_eq!(chance(JobResultKind::Success), 300);
    assert_eq!(chance(JobResultKind::Failure), 400);
    assert_eq!(chance(JobResultKind::Disaster), 200);
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
    for (index, kind) in JobResultKind::ALL.iter().enumerate() {
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
        JobTarget::None,
    )
    .expect("job is defined");

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
        "a civil job has no conditional field contest"
    );
}

#[test]
fn a_forecast_explains_why_a_job_cannot_be_started() {
    let mut h = host(4);
    let cera = char_id(&mut h, "cera-ash");
    let ash = org_id(&mut h, "ash");
    // Put Cera to work, then forecast a second job for her.
    h.submit(PlayerCommand::StartJob {
        job: key("even-gamble"),
        leader: cera,
        target: JobTarget::None,
    })
    .unwrap();
    h.advance_days(1);

    let forecast = aeon_sim::forecast::forecast(
        h.world_mut(),
        ash,
        &key("even-gamble"),
        cera,
        JobTarget::None,
    )
    .expect("job is defined");
    assert!(!forecast.startable(), "a busy leader cannot start a job");
    assert_eq!(
        forecast.blocked,
        Some(aeon_sim::jobs::JobRejection::LeaderBusy)
    );
    // The forecast still describes the job, so the player can plan ahead.
    assert_eq!(forecast.duration_days, 5);
    assert_eq!(forecast.success_chance(), 400);
}

// ---------------------------------------------------------------------------
// Leader availability
// ---------------------------------------------------------------------------

#[test]
fn availability_names_the_specific_reason_a_character_cannot_lead() {
    use aeon_sim::{Assignment, LeaderAvailability, leader_availability};

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

    // Busy names the job and when it ends, not merely "unavailable".
    h.submit(PlayerCommand::StartJob {
        job: key("sure-court"),
        leader: aron,
        target: JobTarget::Org(birch),
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
        LeaderAvailability::Assigned(Assignment::General { army: id, .. }) => {
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
    use aeon_sim::{Assignment, JobRejection, LeaderAvailability};

    let commanding = LeaderAvailability::Assigned(Assignment::General {
        army: aeon_sim::ArmyId::from_raw(7).unwrap(),
        name: "First Levy".to_owned(),
    });
    let other_army = aeon_sim::ArmyId::from_raw(9).unwrap();
    let own_army = aeon_sim::ArmyId::from_raw(7).unwrap();

    // A general may still court, scheme, and administer.
    assert_eq!(commanding.blocks_job(JobTarget::None), None);
    assert_eq!(
        commanding.blocks_job(JobTarget::Org(aeon_sim::OrgId::from_raw(3).unwrap())),
        None
    );

    // And may order the force they actually command.
    assert_eq!(commanding.blocks_job(JobTarget::OwnArmy(own_army)), None);

    // But not somebody else's.
    assert_eq!(
        commanding.blocks_job(JobTarget::OwnArmy(other_army)),
        Some(JobRejection::AlreadyAssigned)
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
    h.submit(PlayerCommand::StartJob {
        job: key("sure-court"),
        leader: aron,
        target: JobTarget::Org(birch),
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
        for target in [JobTarget::None, JobTarget::Org(birch)] {
            let reported = leader_availability(h.world_mut(), ash, character, date)
                .blocks_job(target)
                .is_none();
            let accepted = aeon_sim::command::validate_command(
                h.world_mut(),
                &PlayerCommand::StartJob {
                    job: key("sure-court"),
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
