//! Saves are bound to the exact authored content that produced them.

use std::sync::Arc;

use aeon_core::calendar::GameDate;
use aeon_data::{ContentSource, load_content};
use aeon_sim::{CampaignConfig, SimHost, SnapshotError};

fn content(source_text: &str) -> Arc<aeon_data::ContentSet> {
    let (set, report) = load_content(&[ContentSource {
        path: "test.rhai".to_owned(),
        source: source_text.to_owned(),
    }], &aeon_data::StringTable::blank());
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    Arc::new(set.unwrap())
}

fn config() -> CampaignConfig {
    CampaignConfig {
        name: "Content Binding".to_owned(),
        seed: 11,
        start_date: GameDate::EPOCH,
    }
}

// The two differ in a mechanical fact, not a display name: prose lives in
// the string table now, so two content sets that differed only by what
// they were called would hash identically — and should.
const WORLD_A: &str =
    r#"define_body(#{ id: "world", kind: "planet", radius_km: 6000 });"#;
const WORLD_B: &str =
    r#"define_body(#{ id: "world", kind: "planet", radius_km: 7000 });"#;

#[test]
fn snapshots_restore_only_with_matching_content() {
    let content_a = content(WORLD_A);
    let mut host = SimHost::new_with_content(config(), content_a.clone());
    host.advance_days(30);
    let snapshot = host.snapshot();

    // Same content restores and continues to the same hash.
    let restored = SimHost::restore_with_content(snapshot.clone(), content_a).unwrap();
    assert_eq!(restored.state_hash(), host.state_hash());

    // Different content is refused.
    let content_b = content(WORLD_B);
    assert!(matches!(
        SimHost::restore_with_content(snapshot.clone(), content_b),
        Err(SnapshotError::ContentMismatch { .. })
    ));

    // Restoring without content at all is refused.
    assert!(matches!(
        SimHost::restore(snapshot),
        Err(SnapshotError::ContentRequired { .. })
    ));
}

#[test]
fn content_free_snapshots_refuse_content_on_restore() {
    let mut host = SimHost::new(config());
    host.advance_days(5);
    let snapshot = host.snapshot();

    assert!(SimHost::restore(snapshot.clone()).is_ok());
    assert!(matches!(
        SimHost::restore_with_content(snapshot, content(WORLD_A)),
        Err(SnapshotError::ContentNotExpected)
    ));
}

const SMALL_SYSTEM: &str = r#"
define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_body(#{ id: "moon", kind: "moon", radius_km: 1500,
               parent: "world", orbit_radius_mm: 300, orbit_days: 20 });
define_province(#{ id: "alpha", body: "world",
                   latitude_mdeg: 10000, longitude_mdeg: 20000 });
define_province(#{ id: "beta", body: "moon",
                   latitude_mdeg: -5000, longitude_mdeg: 90000 });
"#;

#[test]
fn map_spawns_with_deterministic_stable_ids() {
    let content = content(SMALL_SYSTEM);
    let mut a = SimHost::new_with_content(config(), content.clone());
    let mut b = SimHost::new_with_content(config(), content);

    for host in [&mut a, &mut b] {
        let world = host.world_mut();
        let index = world.resource::<aeon_sim::MapIndex>().clone();
        assert_eq!(index.bodies.len(), 2);
        assert_eq!(index.provinces.len(), 2);
        // Bodies allocate before provinces, each in content-key order:
        // moon(1), world(2), then alpha(3), beta(4).
        let ids: Vec<u64> = index.body_ids.keys().map(|id| id.raw()).collect();
        assert_eq!(ids, vec![1, 2]);
        let province_ids: Vec<u64> = index.province_ids.keys().map(|id| id.raw()).collect();
        assert_eq!(province_ids, vec![3, 4]);
    }
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn map_survives_snapshot_restore_identically() {
    let content = content(SMALL_SYSTEM);
    let mut host = SimHost::new_with_content(config(), content.clone());
    host.advance_days(10);
    let hash = host.state_hash();
    let snapshot = host.snapshot();
    assert_eq!(snapshot.state.map.bodies.len(), 2);
    assert_eq!(snapshot.state.map.provinces.len(), 2);

    let mut restored = SimHost::restore_with_content(snapshot, content).unwrap();
    assert_eq!(restored.state_hash(), hash);

    let world = restored.world_mut();
    let index = world.resource::<aeon_sim::MapIndex>();
    assert_eq!(index.bodies.len(), 2);
    assert_eq!(index.provinces.len(), 2);
    let record = world
        .get::<aeon_sim::ProvinceRecord>(index.provinces[index.province_ids.keys().next().unwrap()])
        .unwrap();
    assert_eq!(record.key.as_str(), "alpha");
}
