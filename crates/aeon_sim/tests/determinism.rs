//! End-to-end determinism guarantees.
//!
//! These tests are the executable form of the deterministic-seed-and-
//! command-replay decision: a campaign seed, authored data, and an ordered
//! command sequence fully determine campaign state, snapshots restore
//! exactly, and a recorded log replays from a snapshot to an identical
//! state hash.

use aeon_core::calendar::{CalendarDate, GameDate};
use aeon_sim::persistence;
use aeon_sim::{CampaignConfig, PlayerCommand, SimHost};

fn config(seed: u64) -> CampaignConfig {
    CampaignConfig {
        name: "Determinism Trial".to_owned(),
        seed,
        start_date: CalendarDate {
            year: 411,
            month: 1,
            day: 1,
        }
        .to_date()
        .unwrap(),
    }
}

/// Runs a fixed scripted campaign: rename at two points, advance 400 days
/// (crossing a year boundary).
fn scripted_run(seed: u64) -> SimHost {
    let mut host = SimHost::new(config(seed));
    host.advance_days(10);
    host.submit(PlayerCommand::RenameCampaign {
        name: "After The First Decade".to_owned(),
    })
    .unwrap();
    host.advance_days(200);
    host.submit(PlayerCommand::Noop).unwrap();
    host.submit(PlayerCommand::RenameCampaign {
        name: "Deep Into The Year".to_owned(),
    })
    .unwrap();
    host.advance_days(190);
    host
}

#[test]
fn identical_runs_produce_identical_hashes() {
    let a = scripted_run(7);
    let b = scripted_run(7);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.date(), b.date());
    let calendar = a.date().calendar();
    assert_eq!((calendar.year, calendar.month), (412, 2));
    assert_eq!(a.campaign_name(), "Deep Into The Year");
}

#[test]
fn different_seeds_diverge() {
    let a = scripted_run(7);
    let b = scripted_run(8);
    assert_ne!(a.state_hash(), b.state_hash());
}

#[test]
fn command_order_within_a_day_matters() {
    let mut ab = SimHost::new(config(1));
    ab.submit(PlayerCommand::RenameCampaign {
        name: "Alpha".to_owned(),
    })
    .unwrap();
    ab.submit(PlayerCommand::RenameCampaign {
        name: "Beta".to_owned(),
    })
    .unwrap();
    ab.advance_days(1);

    let mut ba = SimHost::new(config(1));
    ba.submit(PlayerCommand::RenameCampaign {
        name: "Beta".to_owned(),
    })
    .unwrap();
    ba.submit(PlayerCommand::RenameCampaign {
        name: "Alpha".to_owned(),
    })
    .unwrap();
    ba.advance_days(1);

    assert_eq!(ab.campaign_name(), "Beta");
    assert_eq!(ba.campaign_name(), "Alpha");
    assert_ne!(ab.state_hash(), ba.state_hash());
}

#[test]
fn snapshot_restores_to_an_identical_campaign() {
    let original = scripted_run(21);
    let hash_before = original.state_hash();
    let snapshot = original.snapshot();

    let document = persistence::snapshot_to_ron(&snapshot).unwrap();
    let parsed = persistence::snapshot_from_ron(&document).unwrap();
    let restored = SimHost::restore(parsed).unwrap();

    assert_eq!(restored.state_hash(), hash_before);
    assert_eq!(restored.date(), original.date());
    assert_eq!(restored.campaign_name(), original.campaign_name());
}

#[test]
fn restored_campaigns_continue_identically() {
    let mut original = scripted_run(33);
    let mut restored = SimHost::restore(original.snapshot()).unwrap();

    for host in [&mut original, &mut restored] {
        host.submit(PlayerCommand::RenameCampaign {
            name: "Continued".to_owned(),
        })
        .unwrap();
        host.advance_days(45);
    }
    assert_eq!(original.state_hash(), restored.state_hash());
}

#[test]
fn command_log_replays_from_a_snapshot_to_the_original_hash() {
    // Original timeline: snapshot mid-run, then further commands and days.
    let mut original = SimHost::new(config(99));
    original.advance_days(20);
    original
        .submit(PlayerCommand::RenameCampaign {
            name: "Before The Snapshot".to_owned(),
        })
        .unwrap();
    original.advance_days(20);
    let mid_snapshot = original.snapshot();
    let snapshot_date = original.date();

    original.submit(PlayerCommand::Noop).unwrap();
    original.advance_days(15);
    original
        .submit(PlayerCommand::RenameCampaign {
            name: "After The Snapshot".to_owned(),
        })
        .unwrap();
    original.advance_days(25);
    let final_hash = original.state_hash();
    let final_date = original.date();

    // Persist the full command log as JSONL, as the game would on disk.
    let mut log_bytes = Vec::new();
    persistence::write_command_log(&mut log_bytes, &original.applied_commands()).unwrap();

    // Replay: restore the snapshot, feed logged commands after its date,
    // advance to the original's final date.
    let mut replayed = SimHost::restore(mid_snapshot).unwrap();
    let log = persistence::read_command_log(log_bytes.as_slice()).unwrap();
    for envelope in log {
        if envelope.day > snapshot_date {
            replayed.submit_recorded(envelope).unwrap();
        }
    }
    let remaining = replayed.date().days_until(final_date);
    replayed.advance_days(remaining as u32);

    assert_eq!(replayed.state_hash(), final_hash);
    assert_eq!(replayed.campaign_name(), "After The Snapshot");
}

#[test]
fn tampered_snapshots_are_rejected() {
    let host = SimHost::new(config(5));
    let mut snapshot = host.snapshot();
    snapshot.state.name = "Edited By Hand".to_owned();
    assert!(SimHost::restore(snapshot).is_err());
}

#[test]
fn future_format_versions_are_rejected() {
    let host = SimHost::new(config(5));
    let mut snapshot = host.snapshot();
    snapshot.format_version += 1;
    assert!(SimHost::restore(snapshot).is_err());
}

#[test]
fn monthly_and_yearly_pulses_fire_on_boundaries() {
    use aeon_sim::{MonthlyPulse, YearlyPulse};
    use bevy::ecs::schedule::Schedules;
    use bevy::prelude::Resource;

    #[derive(Resource, Default)]
    struct PulseCounts {
        monthly: u32,
        yearly: u32,
    }

    let mut host = SimHost::new(config(3));
    let world = host.world_mut();
    world.insert_resource(PulseCounts::default());
    let mut schedules = world.resource_mut::<Schedules>();
    schedules.add_systems(
        MonthlyPulse,
        |mut counts: bevy::prelude::ResMut<PulseCounts>| {
            counts.monthly += 1;
        },
    );
    schedules.add_systems(
        YearlyPulse,
        |mut counts: bevy::prelude::ResMut<PulseCounts>| {
            counts.yearly += 1;
        },
    );

    // Start date is 411.01.01; advancing 360 days crosses eleven month
    // starts within year 411 plus the 412.01.01 boundary, which is both a
    // month and a year start.
    host.advance_days(360);

    let world = host.world_mut();
    let counts = world.resource::<PulseCounts>();
    assert_eq!(counts.monthly, 12);
    assert_eq!(counts.yearly, 1);
}

#[test]
fn replay_rejects_stale_or_reordered_envelopes() {
    let mut host = SimHost::new(config(2));
    let envelope = host.submit(PlayerCommand::Noop).unwrap();
    host.advance_days(5);

    // Day already passed.
    assert!(host.submit_recorded(envelope.clone()).is_err());

    // Sequence regression.
    let mut future = envelope;
    future.day = GameDate::from_days(host.date().days_since_epoch() + 10);
    future.seq = 0;
    assert!(host.submit_recorded(future).is_err());
}
