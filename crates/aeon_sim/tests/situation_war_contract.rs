//! Cross-system contracts for authored Situations and occurrence-stable wars.

use std::sync::Arc;

use aeon_core::calendar::{CalendarDate, GameDate};
use aeon_data::model::PlanTargetSelector;
use aeon_data::{ContentKey, ContentSet, ContentSource, load_content};
use aeon_sim::assignments::{
    ActiveAssignment, AssignmentTarget, AssignmentsIndex, LogChannel, LogSubject, MessageLog,
    start_assignment_in_war,
};
use aeon_sim::plans::{ActivePlan, PlanStepInstance, Plans, StepTask};
use aeon_sim::politics::{PlayerHouse, TitleHolder, TitleRecord};
use aeon_sim::situations::{
    SituationInstanceKey, SituationOccurrence, SituationState, SituationSubject, active_cards,
    evaluate,
};
use aeon_sim::wars::{WarConclusionKind, conclude_war, declare_war};
use aeon_sim::{CampaignConfig, CharacterId, OrgId, PlayerCommand, PoliticsIndex, SimHost};

fn key(text: &str) -> ContentKey {
    ContentKey::new(text).unwrap()
}

fn sources() -> Vec<ContentSource> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    aeon_data::fs::read_content_dir(&root).expect("assets/content readable")
}

fn load_sources(sources: &[ContentSource]) -> Arc<ContentSet> {
    let (set, report) = load_content(sources, &aeon_data::StringTable::blank());
    assert!(
        set.is_some(),
        "repository content must load: {:?}",
        report.findings
    );
    Arc::new(set.unwrap())
}

fn repository_content() -> Arc<ContentSet> {
    load_sources(&sources())
}

fn scenario_host(seed: u64, content: Arc<ContentSet>) -> SimHost {
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

fn org(host: &mut SimHost, name: &str) -> OrgId {
    host.world_mut().resource::<PoliticsIndex>().org_keys[&key(name)]
}

fn head(host: &mut SimHost, organisation: OrgId) -> CharacterId {
    aeon_sim::access::org_head(host.world_mut(), organisation).expect("organisation has a head")
}

fn set_paramount_holder(host: &mut SimHost, holder: TitleHolder) {
    let entity = {
        let index = host.world_mut().resource::<PoliticsIndex>();
        let title = index.title_keys[&key("paramountcy-of-ashkarr")];
        index.titles[&title]
    };
    host.world_mut()
        .get_mut::<TitleRecord>(entity)
        .expect("paramount title exists")
        .holder = holder;
}

fn situation(
    host: &mut SimHost,
    definition: &str,
    war: Option<aeon_sim::WarId>,
) -> SituationInstanceKey {
    host.world_mut()
        .resource::<SituationState>()
        .active
        .keys()
        .find(|instance| {
            instance.definition == key(definition)
                && war.is_none_or(|war| {
                    instance.bindings.get("war") == Some(&SituationSubject::War(war))
                })
        })
        .cloned()
        .unwrap_or_else(|| panic!("missing active {definition} Situation"))
}

fn occurrence(host: &mut SimHost, situation: &SituationInstanceKey) -> SituationOccurrence {
    host.world_mut().resource::<SituationState>().active[situation].occurrence()
}

fn advance_to(host: &mut SimHost, date: GameDate) {
    while host.date() < date {
        host.advance_days(1);
    }
}

fn assignments(host: &mut SimHost) -> Vec<ActiveAssignment> {
    let world = host.world_mut();
    world
        .resource::<AssignmentsIndex>()
        .assignments
        .values()
        .filter_map(|entity| world.get::<ActiveAssignment>(*entity).cloned())
        .collect()
}

#[test]
fn reused_structural_key_starts_a_new_lifecycle_and_keeps_monotonic_notices() {
    let content = repository_content();
    let mut host = scenario_host(301, Arc::clone(&content));
    let initial = situation(&mut host, "planetary-succession", None);
    let initial_occurrence = occurrence(&mut host, &initial);
    let player = host.world_mut().resource::<PlayerHouse>().0.unwrap();
    let claimant = head(&mut host, player);

    set_paramount_holder(&mut host, TitleHolder::Character(claimant));
    evaluate(host.world_mut());
    let first = host
        .world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .find(|notice| notice.situation == initial)
        .cloned()
        .expect("first lifecycle leaves a notice");
    assert_eq!(first.occurrence(), initial_occurrence);
    assert!(
        !host
            .world_mut()
            .resource::<SituationState>()
            .active
            .contains_key(&initial)
    );

    set_paramount_holder(&mut host, TitleHolder::Vacant);
    host.advance_days(1);
    let reactivated = host.world_mut().resource::<SituationState>().active[&initial].clone();
    let reactivated_occurrence = reactivated.occurrence();
    assert!(reactivated.activated > initial_occurrence.activated);
    assert_ne!(reactivated_occurrence, initial_occurrence);
    assert!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .any(|notice| notice.id == first.id)
    );

    set_paramount_holder(&mut host, TitleHolder::Character(claimant));
    host.advance_days(1);
    let notices: Vec<_> = host
        .world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .filter(|notice| notice.situation == initial)
        .cloned()
        .collect();
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[1].id, notices[0].id + 1);
    assert_eq!(notices[0].occurrence(), initial_occurrence);
    assert_eq!(notices[1].occurrence(), reactivated_occurrence);
    let log = host.world_mut().resource::<MessageLog>();
    assert!(
        log.entries
            .iter()
            .any(|entry| entry.situations.contains(&initial_occurrence))
    );
    assert!(
        log.entries
            .iter()
            .any(|entry| entry.situations.contains(&reactivated_occurrence))
    );

    let dismissal = host
        .submit(PlayerCommand::DismissSituationResolution {
            resolution: notices[0].id,
        })
        .unwrap();
    advance_to(&mut host, dismissal.day);
    assert!(host.applied_commands().contains(&dismissal));
    assert_eq!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation == initial)
            .map(|notice| notice.id)
            .collect::<Vec<_>>(),
        vec![notices[1].id]
    );

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert_eq!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation == initial)
            .map(|notice| notice.id)
            .collect::<Vec<_>>(),
        vec![notices[1].id]
    );
}

#[test]
fn outcome_error_keeps_the_lifecycle_unavailable_and_logs_once_after_restore() {
    let mut broken_sources = sources();
    let source = broken_sources
        .iter_mut()
        .find(|source| source.path.ends_with("situations.rhai"))
        .unwrap();
    let original = r#"fn paramount_was_appointed(ctx) {
    for title in ctx.world.titles {
        if title.id == ctx.source.id && title.holder_kind == "character" {
            return true;
        }
    }
    false
}"#;
    let broken = original.replace("return true;", "return \"not-a-bool\";");
    assert!(source.source.contains(original));
    source.source = source.source.replace(original, &broken);

    let content = load_sources(&broken_sources);
    let mut host = scenario_host(302, Arc::clone(&content));
    let instance = situation(&mut host, "planetary-succession", None);
    let exact = occurrence(&mut host, &instance);
    let player = host.world_mut().resource::<PlayerHouse>().0.unwrap();
    let claimant = head(&mut host, player);
    set_paramount_holder(&mut host, TitleHolder::Character(claimant));
    evaluate(host.world_mut());

    let state = host.world_mut().resource::<SituationState>();
    assert!(state.active.contains_key(&instance));
    assert!(
        state
            .runtime_errors
            .get(&instance)
            .is_some_and(|error| error.contains("predicate must return a bool"))
    );
    assert!(
        state
            .resolutions
            .iter()
            .all(|notice| notice.situation != instance)
    );
    let card = active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key == instance)
        .unwrap();
    assert!(card.projection.is_none());
    assert!(card.unavailable.is_some());

    let logged = host
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .filter(|entry| entry.situations.contains(&exact))
        .count();
    assert_eq!(logged, 1);
    evaluate(host.world_mut());
    assert_eq!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .filter(|entry| entry.situations.contains(&exact))
            .count(),
        logged
    );

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .active
            .contains_key(&instance)
    );
    assert_eq!(
        restored
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .filter(|entry| entry.situations.contains(&exact))
            .count(),
        logged
    );
}

#[test]
fn situation_assignment_origin_survives_snapshot_and_tags_its_result_log() {
    let content = repository_content();
    let mut host = scenario_host(303, Arc::clone(&content));
    let independent = org(&mut host, "veyrin");
    let leader = head(&mut host, independent);
    *host.world_mut().resource_mut::<PlayerHouse>() = PlayerHouse(Some(independent));
    evaluate(host.world_mut());
    let instance = situation(&mut host, "planetary-succession", None);
    let exact = occurrence(&mut host, &instance);

    let projected = active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key == instance)
        .and_then(|card| card.projection)
        .and_then(|projection| {
            projection
                .actions
                .into_iter()
                .find(|action| action.id == key("declare-claim"))
        })
        .expect("independent head is offered a declaration");
    assert_eq!(projected.leader, Some(leader));
    assert_eq!(projected.target, AssignmentTarget::None);

    let envelope = host
        .submit(PlayerCommand::StartSituationAssignment {
            situation: instance.clone(),
            action: projected.id,
            leader,
            target: projected.target,
            war: None,
        })
        .unwrap();
    advance_to(&mut host, envelope.day);
    let active = assignments(&mut host)
        .into_iter()
        .find(|assignment| assignment.def == key("declare-paramount-claim"))
        .expect("Situation command starts the ordinary assignment");
    assert_eq!(active.origin_situation.as_ref(), Some(&exact));
    assert_eq!(active.war, None);

    let mut restored =
        SimHost::restore_with_content(host.snapshot(), Arc::clone(&content)).unwrap();
    let restored_assignment = assignments(&mut restored)
        .into_iter()
        .find(|assignment| assignment.id == active.id)
        .expect("active assignment survives snapshot");
    assert_eq!(restored_assignment.origin_situation.as_ref(), Some(&exact));
    advance_to(&mut restored, restored_assignment.completes);

    let (title, _) = aeon_sim::crisis::paramountcy(restored.world_mut()).unwrap();
    assert!(aeon_sim::crisis::has_claim(
        restored.world_mut(),
        title,
        leader
    ));
    assert!(
        restored
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| {
                entry.org == Some(independent)
                    && entry.subject == Some(LogSubject::Character(leader))
                    && entry.situations == vec![exact.clone()]
            })
    );
}

#[test]
fn formal_war_action_and_peace_keep_exact_situation_and_war_provenance() {
    let content = repository_content();
    let mut host = scenario_host(304, Arc::clone(&content));
    let attacker = org(&mut host, "harrow");
    let defender = org(&mut host, "vantar");
    let leader = head(&mut host, attacker);
    let war = declare_war(host.world_mut(), attacker, defender, key("provenance-war")).unwrap();
    evaluate(host.world_mut());
    let instance = situation(&mut host, "formal-war", Some(war));
    let exact = occurrence(&mut host, &instance);

    let envelope = host
        .submit(PlayerCommand::StartSituationAssignment {
            situation: instance.clone(),
            action: key("negotiate"),
            leader,
            target: AssignmentTarget::War(war),
            war: Some(war),
        })
        .unwrap();
    advance_to(&mut host, envelope.day);
    let active = assignments(&mut host)
        .into_iter()
        .find(|assignment| assignment.origin_situation.as_ref() == Some(&exact))
        .expect("formal-war action starts an assignment");
    assert_eq!(active.target, AssignmentTarget::War(war));
    assert_eq!(active.war, Some(war));

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert!(assignments(&mut restored).iter().any(|assignment| {
        assignment.id == active.id
            && assignment.war == Some(war)
            && assignment.origin_situation.as_ref() == Some(&exact)
    }));
    let tagged_before = restored
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .filter(|entry| entry.situations.contains(&exact))
        .count();

    conclude_war(
        restored.world_mut(),
        war,
        WarConclusionKind::NegotiatedPeace,
    )
    .unwrap();
    assert!(
        assignments(&mut restored)
            .iter()
            .all(|assignment| assignment.id != active.id)
    );
    assert!(
        restored
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .skip(tagged_before)
            .any(|entry| {
                entry.channel == LogChannel::Military
                    && entry.subject == Some(LogSubject::Character(leader))
                    && entry.situations.contains(&exact)
            })
    );

    evaluate(restored.world_mut());
    assert!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .any(|notice| notice.situation == instance)
    );
    let next = declare_war(
        restored.world_mut(),
        attacker,
        defender,
        key("provenance-war"),
    )
    .unwrap();
    assert_ne!(next, war);
    evaluate(restored.world_mut());
    let next_instance = situation(&mut restored, "formal-war", Some(next));
    assert_ne!(next_instance, instance);

    let mut round_trip = SimHost::restore_with_content(restored.snapshot(), repository_content())
        .expect("concluded and active war ledgers restore together");
    assert_eq!(
        aeon_sim::wars::war(round_trip.world_mut(), war)
            .unwrap()
            .conclusion
            .unwrap()
            .kind,
        WarConclusionKind::NegotiatedPeace
    );
    assert!(
        aeon_sim::wars::war(round_trip.world_mut(), next)
            .unwrap()
            .active()
    );
    assert!(
        round_trip
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .any(|notice| notice.situation == instance)
    );
    assert!(
        round_trip
            .world_mut()
            .resource::<SituationState>()
            .active
            .contains_key(&next_instance)
    );
}

#[test]
fn successful_peace_finishes_its_plan_and_safely_skips_simultaneous_aborted_work() {
    let mut host = scenario_host(309, repository_content());
    let attacker = org(&mut host, "harrow");
    let defender = org(&mut host, "vantar");
    let attacker_head = head(&mut host, attacker);
    let defender_head = head(&mut host, defender);
    let war = declare_war(
        host.world_mut(),
        attacker,
        defender,
        key("simultaneous-peace"),
    )
    .unwrap();
    let resolving = start_assignment_in_war(
        host.world_mut(),
        attacker,
        &key("negotiate"),
        attacker_head,
        AssignmentTarget::War(war),
        Some(war),
    );
    let aborted = start_assignment_in_war(
        host.world_mut(),
        defender,
        &key("negotiate"),
        defender_head,
        AssignmentTarget::War(war),
        Some(war),
    );
    let due = host.date().add_days(1);
    for assignment in [resolving, aborted] {
        let entity = aeon_sim::access::assignment_entity(host.world_mut(), assignment).unwrap();
        host.world_mut()
            .get_mut::<ActiveAssignment>(entity)
            .unwrap()
            .completes = due;
    }
    let started = host.date();
    host.world_mut().resource_mut::<Plans>().active.insert(
        attacker_head,
        ActivePlan {
            def: key("prosecute-claimant-war"),
            method: "settle-from-strength".to_owned(),
            steps: vec![PlanStepInstance {
                id: "peace".to_owned(),
                task: StepTask::Start {
                    assignment: key("negotiate"),
                    target: PlanTargetSelector::PlanTarget,
                },
                skip_if: None,
            }],
            target: AssignmentTarget::War(war),
            step: 0,
            started,
            current_assignment: Some(resolving),
            retries: 0,
            reason: "test peace".to_owned(),
        },
    );

    advance_to(&mut host, due);

    assert!(
        !aeon_sim::wars::war(host.world_mut(), war).unwrap().active(),
        "the first due negotiation must conclude the war"
    );
    assert!(
        !host
            .world_mut()
            .resource::<Plans>()
            .active
            .contains_key(&attacker_head),
        "the resolving negotiation reports success to its plan"
    );
    assert!(assignments(&mut host).is_empty());
    let log = host.world_mut().resource::<MessageLog>();
    assert!(
        log.entries.iter().any(|entry| {
            entry.war == Some(war)
                && entry.subject == Some(LogSubject::Character(attacker_head))
                && entry.channel == LogChannel::Assignments
        }),
        "logs: {:?}",
        log.entries
    );
    assert!(log.entries.iter().any(|entry| {
        entry.war == Some(war)
            && entry.subject == Some(LogSubject::Character(defender_head))
            && entry.channel == LogChannel::Military
            && entry.situations.is_empty()
    }));
    assert!(
        !log.entries.iter().any(|entry| {
            entry.subject == Some(LogSubject::Character(attacker_head))
                && entry.channel == LogChannel::Military
        }),
        "the successful negotiation must not also say that peace aborted it"
    );
}

#[test]
fn formal_war_blockade_action_uses_the_selected_ships_captain() {
    let mut host = scenario_host(307, repository_content());
    let attacker = org(&mut host, "harrow");
    let defender = org(&mut host, "vantar");
    let war = declare_war(
        host.world_mut(),
        attacker,
        defender,
        key("blockade-action-war"),
    )
    .unwrap();
    evaluate(host.world_mut());
    let instance = situation(&mut host, "formal-war", Some(war));
    let action = active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key == instance)
        .and_then(|card| card.projection)
        .and_then(|projection| {
            projection
                .actions
                .into_iter()
                .find(|action| action.id == key("blockade"))
        })
        .expect("participating player is offered a blockade action");
    let AssignmentTarget::ShipToProvince(ship, _) = action.target else {
        panic!("blockade action has a ship-and-province target");
    };
    let captain = aeon_sim::access::ship(host.world_mut(), ship)
        .and_then(|record| record.captain)
        .expect("projected ship has a captain");
    assert_eq!(
        action.leader,
        Some(captain),
        "the fixed action leader must be the officer authorised to order the ship"
    );
    assert!(
        aeon_sim::situations::forecast_for_action(
            host.world_mut(),
            &instance,
            &action.id,
            captain,
            action.target,
        )
        .unwrap()
        .startable(),
        "the day-one action should lead to an order the authoritative validator accepts"
    );
}

#[test]
fn queued_war_action_cannot_revive_in_a_redeclared_occurrence() {
    let mut host = scenario_host(305, repository_content());
    let attacker = org(&mut host, "harrow");
    let defender = org(&mut host, "vantar");
    let leader = head(&mut host, attacker);
    let first = declare_war(host.world_mut(), attacker, defender, key("queued-war")).unwrap();
    evaluate(host.world_mut());
    let first_instance = situation(&mut host, "formal-war", Some(first));
    let first_occurrence = occurrence(&mut host, &first_instance);
    let queued = host
        .submit(PlayerCommand::StartSituationAssignment {
            situation: first_instance.clone(),
            action: key("negotiate"),
            leader,
            target: AssignmentTarget::War(first),
            war: Some(first),
        })
        .unwrap();

    conclude_war(host.world_mut(), first, WarConclusionKind::NegotiatedPeace).unwrap();
    evaluate(host.world_mut());
    let second = declare_war(host.world_mut(), attacker, defender, key("queued-war")).unwrap();
    evaluate(host.world_mut());
    assert_ne!(second, first);

    advance_to(&mut host, queued.day);
    assert!(host.applied_commands().contains(&queued));
    assert!(assignments(&mut host).iter().all(|assignment| {
        assignment.origin_situation.as_ref() != Some(&first_occurrence)
            && assignment.war != Some(first)
            && assignment.war != Some(second)
    }));
    assert!(
        host.world_mut()
            .resource::<SituationState>()
            .active
            .keys()
            .any(|instance| {
                instance.bindings.get("war") == Some(&SituationSubject::War(second))
            })
    );
}

#[test]
fn each_broken_formal_war_instance_gets_its_own_tagged_diagnostic() {
    let mut broken_sources = sources();
    let source = broken_sources
        .iter_mut()
        .find(|source| source.path.ends_with("situations.rhai"))
        .unwrap();
    source.source = source.source.replace(
        "projection_fn: \"formal_war_projection\"",
        "projection_fn: \"formal_war_instances\"",
    );
    let mut host = scenario_host(306, load_sources(&broken_sources));
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let draksha = org(&mut host, "draksha");
    let meloch = org(&mut host, "meloch");
    let first = declare_war(host.world_mut(), harrow, vantar, key("first-broken")).unwrap();
    let second = declare_war(host.world_mut(), draksha, meloch, key("second-broken")).unwrap();
    evaluate(host.world_mut());

    let instances = [
        situation(&mut host, "formal-war", Some(first)),
        situation(&mut host, "formal-war", Some(second)),
    ];
    let occurrences = instances
        .iter()
        .map(|instance| occurrence(&mut host, instance))
        .collect::<Vec<_>>();
    for instance in &instances {
        assert!(
            host.world_mut()
                .resource::<SituationState>()
                .runtime_errors
                .contains_key(instance)
        );
    }
    let diagnostics: Vec<_> = host
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .filter(|entry| entry.text.contains(" is unavailable:"))
        .collect();
    assert_eq!(
        diagnostics.len(),
        instances.len(),
        "each structural instance needs its own diagnostic"
    );
    for instance in &occurrences {
        assert!(
            diagnostics
                .iter()
                .any(|entry| entry.situations.contains(instance)),
            "each diagnostic needs its exact structural Situation tag"
        );
    }
}
