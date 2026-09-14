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

// A house that has wronged another, and a plan to answer the grievance
// in blood rather than court.
const SCHEME_FIXTURE: &str = r#"
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
    provinces: ["beta"], wealth: 400, manpower: 2000, supplies: 400, legitimacy: 60,
});
define_character(#{ id: "aron-ash", gender: "male", birth_year: 370, organisation: "ash",
    skills: #{ command: 8, diplomacy: 6, intrigue: 4, stewardship: 7 } });
define_character(#{ id: "bela-birch", gender: "female", birth_year: 372, organisation: "birch",
    skills: #{ command: 6, diplomacy: 9, intrigue: 14, stewardship: 5 } });

// Birch has wronged Ash, and Ash resents it — the pressure a house
// answers.
define_obligation(#{
    id: "birch-wronged-ash", kind: "grievance",
    debtor: "birch", creditor: "ash", weight: 80,
    origin: "an old and bitter injury",
});

// A standing assignment aimed at an organisation, so the grievance
// scores a pressure the vendetta can answer. The scorer names a pressure
// only when some assignment could serve it.
define_assignment(#{
    id: "court", ai_intent: "standing", category: "consequential",
    duration_days: 30, skill: "diplomacy", difficulty: 5, target: "organisation",
    results: #{ success: #{ weight: 900 }, failure: #{ weight: 100 } },
});

// The knife an assassination lays on its mark.
define_assignment(#{
    id: "assassinate", category: "consequential",
    duration_days: 30, skill: "intrigue", difficulty: 4, target: "character",
    risks: ["scandal"], ai_available: false,
    requires: #{ target_house: "other" },
    results: #{
        success: #{ weight: 900, effect_fn: "struck" },
        failure: #{ weight: 100 },
    },
});
fn struck(ctx) { [#{ kind: "condition", target: "target", tag: "death" }] }

// A vendetta: answer the grievance by removing the rival's head. The one
// standing plan here, so the house that resents another reaches for it.
define_plan(#{
    id: "vendetta",
    goal: "standing",
    target: "organisation",
    max_days: 360,
    methods: [
        #{ id: "in-the-dark",
           steps: [ #{ id: "remove", start: "assassinate", target: "target-head" } ] },
    ],
});
"#;

fn scheme_host(seed: u64) -> SimHost {
    let (set, report) = load_content(
        &[ContentSource {
            path: "fixture.rhai".to_owned(),
            source: SCHEME_FIXTURE.to_owned(),
        }],
        &aeon_data::StringTable::blank(),
    );
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    SimHost::new_with_content(
        CampaignConfig {
            name: "Vendetta Trial".to_owned(),
            seed,
            start_date: CalendarDate {
                year: 411,
                month: 1,
                day: 1,
            }
            .to_date()
            .unwrap(),
        },
        Arc::new(set.unwrap()),
    )
}

#[test]
fn a_house_weaves_the_knife_into_its_ambition() {
    let mut h = scheme_host(94);
    let aron = character(&mut h, "aron-ash"); // the rival's head, the mark
    let bela = character(&mut h, "bela-birch"); // the schemer

    // Left to run, Birch answers the grievance from the shadows: it takes
    // up the vendetta and aims the knife at Ash's head.
    let mut aimed = false;
    for _ in 0..400 {
        h.advance_days(1);
        let index = h
            .world_mut()
            .resource::<aeon_sim::AssignmentsIndex>()
            .clone();
        for entity in index.assignments.values() {
            if let Some(active) = h
                .world_mut()
                .get::<aeon_sim::assignments::ActiveAssignment>(*entity)
                && active.def.as_str() == "assassinate"
            {
                assert_eq!(active.leader, bela, "Birch's head leads the scheme");
                assert_eq!(
                    active.target,
                    aeon_sim::AssignmentTarget::Character(aron),
                    "and the knife is aimed at the rival's head"
                );
                aimed = true;
            }
        }
        if aimed {
            break;
        }
    }
    assert!(
        aimed,
        "a house's ambition can now include removing the person who blocks it"
    );
}

// A house that wants the Consulship and will not wait on a natural death:
// the sitting Consul belongs to the Sanctora, and Birch is strong enough
// to make the vacancy it means to win.
const SEAT_FIXTURE: &str = r#"
define_scenario(#{
    id: "fixture", start_year: 411, start_month: 1, start_day: 1,
    player_house: "ash",
});
define_name_pool(#{ id: "names", male: ["Bram"], female: ["Yeva"] });

define_body(#{ id: "world", kind: "planet", radius_km: 6000 });
define_province(#{ id: "alpha", body: "world", latitude_mdeg: 0, longitude_mdeg: 0 });
define_province(#{ id: "beta", body: "world", latitude_mdeg: 10000, longitude_mdeg: 10000 });
define_province(#{ id: "gamma", body: "world", latitude_mdeg: -10000, longitude_mdeg: -10000 });

define_house(#{
    id: "ash", tier: "great", head: "aron-ash", color: [200, 60, 60],
    provinces: ["alpha"], wealth: 500, manpower: 5000, supplies: 800, legitimacy: 60,
});
define_house(#{
    id: "birch", tier: "great", head: "bela-birch", color: [60, 60, 200],
    provinces: ["beta"], wealth: 400, manpower: 2000, supplies: 400, legitimacy: 70,
});
define_organisation(#{
    id: "sanctora-imperim", kind: "sanctora-imperim",
    head: "consul-vex", color: [212, 175, 55], provinces: ["gamma"],
});
define_title(#{ id: "consulate", kind: "consul", holder_character: "consul-vex" });

define_character(#{ id: "aron-ash", gender: "male", birth_year: 370, organisation: "ash",
    skills: #{ command: 8, diplomacy: 6, intrigue: 4, stewardship: 7 } });
define_character(#{ id: "bela-birch", gender: "female", birth_year: 372, organisation: "birch",
    skills: #{ command: 6, diplomacy: 9, intrigue: 14, stewardship: 5 } });
define_character(#{ id: "consul-vex", gender: "male", birth_year: 360,
    organisation: "sanctora-imperim",
    skills: #{ command: 3, diplomacy: 12, intrigue: 6, stewardship: 9 } });
define_character(#{ id: "prefect-hale", gender: "male", birth_year: 375,
    organisation: "sanctora-imperim",
    skills: #{ command: 5, diplomacy: 8, intrigue: 5, stewardship: 8 } });

// The deniable pressure needs some assignment that could serve it before
// the scorer names it; the plan then chooses the knife.
define_assignment(#{
    id: "quiet-work", ai_intent: "subvert", category: "consequential",
    duration_days: 30, skill: "intrigue", difficulty: 5, target: "organisation",
    covert: true, ai_available: false,
    results: #{ success: #{ weight: 900 }, failure: #{ weight: 100 } },
});
// The knife, sure to land, so the trial is about reach and consequence
// rather than the roll.
define_assignment(#{
    id: "assassinate", category: "consequential",
    duration_days: 30, skill: "intrigue", difficulty: 1, target: "character",
    risks: ["scandal"], ai_available: false,
    requires: #{ target_house: "other" },
    results: #{
        success: #{ weight: 9999, effect_fn: "struck" },
        failure: #{ weight: 1 },
    },
});
fn struck(ctx) { [#{ kind: "condition", target: "target", tag: "death" }] }

define_plan(#{
    id: "make-the-vacancy",
    goal: "subvert",
    target: "organisation",
    covert: true,
    max_days: 360,
    abandon_when: #{ target_head_lacks_title: "consul" },
    methods: [
        #{ id: "from-the-shadows",
           requires: #{ target_head_holds_title: "consul" },
           steps: [ #{ id: "remove", start: "assassinate", target: "target-head" } ] },
    ],
});
define_goal(#{
    id: "unseat-the-consul",
    favours: ["subvert"],
    favour_bonus: 60,
    target: "organisation",
    target_selector: #{ kind: "consul-house" },
    covert: true,
    trigger: #{ is_vassal: false, min_legitimacy: 55 },
    set_aside_when: #{ target_head_lacks_title: "consul" },
    max_days: 720,
    cooldown_days: 1440,
});
"#;

fn seat_host(seed: u64) -> SimHost {
    let (set, report) = load_content(
        &[ContentSource {
            path: "fixture.rhai".to_owned(),
            source: SEAT_FIXTURE.to_owned(),
        }],
        &aeon_data::StringTable::blank(),
    );
    assert!(!report.has_errors(), "findings: {:?}", report.findings);
    SimHost::new_with_content(
        CampaignConfig {
            name: "Seat Trial".to_owned(),
            seed,
            start_date: CalendarDate {
                year: 411,
                month: 1,
                day: 1,
            }
            .to_date()
            .unwrap(),
        },
        Arc::new(set.unwrap()),
    )
}

fn org(h: &mut SimHost, name: &str) -> aeon_sim::OrgId {
    h.world_mut().resource::<aeon_sim::PoliticsIndex>().org_keys[&key(name)]
}

#[test]
fn a_house_set_on_the_consulship_makes_the_vacancy() {
    let mut h = seat_host(17);
    let vex = character(&mut h, "consul-vex");
    let bela = character(&mut h, "bela-birch");
    let birch = org(&mut h, "birch");
    let sanctora = org(&mut h, "sanctora-imperim");

    // Left to run, Birch forms the ambition, aimed at the Consul's house,
    // and its campaign aims the knife at the Consul in person.
    let mut aimed = false;
    for _ in 0..720 {
        h.advance_days(1);
        let index = h
            .world_mut()
            .resource::<aeon_sim::AssignmentsIndex>()
            .clone();
        for entity in index.assignments.values() {
            if let Some(active) = h
                .world_mut()
                .get::<aeon_sim::assignments::ActiveAssignment>(*entity)
                && active.def.as_str() == "assassinate"
            {
                assert_eq!(active.leader, bela, "Birch's head leads the scheme");
                assert_eq!(
                    active.target,
                    aeon_sim::AssignmentTarget::Character(vex),
                    "and the knife is aimed at the sitting Consul"
                );
                aimed = true;
            }
        }
        if aimed {
            break;
        }
    }
    assert!(aimed, "a house set on the Consulship reaches for the knife");
    let goal = h.world_mut().resource::<aeon_sim::goals::Goals>().clone();
    assert_eq!(
        goal.active[&birch].target,
        aeon_sim::AssignmentTarget::Org(sanctora),
        "the ambition is aimed at the house whose member holds the seat"
    );

    // The knife lands: the Consul is dead, the seat is vacant, the
    // contest to fill it is open, and the ambition has lost its grounds.
    h.advance_days(60);
    let vex_entity = h
        .world_mut()
        .resource::<aeon_sim::PoliticsIndex>()
        .characters[&vex];
    assert!(
        h.world_mut()
            .get::<aeon_sim::CharacterRecord>(vex_entity)
            .is_some_and(|record| !record.alive()),
        "the sitting Consul is dead"
    );
    assert!(
        aeon_sim::access::consul(h.world_mut()).is_none(),
        "the seat is vacant"
    );
    assert!(
        h.world_mut()
            .get_resource::<aeon_sim::politics::ConsulContest>()
            .is_some(),
        "and the contest to fill it is open"
    );
    assert!(
        !h.world_mut()
            .resource::<aeon_sim::goals::Goals>()
            .active
            .contains_key(&birch),
        "the ambition is set aside once the seat has passed from that house"
    );
}

#[test]
fn a_vacant_seat_gives_the_ambition_nothing_to_aim_at() {
    // With no Consul, the selector names nobody and the ambition is never
    // adopted; the become-consul ambition is the one for a vacancy.
    let mut h = seat_host(17);
    let vex = character(&mut h, "consul-vex");
    let birch = org(&mut h, "birch");
    let today = h.date();
    aeon_sim::politics::process_death(h.world_mut(), vex, today);
    h.advance_days(400);
    assert!(
        !h.world_mut()
            .resource::<aeon_sim::goals::Goals>()
            .active
            .contains_key(&birch),
        "a vacant seat is nothing to make vacant"
    );
}
