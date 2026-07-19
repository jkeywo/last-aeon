//! Content-pipeline guarantees: loading, validation, sandboxing,
//! determinism, and the effect boundary.

use aeon_data::model::{BodyKind, JobCategory, JobResultKind};
use aeon_data::{ContentSource, ScriptEffect, ScriptHost, Severity, load_content};

fn source(path: &str, text: &str) -> ContentSource {
    ContentSource {
        path: path.to_owned(),
        source: text.to_owned(),
    }
}

const GOOD_JOBS: &str = r#"
define_job(#{
    id: "manage-estates",
    title: "Manage the Estates",
    summary: "Routine administration of the house's holdings.",
    category: "routine",
    duration_days: 30,
    skill: "stewardship",
    difficulty: 6,
    results: #{
        success: #{ weight: 850 },
        failure: #{ weight: 150 },
    },
});

define_job(#{
    id: "court-a-rival",
    title: "Court a Rival House",
    summary: "Send an envoy to soften a rival's stance.",
    category: "consequential",
    duration_days: 45,
    skill: "diplomacy",
    difficulty: 10,
    target: "organisation",
    risks: ["scandal"],
    results: #{
        critical_success: #{
            weight: 100, popup: true, log: true,
            popup_text: "{leader} returns triumphant from {target}.",
        },
        success: #{ weight: 500, log: true },
        failure: #{ weight: 300 },
        disaster: #{
            weight: 100, popup: true, log: true,
            popup_text: "{leader} gave insult at {target}.",
            effect_fn: "courting_disaster",
        },
    },
});

fn courting_disaster(ctx) {
    [#{ kind: "log", message: "The envoy insulted " + ctx.target + " (" + ctx.result + ")." }]
}
"#;

const GOOD_SYSTEM: &str = r#"
define_body(#{
    id: "the-world", name: "The World", kind: "planet", radius_km: 6400,
});
define_body(#{
    id: "the-moon", name: "The Moon", kind: "moon", radius_km: 1700,
    parent: "the-world", orbit_radius_mm: 384, orbit_days: 27,
});
define_province(#{
    id: "first-landing", name: "First Landing", body: "the-world",
    latitude_mdeg: 12500, longitude_mdeg: -30250,
});
"#;

#[test]
fn loads_a_valid_content_set() {
    let (set, report) = load_content(&[
        source("core/jobs.rhai", GOOD_JOBS),
        source("system/bodies.rhai", GOOD_SYSTEM),
    ]);
    assert!(
        !report.has_errors(),
        "unexpected findings: {:?}",
        report.findings
    );
    let set = set.expect("valid content loads");

    assert_eq!(set.jobs.len(), 2);
    let estates = set.jobs.values().next().unwrap();
    assert_eq!(estates.key.as_str(), "court-a-rival");
    assert_eq!(set.bodies.len(), 2);
    assert_eq!(set.provinces.len(), 1);

    let rival = &set.jobs[&aeon_data::ContentKey::new("court-a-rival").unwrap()];
    assert_eq!(rival.category, JobCategory::Consequential);
    assert_eq!(rival.results.len(), 4);
    assert!(rival.results[&JobResultKind::Disaster].effect_fn.is_some());

    let moon = &set.bodies[&aeon_data::ContentKey::new("the-moon").unwrap()];
    assert_eq!(moon.kind, BodyKind::Moon);
    assert_eq!(moon.orbit_days, 27);
}

#[test]
fn loading_is_deterministic_across_runs_and_input_order() {
    let forward = [
        source("core/jobs.rhai", GOOD_JOBS),
        source("system/bodies.rhai", GOOD_SYSTEM),
    ];
    let reversed = [
        source("system/bodies.rhai", GOOD_SYSTEM),
        source("core/jobs.rhai", GOOD_JOBS),
    ];
    let (a, _) = load_content(&forward);
    let (b, _) = load_content(&forward);
    let (c, _) = load_content(&reversed);
    let (a, b, c) = (a.unwrap(), b.unwrap(), c.unwrap());
    assert!(a.data_eq(&b));
    assert!(a.data_eq(&c));
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.content_hash, c.content_hash);
}

#[test]
fn effect_functions_run_against_read_context() {
    let (set, _) = load_content(&[
        source("core/jobs.rhai", GOOD_JOBS),
        source("system/bodies.rhai", GOOD_SYSTEM),
    ]);
    let set = set.unwrap();
    let host = ScriptHost::new();

    let disaster = set.jobs[&aeon_data::ContentKey::new("court-a-rival").unwrap()].results
        [&JobResultKind::Disaster]
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
            message: "The envoy insulted Lady Calder (Disaster).".to_owned()
        }]
    );
}

#[test]
fn missing_mandatory_results_are_errors() {
    let bad = r#"
define_job(#{
    id: "half-defined", title: "Half Defined", summary: "s",
    category: "routine", duration_days: 10,
    skill: "stewardship", difficulty: 5,
    results: #{ success: #{ weight: 1000 } },
});
"#;
    let (set, report) = load_content(&[source("bad.rhai", bad)]);
    assert!(set.is_none());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.severity == Severity::Error && f.message.contains("Failure"))
    );
}

#[test]
fn duplicate_ids_and_bad_references_are_errors() {
    let bad = r#"
define_body(#{ id: "world", name: "World", kind: "planet", radius_km: 6000 });
define_body(#{ id: "world", name: "World Again", kind: "planet", radius_km: 6000 });
define_province(#{
    id: "lost", name: "Lost", body: "nowhere",
    latitude_mdeg: 0, longitude_mdeg: 0,
});
define_job(#{
    id: "ghost-effect", title: "Ghost", summary: "s",
    category: "routine", duration_days: 1,
    skill: "intrigue", difficulty: 5,
    results: #{
        success: #{ weight: 1, effect_fn: "does_not_exist" },
        failure: #{ weight: 1 },
    },
});
"#;
    let (set, report) = load_content(&[source("bad.rhai", bad)]);
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
define_body(#{ id: "drifting-moon", name: "Drifter", kind: "moon", radius_km: 1000 });
define_body(#{ id: "odd-planet", name: "Odd", kind: "planet", radius_km: 6000, parent: "drifting-moon" });
"#;
    let (set, report) = load_content(&[source("bad.rhai", bad)]);
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
        let (set, report) = load_content(&[source("sneaky.rhai", script)]);
        assert!(set.is_none(), "{name} should be blocked");
        assert!(
            report.has_errors(),
            "{name} should produce an error finding"
        );
    }
}

#[test]
fn runaway_scripts_hit_the_operation_limit() {
    let (set, report) = load_content(&[source("spin.rhai", "loop { }")]);
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
    let script = r#"print("checking in"); define_body(#{ id: "w", name: "W", kind: "planet", radius_km: 6000 });"#;
    let (set, report) = load_content(&[source("noisy.rhai", script)]);
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
define_body(#{ id: "w", name: "W", kind: "planet", radius_km: 6000, colour: "teal" });
"#;
    let (set, report) = load_content(&[source("typo.rhai", script)]);
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
    let (set, report) = load_content(&sources);
    for finding in &report.findings {
        eprintln!("{finding}");
    }
    assert!(set.is_some(), "repository content must load without errors");
}
