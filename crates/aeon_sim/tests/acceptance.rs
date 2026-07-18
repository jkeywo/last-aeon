//! End-to-end acceptance: a scripted player playthrough on the real
//! authored scenario, replayed from a mid-run snapshot through its command
//! log to an identical final state. This is the executable form of the
//! deterministic-seed-and-command-replay guarantee applied to a full
//! campaign with real player decisions.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSet, load_content};
use aeon_sim::persistence;
use aeon_sim::{
    CampaignConfig, CharacterId, JobTarget, OrgId, PlayerCommand, PoliticsIndex, SimHost,
};

fn repository_content() -> Arc<ContentSet> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (set, report) = load_content(&sources);
    assert!(set.is_some(), "content loads: {:?}", report.findings);
    Arc::new(set.unwrap())
}

fn scenario_host(content: Arc<ContentSet>, seed: u64) -> SimHost {
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
            seed,
            start_date: start,
        },
        content,
    )
}

fn key(text: &str) -> ContentKey {
    ContentKey::new(text).unwrap()
}

fn char_id(h: &mut SimHost, name: &str) -> CharacterId {
    h.world_mut().resource::<PoliticsIndex>().character_keys[&key(name)]
}

fn org_id(h: &mut SimHost, name: &str) -> OrgId {
    h.world_mut().resource::<PoliticsIndex>().org_keys[&key(name)]
}

/// Plays a fixed sequence of player decisions for House Harrow across
/// several years: routine administration, courting a rival, currying
/// favour with the Sanctora, mustering a levy, and sending the heir on a
/// tour. Returns the finished host.
fn scripted_playthrough(content: Arc<ContentSet>, seed: u64) -> SimHost {
    let mut h = scenario_host(content, seed);
    let edrun = char_id(&mut h, "edrun-harrow"); // the player's head
    let kessarin = char_id(&mut h, "kessarin-harrow"); // spouse (delegate)
    let veyrin = org_id(&mut h, "veyrin"); // liege great house

    // The head courts the liege while the spouse manages the estates.
    h.submit(PlayerCommand::StartJob {
        job: key("court-a-rival-house"),
        leader: edrun,
        target: JobTarget::Org(veyrin),
    })
    .unwrap();
    h.submit(PlayerCommand::StartJob {
        job: key("manage-estates"),
        leader: kessarin,
        target: JobTarget::None,
    })
    .unwrap();
    h.advance_days(120);

    // The head curries Sanctora favour, then musters a levy.
    h.submit(PlayerCommand::StartJob {
        job: key("curry-favour-with-the-sanctora"),
        leader: edrun,
        target: JobTarget::None,
    })
    .unwrap();
    h.advance_days(120);

    h.submit(PlayerCommand::StartJob {
        job: key("muster-the-levies"),
        leader: edrun,
        target: JobTarget::None,
    })
    .unwrap();
    h.advance_days(200);

    // Send the daughter to tour a holding on the far side of the planet.
    let senna = char_id(&mut h, "senna-harrow");
    let tsarovka = h.world_mut().resource::<aeon_sim::MapIndex>().province_keys[&key("tsarovka")];
    h.submit(PlayerCommand::Travel {
        character: senna,
        destination: tsarovka,
    })
    .unwrap();
    h.advance_days(400);

    h
}

#[test]
fn a_scripted_campaign_is_deterministic() {
    let content = repository_content();
    let a = scripted_playthrough(content.clone(), 7);
    let b = scripted_playthrough(content, 7);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn a_scripted_campaign_replays_from_a_snapshot_through_its_log() {
    let content = repository_content();

    // Original timeline: play, snapshot mid-run, keep playing to the end.
    let mut original = scenario_host(content.clone(), 99);
    let edrun = char_id(&mut original, "edrun-harrow");
    let veyrin = org_id(&mut original, "veyrin");

    original
        .submit(PlayerCommand::StartJob {
            job: key("court-a-rival-house"),
            leader: edrun,
            target: JobTarget::Org(veyrin),
        })
        .unwrap();
    original.advance_days(300);

    // Snapshot here, then continue with more decisions.
    let mid_snapshot = original.snapshot();
    let snapshot_date = original.date();

    original
        .submit(PlayerCommand::StartJob {
            job: key("muster-the-levies"),
            leader: edrun,
            target: JobTarget::None,
        })
        .unwrap();
    original.advance_days(150);
    original
        .submit(PlayerCommand::StartJob {
            job: key("curry-favour-with-the-sanctora"),
            leader: edrun,
            target: JobTarget::None,
        })
        .unwrap();
    original.advance_days(300);
    let final_hash = original.state_hash();
    let final_date = original.date();

    // Persist the applied-command log as JSONL, exactly as the game saves.
    let mut log_bytes = Vec::new();
    persistence::write_command_log(&mut log_bytes, &original.applied_commands()).unwrap();

    // Replay: restore the snapshot against the same content, feed the
    // logged commands issued after the snapshot, and advance to the end.
    let mut replayed = SimHost::restore_with_content(mid_snapshot, content).unwrap();
    let log = persistence::read_command_log(log_bytes.as_slice()).unwrap();
    for envelope in log {
        if envelope.day > snapshot_date {
            replayed.submit_recorded(envelope).unwrap();
        }
    }
    let remaining = replayed.date().days_until(final_date);
    replayed.advance_days(remaining as u32);

    assert_eq!(
        replayed.state_hash(),
        final_hash,
        "replay from the snapshot reproduced the final campaign state"
    );
}
