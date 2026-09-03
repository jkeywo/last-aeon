//! Content-pipeline guarantees: loading, validation, sandboxing,
//! determinism, and the effect boundary.

use aeon_data::model::{AssignmentCategory, BodyKind, OutcomeKind};
use aeon_data::{ContentSource, ScriptEffect, ScriptHost, Severity, load_content};

fn source(path: &str, text: &str) -> ContentSource {
    ContentSource {
        path: path.to_owned(),
        source: text.to_owned(),
    }
}

const GOOD_JOBS: &str = r#"
define_assignment(#{
    id: "manage-estates",
    category: "routine",
    duration_days: 30,
    skill: "stewardship",
    difficulty: 6,
    results: #{
        success: #{ weight: 850 },
        failure: #{ weight: 150 },
    },
});

define_assignment(#{
    id: "court-a-rival",
    category: "consequential",
    duration_days: 45,
    skill: "diplomacy",
    difficulty: 10,
    target: "organisation",
    risks: ["scandal"],
    results: #{
        critical_success: #{
            weight: 100, popup: true, log: true,
        },
        success: #{ weight: 500, log: true },
        failure: #{ weight: 300 },
        disaster: #{
            weight: 100, popup: true, log: true,
            effect_fn: "courting_disaster",
        },
    },
});

fn courting_disaster(ctx) {
    [#{ kind: "log", message_key: "assignment.court-a-rival.disaster.log" }]
}
"#;

const GOOD_SYSTEM: &str = r#"
define_body(#{
    id: "the-world", kind: "planet", radius_km: 6400,
});
define_body(#{
    id: "the-moon", kind: "moon", radius_km: 1700,
    parent: "the-world", orbit_radius_mm: 384, orbit_days: 27,
});
define_province(#{
    id: "first-landing", body: "the-world",
    latitude_mdeg: 12500, longitude_mdeg: -30250,
});
"#;

const GOOD_SITUATION: &str = r#"
define_assignment(#{
    id: "negotiate-war",
    category: "routine",
    duration_days: 20,
    skill: "diplomacy",
    difficulty: 8,
    target: "war",
    results: #{ success: #{ weight: 700 }, failure: #{ weight: 300 } },
});

define_situation(#{
    id: "formal-war",
    source: "scenario",
    bindings: #{ war: "war", side: "organisation" },
    trigger_fn: "war_instances",
    projection_fn: "war_projection",
    priority: 50,
    log_activation: true,
    stages: ["open", "negotiating"],
    actions: [#{ id: "negotiate", assignment: "negotiate-war" }],
    outcomes: [
        #{ id: "peace", when_fn: "ended_in_peace" },
        #{ id: "ended", fallback: true },
    ],
});

define_scenario(#{
    id: "situation-test",
    start_year: 411, start_month: 1, start_day: 1,
    situations: ["formal-war"],
});

fn war_instances(ctx) { [#{ war: 17, side: 23 }] }
fn war_projection(ctx) { #{ stage: "open" } }
fn ended_in_peace(ctx) { false }
"#;

#[test]
fn loads_a_valid_content_set() {
    let (set, report) = load_content(
        &[
            source("core/assignments.rhai", GOOD_JOBS),
            source("system/bodies.rhai", GOOD_SYSTEM),
        ],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("valid content loads");

    assert_eq!(set.assignments.len(), 2);
    let estates = set.assignments.values().next().unwrap();
    assert_eq!(estates.key.as_str(), "court-a-rival");
    assert_eq!(set.bodies.len(), 2);
    assert_eq!(set.provinces.len(), 1);

    let rival = &set.assignments[&aeon_data::ContentKey::new("court-a-rival").unwrap()];
    assert_eq!(rival.category, AssignmentCategory::Consequential);
    assert_eq!(rival.results.len(), 4);
    assert!(rival.results[&OutcomeKind::Disaster].effect_fn.is_some());

    let moon = &set.bodies[&aeon_data::ContentKey::new("the-moon").unwrap()];
    assert_eq!(moon.kind, BodyKind::Moon);
    assert_eq!(moon.orbit_days, 27);
}

#[test]
fn loading_is_deterministic_across_runs_and_input_order() {
    let forward = [
        source("core/assignments.rhai", GOOD_JOBS),
        source("system/bodies.rhai", GOOD_SYSTEM),
    ];
    let reversed = [
        source("system/bodies.rhai", GOOD_SYSTEM),
        source("core/assignments.rhai", GOOD_JOBS),
    ];
    let (a, _) = load_content(&forward, &aeon_data::StringTable::blank());
    let (b, _) = load_content(&forward, &aeon_data::StringTable::blank());
    let (c, _) = load_content(&reversed, &aeon_data::StringTable::blank());
    let (a, b, c) = (a.unwrap(), b.unwrap(), c.unwrap());
    assert!(a.data_eq(&b));
    assert!(a.data_eq(&c));
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.content_hash, c.content_hash);
}

#[test]
fn effect_functions_run_against_read_context() {
    let (set, _) = load_content(
        &[
            source("core/assignments.rhai", GOOD_JOBS),
            source("system/bodies.rhai", GOOD_SYSTEM),
        ],
        &aeon_data::StringTable::blank(),
    );
    let set = set.unwrap();
    let host = ScriptHost::new();

    let disaster = set.assignments[&aeon_data::ContentKey::new("court-a-rival").unwrap()].results
        [&OutcomeKind::Disaster]
        .effect_fn
        .clone()
        .unwrap();

    // The fields of the documented context schema: source, result,
    // leader, target. The authored function reads two of them.
    let mut context = rhai::Map::new();
    context.insert("source".into(), "court-a-rival".into());
    context.insert("result".into(), "Disaster".into());
    context.insert("leader".into(), "Aron Veyrin".into());
    context.insert("target".into(), "Lady Calder".into());
    let effects = host.call_effect_fn(&set, &disaster, context).unwrap();
    assert_eq!(
        effects,
        vec![ScriptEffect::Log {
            message_key: "assignment.court-a-rival.disaster.log".to_owned()
        }]
    );
}

#[test]
fn situations_load_with_typed_bindings_actions_outcomes_and_attachments() {
    let (set, report) = load_content(
        &[source("core/situations.rhai", GOOD_SITUATION)],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("valid Situation content loads");
    let key = aeon_data::ContentKey::new("formal-war").unwrap();
    let situation = &set.situations[&key];
    assert_eq!(situation.priority, 50);
    assert!(situation.log_activation);
    assert_eq!(situation.stages.len(), 2);
    assert_eq!(situation.actions[0].assignment.as_str(), "negotiate-war");
    assert!(situation.outcomes.last().unwrap().predicate_fn.is_none());
    assert_eq!(set.scenario.as_ref().unwrap().situations, vec![key.clone()]);

    let returned = ScriptHost::new()
        .call_dynamic_fn(&set, &situation.trigger_fn, rhai::Map::new())
        .expect("raw Situation function returns through the shared host");
    assert_eq!(returned.try_cast::<rhai::Array>().unwrap().len(), 1);

    let keys = aeon_data::text_keys(&set);
    assert!(keys.contains("situation.formal-war.title"));
    assert!(keys.contains("situation.formal-war.stage.open.summary"));
    assert!(keys.contains("situation.formal-war.action.negotiate.label"));
    assert!(keys.contains("situation.formal-war.resolution.peace.text"));
}

#[test]
fn situation_validation_rejects_bad_functions_actions_audiences_and_fallbacks() {
    let bad = r#"
define_situation(#{
    id: "broken-situation",
    source: "scenario",
    bindings: #{ war: "war" },
    trigger_fn: "missing_trigger",
    projection_fn: "missing_projection",
    audience: ["war"],
    stages: ["open", "open"],
    actions: [#{ id: "act", assignment: "missing-assignment" }],
    outcomes: [
        #{ id: "fallback-first", fallback: true },
        #{ id: "conditional", when_fn: "missing_outcome" },
    ],
});
define_scenario(#{
    id: "bad-situation-test",
    start_year: 411, start_month: 1, start_day: 1,
    situations: ["broken-situation"],
});
"#;
    let (set, report) = load_content(
        &[source("bad/situations.rhai", bad)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    let messages: Vec<&str> = report
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("trigger_fn"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("projection_fn"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("undefined assignment"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("audiences must bind"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("fallback outcome must be last"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("duplicate Situation stage"))
    );
}

#[test]
fn situation_attachments_validate_source_kind_and_definition() {
    let bad = r#"
define_situation(#{
    id: "title-only",
    source: "title",
    trigger_fn: "none",
    projection_fn: "view",
    stages: ["open"],
    outcomes: [#{ id: "ended", fallback: true }],
});
define_scenario(#{
    id: "bad-attachment",
    start_year: 411, start_month: 1, start_day: 1,
    situations: ["title-only", "not-defined"],
});
fn none(ctx) { [] }
fn view(ctx) { #{ stage: "open" } }
"#;
    let (set, report) = load_content(
        &[source("bad/attachments.rhai", bad)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(report.findings.iter().any(|finding| {
        finding
            .message
            .contains("expects a Title source, not Scenario")
    }));
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.message.contains("'not-defined' is not defined"))
    );
}

#[test]
fn situation_validation_rejects_sources_without_attachment_support() {
    let bad = r#"
define_situation(#{
    id: "war-sourced",
    source: "war",
    trigger_fn: "none",
    projection_fn: "view",
    stages: ["open"],
    outcomes: [#{ id: "ended", fallback: true }],
});
fn none(ctx) { [] }
fn view(ctx) { #{ stage: "open" } }
"#;
    let (set, report) = load_content(
        &[source("bad/unsupported-source.rhai", bad)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(report.findings.iter().any(|finding| {
        finding
            .message
            .contains("source kind 'war' cannot be attached")
            && finding
                .message
                .contains("use source: \"scenario\" plus a typed binding")
    }));
}

#[test]
fn situation_outcome_effects_require_a_declared_organisation_owner_binding() {
    // effects_fn with no owner_binding: the consequence would address nobody.
    let orphan_effects = r#"
define_situation(#{
    id: "orphan-effects",
    source: "scenario",
    trigger_fn: "none",
    projection_fn: "view",
    stages: ["open"],
    outcomes: [#{ id: "ended", fallback: true, effects_fn: "consequence" }],
});
fn none(ctx) { [] }
fn view(ctx) { #{ stage: "open" } }
fn consequence(ctx) { [#{ kind: "resources", influence: -10 }] }
"#;
    let (set, report) = load_content(
        &[source("bad/orphan-effects.rhai", orphan_effects)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(report.findings.iter().any(|finding| {
        finding
            .message
            .contains("outcomes with effects_fn require an owner_binding")
    }));

    // An undeclared or wrongly typed owner binding, and a missing effects
    // function, are each named errors.
    let bad_bindings = r#"
define_situation(#{
    id: "bad-owner",
    source: "scenario",
    bindings: #{ battlefield: "province" },
    owner_binding: "nobody",
    trigger_fn: "none",
    projection_fn: "view",
    stages: ["open"],
    outcomes: [#{ id: "ended", fallback: true, effects_fn: "missing_effects" }],
});
define_situation(#{
    id: "wrong-kind-owner",
    source: "scenario",
    bindings: #{ battlefield: "province" },
    owner_binding: "battlefield",
    trigger_fn: "none",
    projection_fn: "view",
    stages: ["open"],
    outcomes: [#{ id: "ended", fallback: true }],
});
fn none(ctx) { [] }
fn view(ctx) { #{ stage: "open" } }
"#;
    let (set, report) = load_content(
        &[source("bad/owner-bindings.rhai", bad_bindings)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    let messages: Vec<&str> = report
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("owner_binding 'nobody' is not a declared binding"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("it must bind an organisation"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("effects_fn 'missing_effects' is not defined"))
    );
}

#[test]
fn situation_subject_bindings_and_responses_are_validated() {
    // A subject binding must be declared and typed as a character, and
    // response ids must be unique — a mistake is a loud load error.
    let bad = r#"
define_situation(#{
    id: "bad-subject",
    source: "scenario",
    bindings: #{ battlefield: "province" },
    subject_binding: "nobody",
    trigger_fn: "none",
    projection_fn: "view",
    stages: ["open"],
    outcomes: [#{ id: "ended", fallback: true }],
});
define_situation(#{
    id: "wrong-kind-subject",
    source: "scenario",
    bindings: #{ battlefield: "province" },
    subject_binding: "battlefield",
    trigger_fn: "none",
    projection_fn: "view",
    stages: ["open"],
    outcomes: [#{ id: "ended", fallback: true }],
});
define_situation(#{
    id: "twice-answered",
    source: "scenario",
    responses: [#{ id: "promise" }, #{ id: "promise" }],
    trigger_fn: "none",
    projection_fn: "view",
    stages: ["open"],
    outcomes: [#{ id: "ended", fallback: true }],
});
fn none(ctx) { [] }
fn view(ctx) { #{ stage: "open" } }
"#;
    let (set, report) = load_content(
        &[source("bad/subject-bindings.rhai", bad)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    let messages: Vec<&str> = report
        .findings
        .iter()
        .map(|finding| finding.message.as_str())
        .collect();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("subject_binding 'nobody' is not a declared binding"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("it must bind a character"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("duplicate Situation response id 'promise'"))
    );
}

#[test]
fn kessarins_demand_carries_responses_a_subject_binding_and_tiered_effects() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
    assert!(
        !report.has_errors(),
        "content findings: {:?}",
        report.findings
    );
    let set = set.expect("repository content loads");
    let demand = &set.situations[&aeon_data::ContentKey::new("kessarin-order").unwrap()];

    // The demand acts for the house and its consequences fall on the bound
    // requester; its pure choices are the two authored responses.
    assert_eq!(demand.owner_binding.as_deref(), Some("house"));
    assert_eq!(demand.subject_binding.as_deref(), Some("requester"));
    let responses: Vec<&str> = demand
        .responses
        .iter()
        .map(|response| response.key.as_str())
        .collect();
    assert_eq!(responses, ["promise", "refuse"]);
    assert!(
        demand
            .responses
            .iter()
            .all(|response| !response.label.is_empty()),
        "response labels are table-decided and filled"
    );
    assert!(demand.announcement.is_some());
    assert!(demand.guidance_objective.is_some());
    assert!(demand.guidance_how.is_some());
    assert!(demand.guidance_why.is_some());

    // Exactly the four relationship tiers carry effects; passing the demand
    // on carries none.
    let effects: Vec<&str> = demand
        .outcomes
        .iter()
        .filter(|outcome| outcome.effects_fn.is_some())
        .map(|outcome| outcome.key.as_str())
        .collect();
    assert_eq!(effects, ["achieved", "refused", "broken", "ignored"]);

    // The derived key mirror covers the new response rows, so the orphan
    // and missing-row audits keep covering them.
    let keys = aeon_data::text_keys(&set);
    for expected in [
        "situation.kessarin-order.response.promise.label",
        "situation.kessarin-order.response.refuse.label",
        "situation.kessarin-order.announcement",
        "situation.kessarin-order.resolution.passed-on.text",
        "situation.kessarin-order.guidance.objective",
    ] {
        assert!(keys.contains(expected), "missing derived key {expected}");
    }
}

#[test]
fn aleyns_demand_carries_responses_a_subject_binding_and_tiered_effects() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
    assert!(
        !report.has_errors(),
        "content findings: {:?}",
        report.findings
    );
    let set = set.expect("repository content loads");
    let demand = &set.situations[&aeon_data::ContentKey::new("aleyn-levies").unwrap()];

    // The demand acts for the house and its consequences fall on the bound
    // requester; its pure choices are the two authored responses, and its
    // one action is the honest single route to fielded strength.
    assert_eq!(demand.owner_binding.as_deref(), Some("house"));
    assert_eq!(demand.subject_binding.as_deref(), Some("requester"));
    let responses: Vec<&str> = demand
        .responses
        .iter()
        .map(|response| response.key.as_str())
        .collect();
    assert_eq!(responses, ["promise", "refuse"]);
    assert!(
        demand
            .responses
            .iter()
            .all(|response| !response.label.is_empty()),
        "response labels are table-decided and filled"
    );
    let actions: Vec<&str> = demand
        .actions
        .iter()
        .map(|action| action.key.as_str())
        .collect();
    assert_eq!(actions, ["muster"]);
    assert!(demand.announcement.is_some());
    assert!(demand.guidance_objective.is_some());
    assert!(demand.guidance_how.is_some());
    assert!(demand.guidance_why.is_some());

    // Exactly the four relationship tiers carry effects; passing the demand
    // on carries none.
    let effects: Vec<&str> = demand
        .outcomes
        .iter()
        .filter(|outcome| outcome.effects_fn.is_some())
        .map(|outcome| outcome.key.as_str())
        .collect();
    assert_eq!(effects, ["achieved", "refused", "broken", "ignored"]);

    // The derived key mirror covers the new rows, so the orphan and
    // missing-row audits keep covering them.
    let keys = aeon_data::text_keys(&set);
    for expected in [
        "situation.aleyn-levies.response.promise.label",
        "situation.aleyn-levies.response.refuse.label",
        "situation.aleyn-levies.action.muster.label",
        "situation.aleyn-levies.announcement",
        "situation.aleyn-levies.resolution.passed-on.text",
        "situation.aleyn-levies.guidance.objective",
    ] {
        assert!(keys.contains(expected), "missing derived key {expected}");
    }
}

#[test]
fn torvalds_demand_carries_responses_a_liege_head_binding_and_tiered_effects() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
    assert!(
        !report.has_errors(),
        "content findings: {:?}",
        report.findings
    );
    let set = set.expect("repository content loads");
    let demand = &set.situations[&aeon_data::ContentKey::new("torvald-standing").unwrap()];

    // The demand acts for the house and its consequences fall on the bound
    // requester, and it additionally binds the exact liege head whose
    // regard the goal is judged against — a changed liege or head is a
    // different structural instance.
    assert_eq!(demand.owner_binding.as_deref(), Some("house"));
    assert_eq!(demand.subject_binding.as_deref(), Some("requester"));
    let bindings: Vec<(&str, aeon_data::model::SituationSubjectKind)> = demand
        .bindings
        .iter()
        .map(|(name, kind)| (name.as_str(), *kind))
        .collect();
    assert_eq!(
        bindings,
        [
            (
                "house",
                aeon_data::model::SituationSubjectKind::Organisation
            ),
            (
                "liege-head",
                aeon_data::model::SituationSubjectKind::Character
            ),
            (
                "requester",
                aeon_data::model::SituationSubjectKind::Character
            ),
        ]
    );
    let responses: Vec<&str> = demand
        .responses
        .iter()
        .map(|response| response.key.as_str())
        .collect();
    assert_eq!(responses, ["promise", "refuse"]);
    assert!(
        demand
            .responses
            .iter()
            .all(|response| !response.label.is_empty()),
        "response labels are table-decided and filled"
    );
    let actions: Vec<&str> = demand
        .actions
        .iter()
        .map(|action| action.key.as_str())
        .collect();
    assert_eq!(actions, ["court"]);
    assert!(demand.announcement.is_some());
    assert!(demand.guidance_objective.is_some());
    assert!(demand.guidance_how.is_some());
    assert!(demand.guidance_why.is_some());

    // Exactly the four relationship tiers carry effects; passing the demand
    // on carries none.
    let effects: Vec<&str> = demand
        .outcomes
        .iter()
        .filter(|outcome| outcome.effects_fn.is_some())
        .map(|outcome| outcome.key.as_str())
        .collect();
    assert_eq!(effects, ["achieved", "refused", "broken", "ignored"]);

    // The derived key mirror covers the new rows, so the orphan and
    // missing-row audits keep covering them.
    let keys = aeon_data::text_keys(&set);
    for expected in [
        "situation.torvald-standing.response.promise.label",
        "situation.torvald-standing.response.refuse.label",
        "situation.torvald-standing.action.court.label",
        "situation.torvald-standing.announcement",
        "situation.torvald-standing.resolution.passed-on.text",
        "situation.torvald-standing.guidance.objective",
    ] {
        assert!(keys.contains(expected), "missing derived key {expected}");
    }
}

#[test]
fn the_court_awaits_carries_announcement_guidance_and_outcome_effects() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
    assert!(
        !report.has_errors(),
        "content findings: {:?}",
        report.findings
    );
    let set = set.expect("repository content loads");
    let court = &set.situations[&aeon_data::ContentKey::new("court-awaits").unwrap()];

    // The activation announcement and the client-only guidance prose are
    // table-decided and filled; the simulation reads only the announcement.
    assert!(court.announcement.is_some());
    assert!(court.guidance_objective.is_some());
    assert!(court.guidance_how.is_some());
    assert!(court.guidance_why.is_some());
    assert_eq!(court.owner_binding.as_deref(), Some("house"));

    // Only the lapsed outcome carries effects: the stated Influence forfeit.
    let effects: Vec<_> = court
        .outcomes
        .iter()
        .filter(|outcome| outcome.effects_fn.is_some())
        .map(|outcome| outcome.key.as_str())
        .collect();
    assert_eq!(effects, ["lapsed"]);

    // The derived key mirror knows about the optional prose, so the orphan
    // and missing-row audits keep covering it.
    let keys = aeon_data::text_keys(&set);
    for expected in [
        "situation.court-awaits.announcement",
        "situation.court-awaits.guidance.objective",
        "situation.court-awaits.guidance.how",
        "situation.court-awaits.guidance.why",
        "situation.court-awaits.resolution.lapsed.text",
    ] {
        assert!(keys.contains(expected), "missing derived key {expected}");
    }
}

#[test]
fn the_visit_carries_windowed_hosting_with_authored_opinion_modifiers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
    assert!(
        !report.has_errors(),
        "content findings: {:?}",
        report.findings
    );
    let set = set.expect("repository content loads");

    // The Situation: bound to the house and the exact liege head, whose
    // person the subject binding lets the slighted outcome address.
    let visit = &set.situations[&aeon_data::ContentKey::new("casimir-visit").unwrap()];
    assert_eq!(visit.owner_binding.as_deref(), Some("house"));
    assert_eq!(visit.subject_binding.as_deref(), Some("liege-head"));
    let bindings: Vec<(&str, aeon_data::model::SituationSubjectKind)> = visit
        .bindings
        .iter()
        .map(|(name, kind)| (name.as_str(), *kind))
        .collect();
    assert_eq!(
        bindings,
        [
            (
                "house",
                aeon_data::model::SituationSubjectKind::Organisation
            ),
            (
                "liege-head",
                aeon_data::model::SituationSubjectKind::Character
            ),
        ]
    );
    let actions: Vec<&str> = visit
        .actions
        .iter()
        .map(|action| action.key.as_str())
        .collect();
    assert_eq!(actions, ["host-restrained", "host-proper", "host-lavish"]);
    assert!(visit.responses.is_empty(), "hosting has no promise/refuse");
    assert!(visit.announcement.is_some());
    assert!(visit.guidance_objective.is_some());
    assert!(visit.guidance_how.is_some());
    assert!(visit.guidance_why.is_some());

    // Only the slight carries effects; being hosted, passed on, or closed
    // resolves through the assignment results or through nothing.
    let effects: Vec<&str> = visit
        .outcomes
        .iter()
        .filter(|outcome| outcome.effects_fn.is_some())
        .map(|outcome| outcome.key.as_str())
        .collect();
    assert_eq!(effects, ["slighted"]);

    // The three tiers: ordinary hosted assignments with distinct authored
    // trade-offs, closed to autonomous houses, each reading the liege
    // head's live regard for the owner's head into its own odds.
    use aeon_data::EffectRole;
    let tier = |name: &str| &set.assignments[&aeon_data::ContentKey::new(name).unwrap()];
    let restrained = tier("host-visit-restrained");
    let proper = tier("host-visit-proper");
    let lavish = tier("host-visit-lavish");
    for (name, def) in [
        ("restrained", restrained),
        ("proper", proper),
        ("lavish", lavish),
    ] {
        assert!(!def.ai_available, "{name}: the visit is the player's");
        let modifier = def
            .opinion_modifier
            .as_ref()
            .unwrap_or_else(|| panic!("{name} authors an opinion modifier"));
        assert_eq!(modifier.from, EffectRole::LiegeHead);
        assert_eq!(modifier.toward, EffectRole::OwnerHead);
        assert!(modifier.per_point > 0);
        assert!(modifier.min <= 0 && modifier.max >= 0);
    }
    assert!(
        restrained.wealth_cost < proper.wealth_cost && proper.wealth_cost < lavish.wealth_cost,
        "spending rises with the tier"
    );
    assert!(
        restrained.difficulty > proper.difficulty && proper.difficulty > lavish.difficulty,
        "spending buys an easier contest"
    );
    assert!(
        restrained.duration_days < proper.duration_days
            && proper.duration_days < lavish.duration_days,
        "grander hospitality takes longer"
    );
    // The derived key mirror covers the new rows, so the orphan and
    // missing-row audits keep covering them.
    let keys = aeon_data::text_keys(&set);
    for expected in [
        "situation.casimir-visit.announcement",
        "situation.casimir-visit.action.host-lavish.label",
        "situation.casimir-visit.stage.awaiting.warning",
        "situation.casimir-visit.resolution.slighted.text",
        "situation.casimir-visit.guidance.objective",
        "assignment.host-visit-proper.title",
        "assignment.host-visit-lavish.disaster.popup-text",
    ] {
        assert!(keys.contains(expected), "missing derived key {expected}");
    }
}

#[test]
fn opinion_modifier_roles_and_ranges_fail_loudly_at_load() {
    let with_modifier = |modifier: &str| {
        format!(
            r#"
define_assignment(#{{
    id: "regarded-work",
    category: "routine", duration_days: 10,
    skill: "diplomacy", difficulty: 5,
    opinion_modifier: {modifier},
    results: #{{ success: #{{ weight: 800 }}, failure: #{{ weight: 200 }} }},
}});
"#
        )
    };
    let failing = [
        (
            // A mistyped role spells out the resolvable vocabulary.
            r#"#{ from: "rival-head", toward: "owner-head", per_point: 50, min: -6, max: 6 }"#,
            "expected leader, owner-head, liege-head, consul",
        ),
        (
            // A real effect role that needs a target is refused here.
            r#"#{ from: "target-head", toward: "owner-head", per_point: 50, min: -6, max: 6 }"#,
            "cannot be resolved before a target is chosen",
        ),
        (
            r#"#{ from: "liege-head", toward: "owner-head", per_point: 0, min: -6, max: 6 }"#,
            "per_point must be 1..=1000",
        ),
        (
            // A positive floor would turn a neutral relationship into a
            // standing bonus.
            r#"#{ from: "liege-head", toward: "owner-head", per_point: 50, min: 2, max: 6 }"#,
            "min must be -40..=0",
        ),
        (
            r#"#{ from: "liege-head", toward: "owner-head", per_point: 50, min: -6, max: -1 }"#,
            "max must be 0..=40",
        ),
        (
            r#"#{ from: "liege-head", toward: "owner-head", min: -6, max: 6 }"#,
            "needs an integer 'per_point'",
        ),
    ];
    for (modifier, expected) in failing {
        let (set, report) = load_content(
            &[source("bad.rhai", &with_modifier(modifier))],
            &aeon_data::StringTable::blank(),
        );
        assert!(set.is_none(), "{modifier} must fail to load");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.severity == Severity::Error && f.message.contains(expected)),
            "{modifier}: findings {:?}",
            report.findings
        );
    }

    // An unknown field inside the block warns like every authored map.
    let (set, report) = load_content(
        &[source(
            "warned.rhai",
            &with_modifier(
                r#"#{ from: "liege-head", toward: "owner-head", per_point: 50, min: -6, max: 6, mood: 3 }"#,
            ),
        )],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_some(), "an unknown field warns without failing");
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("'mood'")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn missing_mandatory_results_are_errors() {
    let bad = r#"
define_assignment(#{
    id: "half-defined", 
    category: "routine", duration_days: 10,
    skill: "stewardship", difficulty: 5,
    results: #{ success: #{ weight: 1000 } },
});
"#;
    let (set, report) = load_content(&[source("bad.rhai", bad)], &aeon_data::StringTable::blank());
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Error && f.message.contains("Failure"))
    );
}

#[test]
fn guaranteed_assignments_define_success_only() {
    let good = r#"
define_assignment(#{
    id: "declare",
    category: "routine",
    guaranteed: true,
    duration_days: 1,
    skill: "diplomacy",
    difficulty: 0,
    results: #{ success: #{ weight: 1 } },
});
"#;
    let (set, report) = load_content(
        &[source("guaranteed.rhai", good)],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    assert!(set.unwrap().assignments.values().next().unwrap().guaranteed);

    let bad = r#"
define_assignment(#{
    id: "uncertain-declaration",
    category: "routine",
    guaranteed: true,
    duration_days: 1,
    skill: "diplomacy",
    difficulty: 0,
    results: #{
        success: #{ weight: 1 },
        failure: #{ weight: 1 },
    },
});
"#;
    let (set, report) = load_content(
        &[source("bad-guaranteed.rhai", bad)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(report.findings.iter().any(|finding| {
        finding
            .message
            .contains("guaranteed assignments may define only a Success result")
    }));
}

#[test]
fn duplicate_ids_and_bad_references_are_errors() {
    let bad = r#"
define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_province(#{
    id: "lost", body: "nowhere",
    latitude_mdeg: 0, longitude_mdeg: 0,
});
define_assignment(#{
    id: "ghost-effect", 
    category: "routine", duration_days: 1,
    skill: "intrigue", difficulty: 5,
    results: #{
        success: #{ weight: 1, effect_fn: "does_not_exist" },
        failure: #{ weight: 1 },
    },
});
"#;
    let (set, report) = load_content(&[source("bad.rhai", bad)], &aeon_data::StringTable::blank());
    assert!(set.is_none());
    let messages: Vec<&str> = report.findings.iter().map(|f| f.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("duplicate body id")));
    assert!(
        messages
            .iter()
            .any(|m| m.contains("'nowhere' is not defined"))
    );
    assert!(messages.iter().any(|m| m.contains("does_not_exist")));
}

#[test]
fn orphan_moons_and_parented_planets_are_errors() {
    let bad = r#"
define_body(#{ id: "drifting-moon", kind: "moon", radius_km: 1000 });
define_body(#{ id: "odd-planet", kind: "planet", radius_km: 6000, parent: "drifting-moon" });
"#;
    let (set, report) = load_content(&[source("bad.rhai", bad)], &aeon_data::StringTable::blank());
    assert!(set.is_none());
    let messages: Vec<&str> = report.findings.iter().map(|f| f.message.as_str()).collect();
    assert!(messages.iter().any(|m| m.contains("must declare a parent")));
    assert!(
        messages
            .iter()
            .any(|m| m.contains("must not declare a parent"))
    );
}

#[test]
fn sandbox_blocks_nondeterminism_and_imports() {
    for (name, script) in [
        ("timestamp", "let t = timestamp();"),
        ("eval", r#"eval("1 + 1");"#),
        ("import", r#"import "something" as s;"#),
    ] {
        let (set, report) = load_content(
            &[source("sneaky.rhai", script)],
            &aeon_data::StringTable::blank(),
        );
        assert!(set.is_none(), "{name} should be blocked");
        assert!(
            report.has_errors(),
            "{name} should produce an error finding"
        );
    }
}

#[test]
fn runaway_scripts_hit_the_operation_limit() {
    let (set, report) = load_content(
        &[source("spin.rhai", "loop { }")],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("runtime error"))
    );
}

#[test]
fn print_output_is_captured_as_info() {
    let script =
        r#"print("checking in"); define_body(#{ id: "w", kind: "planet", radius_km: 6000 });"#;
    let (set, report) = load_content(
        &[source("noisy.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_some());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Info && f.message.contains("checking in"))
    );
}

#[test]
fn unknown_fields_warn_but_load() {
    let script = r#"
define_body(#{ id: "w", kind: "planet", radius_km: 6000, colour: "teal" });
"#;
    let (set, report) = load_content(
        &[source("typo.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_some());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("colour"))
    );
}

/// The repository's real authored content must always load cleanly.
#[test]
fn repository_content_loads() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    assert!(!sources.is_empty(), "repository content should exist");
    let (set, report) = load_content(&sources, &aeon_data::StringTable::blank());
    for finding in &report.findings {
        eprintln!("{finding}");
    }
    assert!(set.is_some(), "repository content must load without errors");
}

/// The authored assassination lays death on its target; exposed, it
/// swears a grievance against the house that tried.
#[test]
fn the_authored_assassination_strikes_the_target() {
    use aeon_data::model::{OutcomeKind, RiskTag};
    use aeon_data::{EffectRole, ScriptEffect, ScriptHost};

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (set, _) = load_content(&sources, &aeon_data::StringTable::blank());
    let set = set.expect("repository content loads");
    let host = ScriptHost::new();
    let assassinate = &set.assignments[&aeon_data::ContentKey::new("assassinate").unwrap()];

    let done = host
        .call_effect_fn(
            &set,
            assassinate.results[&OutcomeKind::Success]
                .effect_fn
                .as_ref()
                .unwrap(),
            rhai::Map::new(),
        )
        .unwrap();
    assert_eq!(
        done,
        vec![ScriptEffect::Condition {
            target: EffectRole::Target,
            tag: RiskTag::Death
        }],
        "success ends the mark's life"
    );

    let exposed = host
        .call_effect_fn(
            &set,
            assassinate.results[&OutcomeKind::Disaster]
                .effect_fn
                .as_ref()
                .unwrap(),
            rhai::Map::new(),
        )
        .unwrap();
    assert!(
        matches!(
            exposed.as_slice(),
            [ScriptEffect::Obligation {
                kind: aeon_data::model::ObligationKind::Grievance,
                ..
            }]
        ),
        "exposure swears a grievance, got {exposed:?}"
    );
}

// ---------------------------------------------------------------------------
// Individual validation branches, exercised through small fixtures
// ---------------------------------------------------------------------------

/// The smallest political world that passes validation: one great house,
/// one vassal bound to it, a head each, a name pool, and ground to stand
/// on. Tests perturb one fact and assert on the one finding it causes.
fn political_fixture(vassal_liege: &str, spouse_line: &str, ship_captain: &str) -> String {
    format!(
        r#"
define_body(#{{ id: "world", kind: "planet", radius_km: 6000 }});
define_province(#{{ id: "home", body: "world", latitude_mdeg: 0, longitude_mdeg: 0, starport: true }});
define_province(#{{ id: "march", body: "world", latitude_mdeg: 1000, longitude_mdeg: 1000 }});
define_name_pool(#{{ id: "names", male: ["Aron"], female: ["Bela"] }});
define_character(#{{ id: "gale", gender: "male", birth_year: 370, organisation: "greatwood" }});
define_character(#{{ id: "vale", gender: "female", birth_year: 372, organisation: "varga"{spouse_line} }});
define_house(#{{ id: "greatwood", tier: "great", head: "gale", provinces: ["home"], color: [200, 40, 40] }});
define_house(#{{ id: "varga", tier: "vassal", liege: "{vassal_liege}", head: "vale", provinces: ["march"], color: [40, 40, 200] }});
define_ship(#{{ id: "lantern", class: "capital", owner: "greatwood", captain: "{ship_captain}", location: "home" }});
"#
    )
}

#[test]
fn the_political_fixture_is_itself_valid() {
    let (set, report) = load_content(
        &[source(
            "fixture.rhai",
            &political_fixture("greatwood", "", "gale"),
        )],
        &aeon_data::StringTable::blank(),
    );
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    assert!(set.is_some());
}

#[test]
fn a_vassals_liege_must_be_a_great_house() {
    // Varga swears to itself: a vassal, not a great house.
    let (set, report) = load_content(
        &[source(
            "fixture.rhai",
            &political_fixture("varga", "", "gale"),
        )],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("must be a great house")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn conflicting_spouse_declarations_are_errors() {
    // Vale declares Gale; Gale declares nobody -- fine (mirrored). But a
    // spouse who names a *different* spouse is an authoring conflict.
    let mirrored = political_fixture("greatwood", r#", spouse: "gale""#, "gale");
    let (set, _) = load_content(
        &[source("fixture.rhai", &mirrored)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_some(), "one-sided declarations are mirrored");

    let conflicted = format!(
        "{}\ndefine_character(#{{ id: \"rook\", name: \"Rook\", gender: \"male\", birth_year: 371, organisation: \"greatwood\", spouse: \"vale\" }});",
        political_fixture("greatwood", r#", spouse: "gale""#, "gale")
    );
    let (set, report) = load_content(
        &[source("fixture.rhai", &conflicted)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("declares a different spouse")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn a_ships_captain_must_belong_to_its_owner() {
    // Vale belongs to Varga; the Lantern belongs to Greatwood.
    let (set, report) = load_content(
        &[source(
            "fixture.rhai",
            &political_fixture("greatwood", "", "vale"),
        )],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("does not belong to the owning house")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn mistyped_vocabulary_fields_spell_out_the_options() {
    let script = r#"
define_assignment(#{
    id: "odd-assignment", category: "sometimes",
    duration_days: 10, skill: "stewardship", difficulty: 5,
    results: #{ success: #{ weight: 800 }, failure: #{ weight: 200 } },
});
"#;
    let (set, report) = load_content(
        &[source("bad.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("expected routine, consequential")),
        "findings: {:?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// Plans
// ---------------------------------------------------------------------------

const GOOD_PLANS: &str = r#"
define_plan(#{
    id: "court-the-court",
    goal: "standing",
    target: "organisation",
    score_bonus: 25,
    cooldown_days: 180,
    max_days: 360,
    max_step_retries: 2,
    methods: [
        #{ id: "the-patient-way",
           requires: #{ min_wealth: 50 },
           steps: [
               #{ start: "manage-estates", skip_if: #{ min_legitimacy: 80 } },
               #{ id: "approach", start: "court-a-rival", target: "plan" },
               #{ plan: "tend-the-books" },
           ] },
    ],
});
define_plan(#{
    id: "tend-the-books",
    goal: "resources",
    max_days: 90,
    methods: [
        #{ id: "only-way", steps: [ #{ start: "manage-estates" } ] },
    ],
});
"#;

#[test]
fn loads_a_valid_plan() {
    use aeon_data::model::{AiIntent, AssignmentTargetKind, PlanStepAction, PlanTargetSelector};

    let (set, report) = load_content(
        &[
            source("core/assignments.rhai", GOOD_JOBS),
            source("core/plans.rhai", GOOD_PLANS),
        ],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("valid plans load");
    assert_eq!(set.plans.len(), 2);

    let plan = &set.plans[&aeon_data::ContentKey::new("court-the-court").unwrap()];
    assert_eq!(plan.goal, AiIntent::Standing);
    assert_eq!(plan.target, AssignmentTargetKind::Organisation);
    assert_eq!(plan.score_bonus, 25);
    assert_eq!(plan.max_step_retries, 2);

    let method = &plan.methods[0];
    assert_eq!(method.id, "the-patient-way");
    assert_eq!(method.requires.min_wealth, Some(50));

    // A step without an id borrows its action's key.
    assert_eq!(method.steps[0].id, "manage-estates");
    assert_eq!(
        method.steps[0].skip_if.as_ref().unwrap().min_legitimacy,
        Some(80)
    );
    assert!(matches!(
        &method.steps[1].action,
        PlanStepAction::Assignment {
            target: PlanTargetSelector::PlanTarget,
            ..
        }
    ));
    assert!(matches!(
        &method.steps[2].action,
        PlanStepAction::SubPlan(key) if key.as_str() == "tend-the-books"
    ));
}

#[test]
fn a_plan_step_naming_a_missing_assignment_fails_to_load() {
    let script = r#"
define_plan(#{
    id: "castles-in-the-air",
    goal: "routine",
    max_days: 30,
    methods: [ #{ id: "somehow", steps: [ #{ start: "no-such-work" } ] } ],
});
"#;
    let (set, report) = load_content(
        &[source("core/plans.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report.findings.iter().any(|f| f
            .message
            .contains("assignment 'no-such-work' is not defined")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn sub_plans_that_form_a_cycle_fail_to_load() {
    let script = r#"
define_plan(#{
    id: "the-chicken", goal: "routine", max_days: 30,
    methods: [ #{ id: "m", steps: [ #{ plan: "the-egg" } ] } ],
});
define_plan(#{
    id: "the-egg", goal: "routine", max_days: 30,
    methods: [ #{ id: "m", steps: [ #{ plan: "the-chicken" } ] } ],
});
"#;
    let (set, report) = load_content(
        &[source("core/plans.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report.findings.iter().any(|f| f.message.contains("cycle")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn a_step_whose_target_kind_disagrees_with_its_assignment_fails_to_load() {
    let script = r#"
define_plan(#{
    id: "aim-at-nothing",
    goal: "standing",
    max_days: 60,
    methods: [ #{ id: "m", steps: [ #{ start: "court-a-rival" } ] } ],
});
"#;
    let (set, report) = load_content(
        &[
            source("core/assignments.rhai", GOOD_JOBS),
            source("core/plans.rhai", script),
        ],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("wants a Organisation target")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn a_step_aiming_at_a_target_the_plan_does_not_have_fails_to_load() {
    let script = r#"
define_plan(#{
    id: "aim-at-the-void",
    goal: "standing",
    max_days: 60,
    methods: [ #{ id: "m", steps: [ #{ start: "court-a-rival", target: "plan" } ] } ],
});
"#;
    let (set, report) = load_content(
        &[
            source("core/assignments.rhai", GOOD_JOBS),
            source("core/plans.rhai", script),
        ],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("the plan has none")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn an_orders_step_naming_a_non_army_assignment_fails_to_load() {
    let script = r#"
define_plan(#{
    id: "misfiled-doctrine",
    goal: "muster",
    max_days: 60,
    methods: [ #{ id: "m", steps: [ #{ orders: ["manage-estates"] } ] } ],
});
"#;
    let (set, report) = load_content(
        &[
            source("core/assignments.rhai", GOOD_JOBS),
            source("core/plans.rhai", script),
        ],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("cannot be a standing order")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn an_orders_step_naming_a_missing_assignment_fails_to_load() {
    let script = r#"
define_plan(#{
    id: "phantom-doctrine",
    goal: "muster",
    max_days: 60,
    methods: [ #{ id: "m", steps: [ #{ orders: ["no-such-doctrine"] } ] } ],
});
"#;
    let (set, report) = load_content(
        &[source("core/plans.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report.findings.iter().any(|f| f
            .message
            .contains("standing order 'no-such-doctrine' is not defined")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn a_worst_holding_selector_must_feed_a_province_assignment() {
    let script = r#"
define_plan(#{
    id: "misaimed-care",
    goal: "order",
    max_days: 60,
    methods: [ #{ id: "m", steps: [ #{ start: "manage-estates", target: "worst-holding" } ] } ],
});
"#;
    let (set, report) = load_content(
        &[
            source("core/assignments.rhai", GOOD_JOBS),
            source("core/plans.rhai", script),
        ],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("wants a None target")),
        "findings: {:?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// Goals
// ---------------------------------------------------------------------------

const GOOD_GOALS: &str = r#"
define_goal(#{
    id: "become-consul",
    favours: ["standing"],
    favour_bonus: 40,
    max_days: 3600,
    cooldown_days: 720,
    trigger: #{ min_legitimacy: 40, is_vassal: false },
});
define_goal(#{
    id: "conquer-a-neighbour",
    favours: ["muster", "order"],
    favour_bonus: 30,
    target: "organisation",
    max_days: 5400,
    trigger: #{ has_army: true, has_vassals: true },
    directives: [
        #{ intent: "muster", target: "goal" },
        #{ intent: "standing" },
    ],
});
"#;

#[test]
fn loads_a_valid_goal() {
    use aeon_data::model::{AiIntent, AssignmentTargetKind, DirectiveTarget};

    let (set, report) = load_content(
        &[source("core/goals.rhai", GOOD_GOALS)],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("valid goals load");
    assert_eq!(set.goals.len(), 2);

    let consul = &set.goals[&aeon_data::ContentKey::new("become-consul").unwrap()];
    assert_eq!(consul.favours, vec![AiIntent::Standing]);
    assert_eq!(consul.favour_bonus, 40);
    assert_eq!(consul.target, AssignmentTargetKind::None);
    assert_eq!(consul.trigger.min_legitimacy, Some(40));
    assert_eq!(consul.trigger.is_vassal, Some(false));

    let conquer = &set.goals[&aeon_data::ContentKey::new("conquer-a-neighbour").unwrap()];
    assert_eq!(conquer.favours, vec![AiIntent::Muster, AiIntent::Order]);
    assert_eq!(conquer.target, AssignmentTargetKind::Organisation);
    assert_eq!(conquer.trigger.has_vassals, Some(true));
    assert_eq!(conquer.directives.len(), 2);
    assert_eq!(conquer.directives[0].intent, AiIntent::Muster);
    assert_eq!(conquer.directives[0].target, DirectiveTarget::GoalTarget);
    assert_eq!(conquer.directives[1].target, DirectiveTarget::None);
}

#[test]
fn a_goal_with_no_favoured_pressure_fails_to_load() {
    let script = r#"
define_goal(#{ id: "aimless", favours: [], max_days: 100 });
"#;
    let (set, report) = load_content(
        &[source("core/goals.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("favour at least one pressure")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn a_goal_favouring_an_unknown_pressure_fails_to_load() {
    let script = r#"
define_goal(#{ id: "confused", favours: ["conquest"], max_days: 100 });
"#;
    let (set, report) = load_content(
        &[source("core/goals.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("unknown favoured pressure 'conquest'")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn a_directive_aiming_at_a_target_the_goal_lacks_fails_to_load() {
    let script = r#"
define_goal(#{
    id: "misaimed",
    favours: ["standing"],
    max_days: 100,
    directives: [ #{ intent: "muster", target: "goal" } ],
});
"#;
    let (set, report) = load_content(
        &[source("core/goals.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("the goal has none")),
        "findings: {:?}",
        report.findings
    );
}

// ---------------------------------------------------------------------------
// Goods
// ---------------------------------------------------------------------------

const GOOD_GOODS_SYSTEM: &str = r#"
define_body(#{ id: "the-world", kind: "planet", radius_km: 6400 });
define_good(#{ id: "grain", value: 2 });
define_good(#{ id: "ore", value: 5 });
define_province(#{
    id: "farmstead", body: "the-world",
    latitude_mdeg: 0, longitude_mdeg: 0,
    produces: #{ grain: 30 }, consumes: #{ ore: 5 },
});
define_province(#{
    id: "minehead", body: "the-world",
    latitude_mdeg: 1000, longitude_mdeg: 1000,
    produces: #{ ore: 20 }, consumes: #{ grain: 10 },
});
"#;

#[test]
fn loads_provinces_with_typed_goods() {
    let (set, report) = load_content(
        &[source("system/goods.rhai", GOOD_GOODS_SYSTEM)],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("valid goods content loads");
    assert_eq!(set.goods.len(), 2);
    assert_eq!(
        set.goods[&aeon_data::ContentKey::new("ore").unwrap()].value,
        5
    );

    let farm = &set.provinces[&aeon_data::ContentKey::new("farmstead").unwrap()];
    assert_eq!(
        farm.produces[&aeon_data::ContentKey::new("grain").unwrap()],
        30
    );
    assert_eq!(
        farm.consumes[&aeon_data::ContentKey::new("ore").unwrap()],
        5
    );
}

#[test]
fn a_province_producing_an_undefined_good_fails_to_load() {
    let script = r#"
define_body(#{ id: "the-world", kind: "planet", radius_km: 6400 });
define_province(#{
    id: "smuggler", body: "the-world",
    latitude_mdeg: 0, longitude_mdeg: 0,
    produces: #{ contraband: 5 },
});
"#;
    let (set, report) = load_content(
        &[source("system/goods.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("good 'contraband' is not defined")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn loads_a_building_and_a_construct_effect() {
    use aeon_data::{ScriptEffect, ScriptHost};

    let script = r#"
define_body(#{ id: "the-world", kind: "planet", radius_km: 6400 });
define_good(#{ id: "grain", value: 2 });
define_building(#{
    id: "granary", build_days: 60,
    wealth_cost: 40, supplies_cost: 10,
    adds_wealth: 5, produces: #{ grain: 15 },
});
define_assignment(#{
    id: "raise-a-granary", category: "consequential",
    duration_days: 60, skill: "stewardship", difficulty: 6, target: "province",
    results: #{
        success: #{ weight: 900, effect_fn: "built" },
        failure: #{ weight: 100 },
    },
});
fn built(ctx) { [#{ kind: "construct", building: "granary" }] }
"#;
    let (set, report) = load_content(
        &[source("system/buildings.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("valid buildings content loads");
    let granary = &set.buildings[&aeon_data::ContentKey::new("granary").unwrap()];
    assert_eq!(granary.adds_wealth, 5);
    assert_eq!(
        granary.produces[&aeon_data::ContentKey::new("grain").unwrap()],
        15
    );

    // The construct effect parses to the typed variant.
    let host = ScriptHost::new();
    let fn_ref = set.assignments[&aeon_data::ContentKey::new("raise-a-granary").unwrap()].results
        [&aeon_data::model::OutcomeKind::Success]
        .effect_fn
        .clone()
        .unwrap();
    let effects = host
        .call_effect_fn(&set, &fn_ref, rhai::Map::new())
        .unwrap();
    assert_eq!(
        effects,
        vec![ScriptEffect::Construct {
            building: "granary".to_owned()
        }]
    );
}

#[test]
fn a_building_consuming_an_undefined_good_fails_to_load() {
    let script = r#"
define_body(#{ id: "the-world", kind: "planet", radius_km: 6400 });
define_building(#{ id: "smokehouse", build_days: 30, consumes: #{ mystery: 3 } });
"#;
    let (set, report) = load_content(
        &[source("system/buildings.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.message.contains("good 'mystery' is not defined")),
        "findings: {:?}",
        report.findings
    );
}

#[test]
fn a_condition_effect_parses_to_its_typed_variant() {
    use aeon_data::model::RiskTag;
    use aeon_data::{EffectRole, ScriptEffect, ScriptHost};

    let script = r#"
define_assignment(#{
    id: "have-them-killed", category: "consequential",
    duration_days: 30, skill: "intrigue", difficulty: 16, target: "character",
    results: #{
        success: #{ weight: 400, effect_fn: "struck_down" },
        failure: #{ weight: 600 },
    },
});
fn struck_down(ctx) { [#{ kind: "condition", target: "target", tag: "death" }] }
"#;
    let (set, report) = load_content(
        &[source("core/intrigue.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("valid intrigue loads");
    let host = ScriptHost::new();
    let fn_ref = set.assignments[&aeon_data::ContentKey::new("have-them-killed").unwrap()].results
        [&aeon_data::model::OutcomeKind::Success]
        .effect_fn
        .clone()
        .unwrap();
    let effects = host
        .call_effect_fn(&set, &fn_ref, rhai::Map::new())
        .unwrap();
    assert_eq!(
        effects,
        vec![ScriptEffect::Condition {
            target: EffectRole::Target,
            tag: RiskTag::Death
        }]
    );
}

#[test]
fn a_condition_naming_an_unknown_harm_fails_to_parse() {
    use aeon_data::ScriptHost;

    let script = r#"
define_assignment(#{
    id: "botched", category: "consequential",
    duration_days: 10, skill: "intrigue", difficulty: 8, target: "character",
    results: #{
        success: #{ weight: 500, effect_fn: "oops" },
        failure: #{ weight: 500 },
    },
});
fn oops(ctx) { [#{ kind: "condition", target: "target", tag: "curse" }] }
"#;
    let (set, _) = load_content(
        &[source("core/intrigue.rhai", script)],
        &aeon_data::StringTable::blank(),
    );
    let set = set.expect("loads; the bad effect is a runtime failure");
    let host = ScriptHost::new();
    let fn_ref = set.assignments[&aeon_data::ContentKey::new("botched").unwrap()].results
        [&aeon_data::model::OutcomeKind::Success]
        .effect_fn
        .clone()
        .unwrap();
    assert!(
        host.call_effect_fn(&set, &fn_ref, rhai::Map::new())
            .is_err(),
        "an unknown harm is refused"
    );
}

#[test]
fn order_modifier_shapes_fail_loudly_at_load() {
    let with_modifier = |target: &str, modifier: &str| {
        format!(
            r#"
define_assignment(#{{
    id: "resisted-work",
    category: "routine", duration_days: 10,
    skill: "intrigue", difficulty: 5,
    target: {target},
    order_modifier: {modifier},
    results: #{{ success: #{{ weight: 800 }}, failure: #{{ weight: 200 }} }},
}});
"#
        )
    };
    let failing = [
        (
            r#""province""#,
            r#"#{ reference: 1200, per_hundred: 4, min: -8, max: 0 }"#,
            "reference must be 0..=1000",
        ),
        (
            r#""province""#,
            r#"#{ reference: 800, per_hundred: 0, min: -8, max: 0 }"#,
            "per_hundred must be 1..=1000",
        ),
        (
            // A positive floor would turn settled ground into a bonus.
            r#""province""#,
            r#"#{ reference: 800, per_hundred: 4, min: 2, max: 6 }"#,
            "min must be -40..=0",
        ),
        (
            r#""province""#,
            r#"#{ reference: 800, per_hundred: 4, min: -8, max: -1 }"#,
            "max must be 0..=40",
        ),
        (
            r#""province""#,
            r#"#{ per_hundred: 4, min: -8, max: 0 }"#,
            "needs an integer 'reference'",
        ),
        (
            // The modifier reads the target province's live Order, so a
            // targetless assignment cannot author one.
            r#""none""#,
            r#"#{ reference: 800, per_hundred: 4, min: -8, max: 0 }"#,
            "province-bearing target kind",
        ),
    ];
    for (target, modifier, expected) in failing {
        let (set, report) = load_content(
            &[source("bad.rhai", &with_modifier(target, modifier))],
            &aeon_data::StringTable::blank(),
        );
        assert!(set.is_none(), "{modifier} on {target} must fail to load");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.severity == Severity::Error && f.message.contains(expected)),
            "{modifier}: findings {:?}",
            report.findings
        );
    }
}

/// Content authoring covertness, campaign-day windows, and the hostility
/// predicates parses into the model, and every malformed combination
/// fails loudly at load rather than quietly gating nothing at play.
#[test]
fn covert_flags_windows_and_hostility_predicates_load_and_validate() {
    const SHADOW_FIXTURE: &str = r#"
define_assignment(#{
    id: "quiet-work",
    category: "consequential", duration_days: 10,
    skill: "intrigue", difficulty: 5, target: "province",
    ai_available: false, ai_intent: "subvert", covert: true,
    order_modifier: #{ reference: 800, per_hundred: 4, min: -8, max: 0 },
    results: #{ success: #{ weight: 800 }, failure: #{ weight: 200 } },
});
define_plan(#{
    id: "quiet-campaign",
    goal: "subvert",
    target: "organisation",
    covert: true,
    max_days: 100,
    methods: [
        #{ id: "from-ill-will",
           requires: #{ max_target_head_opinion: -10, min_campaign_day: 180,
                        max_campaign_day: 260 },
           steps: [ #{ start: "quiet-work", target: "target-border-province" } ] },
        #{ id: "from-grievance",
           requires: #{ target_owes_grievance: true },
           steps: [ #{ start: "quiet-work", target: "target-border-province" } ] },
    ],
});
define_goal(#{
    id: "quiet-ambition",
    favours: ["subvert"],
    favour_bonus: 60,
    target: "organisation",
    target_selector: #{ kind: "hostile-border-neighbour", max_head_opinion: -10,
                        with_grievance: true },
    covert: true,
    trigger: #{ min_campaign_day: 180, max_campaign_day: 260 },
    max_days: 100,
});
"#;
    let (set, report) = load_content(
        &[source("shadow.rhai", SHADOW_FIXTURE)],
        &aeon_data::StringTable::blank(),
    );
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("the covert fixture loads");
    let work = &set.assignments[&aeon_data::ContentKey::new("quiet-work").unwrap()];
    assert!(work.covert);
    assert_eq!(work.ai_intent, aeon_data::model::AiIntent::Subvert);
    let modifier = work.order_modifier.as_ref().expect("order modifier");
    assert_eq!(
        (
            modifier.reference,
            modifier.per_hundred,
            modifier.min,
            modifier.max
        ),
        (800, 4, -8, 0)
    );
    let plan = &set.plans[&aeon_data::ContentKey::new("quiet-campaign").unwrap()];
    assert!(plan.covert);
    let ill_will = &plan.methods[0].requires;
    assert_eq!(ill_will.max_target_head_opinion, Some(-10));
    assert_eq!(ill_will.min_campaign_day, Some(180));
    assert_eq!(ill_will.max_campaign_day, Some(260));
    assert!(plan.methods[1].requires.target_owes_grievance);
    let goal = &set.goals[&aeon_data::ContentKey::new("quiet-ambition").unwrap()];
    assert!(goal.covert);
    assert_eq!(goal.trigger.min_campaign_day, Some(180));
    assert_eq!(goal.trigger.max_campaign_day, Some(260));
    assert_eq!(
        goal.target_selector,
        aeon_data::model::GoalTargetSelector::HostileBorderNeighbour {
            max_head_opinion: -10,
            with_grievance: true,
        }
    );

    // The malformed combinations, each with the finding it must raise.
    let failing = [
        (
            // A hostility floor without an organisation target reads
            // nobody's head.
            r#"
define_assignment(#{
    id: "quiet-work", category: "consequential", duration_days: 10,
    skill: "intrigue", difficulty: 5, ai_available: false,
    results: #{ success: #{ weight: 800 }, failure: #{ weight: 200 } },
});
define_plan(#{
    id: "aimless", goal: "subvert", max_days: 100,
    methods: [ #{ id: "only",
        requires: #{ max_target_head_opinion: -10 },
        steps: [ #{ start: "quiet-work" } ] } ],
});
"#,
            "compares an organisation target's head",
        ),
        (
            r#"
define_assignment(#{
    id: "quiet-work", category: "consequential", duration_days: 10,
    skill: "intrigue", difficulty: 5, ai_available: false,
    results: #{ success: #{ weight: 800 }, failure: #{ weight: 200 } },
});
define_plan(#{
    id: "aimless", goal: "subvert", max_days: 100,
    methods: [ #{ id: "only",
        requires: #{ target_owes_grievance: true },
        steps: [ #{ start: "quiet-work" } ] } ],
});
"#,
            "reads an organisation target's ledger",
        ),
        (
            // An inverted window can never open.
            r#"
define_assignment(#{
    id: "quiet-work", category: "consequential", duration_days: 10,
    skill: "intrigue", difficulty: 5, ai_available: false,
    results: #{ success: #{ weight: 800 }, failure: #{ weight: 200 } },
});
define_plan(#{
    id: "inverted", goal: "subvert", max_days: 100,
    methods: [ #{ id: "only",
        requires: #{ min_campaign_day: 260, max_campaign_day: 180 },
        steps: [ #{ start: "quiet-work" } ] } ],
});
"#,
            "is after max_campaign_day",
        ),
        (
            r#"
define_goal(#{
    id: "early", favours: ["subvert"], max_days: 100,
    trigger: #{ min_campaign_day: -5 },
});
"#,
            "days must be >= 0",
        ),
        (
            // A selector with nothing to select.
            r#"
define_goal(#{
    id: "aimless", favours: ["subvert"], max_days: 100,
    target_selector: #{ kind: "hostile-border-neighbour", max_head_opinion: -10 },
});
"#,
            "target_selector needs the goal to target an organisation",
        ),
        (
            r#"
define_goal(#{
    id: "floorless", favours: ["subvert"], target: "organisation", max_days: 100,
    target_selector: #{ kind: "hostile-border-neighbour" },
});
"#,
            "needs an integer 'max_head_opinion'",
        ),
        (
            r#"
define_goal(#{
    id: "unknown", favours: ["subvert"], target: "organisation", max_days: 100,
    target_selector: #{ kind: "friendliest-neighbour" },
});
"#,
            "unknown target_selector kind",
        ),
        (
            // The border selector needs an organisation whose border it
            // walks.
            r#"
define_assignment(#{
    id: "quiet-work", category: "consequential", duration_days: 10,
    skill: "intrigue", difficulty: 5, target: "province", ai_available: false,
    results: #{ success: #{ weight: 800 }, failure: #{ weight: 200 } },
});
define_plan(#{
    id: "borderless", goal: "subvert", max_days: 100,
    methods: [ #{ id: "only",
        steps: [ #{ start: "quiet-work", target: "target-border-province" } ] } ],
});
"#,
            "selects the target's border province",
        ),
    ];
    for (fixture, expected) in failing {
        let (set, report) = load_content(
            &[source("bad.rhai", fixture)],
            &aeon_data::StringTable::blank(),
        );
        assert!(set.is_none(), "must fail: {expected}");
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.severity == Severity::Error && f.message.contains(expected)),
            "{expected}: findings {:?}",
            report.findings
        );
    }
}

/// The shipped shadow arc: the covert ambition, the covert campaign, the
/// Order-resisted sabotage, and the Unquiet Holdings card that shows the
/// province while structurally — never visibly — binding the culprit.
#[test]
fn the_shadow_arc_carries_covert_provenance_and_order_resistance() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
    assert!(
        !report.has_errors(),
        "content findings: {:?}",
        report.findings
    );
    let set = set.expect("repository content loads");

    // The Situation: the audience is the targeted house alone; the actor
    // binding gives spectators, replay, and future investigation their
    // authoritative provenance without ever entering the audience.
    let unquiet = &set.situations[&aeon_data::ContentKey::new("unquiet-holdings").unwrap()];
    let bindings: Vec<(&str, aeon_data::model::SituationSubjectKind)> = unquiet
        .bindings
        .iter()
        .map(|(name, kind)| (name.as_str(), *kind))
        .collect();
    assert_eq!(
        bindings,
        [
            (
                "actor",
                aeon_data::model::SituationSubjectKind::Organisation
            ),
            (
                "house",
                aeon_data::model::SituationSubjectKind::Organisation
            ),
            ("province", aeon_data::model::SituationSubjectKind::Province),
        ]
    );
    assert_eq!(
        unquiet.visibility,
        aeon_data::model::SituationVisibilityDef::Bound(vec!["house".to_owned()]),
        "the culprit binding must never join the audience"
    );
    assert!(unquiet.owner_binding.is_none());
    assert!(unquiet.announcement.is_some());
    assert!(
        unquiet
            .outcomes
            .iter()
            .all(|outcome| outcome.effects_fn.is_none()),
        "the sabotage's own authored effect does the damage; outcomes only narrate"
    );
    let outcomes: Vec<&str> = unquiet
        .outcomes
        .iter()
        .map(|outcome| outcome.key.as_str())
        .collect();
    assert_eq!(outcomes, ["passed-on", "struck", "weathered"]);

    // The vehicle: covert, closed to the reactive scorer, answering the
    // subvert pressure, resisted by the target province's live Order.
    let sabotage = &set.assignments[&aeon_data::ContentKey::new("foment-unrest").unwrap()];
    assert!(sabotage.covert);
    assert!(!sabotage.ai_available);
    assert_eq!(sabotage.ai_intent, aeon_data::model::AiIntent::Subvert);
    let resistance = sabotage.order_modifier.as_ref().expect("order modifier");
    assert!(resistance.per_hundred > 0);
    assert_eq!(resistance.max, 0, "disorder never helps beyond neutral");
    assert!(resistance.min < 0, "high Order genuinely resists");

    // The campaign: covert, organisation-aimed, gated on authored
    // hostility both ways — ill will or an open grievance — plus the
    // capability floor.
    let campaign = &set.plans[&aeon_data::ContentKey::new("deniable-pressure").unwrap()];
    assert!(campaign.covert);
    assert_eq!(campaign.goal, aeon_data::model::AiIntent::Subvert);
    assert_eq!(campaign.methods.len(), 2);
    assert_eq!(
        campaign.methods[0].requires.max_target_head_opinion,
        Some(-10),
        "the accepted hostility floor"
    );
    assert!(campaign.methods[1].requires.target_owes_grievance);
    for method in &campaign.methods {
        assert_eq!(method.requires.min_wealth, Some(40), "capability is data");
    }

    // The ambition: covert, windowed in data to the accepted intrigue
    // stretch, resolved against a hostile border neighbour.
    let ambition = &set.goals[&aeon_data::ContentKey::new("undermine-a-neighbour").unwrap()];
    assert!(ambition.covert);
    assert_eq!(ambition.trigger.min_campaign_day, Some(180));
    assert_eq!(ambition.trigger.max_campaign_day, Some(260));
    assert_eq!(
        ambition.target_selector,
        aeon_data::model::GoalTargetSelector::HostileBorderNeighbour {
            max_head_opinion: -10,
            with_grievance: true,
        }
    );
    assert!(
        ambition.directives.is_empty(),
        "a covert ambition presses no directive that could leak it"
    );

    // The derived key mirror covers the new rows, so the orphan and
    // missing-row audits keep covering them.
    let keys = aeon_data::text_keys(&set);
    for expected in [
        "situation.unquiet-holdings.announcement",
        "situation.unquiet-holdings.stage.unrest.warning",
        "situation.unquiet-holdings.resolution.struck.text",
        "situation.unquiet-holdings.guidance.objective",
        "goal.undermine-a-neighbour.title",
        "plan.deniable-pressure.summary",
    ] {
        assert!(keys.contains(expected), "missing derived key {expected}");
    }
}
