//! Shadows: harm that reaches the character it is aimed at, not only the
//! one who leads the work.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSource, load_content};
use aeon_sim::assignments::CharacterCondition;
use aeon_sim::{
    AssignmentTarget, CampaignConfig, CharacterId, PlayerCommand, PoliticsIndex, SimHost,
};

const FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_province(#{ id: "alpha", body: "world", latitude_mdeg: 0, longitude_mdeg: 0 });
define_province(#{ id: "beta", body: "world", latitude_mdeg: 10000, longitude_mdeg: 10000 });

define_house(#{
    id: "ash", tier: "great", head: "aron-ash", color: [200, 60, 60],
    provinces: ["alpha"], wealth: 500, manpower: 5000, supplies: 800, legitimacy: 60,
});
define_house(#{
    id: "birch", tier: "great", head: "bela-birch", color: [60, 60, 200],
    provinces: ["beta"], wealth: 400, manpower: 2000, supplies: 400, legitimacy: 50,
});
define_character(#{ id: "aron-ash", gender: "male", birth_year: 370, organisation: "ash",
    skills: #{ command: 8, diplomacy: 6, intrigue: 18, stewardship: 7 } });
define_character(#{ id: "bela-birch", gender: "female", birth_year: 372, organisation: "birch",
    skills: #{ command: 6, diplomacy: 9, intrigue: 4, stewardship: 5 } });

// A hand laid on a rival: on success the target is injured, on failure
// the leader answers for it.
define_assignment(#{
    id: "waylay", category: "consequential",
    duration_days: 20, skill: "intrigue", difficulty: 2, target: "character",
    risks: ["capture", "scandal"],
    requires: #{ target_house: "other" },
    results: #{
        success: #{ weight: 999, log: true, effect_fn: "waylaid" },
        failure: #{ weight: 1 },
    },
});
fn waylaid(ctx) { [#{ kind: "condition", target: "target", tag: "injury" }] }
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
            name: "Shadow Trial".to_owned(),
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

fn character(h: &mut SimHost, name: &str) -> CharacterId {
    h.world_mut().resource::<PoliticsIndex>().character_keys[&key(name)]
}

fn injured(h: &mut SimHost, who: CharacterId) -> bool {
    let entity = h.world_mut().resource::<PoliticsIndex>().characters[&who];
    h.world_mut()
        .get::<CharacterCondition>(entity)
        .is_some_and(|c| c.injured_until.is_some())
}

#[test]
fn a_scheme_harms_the_target_not_the_leader() {
    let mut h = host(91);
    let aron = character(&mut h, "aron-ash"); // the schemer
    let bela = character(&mut h, "bela-birch"); // the mark

    // Aron waylays Bela; keep trying until the work lands.
    for _ in 0..6 {
        if injured(&mut h, bela) {
            break;
        }
        let _ = h.submit(PlayerCommand::StartAssignment {
            assignment: key("waylay"),
            leader: aron,
            target: AssignmentTarget::Character(bela),
        });
        h.advance_days(22);
    }

    assert!(
        injured(&mut h, bela),
        "the harm reaches the character it was aimed at"
    );
    assert!(!injured(&mut h, aron), "and not the one who leads the work");
}

#[test]
fn a_harmed_target_is_kept_from_leading() {
    // A condition laid by a scheme uses the same machinery a failed
    // leader's does, so a waylaid character is barred from new work just
    // as an injured leader would be.
    use aeon_sim::assignments::apply_risk;

    let mut h = host(92);
    let bela = character(&mut h, "bela-birch");
    let date = h.world_mut().resource::<aeon_sim::CampaignClock>().date;
    apply_risk(h.world_mut(), bela, aeon_data::model::RiskTag::Injury, date);

    let entity = h.world_mut().resource::<PoliticsIndex>().characters[&bela];
    let condition = *h.world_mut().get::<CharacterCondition>(entity).unwrap();
    assert!(
        !condition.can_lead(date),
        "the injured cannot take up new work"
    );
}

#[test]
fn wrecking_pulls_down_a_building_and_stirs_disorder() {
    use aeon_data::ScriptEffect;
    use aeon_data::model::RiskTag;
    use aeon_sim::MapIndex;
    use aeon_sim::assignments::{AssignmentRoles, apply_effects};
    use aeon_sim::order::{ORDER_START, province_order};
    use aeon_sim::trade::Buildings;

    let mut h = host(93);
    let beta = h.world_mut().resource::<MapIndex>().province_keys[&key("beta")];
    let _ = RiskTag::Injury; // keep the import honest across edits

    // Plant a building on Beta to wreck.
    {
        let entity = h.world_mut().resource::<MapIndex>().provinces[&beta];
        h.world_mut()
            .get_mut::<Buildings>(entity)
            .unwrap()
            .0
            .push(key("nonesuch"));
    }

    // Apply the wreck-and-disorder effects as a resolved province action.
    let roles = AssignmentRoles {
        province: Some(beta),
        ..Default::default()
    };
    apply_effects(
        h.world_mut(),
        &[
            ScriptEffect::Wreck,
            ScriptEffect::Order {
                scope: aeon_data::effect::OrderScope::TargetProvince,
                amount: -80,
            },
        ],
        &roles,
        None,
    );

    let entity = h.world_mut().resource::<MapIndex>().provinces[&beta];
    assert!(
        h.world_mut().get::<Buildings>(entity).unwrap().0.is_empty(),
        "the building is pulled down"
    );
    assert!(
        province_order(h.world_mut(), beta).order < ORDER_START,
        "and the province is stirred toward disorder"
    );
}
