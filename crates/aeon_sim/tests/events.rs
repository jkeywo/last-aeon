//! Contextual events and the political obligation ledger: eligibility,
//! determinism, cooldowns, choices, and how obligations are settled.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::events::EventState;
use aeon_sim::map::MapIndex;
use aeon_sim::obligations::{ObligationKind, ObligationStatus, Obligations, create, settle};
use aeon_sim::order::adjust_order;
use aeon_sim::{
    CampaignConfig, MessageLog, OrgId, PendingPopups, PlayerCommand, PoliticsIndex, SimHost,
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
// An outlying Ash holding where nobody stands, so damage there persists.
define_province(#{ id: "gamma", body: "world",
                   latitude_mdeg: -20000, longitude_mdeg: -20000 });

define_house(#{
    id: "ash", tier: "great",
    head: "aron-ash", color: [200, 60, 60], provinces: ["alpha", "gamma"],
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

// A standing debt, so the ledger is live from the first day.
define_obligation(#{
    id: "birch-owes-ash",
    kind: "favour", debtor: "birch", creditor: "ash", weight: 30,
    origin: "Ash grain carried Birch through a bad winter",
});
define_obligation(#{
    id: "ash-resented-by-birch",
    kind: "grievance", debtor: "ash", creditor: "birch", weight: 10,
    origin: "an old border quarrel",
});

// Assignments an autonomous house can reach for, each declaring the pressure it
// answers, so agency is driven by content rather than by hardcoded keys.
define_assignment(#{
    id: "settle-the-shire", 
    ai_intent: "order", category: "consequential", duration_days: 20,
    skill: "diplomacy", difficulty: 5, ai_available: true,
    results: #{
        success: #{ weight: 900, effect_fn: "settled" },
        failure: #{ weight: 100 },
    },
});
fn settled(ctx) {
    [#{ kind: "order", scope: "all-held", amount: 40 }]
}
define_assignment(#{
    id: "collect-what-is-owed", 
    ai_intent: "obligation", category: "consequential", duration_days: 20,
    skill: "diplomacy", difficulty: 5, target: "organisation", ai_available: true,
    results: #{
        success: #{ weight: 800 },
        failure: #{ weight: 200 },
    },
});
define_assignment(#{
    id: "ordinary-business", 
    category: "consequential", duration_days: 30,
    skill: "stewardship", difficulty: 5, ai_available: true,
    results: #{
        success: #{ weight: 800 },
        failure: #{ weight: 200 },
    },
});

// Fires only on badly disordered player ground, and asks a question.
define_event(#{
    id: "unrest-choice",
    family: "province",
    weight: 100,
    cooldown_days: 90,
    weighty: true,
    requires: #{ player_only: true, max_order: 400 },
    choices: [
        #{ id: "firm-hand", effect_fn: "unrest_firm" },
        #{ id: "concessions", effect_fn: "unrest_soft" },
    ],
});
fn unrest_firm(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 200 }]
}
fn unrest_soft(ctx) {
    [#{ kind: "order", scope: "target-province", amount: 100 }]
}

// A minor event that can fire anywhere, to exercise the log-only path.
define_event(#{
    id: "quiet-talk",
    family: "political",
    weight: 100,
    cooldown_days: 30,
    weighty: false,
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
            name: "Event Trial".to_owned(),
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

/// Whether an event has fired at all this campaign.
///
/// Was a sentinel word planted in the event's log line and grepped for
/// out of the message log. The simulation keeps the record itself, so
/// the test asks it rather than reading prose that now lives in the
/// string table.
fn event_fired(h: &mut SimHost, event: &str) -> bool {
    let key = key(event);
    h.world_mut()
        .resource::<aeon_sim::EventState>()
        .history
        .iter()
        .any(|occurrence| occurrence.event == key)
}

// ---------------------------------------------------------------------------
// Obligations
// ---------------------------------------------------------------------------

#[test]
fn authored_obligations_are_seeded_at_the_start() {
    let mut h = host(1);
    let ash = org(&mut h, "ash");
    let birch = org(&mut h, "birch");
    let ledger = h.world_mut().resource::<Obligations>().clone();

    assert_eq!(ledger.open().count(), 2, "both seeds are live");
    assert_eq!(
        ledger.standing(birch, ash),
        30,
        "Birch owes Ash a favour worth 30"
    );
    assert_eq!(
        ledger.standing(ash, birch),
        -10,
        "and Ash carries a grievance the other way"
    );
    let origin = ledger
        .owed(birch, ash)
        .next()
        .map(|entry| entry.origin.clone())
        .unwrap_or_default();
    assert!(
        origin.contains("bad winter"),
        "an obligation records where it came from: {origin}"
    );
}

#[test]
fn obligations_are_separate_from_opinion() {
    // A house can be bound by a favour while thinking little of its
    // creditor: the ledger is not a flavour of opinion.
    let mut h = host(2);
    let ash = org(&mut h, "ash");
    let birch = org(&mut h, "birch");
    let bela = h.world_mut().resource::<PoliticsIndex>().character_keys[&key("bela-birch")];
    let aron = h.world_mut().resource::<PoliticsIndex>().character_keys[&key("aron-ash")];

    let opinion_before = aeon_sim::opinion_between(h.world_mut(), bela, aron);
    let standing_before = h.world_mut().resource::<Obligations>().standing(birch, ash);

    // Settling the debt changes the ledger and leaves opinion untouched.
    assert!(settle(
        h.world_mut(),
        ObligationKind::Favour,
        birch,
        ash,
        ObligationStatus::Fulfilled
    ));
    let opinion_after = aeon_sim::opinion_between(h.world_mut(), bela, aron);
    let standing_after = h.world_mut().resource::<Obligations>().standing(birch, ash);

    assert_eq!(opinion_before, opinion_after, "opinion is untouched");
    assert!(
        standing_after < standing_before,
        "but the debt is discharged"
    );
}

#[test]
fn obligations_expire_on_their_day_and_stay_on_the_record() {
    let mut h = host(3);
    let ash = org(&mut h, "ash");
    let birch = org(&mut h, "birch");
    create(
        h.world_mut(),
        ObligationKind::Promise,
        ash,
        birch,
        "a promise with a deadline",
        20,
        Some(60),
    );
    assert_eq!(
        h.world_mut().resource::<Obligations>().standing(ash, birch),
        10
    );

    h.advance_days(61);
    let ledger = h.world_mut().resource::<Obligations>().clone();
    assert_eq!(
        ledger.standing(ash, birch),
        -10,
        "the lapsed promise stops counting, leaving the grievance"
    );
    assert!(
        ledger
            .entries
            .iter()
            .any(|entry| entry.status == ObligationStatus::Expired),
        "the lapse is recorded rather than erased"
    );
    // The wording lives in the string table; what this test is about is
    // that the lapse reaches the player at all, on the channel it belongs
    // on, attributed to the house that let it go.
    assert!(
        h.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.channel == aeon_sim::LogChannel::Politics
                && entry.org == Some(ash)
                && !entry.text.is_empty()),
        "a lapsed promise is public"
    );
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[test]
fn events_only_fire_where_their_conditions_hold() {
    let mut h = host(4);
    // Every province is settled, so the unrest event can never be drawn.
    h.advance_days(700);
    assert!(
        !event_fired(&mut h, "unrest-choice"),
        "an unrest event must not fire on settled ground"
    );

    // Drive an outlying holding down — far enough to qualify, not so far
    // that it revolts — and it becomes eligible.
    let gamma = h.world_mut().resource::<MapIndex>().province_keys[&key("gamma")];
    adjust_order(h.world_mut(), gamma, -450);
    for _ in 0..2000 {
        h.advance_days(1);
        if event_fired(&mut h, "unrest-choice") {
            return;
        }
    }
    panic!("a disordered province should eventually draw its event");
}

#[test]
fn a_weighty_event_asks_and_its_answer_takes_effect() {
    let mut h = host(5);
    let alpha = h.world_mut().resource::<MapIndex>().province_keys[&key("gamma")];
    adjust_order(h.world_mut(), alpha, -450);

    // Run until the event asks.
    let mut popup = None;
    for _ in 0..2000 {
        h.advance_days(1);
        let pending = h.world_mut().resource::<PendingPopups>().clone();
        if let Some(first) = pending.popups.first() {
            popup = Some(first.clone());
            break;
        }
    }
    let popup = popup.expect("the unrest event should have asked eventually");
    assert_eq!(popup.assignment, key("unrest-choice"));
    assert_eq!(popup.choices.len(), 2, "both answers are offered");

    let before = aeon_sim::order::province_order(h.world_mut(), alpha).order;
    h.submit(PlayerCommand::AnswerPopup {
        popup: popup.id,
        choice: key("firm-hand"),
    })
    .unwrap();
    h.advance_days(2);

    let after = aeon_sim::order::province_order(h.world_mut(), alpha).order;
    assert!(
        after > before,
        "the chosen answer should have restored order: {before} -> {after}"
    );
    assert!(
        h.world_mut().resource::<PendingPopups>().popups.is_empty(),
        "the popup is cleared once answered"
    );

    // The choice is remembered, so the history explains what happened.
    let history = h.world_mut().resource::<EventState>().history.clone();
    let answered = history
        .iter()
        .find(|occurrence| occurrence.event == key("unrest-choice"));
    assert_eq!(
        answered.and_then(|occurrence| occurrence.choice.clone()),
        Some(key("firm-hand")),
        "the answer is recorded against the event"
    );
}

#[test]
fn events_respect_their_cooldown() {
    let mut h = host(6);
    h.advance_days(1200);
    let history = h.world_mut().resource::<EventState>().history.clone();
    let talk: Vec<_> = history
        .iter()
        .filter(|occurrence| occurrence.event == key("quiet-talk"))
        .collect();
    assert!(!talk.is_empty(), "the minor event should have fired");

    // No subject may draw the same event twice inside its cooldown.
    for pair in talk.windows(2) {
        if pair[0].subject == pair[1].subject {
            let gap = pair[0].date.days_until(pair[1].date);
            assert!(
                gap >= 30,
                "the same subject repeated inside the cooldown: {gap} days"
            );
        }
    }
}

#[test]
fn events_replay_identically_and_survive_snapshots() {
    let run = |seed: u64| {
        let mut h = host(seed);
        let gamma = h.world_mut().resource::<MapIndex>().province_keys[&key("gamma")];
        adjust_order(h.world_mut(), gamma, -450);
        h.advance_days(900);
        h
    };
    let mut a = run(9);
    let b = run(9);
    assert_eq!(
        a.state_hash(),
        b.state_hash(),
        "the same seed must produce the same events"
    );

    let snapshot = a.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content()).unwrap();
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "event history and cooldowns must round-trip"
    );

    a.advance_days(300);
    restored.advance_days(300);
    assert_eq!(
        restored.state_hash(),
        a.state_hash(),
        "and keep drawing the same events afterwards"
    );
}

// ---------------------------------------------------------------------------
// Reactive agency
// ---------------------------------------------------------------------------

#[test]
fn houses_score_the_pressures_they_are_actually_under() {
    use aeon_sim::agency::score_intents;

    let mut h = host(11);
    let birch = org(&mut h, "birch");

    // With nothing wrong, Birch has only ordinary business in mind.
    let calm = score_intents(h.world_mut(), birch);
    let calm_top = calm.first().map(|intent| intent.score).unwrap_or(0);

    // Put its holding in disorder, and that pressure outranks everything.
    let beta = h.world_mut().resource::<MapIndex>().province_keys[&key("beta")];
    adjust_order(h.world_mut(), beta, -600);
    let pressed = score_intents(h.world_mut(), birch);
    let top = pressed
        .first()
        .expect("a house under pressure wants something");

    assert!(
        top.score > calm_top,
        "disorder should outrank ordinary business: {} vs {calm_top}",
        top.score
    );
    assert_eq!(
        top.subject,
        Some(aeon_sim::LogSubject::Province(beta)),
        "the intent should be about the province actually under pressure"
    );
}

#[test]
fn agency_notices_an_obligation_it_can_collect() {
    use aeon_sim::agency::score_intents;

    let mut h = host(12);
    let ash = org(&mut h, "ash");
    let birch = org(&mut h, "birch");
    // Ash is owed a favour by Birch from the fixture's seeded ledger.
    let intents = score_intents(h.world_mut(), ash);
    let collecting = intents
        .iter()
        .find(|intent| intent.target == aeon_sim::AssignmentTarget::Org(birch));
    assert!(
        collecting.is_some_and(|intent| intent.reason.contains("owes us")),
        "a house should notice a favour it can call in: {:?}",
        intents.iter().map(|i| &i.reason).collect::<Vec<_>>()
    );
}

#[test]
fn autonomous_houses_act_and_say_why() {
    let mut h = host(13);
    // Put Birch's holding into disorder so it has something to answer.
    let beta = h.world_mut().resource::<MapIndex>().province_keys[&key("beta")];
    adjust_order(h.world_mut(), beta, -600);
    h.advance_days(400);

    let birch = org(&mut h, "birch");
    let explained = h
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .any(|entry| entry.org == Some(birch) && entry.text.contains("began '"));
    let entries: Vec<String> = h
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .map(|entry| format!("{:?} {}", entry.org, entry.text))
        .collect();
    assert!(
        explained,
        "a house acting on a pressure should record why it did; log was {entries:#?}"
    );
}
