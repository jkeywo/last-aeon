//! The performance and state-hash probe behind the faster-days work.
//!
//! Ignored by default: it runs the authored First Reign scenario for ten
//! campaign years, which takes minutes on the pre-optimisation build. Run it
//! with
//!
//! ```text
//! cargo test -p aeon_sim --test perf_probe -- --ignored --nocapture
//! ```
//!
//! It prints two things:
//!
//! 1. The campaign-state hash at fixed campaign days with the content hash
//!    cleared. Two builds that agree on these hashes behave identically on
//!    the scenario, whatever happened to the authored content bytes; a
//!    build that differs changed behaviour, not just content.
//! 2. At campaign year 10, the wall-clock cost of one day: the whole
//!    `advance_one_day`, then each daily and settled-day system run on its
//!    own against the same world, then `situations::evaluate` timed alone.
//!
//! Per-system timing has no hook in the clock. The probe obtains it by
//! taking the systems from a fresh, never-run [`AeonSimPlugin`] app, in the
//! executable order the live schedule reports, and running each one
//! against the probed world in that order. The systems are therefore the
//! production systems, and the sum of the rows is one production day minus
//! the schedule's own overhead.

use std::sync::Arc;
use std::time::{Duration, Instant};

use aeon_core::calendar::CalendarDate;
use aeon_core::hash::StateHash;
use aeon_data::{ContentSet, load_content};
use aeon_sim::clock::{CampaignClock, DailyTick, SettledDay, advance_one_day};
use aeon_sim::snapshot::{capture_state, hash_state};
use aeon_sim::{AeonSimPlugin, CampaignConfig, SimHost};
use bevy::app::App;
use bevy::ecs::schedule::{ScheduleLabel, Schedules};
use bevy::ecs::system::System;
use bevy::prelude::World;

/// The acceptance seed, so the probe and `aeon accept` walk the same campaign.
const SEED: u64 = 0xA301;

/// Campaign days at which the content-hash-cleared state hash is printed.
const HASH_DAYS: [u32; 6] = [8, 130, 200, 400, 1200, 3600];

/// The first day of campaign year 10, which is where the timing runs.
const YEAR_TEN: u32 = 3600;

/// Days timed for the whole-day average; none of them is a month start.
const TIMED_DAYS: u32 = 5;

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

fn scenario_host(content: Arc<ContentSet>) -> SimHost {
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
            seed: SEED,
            start_date: start,
        },
        content,
    )
}

/// The campaign-state hash with the content hash cleared.
fn cleared_hash(world: &World) -> StateHash {
    let mut state = capture_state(world);
    state.content_hash = None;
    hash_state(&state)
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// The system names of one schedule in the order the live executor runs them.
fn executable_order(world: &World, label: impl ScheduleLabel) -> Vec<String> {
    world
        .resource::<Schedules>()
        .get(label)
        .expect("schedule installed")
        .systems()
        .expect("schedule has run")
        .map(|(_, system)| system.name().as_string())
        .collect()
}

/// Runs each system of `label` on its own against `world`, in `order`, and
/// returns the wall-clock cost of each.
fn time_systems(
    fresh: &mut Schedules,
    world: &mut World,
    label: impl ScheduleLabel,
    order: &[String],
) -> Vec<(String, Duration)> {
    let graph = fresh
        .get_mut(label)
        .expect("schedule installed")
        .graph_mut();
    let keys: Vec<_> = graph
        .systems
        .iter()
        .map(|(key, system, conditions)| {
            assert!(
                conditions.is_empty(),
                "the probe runs systems unconditionally; {} has run conditions",
                system.name()
            );
            (key, system.name().as_string())
        })
        .collect();
    order
        .iter()
        .map(|name| {
            let (key, _) = keys
                .iter()
                .find(|(_, candidate)| candidate == name)
                .unwrap_or_else(|| panic!("fresh app registers {name}"));
            let system = graph.systems.get_mut(*key).expect("system still in graph");
            system.initialize(world);
            let started = Instant::now();
            system.run((), world).expect("system runs");
            (name.clone(), started.elapsed())
        })
        .collect()
}

#[test]
#[ignore = "minutes-long performance probe; run with --ignored --nocapture"]
fn perf_probe() {
    let content = repository_content();
    let cores = std::thread::available_parallelism().map_or(0, |n| n.get());
    println!(
        "probe: seed {SEED:#x}, {cores} cores, {} build",
        build_profile()
    );

    // 1. State hashes with the content hash cleared. With
    // PERF_PROBE_SNAPSHOT naming a file, the year-10 snapshot is read from
    // it when it exists (skipping the hashes, for timing-only iteration) and
    // written to it after the first full run.
    let cache = std::env::var_os("PERF_PROBE_SNAPSHOT").map(std::path::PathBuf::from);
    let cached = cache
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| std::fs::read_to_string(path).expect("cached snapshot readable"))
        .map(|document| {
            let mut snapshot = aeon_sim::persistence::snapshot_from_ron(&document)
                .expect("cached snapshot parses");
            // Timing-only mode: the cached campaign is rebound to the content
            // now on disk so that a content edit can be timed against the
            // same year-10 world. The behavioural proof is the hash run.
            snapshot.state.content_hash = Some(content.content_hash);
            snapshot.state_hash = hash_state(&snapshot.state);
            snapshot
        });
    let snapshot = if let Some(snapshot) = cached {
        println!("hashes skipped: year-10 snapshot read from PERF_PROBE_SNAPSHOT");
        snapshot
    } else {
        let mut host = scenario_host(content.clone());
        let mut day = 0;
        for target in HASH_DAYS {
            host.advance_days(target - day);
            day = target;
            println!(
                "hash day {target:>5} ({}): {}",
                host.date(),
                cleared_hash(host.world_mut())
            );
        }
        assert_eq!(day, YEAR_TEN);
        let snapshot = host.snapshot();
        if let Some(path) = &cache {
            let document =
                aeon_sim::persistence::snapshot_to_ron(&snapshot).expect("snapshot serialises");
            std::fs::write(path, document).expect("cached snapshot written");
        }
        snapshot
    };

    // 2a. Whole-day cost, averaged over a few plain days.
    let mut whole = SimHost::restore_with_content(snapshot.clone(), content.clone()).unwrap();
    let mut days = Vec::new();
    for _ in 0..TIMED_DAYS {
        let started = Instant::now();
        advance_one_day(whole.world_mut());
        days.push(started.elapsed());
    }
    let total: Duration = days.iter().sum();
    println!(
        "year-10 day: {:.1} ms average over {TIMED_DAYS} days ({})",
        millis(total) / f64::from(TIMED_DAYS),
        days.iter()
            .map(|d| format!("{:.1}", millis(*d)))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // 2b. Each daily and settled-day system on its own, in executable order.
    let daily_order = executable_order(whole.world_mut(), DailyTick);
    let settled_order = executable_order(whole.world_mut(), SettledDay);
    let mut fresh_app = App::new();
    fresh_app.add_plugins(AeonSimPlugin);
    let mut fresh = fresh_app
        .world_mut()
        .remove_resource::<Schedules>()
        .expect("fresh app has schedules");
    let mut per_system = SimHost::restore_with_content(snapshot.clone(), content.clone()).unwrap();
    let world = per_system.world_mut();
    world.resource_mut::<CampaignClock>().date = world.resource::<CampaignClock>().date.add_days(1);
    assert!(
        !world.resource::<CampaignClock>().date.is_month_start(),
        "the per-system day must be a plain day"
    );
    let mut rows = time_systems(&mut fresh, world, DailyTick, &daily_order);
    rows.extend(time_systems(&mut fresh, world, SettledDay, &settled_order));
    let sum: Duration = rows.iter().map(|(_, d)| *d).sum();
    println!(
        "year-10 per-system (ms, executable order, sum {:.1}):",
        millis(sum)
    );
    for (name, duration) in &rows {
        println!("  {:>8.2}  {name}", millis(*duration));
    }

    // 2c. The world view: how long the shared value takes to build, and how
    // long one deep clone of it takes — the clone every script call's
    // context carries — against the number of calls a settled day makes.
    let mut view_host = SimHost::restore_with_content(snapshot.clone(), content.clone()).unwrap();
    advance_one_day(view_host.world_mut());
    let world = view_host.world_mut();
    let started = Instant::now();
    let view = aeon_sim::script_world::context_value(world);
    let build = started.elapsed();
    let started = Instant::now();
    let copy = view.clone();
    let clone = started.elapsed();
    drop(copy);
    let state = world.resource::<aeon_sim::situations::SituationState>();
    let opinions = view["opinions"].clone().cast::<rhai::Array>().len();
    println!(
        "year-10 world view: build {:.1} ms, one clone {:.2} ms, {} opinion pairs, {} live lifecycles",
        millis(build),
        millis(clone),
        opinions,
        state.active.len()
    );

    // 2c. Situation evaluation timed alone on a settled day. Re-evaluating
    // a settled day makes the same trigger, resolution, and projection
    // calls the day's own evaluation made, so this is the pure cost of
    // `situations::evaluate`.
    let mut alone = SimHost::restore_with_content(snapshot, content).unwrap();
    advance_one_day(alone.world_mut());
    let mut samples = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        aeon_sim::situations::evaluate(alone.world_mut());
        samples.push(started.elapsed());
    }
    println!(
        "year-10 situations::evaluate alone: {}",
        samples
            .iter()
            .map(|d| format!("{:.1} ms", millis(*d)))
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}
