//! The shared semantic world view's keyed record maps.
//!
//! `province_by_id`, `organisation_by_id`, and `character_by_id` publish the
//! same records the ordered arrays hold, keyed by the id rendered as text,
//! so authored helpers answer "which record has this id" with one lookup.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentSet, load_content};
use aeon_sim::script_world::context_value;
use aeon_sim::{CampaignConfig, SimHost};
use rhai::{Array, Map};

fn repository_content() -> Arc<ContentSet> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
    assert!(set.is_some(), "content loads: {:?}", report.findings);
    Arc::new(set.unwrap())
}

fn scenario_host() -> SimHost {
    let content = repository_content();
    let scenario = content.scenario.clone().expect("scenario");
    let start = CalendarDate {
        year: scenario.start_year,
        month: scenario.start_month,
        day: scenario.start_day,
    }
    .to_date()
    .unwrap();
    SimHost::new_with_content(
        CampaignConfig {
            name: scenario.name,
            seed: 7,
            start_date: start,
        },
        content,
    )
}

/// Every array entry is reachable by its id key and equal to it, and the
/// map has no other keys.
fn assert_keyed(view: &Map, array: &str, keyed: &str) {
    let records = view[array].clone().cast::<Array>();
    let by_id = view[keyed].clone().cast::<Map>();
    assert!(!records.is_empty(), "{array} is populated by the scenario");
    assert_eq!(by_id.len(), records.len(), "{keyed} has one key per record");
    for record in records {
        let record = record.cast::<Map>();
        let id = record["id"].as_int().unwrap().to_string();
        let found = by_id
            .get(id.as_str())
            .unwrap_or_else(|| panic!("{keyed} reaches id {id}"))
            .clone()
            .cast::<Map>();
        assert_eq!(
            found
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<Vec<_>>(),
            record
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<Vec<_>>(),
            "{keyed}[{id}] is the same record as the array entry"
        );
    }
}

#[test]
fn keyed_maps_mirror_the_ordered_arrays() {
    let mut host = scenario_host();
    host.advance_days(3);
    let view = context_value(host.world_mut());
    assert_keyed(&view, "provinces", "province_by_id");
    assert_keyed(&view, "organisations", "organisation_by_id");
    assert_keyed(&view, "characters", "character_by_id");
}

/// Opinions are published in the keyed shape alone, so this checks the key
/// against the derived fact rather than against a second published shape:
/// there is one entry per ordered pair of the living, each naming the
/// regard `from` holds of `to`.
#[test]
fn opinion_pairs_are_keyed_from_then_to() {
    let mut host = scenario_host();
    host.advance_days(3);
    let view = context_value(host.world_mut());
    assert!(
        !view.contains_key("opinions"),
        "only the keyed shape is published"
    );
    let by_pair = view["opinion_by_pair"].clone().cast::<Map>();
    assert!(!by_pair.is_empty());

    let living = view["characters"]
        .clone()
        .cast::<Array>()
        .into_iter()
        .map(|character| character.cast::<Map>())
        .filter(|character| character["alive"].as_bool().unwrap())
        .map(|character| character["id"].as_int().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        by_pair.len(),
        living.len() * (living.len() - 1),
        "one key per ordered pair of the living"
    );
    for from in &living {
        for to in &living {
            if from == to {
                continue;
            }
            let key = format!("{from}:{to}");
            assert!(
                by_pair.get(key.as_str()).is_some_and(|v| v.is_int()),
                "opinion_by_pair[{key}] names the regard {from} holds of {to}"
            );
        }
    }
}

#[test]
fn keyed_maps_are_empty_without_a_world() {
    let mut host = SimHost::new(CampaignConfig {
        name: "empty".to_owned(),
        seed: 1,
        start_date: aeon_core::calendar::GameDate::from_days(0),
    });
    let view = context_value(host.world_mut());
    for keyed in ["province_by_id", "organisation_by_id", "character_by_id"] {
        assert!(view[keyed].is_map(), "{keyed} is always a map");
        assert!(view[keyed].clone().cast::<Map>().is_empty());
    }
}
