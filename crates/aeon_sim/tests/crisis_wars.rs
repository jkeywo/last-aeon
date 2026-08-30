//! Personal Paramount claims and occurrence-stable formal-war ledgers.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSet, load_content};
use aeon_sim::assignments::{
    AssignmentTarget, resolve_due_assignments, start_assignment, start_assignment_in_war,
    validate_start,
};
use aeon_sim::crisis::{
    ParamountClaimError, ParamountClaims, claimant_war_blocker, declare_claim, has_claim,
    paramountcy, press_claim, realm_province_counts_on, renounce_claim,
};
use aeon_sim::politics::{TitleHolder, TitleRecord, process_death};
use aeon_sim::wars::{
    WarConclusionKind, WarError, WarSideId, adopt_side, can_adopt_side, declare_war,
    negotiate_peace, opposed_in, side_of, war,
};
use aeon_sim::{CampaignClock, CampaignConfig, OrgId, PoliticsIndex, SimHost};

fn repository_content() -> Arc<ContentSet> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (set, report) = load_content(&sources, &aeon_data::StringTable::blank());
    assert!(
        set.is_some(),
        "repository content must load: {:?}",
        report.findings
    );
    Arc::new(set.unwrap())
}

fn scenario_host(seed: u64) -> SimHost {
    let content = repository_content();
    let scenario = content.scenario.clone().expect("scenario defined");
    let start_date = CalendarDate {
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
            start_date,
        },
        content,
    )
}

fn key(text: &str) -> ContentKey {
    ContentKey::new(text).unwrap()
}

fn org(host: &mut SimHost, name: &str) -> OrgId {
    host.world_mut().resource::<PoliticsIndex>().org_keys[&key(name)]
}

fn head(host: &mut SimHost, org: OrgId) -> aeon_sim::CharacterId {
    aeon_sim::access::org_head(host.world_mut(), org).expect("house has a head")
}

#[test]
fn whole_realm_dominance_and_personal_award_use_one_authoritative_path() {
    let mut host = scenario_host(101);
    let veyrin = org(&mut host, "veyrin");
    let draksha = org(&mut host, "draksha");
    let meloch = org(&mut host, "meloch");
    let claimant = head(&mut host, veyrin);
    let (title, body) = paramountcy(host.world_mut()).unwrap();

    let totals = realm_province_counts_on(host.world_mut(), body);
    assert_eq!(totals[&veyrin], 11);
    assert_eq!(totals[&draksha], 9);
    assert_eq!(totals[&meloch], 9);
    assert_eq!(
        aeon_sim::crisis::dominant_claimant(host.world_mut(), body),
        Some(veyrin)
    );

    let base_legitimacy = {
        let world = host.world_mut();
        let entity = world.resource::<PoliticsIndex>().orgs[&veyrin];
        world
            .get::<aeon_sim::OrgResources>(entity)
            .unwrap()
            .legitimacy
    };
    declare_claim(host.world_mut(), title, claimant).unwrap();
    press_claim(host.world_mut(), title, claimant).unwrap();

    let world = host.world_mut();
    let index = world.resource::<PoliticsIndex>();
    assert_eq!(
        world
            .get::<TitleRecord>(index.titles[&title])
            .unwrap()
            .holder,
        TitleHolder::Character(claimant)
    );
    assert_eq!(
        aeon_sim::economy::effective_legitimacy(world, veyrin),
        base_legitimacy + aeon_sim::economy::PARAMOUNT_LEGITIMACY_BONUS
    );
}

#[test]
fn a_tenuous_independent_claim_is_valid_but_a_vassal_claim_is_not() {
    let mut host = scenario_host(102);
    let pell = org(&mut host, "pell");
    let harrow = org(&mut host, "harrow");
    let pell_head = head(&mut host, pell);
    let harrow_head = head(&mut host, harrow);
    let (title, _) = paramountcy(host.world_mut()).unwrap();

    declare_claim(host.world_mut(), title, pell_head).unwrap();
    assert!(has_claim(host.world_mut(), title, pell_head));
    assert_eq!(
        press_claim(host.world_mut(), title, pell_head),
        Err(ParamountClaimError::NotDominant)
    );
    assert_eq!(
        declare_claim(host.world_mut(), title, harrow_head),
        Err(ParamountClaimError::HasLiege)
    );
}

#[test]
fn a_dead_head_loses_their_claim_and_the_heir_does_not_inherit_it() {
    let mut host = scenario_host(103);
    let veyrin = org(&mut host, "veyrin");
    let claimant = head(&mut host, veyrin);
    let (title, _) = paramountcy(host.world_mut()).unwrap();
    declare_claim(host.world_mut(), title, claimant).unwrap();

    let date = host.world_mut().resource::<CampaignClock>().date;
    process_death(host.world_mut(), claimant, date);
    let heir = head(&mut host, veyrin);
    assert_ne!(heir, claimant);
    assert!(!has_claim(host.world_mut(), title, claimant));
    assert!(!has_claim(host.world_mut(), title, heir));
}

#[test]
fn only_a_current_rival_claimant_war_blocks_the_award() {
    let mut host = scenario_host(104);
    let veyrin = org(&mut host, "veyrin");
    let pell = org(&mut host, "pell");
    let veyrin_head = head(&mut host, veyrin);
    let pell_head = head(&mut host, pell);
    let (title, _) = paramountcy(host.world_mut()).unwrap();
    declare_claim(host.world_mut(), title, veyrin_head).unwrap();
    declare_claim(host.world_mut(), title, pell_head).unwrap();

    let war_id = declare_war(host.world_mut(), veyrin, pell, key("claimant-war")).unwrap();
    assert_eq!(
        claimant_war_blocker(host.world_mut(), title, veyrin_head),
        Some(war_id)
    );
    assert_eq!(
        press_claim(host.world_mut(), title, veyrin_head),
        Err(ParamountClaimError::OpposingClaimantWar(war_id))
    );

    // The organisation war continues, but it ceases to block as soon as the
    // rival character leaves the claimant field.
    renounce_claim(host.world_mut(), title, pell_head).unwrap();
    assert!(war(host.world_mut(), war_id).unwrap().active());
    assert_eq!(
        claimant_war_blocker(host.world_mut(), title, veyrin_head),
        None
    );
    press_claim(host.world_mut(), title, veyrin_head).unwrap();
}

#[test]
fn a_claimant_war_started_during_press_downgrades_the_successful_draw() {
    let mut host = scenario_host(107);
    let veyrin = org(&mut host, "veyrin");
    let pell = org(&mut host, "pell");
    let veyrin_head = head(&mut host, veyrin);
    let pell_head = head(&mut host, pell);
    let (title, _) = paramountcy(host.world_mut()).unwrap();
    declare_claim(host.world_mut(), title, veyrin_head).unwrap();
    declare_claim(host.world_mut(), title, pell_head).unwrap();

    let press = key("press-claim");
    validate_start(
        host.world_mut(),
        veyrin,
        &press,
        veyrin_head,
        AssignmentTarget::None,
    )
    .unwrap();
    let assignment = start_assignment(
        host.world_mut(),
        veyrin,
        &press,
        veyrin_head,
        AssignmentTarget::None,
    );
    let completes = aeon_sim::access::assignment(host.world_mut(), assignment)
        .unwrap()
        .completes;
    let drawn = {
        let world = host.world_mut();
        let content = world.resource::<aeon_sim::state::ContentDb>().0.clone();
        let def = &content.assignments[&press];
        let effectiveness = aeon_sim::forecast::effectiveness(world, veyrin_head, def);
        let mut rng = aeon_sim::access::derived_rng(
            world,
            "job-resolution",
            &[assignment.raw(), completes.days_since_epoch() as u64],
        );
        aeon_sim::forecast::resolve_outcome(def, effectiveness, &mut rng)
    };
    assert!(
        matches!(
            drawn,
            aeon_data::model::OutcomeKind::Success | aeon_data::model::OutcomeKind::CriticalSuccess
        ),
        "the fixture must prove completion revalidation, not an ordinary failed roll"
    );

    let war_id = declare_war(host.world_mut(), veyrin, pell, key("mid-press-challenge")).unwrap();
    host.world_mut().resource_mut::<CampaignClock>().date = completes;
    resolve_due_assignments(host.world_mut());

    assert!(war(host.world_mut(), war_id).unwrap().active());
    assert_eq!(
        aeon_sim::access::title(host.world_mut(), title)
            .unwrap()
            .holder,
        TitleHolder::Vacant,
        "the rival war invalidates the press before its success effect runs"
    );
    assert!(aeon_sim::access::assignment(host.world_mut(), assignment).is_none());
}

#[test]
fn an_invalidated_guaranteed_transition_uses_an_effect_free_failure_result() {
    let mut host = scenario_host(108);
    let veyrin = org(&mut host, "veyrin");
    let pell = org(&mut host, "pell");
    let veyrin_head = head(&mut host, veyrin);
    let pell_head = head(&mut host, pell);
    let (title, _) = paramountcy(host.world_mut()).unwrap();

    let declaration = key("declare-paramount-claim");
    validate_start(
        host.world_mut(),
        pell,
        &declaration,
        pell_head,
        AssignmentTarget::None,
    )
    .unwrap();
    let assignment = start_assignment(
        host.world_mut(),
        pell,
        &declaration,
        pell_head,
        AssignmentTarget::None,
    );
    let completes = aeon_sim::access::assignment(host.world_mut(), assignment)
        .unwrap()
        .completes;

    declare_claim(host.world_mut(), title, veyrin_head).unwrap();
    press_claim(host.world_mut(), title, veyrin_head).unwrap();
    let messages_before = host
        .world_mut()
        .resource::<aeon_sim::MessageLog>()
        .entries
        .len();

    host.world_mut().resource_mut::<CampaignClock>().date = completes;
    resolve_due_assignments(host.world_mut());

    assert!(aeon_sim::access::assignment(host.world_mut(), assignment).is_none());
    assert!(
        !host
            .world_mut()
            .resource::<ParamountClaims>()
            .entries
            .contains_key(&(title, pell_head))
    );
    let messages = &host.world_mut().resource::<aeon_sim::MessageLog>().entries;
    assert_eq!(messages.len(), messages_before + 1);
    assert!(
        messages.last().unwrap().text.contains("Failure"),
        "an invalid guaranteed success must be reported as failure"
    );
}

#[test]
fn sibling_war_adoption_escalates_one_frozen_side_and_grants_peace_authority() {
    let mut host = scenario_host(105);
    let veyrin = org(&mut host, "veyrin");
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let calder = org(&mut host, "calder");
    let draksha = org(&mut host, "draksha");
    let meloch = org(&mut host, "meloch");
    let war_id = declare_war(host.world_mut(), harrow, vantar, key("internal-war")).unwrap();

    assert!(opposed_in(host.world_mut(), war_id, harrow, vantar));
    assert_eq!(side_of(host.world_mut(), war_id, veyrin), None);
    assert!(can_adopt_side(
        host.world_mut(),
        war_id,
        veyrin,
        WarSideId::Attacker
    ));
    adopt_side(host.world_mut(), war_id, veyrin, WarSideId::Attacker).unwrap();

    let record = war(host.world_mut(), war_id).unwrap();
    assert_eq!(record.side(WarSideId::Attacker).leader, veyrin);
    assert!(record.side(WarSideId::Attacker).members.contains(&harrow));
    assert!(record.side(WarSideId::Attacker).members.contains(&calder));
    assert!(!record.side(WarSideId::Attacker).members.contains(&vantar));
    assert_eq!(record.adoption_history.len(), 1);

    // Peace tears down only blockades bound to this exact occurrence.
    let other_war = declare_war(host.world_mut(), draksha, meloch, key("other-war")).unwrap();
    let (this_ship, other_ship, province) = {
        let world = host.world_mut();
        let mut ships = world
            .resource::<aeon_sim::ForcesIndex>()
            .ships
            .values()
            .copied();
        let this_ship = ships.next().unwrap();
        let other_ship = ships.next().unwrap();
        let province = *world
            .resource::<aeon_sim::MapIndex>()
            .provinces
            .keys()
            .next()
            .unwrap();
        (this_ship, other_ship, province)
    };
    host.world_mut()
        .get_mut::<aeon_sim::ShipRecord>(this_ship)
        .unwrap()
        .blockading = Some(aeon_sim::forces::Blockade {
        province,
        war: war_id,
    });
    host.world_mut()
        .get_mut::<aeon_sim::ShipRecord>(other_ship)
        .unwrap()
        .blockading = Some(aeon_sim::forces::Blockade {
        province,
        war: other_war,
    });
    let veyrin_head = head(&mut host, veyrin);
    let draksha_head = head(&mut host, draksha);
    let this_assignment = start_assignment_in_war(
        host.world_mut(),
        veyrin,
        &key("negotiate"),
        veyrin_head,
        AssignmentTarget::War(war_id),
        Some(war_id),
    );
    let other_assignment = start_assignment_in_war(
        host.world_mut(),
        draksha,
        &key("negotiate"),
        draksha_head,
        AssignmentTarget::War(other_war),
        Some(other_war),
    );
    assert_eq!(
        negotiate_peace(host.world_mut(), war_id, harrow),
        Err(WarError::NotSideLeader(harrow, war_id))
    );
    negotiate_peace(host.world_mut(), war_id, veyrin).unwrap();
    assert_eq!(
        war(host.world_mut(), war_id)
            .unwrap()
            .conclusion
            .unwrap()
            .kind,
        WarConclusionKind::NegotiatedPeace
    );
    assert_eq!(
        host.world_mut()
            .get::<aeon_sim::ShipRecord>(this_ship)
            .unwrap()
            .blockading,
        None
    );
    assert_eq!(
        host.world_mut()
            .get::<aeon_sim::ShipRecord>(other_ship)
            .unwrap()
            .blockading
            .unwrap()
            .war,
        other_war
    );
    assert!(
        aeon_sim::access::assignment_entity(host.world_mut(), this_assignment).is_none(),
        "peace aborts assignments authorised by the concluded war"
    );
    assert!(
        aeon_sim::access::assignment_entity(host.world_mut(), other_assignment).is_some(),
        "peace preserves assignments authorised by another war"
    );

    // A later declaration between the same houses is a new occurrence.
    let next = declare_war(host.world_mut(), harrow, vantar, key("internal-war")).unwrap();
    assert!(next > war_id);
}

#[test]
fn rebellion_partition_keeps_the_descendant_branch_out_of_the_lieges_side() {
    let mut host = scenario_host(106);
    let veyrin = org(&mut host, "veyrin");
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let war_id = declare_war(host.world_mut(), harrow, veyrin, key("rebellion")).unwrap();
    let record = war(host.world_mut(), war_id).unwrap();

    assert_eq!(record.side_of(harrow), Some(WarSideId::Attacker));
    assert_eq!(record.side_of(veyrin), Some(WarSideId::Defender));
    assert_eq!(record.side_of(vantar), Some(WarSideId::Defender));
    assert!(
        record
            .side(WarSideId::Attacker)
            .members
            .is_disjoint(&record.side(WarSideId::Defender).members)
    );
}
