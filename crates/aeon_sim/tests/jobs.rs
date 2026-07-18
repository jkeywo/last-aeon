//! Job-system guarantees: command-driven initiation, delegation rules,
//! skill-shifted graded results, routine retries, script effects, popups
//! with choices, personal risks, the notable-result log, AI agency, and
//! snapshot fidelity.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::model::RiskTag;
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
