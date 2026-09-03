use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSet, load_content};
use aeon_sim::assignments::{
    ActiveAssignment, AssignmentRejection, AssignmentTarget, AssignmentsIndex, LogChannel,
    LogEntry, MessageLog, validate_start,
};
use aeon_sim::forces::{ArmyRecord, ForcesIndex, ShipRecord};
use aeon_sim::obligations::{ObligationKind, ObligationStatus, settle};
use aeon_sim::politics::PlayerHouse;
use aeon_sim::situations::{SituationState, SituationSubject, active_cards, evaluate};
use aeon_sim::wars::{
    WarConclusionKind, WarSideId, can_adopt_side, conclude_war, declare_war, war,
};
use aeon_sim::{
    CampaignConfig, CommandRejection, MapIndex, OrgId, PendingPopups, PlayerCommand, PoliticsIndex,
    SimHost,
};

fn key(text: &str) -> ContentKey {
    ContentKey::new(text).unwrap()
}

fn sources() -> Vec<aeon_data::ContentSource> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    aeon_data::fs::read_content_dir(&root).expect("assets/content readable")
}

fn load_sources(sources: &[aeon_data::ContentSource]) -> Arc<ContentSet> {
    // The real string table: authored optional prose (stage warnings,
    // announcements) is table-decided, so a blank table would change which
    // projections are even legal.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(sources, &strings.expect("valid string table"));
    assert!(
        set.is_some(),
        "repository content must load: {:?}",
        report.findings
    );
    Arc::new(set.unwrap())
}

fn repository_content() -> Arc<ContentSet> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources(), &strings.expect("valid string table"));
    assert!(
        !report.has_errors(),
        "content findings: {:?}",
        report.findings
    );
    Arc::new(set.expect("repository content loads"))
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

#[test]
fn opening_deck_projects_day_one_situations_without_runtime_errors() {
    let mut host = scenario_host(201, repository_content());
    assert!(
        host.world_mut()
            .resource::<SituationState>()
            .runtime_errors
            .is_empty()
    );
    let definitions: Vec<_> = active_cards(host.world_mut())
        .into_iter()
        .map(|card| card.active.key.definition)
        .collect();
    for expected in [
        key("planetary-succession"),
        key("favour-debt"),
        key("court-awaits"),
    ] {
        assert!(definitions.contains(&expected), "missing {expected}");
    }
}

#[test]
fn day_one_favour_debt_is_private_to_both_parties_and_visible_to_spectators() {
    let mut host = scenario_host(202, repository_content());
    let debt = active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition.as_str() == "favour-debt")
        .expect("day-one favour debt")
        .active
        .key;
    assert!(aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &debt
    ));

    let veyrin = org(&mut host, "veyrin");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(veyrin);
    assert!(aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &debt
    ));

    let draksha = org(&mut host, "draksha");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(draksha);
    assert!(!aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &debt
    ));

    host.world_mut().resource_mut::<PlayerHouse>().0 = None;
    assert!(aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &debt
    ));
}

#[test]
fn favour_debt_resolution_and_late_assignment_logs_keep_the_private_audience() {
    let content = repository_content();
    let mut host = scenario_host(206, Arc::clone(&content));
    let card = active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition.as_str() == "favour-debt")
        .expect("day-one favour debt");
    let exact = card.active.occurrence();
    let debtor = match exact.situation.bindings.get("debtor") {
        Some(SituationSubject::Organisation(org)) => *org,
        other => panic!("unexpected debtor binding: {other:?}"),
    };
    let creditor = match exact.situation.bindings.get("creditor") {
        Some(SituationSubject::Organisation(org)) => *org,
        other => panic!("unexpected creditor binding: {other:?}"),
    };
    assert_eq!(debtor, org(&mut host, "harrow"));
    assert_eq!(creditor, org(&mut host, "veyrin"));
    let outsider = org(&mut host, "draksha");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(creditor);
    let action = active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key == exact.situation)
        .and_then(|card| card.projection)
        .expect("debt projection")
        .actions
        .into_iter()
        .find(|action| action.id == key("call-favour"))
        .expect("creditor action");
    let envelope = host
        .submit(PlayerCommand::StartSituationAssignment {
            situation: exact.situation.clone(),
            action: action.id,
            leader: action.leader.expect("creditor head"),
            target: action.target,
            war: None,
        })
        .unwrap();
    while host.date() < envelope.day {
        host.advance_days(1);
    }
    let active = {
        let world = host.world_mut();
        world
            .resource::<AssignmentsIndex>()
            .assignments
            .values()
            .find_map(|entity| {
                world
                    .get::<ActiveAssignment>(*entity)
                    .filter(|assignment| assignment.origin_situation.as_ref() == Some(&exact))
                    .cloned()
            })
            .expect("Situation assignment started")
    };

    assert!(settle(
        host.world_mut(),
        ObligationKind::Favour,
        debtor,
        creditor,
        ObligationStatus::Fulfilled,
    ));
    evaluate(host.world_mut());
    assert!(
        !host
            .world_mut()
            .resource::<SituationState>()
            .active
            .contains_key(&exact.situation),
        "the card is gone before its originating assignment reports"
    );
    while host.date() < active.completes {
        host.advance_days(1);
    }

    let private_lines: Vec<_> = host
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .filter(|entry| entry.situations.contains(&exact))
        .cloned()
        .collect();
    assert!(
        private_lines
            .iter()
            .any(|entry| entry.channel == LogChannel::Events),
        "the Situation resolution is permanently tagged"
    );
    assert!(
        private_lines
            .iter()
            .any(|entry| entry.channel == LogChannel::Assignments),
        "the late assignment result retains its ended Situation provenance"
    );
    for entry in &private_lines {
        assert!(entry.audience.visible_to(Some(debtor)));
        assert!(entry.audience.visible_to(Some(creditor)));
        assert!(!entry.audience.visible_to(Some(outsider)));
        assert!(entry.audience.visible_to(None));
    }

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    let restored_lines: Vec<_> = restored
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .filter(|entry| entry.situations.contains(&exact))
        .cloned()
        .collect();
    assert_eq!(restored_lines, private_lines);
}

#[test]
fn succession_projection_reports_a_declared_claimants_live_realm_facts() {
    let mut host = scenario_host(101, repository_content());
    let veyrin = org(&mut host, "veyrin");
    let head = aeon_sim::access::org_head(host.world_mut(), veyrin).expect("Veyrin has a head");
    let (title, _) = aeon_sim::crisis::paramountcy(host.world_mut()).expect("paramountcy");
    aeon_sim::crisis::declare_claim(host.world_mut(), title, head).expect("eligible claim");
    evaluate(host.world_mut());

    let card = active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition.as_str() == "planetary-succession")
        .expect("succession card");
    assert_eq!(card.unavailable, None);
    let metric = card
        .projection
        .expect("projected")
        .metrics
        .into_iter()
        .find(|metric| metric.label_key == "situation.metric.claimant-realm")
        .expect("claimant realm row");
    assert!(
        matches!(metric.value, aeon_sim::SituationMetricValue::Text(value) if value.contains("Veyrin")),
        "the live realm row should name its claimant"
    );
}

#[test]
fn consular_vacancy_projection_reports_authoritative_candidate_scores() {
    let mut host = scenario_host(102, repository_content());
    let (consul, holder) = {
        let world = host.world_mut();
        let index = world.resource::<PoliticsIndex>();
        index
            .titles
            .keys()
            .find_map(|id| {
                let title = aeon_sim::access::title(world, *id)?;
                match (title.kind, title.holder) {
                    (
                        aeon_sim::politics::TitleKind::Consul,
                        aeon_sim::politics::TitleHolder::Character(holder),
                    ) => Some((*id, holder)),
                    _ => None,
                }
            })
            .expect("scenario Consul")
    };
    let date = host.world_mut().resource::<aeon_sim::CampaignClock>().date;
    aeon_sim::politics::process_death(host.world_mut(), holder, date);
    assert_eq!(
        aeon_sim::access::title(host.world_mut(), consul).map(|title| title.holder),
        Some(aeon_sim::politics::TitleHolder::Vacant)
    );
    evaluate(host.world_mut());

    let card = active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition.as_str() == "consular-vacancy")
        .expect("Consular vacancy card");
    assert_eq!(card.unavailable, None);
    assert!(
        card.projection
            .expect("projected")
            .metrics
            .iter()
            .any(|metric| metric.label_key == "situation.metric.consul-score"),
        "the existing contest scores should be projected"
    );
}

#[test]
fn siege_and_blockade_cannot_start_without_an_exact_formal_war() {
    let mut host = scenario_host(202, repository_content());
    let world = host.world_mut();
    let target = world
        .resource::<MapIndex>()
        .provinces
        .keys()
        .copied()
        .find(|province| aeon_sim::warfare::province_holder(world, *province).is_some())
        .unwrap();
    let army = world
        .resource::<ForcesIndex>()
        .armies
        .values()
        .find_map(|entity| world.get::<ArmyRecord>(*entity).cloned())
        .unwrap();
    let ship = world
        .resource::<ForcesIndex>()
        .ships
        .values()
        .find_map(|entity| {
            world
                .get::<ShipRecord>(*entity)
                .filter(|ship| ship.captain.is_some())
                .cloned()
        })
        .unwrap();

    assert_eq!(
        validate_start(
            world,
            army.owner,
            &key("besiege"),
            army.general.expect("starting army has a general"),
            AssignmentTarget::ArmyToProvince(army.id, target),
        ),
        Err(AssignmentRejection::BadTarget)
    );
    assert_eq!(
        validate_start(
            world,
            ship.owner,
            &key("blockade"),
            ship.captain.unwrap(),
            AssignmentTarget::ShipToProvince(ship.id, target),
        ),
        Err(AssignmentRejection::BadTarget)
    );
}

#[test]
fn situation_command_rejects_forged_action_and_war_identity() {
    let mut host = scenario_host(203, repository_content());
    let player = host.world_mut().resource::<PlayerHouse>().0.unwrap();
    let defender = org(&mut host, "vantar");
    let leader = aeon_sim::access::org_head(host.world_mut(), player).unwrap();
    let war = declare_war(host.world_mut(), player, defender, key("test-declaration")).unwrap();
    evaluate(host.world_mut());
    let situation = host
        .world_mut()
        .resource::<SituationState>()
        .active
        .keys()
        .find(|situation| {
            situation.definition == key("formal-war")
                && situation.bindings.get("war") == Some(&SituationSubject::War(war))
        })
        .cloned()
        .unwrap();

    let forged_action = host.submit(PlayerCommand::StartSituationAssignment {
        situation: situation.clone(),
        action: key("not-an-action"),
        leader,
        target: AssignmentTarget::War(war),
        war: Some(war),
    });
    assert!(matches!(forged_action, Err(CommandRejection::Situation(_))));

    let forged_war = host.submit(PlayerCommand::StartSituationAssignment {
        situation,
        action: key("negotiate"),
        leader,
        target: AssignmentTarget::War(war),
        war: None,
    });
    assert!(matches!(forged_war, Err(CommandRejection::Situation(_))));
}

#[test]
fn liege_adoption_action_preserves_the_chosen_war_side() {
    let content = repository_content();
    let mut host = scenario_host(206, Arc::clone(&content));
    let liege = org(&mut host, "veyrin");
    let attacker = org(&mut host, "harrow");
    let defender = org(&mut host, "vantar");
    let outsider = org(&mut host, "draksha");
    let liege_head = aeon_sim::access::org_head(host.world_mut(), liege).unwrap();
    let outsider_head = aeon_sim::access::org_head(host.world_mut(), outsider).unwrap();
    let war_id = declare_war(
        host.world_mut(),
        attacker,
        defender,
        key("two-sided-adoption"),
    )
    .unwrap();
    *host.world_mut().resource_mut::<PlayerHouse>() = PlayerHouse(Some(liege));

    assert!(can_adopt_side(
        host.world_mut(),
        war_id,
        liege,
        WarSideId::Attacker
    ));
    assert!(can_adopt_side(
        host.world_mut(),
        war_id,
        liege,
        WarSideId::Defender
    ));
    assert_eq!(
        validate_start(
            host.world_mut(),
            outsider,
            &key("adopt-formal-war"),
            outsider_head,
            AssignmentTarget::WarSide(war_id, WarSideId::Defender),
        ),
        Err(AssignmentRejection::FormalWarUnavailable)
    );

    evaluate(host.world_mut());
    let card = active_cards(host.world_mut())
        .into_iter()
        .find(|card| {
            card.active.key.definition == key("formal-war")
                && card.active.key.bindings.get("war") == Some(&SituationSubject::War(war_id))
        })
        .expect("formal war Situation");
    let projection = card.projection.expect("formal war projection");
    assert!(projection.actions.iter().any(|action| {
        action.id == key("adopt-attacker")
            && action.target == AssignmentTarget::WarSide(war_id, WarSideId::Attacker)
    }));
    let action = projection
        .actions
        .into_iter()
        .find(|action| action.id == key("adopt-defender"))
        .expect("defender-side adoption action");
    assert_eq!(
        action.target,
        AssignmentTarget::WarSide(war_id, WarSideId::Defender)
    );

    let envelope = host
        .submit(PlayerCommand::StartSituationAssignment {
            situation: card.active.key,
            action: action.id,
            leader: action.leader.expect("liege head"),
            target: action.target,
            war: Some(war_id),
        })
        .unwrap();
    while host.date() < envelope.day {
        host.advance_days(1);
    }
    let active = {
        let world = host.world_mut();
        world
            .resource::<AssignmentsIndex>()
            .assignments
            .values()
            .find_map(|entity| world.get::<ActiveAssignment>(*entity))
            .filter(|assignment| assignment.def == key("adopt-formal-war"))
            .cloned()
            .expect("adoption assignment started")
    };
    assert_eq!(active.leader, liege_head);
    assert_eq!(
        active.target,
        AssignmentTarget::WarSide(war_id, WarSideId::Defender)
    );

    let completes = active.completes;
    host = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    let restored_target = {
        let world = host.world_mut();
        world
            .resource::<AssignmentsIndex>()
            .assignments
            .values()
            .find_map(|entity| world.get::<ActiveAssignment>(*entity))
            .filter(|assignment| assignment.def == key("adopt-formal-war"))
            .map(|assignment| assignment.target)
            .expect("adoption assignment restored")
    };
    assert_eq!(
        restored_target,
        AssignmentTarget::WarSide(war_id, WarSideId::Defender)
    );

    while host.date() < completes {
        host.advance_days(1);
    }
    let record = war(host.world_mut(), war_id).expect("war remains in the ledger");
    assert_eq!(record.side(WarSideId::Defender).leader, liege);
    assert_eq!(record.side(WarSideId::Attacker).leader, attacker);
    assert_eq!(
        record.adoption_history.last().unwrap().side,
        WarSideId::Defender
    );
}

#[test]
fn formal_war_projection_groups_sides_and_offers_every_force_objective_pair() {
    let mut host = scenario_host(207, repository_content());
    let player = host.world_mut().resource::<PlayerHouse>().0.unwrap();
    let defender = org(&mut host, "vantar");
    let war_id = declare_war(
        host.world_mut(),
        player,
        defender,
        key("complete-war-surface"),
    )
    .unwrap();
    let war_record = war(host.world_mut(), war_id).unwrap().clone();
    let player_side = war_record.side_of(player).expect("player is on a side");
    let enemy_members = war_record.side(player_side.opposite()).members.clone();
    let extra_general =
        host.world_mut().resource::<PoliticsIndex>().character_keys[&key("kessarin-harrow")];
    let extra_location = host.world_mut().resource::<MapIndex>().province_keys[&key("ostragard")];
    aeon_sim::forces::form_army(
        host.world_mut(),
        player,
        extra_general,
        500,
        100,
        extra_location,
    );

    let (player_armies, player_ships, enemy_provinces) = {
        let world = host.world_mut();
        let forces = world.resource::<ForcesIndex>();
        let armies = forces
            .armies
            .values()
            .filter_map(|entity| world.get::<ArmyRecord>(*entity))
            .filter(|army| army.owner == player)
            .map(|army| army.id)
            .collect::<Vec<_>>();
        let ships = forces
            .ships
            .values()
            .filter_map(|entity| world.get::<ShipRecord>(*entity))
            .filter(|ship| ship.owner == player && ship.captain.is_some())
            .map(|ship| ship.id)
            .collect::<Vec<_>>();
        let provinces = world
            .resource::<MapIndex>()
            .provinces
            .keys()
            .copied()
            .filter(|province| {
                aeon_sim::warfare::province_holder(world, *province)
                    .is_some_and(|holder| enemy_members.contains(&holder))
            })
            .collect::<Vec<_>>();
        (armies, ships, provinces)
    };
    assert!(player_armies.len() > 1, "fixture exercises multiple armies");
    assert!(!player_ships.is_empty(), "fixture exercises a capital ship");
    assert!(
        enemy_provinces.len() > 1,
        "fixture exercises multiple enemy objectives"
    );

    evaluate(host.world_mut());
    let card = active_cards(host.world_mut())
        .into_iter()
        .find(|card| {
            card.active.key.definition == key("formal-war")
                && card.active.key.bindings.get("war") == Some(&SituationSubject::War(war_id))
        })
        .expect("formal war Situation");
    let instance = card.active.key;
    let projection = card.projection.expect("formal war projection");

    assert_eq!(
        projection
            .participant_groups
            .iter()
            .map(|group| group.label_key.as_str())
            .collect::<Vec<_>>(),
        [
            "situation.formal-war.side.attackers",
            "situation.formal-war.side.defenders",
        ]
    );
    for (group, side) in projection.participant_groups.iter().zip(WarSideId::ALL) {
        assert_eq!(
            group
                .participants
                .iter()
                .map(|participant| OrgId::from_raw(participant.id).unwrap())
                .collect::<Vec<_>>(),
            war_record
                .side(side)
                .members
                .iter()
                .copied()
                .collect::<Vec<_>>()
        );
    }

    let mut expected_sieges = Vec::new();
    for army in &player_armies {
        for province in &enemy_provinces {
            expected_sieges.push((*army, *province));
        }
    }
    let mut expected_blockades = Vec::new();
    for ship in &player_ships {
        for province in &enemy_provinces {
            expected_blockades.push((*ship, *province));
        }
    }
    let sieges = projection
        .actions
        .iter()
        .filter(|action| action.id == key("besiege"))
        .map(|action| {
            let AssignmentTarget::ArmyToProvince(army, province) = action.target else {
                panic!("besiege action must carry an army and province");
            };
            assert_eq!(
                action
                    .context
                    .iter()
                    .map(|link| (link.kind, link.id))
                    .collect::<Vec<_>>(),
                [
                    (aeon_data::model::SituationSubjectKind::Army, army.raw()),
                    (
                        aeon_data::model::SituationSubjectKind::Province,
                        province.raw(),
                    ),
                ]
            );
            (army, province)
        })
        .collect::<Vec<_>>();
    let blockades = projection
        .actions
        .iter()
        .filter(|action| action.id == key("blockade"))
        .map(|action| {
            let AssignmentTarget::ShipToProvince(ship, province) = action.target else {
                panic!("blockade action must carry a ship and province");
            };
            assert_eq!(
                action
                    .context
                    .iter()
                    .map(|link| (link.kind, link.id))
                    .collect::<Vec<_>>(),
                [
                    (aeon_data::model::SituationSubjectKind::Ship, ship.raw()),
                    (
                        aeon_data::model::SituationSubjectKind::Province,
                        province.raw(),
                    ),
                ]
            );
            (ship, province)
        })
        .collect::<Vec<_>>();
    assert_eq!(sieges, expected_sieges);
    assert_eq!(blockades, expected_blockades);

    for action in projection
        .actions
        .iter()
        .filter(|action| action.id == key("besiege") || action.id == key("blockade"))
    {
        let leader = action.leader.expect("eligible force has a commander");
        let forecast = aeon_sim::situations::forecast_for_action(
            host.world_mut(),
            &instance,
            &action.id,
            leader,
            action.target,
        );
        assert!(
            forecast.is_ok(),
            "every listed alternative remains forecast-visible: {:?} -> {forecast:?}",
            action.target,
        );
    }
}

#[test]
fn resolution_is_dismissible_and_a_later_war_is_a_new_lifecycle() {
    let content = repository_content();
    let mut host = scenario_host(204, Arc::clone(&content));
    let player = host.world_mut().resource::<PlayerHouse>().0.unwrap();
    let defender = org(&mut host, "vantar");
    let first = declare_war(host.world_mut(), player, defender, key("first-declaration")).unwrap();
    evaluate(host.world_mut());
    conclude_war(host.world_mut(), first, WarConclusionKind::NegotiatedPeace).unwrap();
    evaluate(host.world_mut());
    let notice = host
        .world_mut()
        .resource::<SituationState>()
        .resolutions
        .last()
        .cloned()
        .unwrap();
    assert_eq!(
        notice.situation.bindings.get("war"),
        Some(&SituationSubject::War(first))
    );
    assert_eq!(
        notice
            .participant_groups
            .iter()
            .map(|group| group.label_key.as_str())
            .collect::<Vec<_>>(),
        [
            "situation.formal-war.side.attackers",
            "situation.formal-war.side.defenders",
        ],
        "the dismissible result preserves the frozen sides"
    );

    host.submit(PlayerCommand::DismissSituationResolution {
        resolution: notice.id,
    })
    .unwrap();
    host.advance_days(1);
    assert!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .all(|candidate| candidate.id != notice.id)
    );

    let second = declare_war(
        host.world_mut(),
        player,
        defender,
        key("second-declaration"),
    )
    .unwrap();
    assert_ne!(first, second);
    evaluate(host.world_mut());
    assert!(
        host.world_mut()
            .resource::<SituationState>()
            .active
            .keys()
            .any(|situation| {
                situation.definition == key("formal-war")
                    && situation.bindings.get("war") == Some(&SituationSubject::War(second))
            })
    );
}

#[test]
fn projection_error_is_unavailable_logged_once_and_snapshot_stable() {
    let mut broken_sources = sources();
    let source = broken_sources
        .iter_mut()
        .find(|source| source.path.ends_with("situations.rhai"))
        .unwrap();
    source.source = source.source.replace(
        "projection_fn: \"planetary_succession_projection\"",
        "projection_fn: \"planetary_succession_instances\"",
    );
    let content = load_sources(&broken_sources);
    let mut host = scenario_host(205, Arc::clone(&content));
    let error_key = host
        .world_mut()
        .resource::<SituationState>()
        .runtime_errors
        .keys()
        .find(|instance| instance.definition == key("planetary-succession"))
        .cloned()
        .unwrap();
    let error_occurrence =
        host.world_mut().resource::<SituationState>().active[&error_key].occurrence();
    let cards = active_cards(host.world_mut());
    let card = cards
        .iter()
        .find(|card| card.active.key == error_key)
        .unwrap();
    assert!(card.projection.is_none());
    assert!(card.unavailable.is_some());
    let log_count = host
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .filter(|entry| entry.situations.contains(&error_occurrence))
        .count();
    assert_eq!(log_count, 1);

    evaluate(host.world_mut());
    assert_eq!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .filter(|entry| entry.situations.contains(&error_occurrence))
            .count(),
        log_count
    );
    let expected_errors = host
        .world_mut()
        .resource::<SituationState>()
        .runtime_errors
        .clone();
    let snapshot = host.snapshot();
    let mut restored = SimHost::restore_with_content(snapshot, content).unwrap();
    assert_eq!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .runtime_errors,
        expected_errors
    );
    assert_eq!(
        restored
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .filter(|entry| entry.situations.contains(&error_occurrence))
            .count(),
        log_count
    );
}

// ---------------------------------------------------------------------------
// The Court Awaits: the day-one authority test of the First Reign arc.
// ---------------------------------------------------------------------------

fn court_card(host: &mut SimHost) -> Option<aeon_sim::situations::SituationCard> {
    active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition == key("court-awaits"))
}

fn influence_of(host: &mut SimHost, org: OrgId) -> i64 {
    let world = host.world_mut();
    let entity = aeon_sim::access::org_entity(world, org).expect("indexed organisation");
    world
        .get::<aeon_sim::OrgResources>(entity)
        .expect("organisations carry resources")
        .influence
}

#[test]
fn court_awaits_activates_on_day_one_with_deadline_announcement_and_history() {
    let mut host = scenario_host(301, repository_content());
    let start = host
        .world_mut()
        .resource::<aeon_sim::CampaignClock>()
        .start_date;
    let card = court_card(&mut host).expect("day-one court demand");
    assert_eq!(card.unavailable, None);
    let projection = card.projection.clone().expect("court projection");
    // The seven-day deadline is authored content projected authoritatively.
    assert_eq!(projection.deadline, Some(start.add_days(7)));
    assert!(projection.warning, "the demand is urgent from day one");
    assert!(
        projection
            .actions
            .iter()
            .any(|action| action.id == key("hold-court")),
        "the card offers an ordinary assignment route"
    );

    // Activation wrote permanent tagged history.
    let occurrence = card.active.occurrence();
    assert_eq!(occurrence.activated, start);
    assert!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&occurrence)),
        "activation is permanent tagged history"
    );

    // Activation raised an ordinary pausing popup stating the demand: the
    // popup path is what auto-pauses the client, and its single choice is
    // acknowledgeable through the ordinary command pipeline.
    let popup = host
        .world_mut()
        .resource::<PendingPopups>()
        .popups
        .iter()
        .find(|popup| popup.assignment == key("court-awaits"))
        .cloned()
        .expect("activation announcement popup");
    assert!(popup.text.contains("seven days"));
    assert!(popup.text.contains("10 Influence"));
    let choice = popup.choices[0].0.clone();
    host.submit(PlayerCommand::AnswerPopup {
        popup: popup.id,
        choice,
    })
    .expect("announcement acknowledgement is an ordinary command");
    host.advance_days(1);
    assert!(
        !host
            .world_mut()
            .resource::<PendingPopups>()
            .popups
            .iter()
            .any(|candidate| candidate.id == popup.id),
        "the acknowledged announcement is cleared"
    );
}

#[test]
fn court_awaits_is_private_to_harrow_and_visible_to_spectators() {
    let mut host = scenario_host(302, repository_content());
    let situation = court_card(&mut host).expect("court demand").active.key;
    assert!(aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &situation
    ));
    let veyrin = org(&mut host, "veyrin");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(veyrin);
    assert!(!aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &situation
    ));
    host.world_mut().resource_mut::<PlayerHouse>().0 = None;
    assert!(aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &situation
    ));
}

#[test]
fn any_accepted_ordinary_assignment_answers_the_court_without_penalty() {
    let content = repository_content();
    let mut host = scenario_host(303, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");
    let opening_influence = influence_of(&mut host, harrow);
    let occurrence = court_card(&mut host)
        .expect("court demand")
        .active
        .occurrence();
    let kessarin =
        host.world_mut().resource::<PoliticsIndex>().character_keys[&key("kessarin-harrow")];

    // Any ordinary assignment counts — not only the card's own shortcut.
    let envelope = host
        .submit(PlayerCommand::StartAssignment {
            assignment: key("manage-estates"),
            leader: kessarin,
            target: AssignmentTarget::None,
        })
        .expect("an ordinary valid command is accepted");
    while host.date() < envelope.day {
        host.advance_days(1);
    }

    assert!(
        court_card(&mut host).is_none(),
        "the demand resolves on the day the assignment is accepted"
    );
    let notice = host
        .world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .find(|notice| notice.situation.definition == key("court-awaits"))
        .cloned()
        .expect("durable resolution notice");
    assert_eq!(notice.outcome, key("answered"));
    assert_eq!(notice.occurrence(), occurrence);
    assert_eq!(
        influence_of(&mut host, harrow),
        opening_influence,
        "answering the court costs nothing beyond the assignment itself"
    );

    // The resolution and its history are private to the house but open to
    // spectators.
    let tagged: Vec<_> = host
        .world_mut()
        .resource::<MessageLog>()
        .entries
        .iter()
        .filter(|entry| entry.situations.contains(&occurrence))
        .cloned()
        .collect();
    assert!(tagged.len() >= 2, "activation and resolution history");
    let veyrin = org(&mut host, "veyrin");
    for entry in &tagged {
        assert!(entry.audience.visible_to(Some(harrow)));
        assert!(!entry.audience.visible_to(Some(veyrin)));
        assert!(entry.audience.visible_to(None));
    }

    // No reactivation: the court gathers once per campaign.
    host.advance_days(60);
    assert!(court_card(&mut host).is_none());
    assert_eq!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation.definition == key("court-awaits"))
            .count(),
        1
    );

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .any(|notice| notice.situation.definition == key("court-awaits")),
        "the resolution is durable across save and load"
    );
}

#[test]
fn an_assignment_accepted_on_the_deadline_day_still_answers_the_court() {
    let mut host = scenario_host(304, repository_content());
    let harrow = org(&mut host, "harrow");
    let opening_influence = influence_of(&mut host, harrow);
    let start = host
        .world_mut()
        .resource::<aeon_sim::CampaignClock>()
        .start_date;
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");

    // The head's own orders carry no delivery delay: submitted on day six,
    // the assignment is accepted exactly on the deadline day.
    host.advance_days(6);
    let envelope = host
        .submit(PlayerCommand::StartAssignment {
            assignment: key("manage-estates"),
            leader: edrun,
            target: AssignmentTarget::None,
        })
        .expect("a valid command on day six");
    assert_eq!(
        envelope.day,
        start.add_days(7),
        "accepted on the deadline day"
    );
    host.advance_days(1);

    let notice = host
        .world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .find(|notice| notice.situation.definition == key("court-awaits"))
        .cloned()
        .expect("boundary resolution");
    assert_eq!(notice.outcome, key("answered"));
    assert_eq!(influence_of(&mut host, harrow), opening_influence);
}

#[test]
fn invalid_and_unaffordable_attempts_do_not_answer_and_the_lapse_forfeits_influence() {
    let content = repository_content();
    let mut host = scenario_host(305, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");
    let opening_influence = influence_of(&mut host, harrow);

    // An ineligible leader is refused by ordinary command validation.
    let casimir =
        host.world_mut().resource::<PoliticsIndex>().character_keys[&key("casimir-veyrin")];
    assert!(matches!(
        host.submit(PlayerCommand::StartAssignment {
            assignment: key("manage-estates"),
            leader: casimir,
            target: AssignmentTarget::None,
        }),
        Err(CommandRejection::Assignment(
            AssignmentRejection::IneligibleLeader
        ))
    ));

    // An unaffordable assignment is refused by the same affordability rule
    // as everywhere else; guidance has no bypass.
    {
        let world = host.world_mut();
        let entity = aeon_sim::access::org_entity(world, harrow).expect("indexed");
        world
            .get_mut::<aeon_sim::OrgResources>(entity)
            .expect("resources")
            .wealth = 0;
    }
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    assert!(matches!(
        host.submit(PlayerCommand::StartAssignment {
            assignment: key("muster"),
            leader: edrun,
            target: AssignmentTarget::None,
        }),
        Err(CommandRejection::Assignment(
            AssignmentRejection::CannotAfford
        ))
    ));

    // Neither refused attempt answered the court; the deadline lapses and
    // exactly the stated Influence is forfeited.
    host.advance_days(7);
    let notice = host
        .world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .find(|notice| notice.situation.definition == key("court-awaits"))
        .cloned()
        .expect("lapsed resolution");
    assert_eq!(notice.outcome, key("lapsed"));
    assert!(notice.text.contains("10 Influence"));
    assert_eq!(influence_of(&mut host, harrow), opening_influence - 10);

    // The campaign stays playable with a durable resolution: ordinary
    // commands still work, the demand never reactivates, and the penalty is
    // applied exactly once.
    host.advance_days(53);
    host.submit(PlayerCommand::StartAssignment {
        assignment: key("manage-estates"),
        leader: edrun,
        target: AssignmentTarget::None,
    })
    .expect("the campaign continues after the forfeit");
    host.advance_days(2);
    assert!(court_card(&mut host).is_none(), "no reactivation");
    assert_eq!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation.definition == key("court-awaits"))
            .count(),
        1
    );
    // Influence has only moved through the ordinary monthly recharge since.
    assert!(influence_of(&mut host, harrow) >= opening_influence - 10);

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert_eq!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation.definition == key("court-awaits"))
            .count(),
        1,
        "the lapsed resolution is durable across save and load"
    );
}

#[test]
fn court_awaits_snapshots_replay_before_at_and_after_the_deadline() {
    let content = repository_content();

    // Both lifecycles: one campaign answers the court, one lets it lapse.
    for answered in [false, true] {
        let seed = if answered { 306 } else { 307 };
        let mut original = scenario_host(seed, Arc::clone(&content));
        if answered {
            let kessarin = original
                .world_mut()
                .resource::<PoliticsIndex>()
                .character_keys[&key("kessarin-harrow")];
            original
                .submit(PlayerCommand::StartAssignment {
                    assignment: key("manage-estates"),
                    leader: kessarin,
                    target: AssignmentTarget::None,
                })
                .unwrap();
        }

        // Snapshot before, at, and after the deadline day.
        let mut checkpoints = Vec::new();
        original.advance_days(3);
        checkpoints.push(original.snapshot());
        original.advance_days(4);
        checkpoints.push(original.snapshot());
        original.advance_days(3);
        checkpoints.push(original.snapshot());
        original.advance_days(20);
        let final_hash = original.state_hash();
        let final_date = original.date();

        for snapshot in checkpoints {
            let expected_mid = snapshot.state_hash;
            let mut replayed =
                SimHost::restore_with_content(snapshot, Arc::clone(&content)).unwrap();
            assert_eq!(replayed.state_hash(), expected_mid, "restore is exact");
            let remaining = replayed.date().days_until(final_date);
            replayed.advance_days(remaining as u32);
            assert_eq!(
                replayed.state_hash(),
                final_hash,
                "every checkpoint replays to the same final state (answered: {answered})"
            );
        }
    }
}

#[test]
fn equal_seed_and_commands_reproduce_the_court_lifecycle_exactly() {
    let content = repository_content();
    let run = |seed: u64| {
        let mut host = scenario_host(seed, Arc::clone(&content));
        let edrun = {
            let harrow = org(&mut host, "harrow");
            aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head")
        };
        host.submit(PlayerCommand::StartAssignment {
            assignment: key("manage-estates"),
            leader: edrun,
            target: AssignmentTarget::None,
        })
        .unwrap();
        host.advance_days(30);
        host
    };
    let mut a = run(308);
    let mut b = run(308);
    assert_eq!(
        a.world_mut().resource::<PendingPopups>(),
        b.world_mut().resource::<PendingPopups>(),
        "announcement popups are deterministic state"
    );
    assert_eq!(a.state_hash(), b.state_hash());
}

// ---------------------------------------------------------------------------
// Kessarin's Order: the household-demand exemplar of the First Reign arc.
// ---------------------------------------------------------------------------

use aeon_sim::CharacterId;
use aeon_sim::order::{adjust_order, held_provinces, province_order};
use aeon_sim::politics::{OpinionLedger, opinion_between, process_death};
use aeon_sim::situations::SituationCard;

/// The authored shared household deadline: the court window plus 120 days.
const HOUSEHOLD_DEADLINE_DAYS: i64 = 7 + 120;

fn kessarin_card(host: &mut SimHost) -> Option<SituationCard> {
    active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition == key("kessarin-order"))
}

fn character(host: &mut SimHost, name: &str) -> CharacterId {
    host.world_mut().resource::<PoliticsIndex>().character_keys[&key(name)]
}

fn start_date(host: &mut SimHost) -> aeon_core::calendar::GameDate {
    host.world_mut()
        .resource::<aeon_sim::CampaignClock>()
        .start_date
}

/// Answers the court with an ordinary head-led assignment and advances to
/// the settled day it is accepted, which is also the day the household's
/// demands open.
fn open_household_demands(host: &mut SimHost) {
    let harrow = org(host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    let envelope = host
        .submit(PlayerCommand::StartAssignment {
            assignment: key("manage-estates"),
            leader: edrun,
            target: AssignmentTarget::None,
        })
        .expect("the court answer is an ordinary valid command");
    while host.date() < envelope.day {
        host.advance_days(1);
    }
}

/// Raises every Harrow-held province to at least the authored 850 target.
fn raise_held_provinces_to_target(host: &mut SimHost) {
    let harrow = org(host, "harrow");
    let held = held_provinces(host.world_mut(), harrow);
    assert!(!held.is_empty(), "Harrow holds provinces");
    for province in held {
        let current = province_order(host.world_mut(), province).order;
        if current < 850 {
            adjust_order(host.world_mut(), province, 850 - current);
        }
    }
}

fn opinion_modifier(
    host: &mut SimHost,
    from: CharacterId,
    reason: &str,
) -> Option<aeon_sim::politics::OpinionEntry> {
    let world = host.world_mut();
    let entity = world.resource::<PoliticsIndex>().characters[&from];
    world
        .get::<OpinionLedger>(entity)
        .and_then(|ledger| ledger.0.iter().find(|entry| entry.reason == reason))
        .cloned()
}

fn kessarin_resolution(host: &mut SimHost) -> Option<aeon_sim::situations::SituationResolution> {
    host.world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .find(|notice| notice.situation.definition == key("kessarin-order"))
        .cloned()
}

fn answer_kessarin(
    host: &mut SimHost,
    response: &str,
) -> Result<aeon_sim::CommandEnvelope, CommandRejection> {
    let situation = kessarin_card(host).expect("live demand").active.key;
    host.submit(PlayerCommand::AnswerSituation {
        situation,
        response: key(response),
    })
}

#[test]
fn kessarins_demand_opens_after_the_court_with_the_shared_deadline_and_live_metrics() {
    let mut host = scenario_host(311, repository_content());
    let start = start_date(&mut host);
    assert!(
        kessarin_card(&mut host).is_none(),
        "the household waits until the court's test is behind the reign"
    );

    open_household_demands(&mut host);
    let card = kessarin_card(&mut host).expect("the demand opens with the court answered");
    assert_eq!(card.unavailable, None);
    let kessarin = character(&mut host, "kessarin-harrow");
    assert_eq!(
        card.active.key.bindings.get("requester"),
        Some(&SituationSubject::Character(kessarin)),
        "Kessarin herself is the bound requester"
    );

    let projection = card.projection.clone().expect("projection");
    // One shared deadline, anchored to pure campaign facts: the day the
    // court's window would have closed, plus the authored 120 days.
    assert_eq!(
        projection.deadline,
        Some(start.add_days(HOUSEHOLD_DEADLINE_DAYS))
    );
    let integer_metric = |label: &str| {
        projection.metrics.iter().find_map(|metric| {
            (metric.label_key == label).then(|| match &metric.value {
                aeon_sim::situations::SituationMetricValue::Integer(value) => *value,
                aeon_sim::situations::SituationMetricValue::Text(text) => {
                    panic!("expected integer metric, got '{text}'")
                }
            })
        })
    };
    assert_eq!(integer_metric("situation.metric.order-target"), Some(850));
    let harrow = org(&mut host, "harrow");
    let held = held_provinces(host.world_mut(), harrow).len() as i64;
    assert_eq!(
        projection
            .metrics
            .iter()
            .filter(|metric| metric.label_key == "situation.metric.province-order")
            .count() as i64,
        held,
        "every held province shows its live Order"
    );
    assert_eq!(
        integer_metric("situation.metric.provinces-below-target"),
        Some(held),
        "all holdings start below the target"
    );
    for consequence in [
        "situation.metric.on-achieved",
        "situation.metric.on-refused",
        "situation.metric.on-ignored",
        "situation.metric.on-broken",
    ] {
        assert!(
            projection
                .metrics
                .iter()
                .any(|metric| metric.label_key == consequence),
            "the card states the relationship consequence {consequence}"
        );
    }
    assert!(
        projection
            .actions
            .iter()
            .any(|action| action.id == key("manage-estates"))
            && projection
                .actions
                .iter()
                .any(|action| action.id == key("hold-court"))
            && projection
                .actions
                .iter()
                .any(|action| action.id == key("tour-holdings")),
        "the card offers the known ordinary routes"
    );
    assert_eq!(
        projection.links.len() as i64,
        held,
        "lagging provinces are navigable links"
    );

    // Activation raised a pausing announcement and permanent tagged history.
    let occurrence = card.active.occurrence();
    assert!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&occurrence))
    );
    let popup = host
        .world_mut()
        .resource::<PendingPopups>()
        .popups
        .iter()
        .find(|popup| popup.assignment == key("kessarin-order"))
        .cloned()
        .expect("activation announcement popup");
    assert!(popup.text.contains("850"));
    assert!(popup.text.contains("120-day"));
}

#[test]
fn achieving_the_goal_resolves_the_demand_and_completion_is_historical() {
    let content = repository_content();
    let mut host = scenario_host(312, Arc::clone(&content));
    open_household_demands(&mut host);
    let kessarin = character(&mut host, "kessarin-harrow");
    let harrow = org(&mut host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    let before = opinion_between(host.world_mut(), kessarin, edrun);

    // No promise was ever given; the provinces themselves satisfy her.
    raise_held_provinces_to_target(&mut host);
    host.advance_days(1);
    let resolved_on = host.date();
    assert!(kessarin_card(&mut host).is_none());
    let notice = kessarin_resolution(&mut host).expect("durable resolution");
    assert_eq!(notice.outcome, key("achieved"));

    // The stated +10 for 1,440 days, directional: Kessarin's opinion of the
    // head, under the tier's own stable reason.
    let entry = opinion_modifier(&mut host, kessarin, "kessarin-order-achieved")
        .expect("achievement modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, 10);
    assert_eq!(entry.expires, Some(resolved_on.add_days(1440)));
    assert_eq!(
        opinion_between(host.world_mut(), kessarin, edrun),
        before + 10
    );

    // Completion is history, not a frozen checkbox: past the shared window
    // the provinces keep moving and nothing reopens or retracts.
    let start = start_date(&mut host);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let remaining = host.date().days_until(deadline);
    host.advance_days(remaining as u32 + 3);
    let held = held_provinces(host.world_mut(), harrow);
    adjust_order(host.world_mut(), held[0], -200);
    host.advance_days(2);
    assert!(province_order(host.world_mut(), held[0]).order < 850);
    assert!(
        kessarin_card(&mut host).is_none(),
        "no reactivation after the window"
    );
    assert_eq!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation.definition == key("kessarin-order"))
            .count(),
        1
    );
    assert!(
        opinion_modifier(&mut host, kessarin, "kessarin-order-achieved").is_some(),
        "the achieved tier still stands while provincial Order moves on"
    );

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .any(|notice| notice.situation.definition == key("kessarin-order")),
        "the resolution is durable across save and load"
    );
}

#[test]
fn a_partly_met_goal_stays_live_and_names_the_one_lagging_province() {
    let mut host = scenario_host(322, repository_content());
    open_household_demands(&mut host);
    let kessarin = character(&mut host, "kessarin-harrow");
    let harrow = org(&mut host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");

    // Raise every held province but one. The goal is universal — every
    // holding at or above the target — so near-success is not success.
    let held = held_provinces(host.world_mut(), harrow);
    assert!(
        held.len() >= 2,
        "a strict-subset case needs at least two holdings"
    );
    let (lagging, raised) = held.split_last().expect("non-empty holdings");
    let lagging = *lagging;
    for &province in raised {
        let current = province_order(host.world_mut(), province).order;
        if current < 850 {
            adjust_order(host.world_mut(), province, 850 - current);
        }
    }
    host.advance_days(1);

    // The demand is still live and unresolved, and the card counts and
    // links exactly the one province still short of the target.
    let card = kessarin_card(&mut host).expect("a partly met demand stays live");
    assert!(
        kessarin_resolution(&mut host).is_none(),
        "no resolution is recorded while any holding lags"
    );
    let projection = card.projection.clone().expect("projection");
    let below = projection.metrics.iter().find_map(|metric| {
        (metric.label_key == "situation.metric.provinces-below-target").then(|| {
            match &metric.value {
                aeon_sim::situations::SituationMetricValue::Integer(value) => *value,
                aeon_sim::situations::SituationMetricValue::Text(text) => {
                    panic!("expected integer metric, got '{text}'")
                }
            }
        })
    });
    assert_eq!(below, Some(1), "exactly one holding is still below target");
    assert_eq!(
        projection.links.len(),
        1,
        "only the lagging province stays navigable"
    );
    assert_eq!(
        projection.links[0].kind,
        aeon_data::model::SituationSubjectKind::Province
    );
    assert_eq!(projection.links[0].id, lagging.raw());

    // Raising the last holding completes the universal goal, with the
    // stated achieved tier and nothing else.
    let before = opinion_between(host.world_mut(), kessarin, edrun);
    let current = province_order(host.world_mut(), lagging).order;
    adjust_order(host.world_mut(), lagging, 850 - current);
    host.advance_days(1);
    let resolved_on = host.date();
    assert!(kessarin_card(&mut host).is_none());
    let notice = kessarin_resolution(&mut host).expect("resolution once every holding stands");
    assert_eq!(notice.outcome, key("achieved"));
    let entry = opinion_modifier(&mut host, kessarin, "kessarin-order-achieved")
        .expect("achievement modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, 10);
    assert_eq!(entry.expires, Some(resolved_on.add_days(1440)));
    assert_eq!(
        opinion_between(host.world_mut(), kessarin, edrun),
        before + 10
    );
}

#[test]
fn refusal_costs_its_stated_tier_and_achievement_still_overrides_it() {
    let content = repository_content();
    let kessarin_key = "kessarin-harrow";

    // An honest refusal, left to stand: -5 for 1,080 days at the deadline.
    let mut refused = scenario_host(313, Arc::clone(&content));
    open_household_demands(&mut refused);
    answer_kessarin(&mut refused, "refuse").expect("refusing is an ordinary command");
    refused.advance_days(1);
    let card = kessarin_card(&mut refused).expect("a refused demand stays live");
    assert_eq!(
        card.projection.as_ref().expect("projection").stage,
        key("refused")
    );
    let start = start_date(&mut refused);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let remaining = refused.date().days_until(deadline);
    refused.advance_days(remaining as u32);
    let notice = kessarin_resolution(&mut refused).expect("deadline resolution");
    assert_eq!(notice.outcome, key("refused"));
    assert_eq!(notice.resolved, deadline, "the boundary day is exact");
    let kessarin = character(&mut refused, kessarin_key);
    let harrow = org(&mut refused, "harrow");
    let edrun = aeon_sim::access::org_head(refused.world_mut(), harrow).expect("head");
    let entry = opinion_modifier(&mut refused, kessarin, "kessarin-order-refused")
        .expect("refusal modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, -5);
    assert_eq!(entry.expires, Some(deadline.add_days(1080)));

    // The same refusal followed by the goal anyway: achievement counts
    // whatever was said, and only the achieved tier applies.
    let mut anyway = scenario_host(314, Arc::clone(&content));
    open_household_demands(&mut anyway);
    answer_kessarin(&mut anyway, "refuse").expect("refusal accepted");
    anyway.advance_days(1);
    raise_held_provinces_to_target(&mut anyway);
    anyway.advance_days(1);
    let notice = kessarin_resolution(&mut anyway).expect("resolution");
    assert_eq!(notice.outcome, key("achieved"));
    let kessarin = character(&mut anyway, kessarin_key);
    assert!(opinion_modifier(&mut anyway, kessarin, "kessarin-order-achieved").is_some());
    assert!(
        opinion_modifier(&mut anyway, kessarin, "kessarin-order-refused").is_none(),
        "tiers never stack on one lifecycle"
    );
}

#[test]
fn silence_and_broken_promises_cost_their_tiers_at_the_exact_deadline() {
    let content = repository_content();

    // Silence: the demand opens when the court window lapses, is never
    // answered, and the goal is never met.
    let mut silent = scenario_host(315, Arc::clone(&content));
    let start = start_date(&mut silent);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    silent.advance_days(7);
    assert!(
        kessarin_card(&mut silent).is_some(),
        "a lapsed court still opens the household demands"
    );
    let remaining = silent.date().days_until(deadline);
    silent.advance_days(remaining as u32 - 1);
    assert!(
        kessarin_card(&mut silent).is_some(),
        "the demand is still live the day before the deadline"
    );
    silent.advance_days(1);
    let notice = kessarin_resolution(&mut silent).expect("deadline resolution");
    assert_eq!(notice.outcome, key("ignored"));
    assert_eq!(notice.resolved, deadline);
    let kessarin = character(&mut silent, "kessarin-harrow");
    let harrow = org(&mut silent, "harrow");
    let edrun = aeon_sim::access::org_head(silent.world_mut(), harrow).expect("head");
    let entry = opinion_modifier(&mut silent, kessarin, "kessarin-order-ignored")
        .expect("silence modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, -10);
    assert_eq!(entry.expires, Some(deadline.add_days(1440)));

    // A promise given and missed: -20 for 1,800 days.
    let mut broken = scenario_host(316, Arc::clone(&content));
    open_household_demands(&mut broken);
    answer_kessarin(&mut broken, "promise").expect("promising is an ordinary command");
    broken.advance_days(1);
    assert_eq!(
        kessarin_card(&mut broken)
            .expect("live demand")
            .projection
            .expect("projection")
            .stage,
        key("promised")
    );
    let remaining = broken.date().days_until(deadline);
    broken.advance_days(remaining as u32);
    let notice = kessarin_resolution(&mut broken).expect("deadline resolution");
    assert_eq!(notice.outcome, key("broken"));
    let kessarin = character(&mut broken, "kessarin-harrow");
    let entry = opinion_modifier(&mut broken, kessarin, "kessarin-order-broken")
        .expect("broken-promise modifier");
    assert_eq!(entry.amount, -20);
    assert_eq!(entry.expires, Some(deadline.add_days(1800)));

    // The goal met exactly on the deadline day still counts as achievement.
    let mut boundary = scenario_host(317, Arc::clone(&content));
    boundary.advance_days(7);
    let remaining = boundary.date().days_until(deadline);
    boundary.advance_days(remaining as u32 - 1);
    assert!(kessarin_card(&mut boundary).is_some());
    raise_held_provinces_to_target(&mut boundary);
    boundary.advance_days(1);
    let notice = kessarin_resolution(&mut boundary).expect("boundary resolution");
    assert_eq!(notice.outcome, key("achieved"));
    assert_eq!(notice.resolved, deadline);
}

#[test]
fn the_demand_passes_on_when_kessarin_dies_and_the_successor_takes_it_up() {
    let mut host = scenario_host(318, repository_content());
    open_household_demands(&mut host);
    answer_kessarin(&mut host, "promise").expect("promise accepted");
    host.advance_days(1);
    let kessarin = character(&mut host, "kessarin-harrow");
    let aleyn = character(&mut host, "aleyn-harrow");

    let date = host.date();
    process_death(host.world_mut(), kessarin, date);
    evaluate(host.world_mut());

    // The dead requester's lifecycle ends without a relationship penalty.
    let notice = kessarin_resolution(&mut host).expect("passed-on resolution");
    assert_eq!(notice.outcome, key("passed-on"));
    for reason in [
        "kessarin-order-achieved",
        "kessarin-order-refused",
        "kessarin-order-ignored",
        "kessarin-order-broken",
    ] {
        assert!(
            opinion_modifier(&mut host, kessarin, reason).is_none(),
            "death carries no household tier ({reason})"
        );
    }

    // The replacement is the authored pure rule: the first living adult
    // non-head member in stable ID order — Aleyn. Her lifecycle is a new
    // occurrence, so the promise made to Kessarin does not transfer.
    let card = kessarin_card(&mut host).expect("the successor presses the demand");
    assert_eq!(
        card.active.key.bindings.get("requester"),
        Some(&SituationSubject::Character(aleyn))
    );
    assert_eq!(
        aeon_sim::situations::recorded_answer(host.world_mut(), &card.active.key),
        None,
        "a reactivation starts unanswered"
    );
}

#[test]
fn answers_validate_apply_once_and_survive_snapshots() {
    let content = repository_content();
    let mut host = scenario_host(319, Arc::clone(&content));
    open_household_demands(&mut host);
    let situation = kessarin_card(&mut host).expect("live demand").active.key;

    // Spectators and other houses cannot answer, and only declared
    // responses exist.
    host.world_mut().resource_mut::<PlayerHouse>().0 = None;
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Assignment(
            AssignmentRejection::NoPlayerOrg
        ))
    ));
    let veyrin = org(&mut host, "veyrin");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(veyrin);
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Situation(_))
    ));
    let harrow = org(&mut host, "harrow");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(harrow);
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("dither"),
        }),
        Err(CommandRejection::Situation(_))
    ));

    // Two answers queued the same day: the first applies, the second is
    // dropped by the same re-validation every delayed command runs.
    answer_kessarin(&mut host, "refuse").expect("first answer accepted");
    answer_kessarin(&mut host, "promise").expect("second accepted at submission");
    host.advance_days(1);
    assert_eq!(
        aeon_sim::situations::recorded_answer(host.world_mut(), &situation),
        Some(key("refuse")),
        "the first recorded answer is final"
    );
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Situation(_))
    ));

    // The recorded answer is tagged permanent history and durable state.
    let occurrence = kessarin_card(&mut host)
        .expect("live demand")
        .active
        .occurrence();
    assert!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&occurrence) && entry.text.contains("Refuse")),
        "the answer wrote a tagged history line"
    );
    let hash = host.state_hash();
    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert_eq!(restored.state_hash(), hash);
    assert_eq!(
        aeon_sim::situations::recorded_answer(restored.world_mut(), &situation),
        Some(key("refuse")),
        "the answer survives save and load"
    );
}

#[test]
fn kessarin_lifecycles_snapshot_and_replay_across_their_resolutions() {
    let content = repository_content();

    // Path one: an early achievement. The direct Order mutation lands
    // before the resolution day is settled, so every checkpoint is a
    // settled state and every replay is command-driven from there.
    let mut achieved = scenario_host(320, Arc::clone(&content));
    open_household_demands(&mut achieved);
    achieved.advance_days(12);
    raise_held_provinces_to_target(&mut achieved);
    achieved.advance_days(1);
    let mut achieved_checkpoints = vec![achieved.snapshot()];
    achieved.advance_days(10);
    achieved_checkpoints.push(achieved.snapshot());

    // Path two: a promise left to break, checkpointed before, at, and after
    // the shared deadline.
    let mut broken = scenario_host(321, Arc::clone(&content));
    open_household_demands(&mut broken);
    answer_kessarin(&mut broken, "promise").expect("promise accepted");
    broken.advance_days(1);
    let start = start_date(&mut broken);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let remaining = broken.date().days_until(deadline);
    broken.advance_days(remaining as u32 - 3);
    let mid = broken.snapshot();
    assert!(
        !mid.state.situations.answers.is_empty(),
        "the promise is snapshotted authoritative state"
    );
    let mut broken_checkpoints = vec![mid];
    broken.advance_days(3);
    broken_checkpoints.push(broken.snapshot());
    broken.advance_days(3);
    broken_checkpoints.push(broken.snapshot());

    for (index, (mut host, checkpoints)) in [
        (achieved, achieved_checkpoints),
        (broken, broken_checkpoints),
    ]
    .into_iter()
    .enumerate()
    {
        let final_day = start_date(&mut host).add_days(150);
        let remaining = host.date().days_until(final_day);
        host.advance_days(remaining as u32);
        let final_hash = host.state_hash();
        for snapshot in checkpoints {
            let expected_mid = snapshot.state_hash;
            let mut replayed =
                SimHost::restore_with_content(snapshot, Arc::clone(&content)).unwrap();
            assert_eq!(replayed.state_hash(), expected_mid, "restore is exact");
            let remaining = replayed.date().days_until(final_day);
            replayed.advance_days(remaining as u32);
            assert_eq!(
                replayed.state_hash(),
                final_hash,
                "every checkpoint replays to the same final state (path {index})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Aleyn's Levies: the second household demand — a live military-strength
// predicate over the armies the house actually fields.
// ---------------------------------------------------------------------------

/// The authored fielded-manpower target of Aleyn's demand.
const ALEYN_MANPOWER_TARGET: i64 = 1_000;

/// Harrow's starting fielded strength: the 600-strong Harrow Guard.
const STARTING_FIELDED_MANPOWER: i64 = 600;

fn aleyn_card(host: &mut SimHost) -> Option<SituationCard> {
    active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition == key("aleyn-levies"))
}

fn aleyn_resolution(host: &mut SimHost) -> Option<aeon_sim::situations::SituationResolution> {
    host.world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .find(|notice| notice.situation.definition == key("aleyn-levies"))
        .cloned()
}

fn answer_aleyn(
    host: &mut SimHost,
    response: &str,
) -> Result<aeon_sim::CommandEnvelope, CommandRejection> {
    let situation = aleyn_card(host).expect("live demand").active.key;
    host.submit(PlayerCommand::AnswerSituation {
        situation,
        response: key(response),
    })
}

/// An organisation's live fielded strength: the sum of every army it owns.
fn fielded_manpower(host: &mut SimHost, owner: OrgId) -> i64 {
    let world = host.world_mut();
    let forces = world.resource::<ForcesIndex>();
    forces
        .armies
        .values()
        .filter_map(|entity| world.get::<ArmyRecord>(*entity))
        .filter(|army| army.owner == owner)
        .map(|army| army.manpower)
        .sum()
}

/// Fields one fixture army of the given strength. `form_army` does not
/// deduct from the owner's pool, which is fine for a test fixture: the
/// goal is a predicate over fielded armies, however they came to exist.
fn form_fixture_army(host: &mut SimHost, manpower: i64) -> aeon_sim::ArmyId {
    let harrow = org(host, "harrow");
    let general = character(host, "reyn-harrow");
    let location = host.world_mut().resource::<MapIndex>().province_keys[&key("ostragard")];
    aeon_sim::forces::form_army(host.world_mut(), harrow, general, manpower, 50, location)
}

/// Fields exactly enough additional soldiers to reach the authored target.
fn raise_fielded_manpower_to_target(host: &mut SimHost) -> aeon_sim::ArmyId {
    let harrow = org(host, "harrow");
    let gap = ALEYN_MANPOWER_TARGET - fielded_manpower(host, harrow);
    assert!(gap > 0, "the start state is under strength");
    form_fixture_army(host, gap)
}

#[test]
fn aleyns_demand_opens_after_the_court_with_the_shared_deadline_and_live_metrics() {
    let mut host = scenario_host(323, repository_content());
    let start = start_date(&mut host);
    assert!(
        aleyn_card(&mut host).is_none(),
        "the household waits until the court's test is behind the reign"
    );

    open_household_demands(&mut host);
    let card = aleyn_card(&mut host).expect("the demand opens with the court answered");
    assert_eq!(card.unavailable, None);
    let aleyn = character(&mut host, "aleyn-harrow");
    assert_eq!(
        card.active.key.bindings.get("requester"),
        Some(&SituationSubject::Character(aleyn)),
        "Aleyn herself is the bound requester"
    );

    let projection = card.projection.clone().expect("projection");
    // The same shared deadline as Kessarin's demand: the day the court's
    // window would have closed, plus the authored 120 days.
    assert_eq!(
        projection.deadline,
        Some(start.add_days(HOUSEHOLD_DEADLINE_DAYS))
    );
    let integer_metric = |label: &str| {
        projection.metrics.iter().find_map(|metric| {
            (metric.label_key == label).then(|| match &metric.value {
                aeon_sim::situations::SituationMetricValue::Integer(value) => *value,
                aeon_sim::situations::SituationMetricValue::Text(text) => {
                    panic!("expected integer metric, got '{text}'")
                }
            })
        })
    };
    assert_eq!(
        integer_metric("situation.metric.manpower-target"),
        Some(ALEYN_MANPOWER_TARGET)
    );
    assert_eq!(
        integer_metric("situation.metric.army-manpower"),
        Some(STARTING_FIELDED_MANPOWER),
        "the card reads the live fielded total"
    );
    assert_eq!(
        integer_metric("situation.metric.manpower-shortfall"),
        Some(ALEYN_MANPOWER_TARGET - STARTING_FIELDED_MANPOWER)
    );
    assert_eq!(
        projection
            .metrics
            .iter()
            .filter(|metric| metric.label_key == "situation.metric.army-strength")
            .count(),
        1,
        "every fielded army shows its live strength"
    );
    for consequence in [
        "situation.metric.on-achieved",
        "situation.metric.on-refused",
        "situation.metric.on-ignored",
        "situation.metric.on-broken",
    ] {
        assert!(
            projection
                .metrics
                .iter()
                .any(|metric| metric.label_key == consequence),
            "the card states the relationship consequence {consequence}"
        );
    }
    assert!(
        projection
            .actions
            .iter()
            .any(|action| action.id == key("muster")),
        "the card offers the one honest ordinary route"
    );
    // The fielded armies are navigable links.
    let harrow = org(&mut host, "harrow");
    let guard = {
        let world = host.world_mut();
        let forces = world.resource::<ForcesIndex>();
        forces
            .armies
            .values()
            .filter_map(|entity| world.get::<ArmyRecord>(*entity))
            .find(|army| army.owner == harrow)
            .map(|army| army.id)
            .expect("Harrow fields its starting guard")
    };
    assert_eq!(projection.links.len(), 1);
    assert_eq!(
        projection.links[0].kind,
        aeon_data::model::SituationSubjectKind::Army
    );
    assert_eq!(projection.links[0].id, guard.raw());

    // Activation raised a pausing announcement and permanent tagged history.
    let occurrence = card.active.occurrence();
    assert!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&occurrence))
    );
    let popup = host
        .world_mut()
        .resource::<PendingPopups>()
        .popups
        .iter()
        .find(|popup| popup.assignment == key("aleyn-levies"))
        .cloned()
        .expect("activation announcement popup");
    assert!(popup.text.contains("1,000"));
    assert!(popup.text.contains("120-day"));
}

#[test]
fn fielding_the_strength_resolves_the_demand_and_completion_is_historical() {
    let content = repository_content();
    let mut host = scenario_host(324, Arc::clone(&content));
    open_household_demands(&mut host);
    let aleyn = character(&mut host, "aleyn-harrow");
    let harrow = org(&mut host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    let before = opinion_between(host.world_mut(), aleyn, edrun);

    // No promise was ever given; the armies themselves satisfy her.
    let fixture = raise_fielded_manpower_to_target(&mut host);
    host.advance_days(1);
    let resolved_on = host.date();
    assert!(aleyn_card(&mut host).is_none());
    let notice = aleyn_resolution(&mut host).expect("durable resolution");
    assert_eq!(notice.outcome, key("achieved"));

    // The stated +10 for 1,440 days, directional: Aleyn's opinion of the
    // head, under the tier's own stable reason.
    let entry =
        opinion_modifier(&mut host, aleyn, "aleyn-levies-achieved").expect("achievement modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, 10);
    assert_eq!(entry.expires, Some(resolved_on.add_days(1440)));
    assert_eq!(opinion_between(host.world_mut(), aleyn, edrun), before + 10);

    // Completion is history, not a frozen checkbox: past the shared window
    // the armies keep moving and a disbanded levy reopens nothing.
    let start = start_date(&mut host);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let remaining = host.date().days_until(deadline);
    host.advance_days(remaining as u32 + 3);
    aeon_sim::forces::disband_army(host.world_mut(), fixture);
    host.advance_days(2);
    assert!(fielded_manpower(&mut host, harrow) < ALEYN_MANPOWER_TARGET);
    assert!(
        aleyn_card(&mut host).is_none(),
        "no reactivation after the window"
    );
    assert_eq!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation.definition == key("aleyn-levies"))
            .count(),
        1
    );
    assert!(
        opinion_modifier(&mut host, aleyn, "aleyn-levies-achieved").is_some(),
        "the achieved tier still stands while the armies move on"
    );

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .any(|notice| notice.situation.definition == key("aleyn-levies")),
        "the resolution is durable across save and load"
    );
}

#[test]
fn partial_strength_stays_live_and_reads_the_exact_shortfall() {
    let mut host = scenario_host(325, repository_content());
    open_household_demands(&mut host);
    let aleyn = character(&mut host, "aleyn-harrow");
    let harrow = org(&mut host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");

    // Field 300 of the missing 400: the goal is a threshold on the live
    // total, so near-strength is not strength.
    form_fixture_army(&mut host, 300);
    host.advance_days(1);
    let card = aleyn_card(&mut host).expect("a partly met demand stays live");
    assert!(
        aleyn_resolution(&mut host).is_none(),
        "no resolution is recorded while the total lags"
    );
    let projection = card.projection.clone().expect("projection");
    let integer_metric = |label: &str| {
        projection.metrics.iter().find_map(|metric| {
            (metric.label_key == label).then(|| match &metric.value {
                aeon_sim::situations::SituationMetricValue::Integer(value) => *value,
                aeon_sim::situations::SituationMetricValue::Text(text) => {
                    panic!("expected integer metric, got '{text}'")
                }
            })
        })
    };
    assert_eq!(
        integer_metric("situation.metric.army-manpower"),
        Some(900),
        "the live total reads both fielded armies"
    );
    assert_eq!(
        integer_metric("situation.metric.manpower-shortfall"),
        Some(100),
        "the shortfall reads the exact remaining gap"
    );
    assert_eq!(
        projection
            .metrics
            .iter()
            .filter(|metric| metric.label_key == "situation.metric.army-strength")
            .count(),
        2,
        "both fielded armies show their live strength"
    );
    assert_eq!(projection.links.len(), 2, "both armies are navigable links");

    // Closing the gap completes the goal, with the stated achieved tier
    // and nothing else.
    let before = opinion_between(host.world_mut(), aleyn, edrun);
    form_fixture_army(&mut host, 100);
    host.advance_days(1);
    let resolved_on = host.date();
    assert!(aleyn_card(&mut host).is_none());
    let notice = aleyn_resolution(&mut host).expect("resolution once the strength stands");
    assert_eq!(notice.outcome, key("achieved"));
    let entry =
        opinion_modifier(&mut host, aleyn, "aleyn-levies-achieved").expect("achievement modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, 10);
    assert_eq!(entry.expires, Some(resolved_on.add_days(1440)));
    assert_eq!(opinion_between(host.world_mut(), aleyn, edrun), before + 10);
}

#[test]
fn refusing_aleyn_costs_its_stated_tier_and_achievement_still_overrides_it() {
    let content = repository_content();

    // An honest refusal, left to stand: -5 for 1,080 days at the deadline.
    let mut refused = scenario_host(326, Arc::clone(&content));
    open_household_demands(&mut refused);
    answer_aleyn(&mut refused, "refuse").expect("refusing is an ordinary command");
    refused.advance_days(1);
    let card = aleyn_card(&mut refused).expect("a refused demand stays live");
    assert_eq!(
        card.projection.as_ref().expect("projection").stage,
        key("refused")
    );
    let start = start_date(&mut refused);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let remaining = refused.date().days_until(deadline);
    refused.advance_days(remaining as u32);
    let notice = aleyn_resolution(&mut refused).expect("deadline resolution");
    assert_eq!(notice.outcome, key("refused"));
    assert_eq!(notice.resolved, deadline, "the boundary day is exact");
    let aleyn = character(&mut refused, "aleyn-harrow");
    let harrow = org(&mut refused, "harrow");
    let edrun = aeon_sim::access::org_head(refused.world_mut(), harrow).expect("head");
    let entry =
        opinion_modifier(&mut refused, aleyn, "aleyn-levies-refused").expect("refusal modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, -5);
    assert_eq!(entry.expires, Some(deadline.add_days(1080)));

    // The same refusal followed by the strength anyway: achievement counts
    // whatever was said, and only the achieved tier applies.
    let mut anyway = scenario_host(327, Arc::clone(&content));
    open_household_demands(&mut anyway);
    answer_aleyn(&mut anyway, "refuse").expect("refusal accepted");
    anyway.advance_days(1);
    raise_fielded_manpower_to_target(&mut anyway);
    anyway.advance_days(1);
    let notice = aleyn_resolution(&mut anyway).expect("resolution");
    assert_eq!(notice.outcome, key("achieved"));
    let aleyn = character(&mut anyway, "aleyn-harrow");
    assert!(opinion_modifier(&mut anyway, aleyn, "aleyn-levies-achieved").is_some());
    assert!(
        opinion_modifier(&mut anyway, aleyn, "aleyn-levies-refused").is_none(),
        "tiers never stack on one lifecycle"
    );
}

#[test]
fn silence_and_broken_promises_to_aleyn_cost_their_tiers_at_the_exact_deadline() {
    let content = repository_content();

    // Silence: the demand opens when the court window lapses, is never
    // answered, and the strength is never fielded.
    let mut silent = scenario_host(328, Arc::clone(&content));
    let start = start_date(&mut silent);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    silent.advance_days(7);
    assert!(
        aleyn_card(&mut silent).is_some(),
        "a lapsed court still opens the household demands"
    );
    let remaining = silent.date().days_until(deadline);
    silent.advance_days(remaining as u32 - 1);
    assert!(
        aleyn_card(&mut silent).is_some(),
        "the demand is still live the day before the deadline"
    );
    silent.advance_days(1);
    let notice = aleyn_resolution(&mut silent).expect("deadline resolution");
    assert_eq!(notice.outcome, key("ignored"));
    assert_eq!(notice.resolved, deadline);
    let aleyn = character(&mut silent, "aleyn-harrow");
    let harrow = org(&mut silent, "harrow");
    let edrun = aeon_sim::access::org_head(silent.world_mut(), harrow).expect("head");
    let entry =
        opinion_modifier(&mut silent, aleyn, "aleyn-levies-ignored").expect("silence modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, -10);
    assert_eq!(entry.expires, Some(deadline.add_days(1440)));

    // A promise given and missed: -20 for 1,800 days.
    let mut broken = scenario_host(329, Arc::clone(&content));
    open_household_demands(&mut broken);
    answer_aleyn(&mut broken, "promise").expect("promising is an ordinary command");
    broken.advance_days(1);
    assert_eq!(
        aleyn_card(&mut broken)
            .expect("live demand")
            .projection
            .expect("projection")
            .stage,
        key("promised")
    );
    let remaining = broken.date().days_until(deadline);
    broken.advance_days(remaining as u32);
    let notice = aleyn_resolution(&mut broken).expect("deadline resolution");
    assert_eq!(notice.outcome, key("broken"));
    let aleyn = character(&mut broken, "aleyn-harrow");
    let entry = opinion_modifier(&mut broken, aleyn, "aleyn-levies-broken")
        .expect("broken-promise modifier");
    assert_eq!(entry.amount, -20);
    assert_eq!(entry.expires, Some(deadline.add_days(1800)));

    // The strength fielded exactly on the deadline day still counts as
    // achievement.
    let mut boundary = scenario_host(330, Arc::clone(&content));
    boundary.advance_days(7);
    let remaining = boundary.date().days_until(deadline);
    boundary.advance_days(remaining as u32 - 1);
    assert!(aleyn_card(&mut boundary).is_some());
    raise_fielded_manpower_to_target(&mut boundary);
    boundary.advance_days(1);
    let notice = aleyn_resolution(&mut boundary).expect("boundary resolution");
    assert_eq!(notice.outcome, key("achieved"));
    assert_eq!(notice.resolved, deadline);
}

#[test]
fn the_demand_passes_on_when_aleyn_dies_and_the_successor_takes_it_up() {
    let mut host = scenario_host(331, repository_content());
    open_household_demands(&mut host);
    answer_aleyn(&mut host, "promise").expect("promise accepted");
    host.advance_days(1);
    let aleyn = character(&mut host, "aleyn-harrow");
    let lira = character(&mut host, "captain-lira");

    let date = host.date();
    process_death(host.world_mut(), aleyn, date);
    evaluate(host.world_mut());

    // The dead requester's lifecycle ends without a relationship penalty.
    let notice = aleyn_resolution(&mut host).expect("passed-on resolution");
    assert_eq!(notice.outcome, key("passed-on"));
    for reason in [
        "aleyn-levies-achieved",
        "aleyn-levies-refused",
        "aleyn-levies-ignored",
        "aleyn-levies-broken",
    ] {
        assert!(
            opinion_modifier(&mut host, aleyn, reason).is_none(),
            "death carries no household tier ({reason})"
        );
    }

    // The replacement is the authored pure rule: the first living adult
    // non-head member in stable ID order. Stable IDs follow authored key
    // order, so with Aleyn (#47) dead and Brant (#49) a child, Captain
    // Lira (#52) precedes Kessarin (#70). Her lifecycle is a new
    // occurrence, so the promise made to Aleyn does not transfer.
    let card = aleyn_card(&mut host).expect("the successor presses the demand");
    assert_eq!(
        card.active.key.bindings.get("requester"),
        Some(&SituationSubject::Character(lira))
    );
    assert_eq!(
        aeon_sim::situations::recorded_answer(host.world_mut(), &card.active.key),
        None,
        "a reactivation starts unanswered"
    );
}

#[test]
fn aleyn_answers_validate_apply_once_and_survive_snapshots() {
    let content = repository_content();
    let mut host = scenario_host(332, Arc::clone(&content));
    open_household_demands(&mut host);
    let situation = aleyn_card(&mut host).expect("live demand").active.key;

    // Spectators and other houses cannot answer, and only declared
    // responses exist.
    host.world_mut().resource_mut::<PlayerHouse>().0 = None;
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Assignment(
            AssignmentRejection::NoPlayerOrg
        ))
    ));
    let veyrin = org(&mut host, "veyrin");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(veyrin);
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Situation(_))
    ));
    let harrow = org(&mut host, "harrow");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(harrow);
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("dither"),
        }),
        Err(CommandRejection::Situation(_))
    ));

    // Two answers queued the same day: the first applies, the second is
    // dropped by the same re-validation every delayed command runs.
    answer_aleyn(&mut host, "refuse").expect("first answer accepted");
    answer_aleyn(&mut host, "promise").expect("second accepted at submission");
    host.advance_days(1);
    assert_eq!(
        aeon_sim::situations::recorded_answer(host.world_mut(), &situation),
        Some(key("refuse")),
        "the first recorded answer is final"
    );
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Situation(_))
    ));

    // The recorded answer is tagged permanent history and durable state.
    let occurrence = aleyn_card(&mut host)
        .expect("live demand")
        .active
        .occurrence();
    assert!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&occurrence) && entry.text.contains("Refuse")),
        "the answer wrote a tagged history line"
    );
    let hash = host.state_hash();
    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert_eq!(restored.state_hash(), hash);
    assert_eq!(
        aeon_sim::situations::recorded_answer(restored.world_mut(), &situation),
        Some(key("refuse")),
        "the answer survives save and load"
    );
}

#[test]
fn aleyn_lifecycles_snapshot_and_replay_across_their_resolutions() {
    let content = repository_content();

    // Path one: an early achievement. The direct army fixture lands before
    // the resolution day is settled, so every checkpoint is a settled
    // state and every replay is command-driven from there.
    let mut achieved = scenario_host(333, Arc::clone(&content));
    open_household_demands(&mut achieved);
    achieved.advance_days(12);
    raise_fielded_manpower_to_target(&mut achieved);
    achieved.advance_days(1);
    let mut achieved_checkpoints = vec![achieved.snapshot()];
    achieved.advance_days(10);
    achieved_checkpoints.push(achieved.snapshot());

    // Path two: a promise left to break, checkpointed before, at, and after
    // the shared deadline.
    let mut broken = scenario_host(334, Arc::clone(&content));
    open_household_demands(&mut broken);
    answer_aleyn(&mut broken, "promise").expect("promise accepted");
    broken.advance_days(1);
    let start = start_date(&mut broken);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let remaining = broken.date().days_until(deadline);
    broken.advance_days(remaining as u32 - 3);
    let mid = broken.snapshot();
    assert!(
        !mid.state.situations.answers.is_empty(),
        "the promise is snapshotted authoritative state"
    );
    let mut broken_checkpoints = vec![mid];
    broken.advance_days(3);
    broken_checkpoints.push(broken.snapshot());
    broken.advance_days(3);
    broken_checkpoints.push(broken.snapshot());

    for (index, (mut host, checkpoints)) in [
        (achieved, achieved_checkpoints),
        (broken, broken_checkpoints),
    ]
    .into_iter()
    .enumerate()
    {
        let final_day = start_date(&mut host).add_days(150);
        let remaining = host.date().days_until(final_day);
        host.advance_days(remaining as u32);
        let final_hash = host.state_hash();
        for snapshot in checkpoints {
            let expected_mid = snapshot.state_hash;
            let mut replayed =
                SimHost::restore_with_content(snapshot, Arc::clone(&content)).unwrap();
            assert_eq!(replayed.state_hash(), expected_mid, "restore is exact");
            let remaining = replayed.date().days_until(final_day);
            replayed.advance_days(remaining as u32);
            assert_eq!(
                replayed.state_hash(),
                final_hash,
                "every checkpoint replays to the same final state (path {index})"
            );
        }
    }
}

/// A campaign seed under which Edrun's muster rolls a plain failure.
const MUSTER_FAILURE_SEED: u64 = 341;

fn org_resources(host: &mut SimHost, org: OrgId) -> (i64, i64) {
    let world = host.world_mut();
    let entity = aeon_sim::access::org_entity(world, org).expect("indexed organisation");
    let resources = world
        .get::<aeon_sim::OrgResources>(entity)
        .expect("organisations carry resources");
    (resources.wealth, resources.influence)
}

#[test]
fn a_failed_muster_is_an_ordinary_forecast_loss_with_no_special_protection() {
    let content = repository_content();
    // A twin campaign on the same seed that never musters: derived RNG
    // streams keep every other system identical, so any final resource
    // difference is exactly the muster's own footprint.
    let mut host = scenario_host(MUSTER_FAILURE_SEED, Arc::clone(&content));
    let mut idle = scenario_host(MUSTER_FAILURE_SEED, content);

    // Let the court lapse so the head is free and the demands are open.
    host.advance_days(7);
    idle.advance_days(7);
    let harrow = org(&mut host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    let occurrence = aleyn_card(&mut host)
        .expect("live demand")
        .active
        .occurrence();
    let (wealth_before, influence_before) = org_resources(&mut host, harrow);

    let envelope = host
        .submit(PlayerCommand::StartAssignment {
            assignment: key("muster"),
            leader: edrun,
            target: AssignmentTarget::None,
        })
        .expect("muster is an ordinary valid command");
    while host.date() < envelope.day {
        host.advance_days(1);
    }

    // The authored costs are paid when the order is accepted, before any
    // roll: 50 wealth and 10 Influence, gone up front.
    let (wealth_started, influence_started) = org_resources(&mut host, harrow);
    assert_eq!(wealth_started, wealth_before - 50);
    assert_eq!(influence_started, influence_before - 10);

    // Past the 40-day duration the roll has failed: no army formed, no
    // costs returned, and the demand simply stays live — the same open
    // lifecycle, no compensation, no protection.
    let after_resolution = 46;
    host.advance_days(after_resolution);
    idle.advance_days(1 + after_resolution);
    assert_eq!(
        fielded_manpower(&mut host, harrow),
        STARTING_FIELDED_MANPOWER,
        "a failed muster fields nothing"
    );
    let (wealth_after, influence_after) = org_resources(&mut host, harrow);
    let (wealth_idle, influence_idle) = org_resources(&mut idle, harrow);
    assert_eq!(
        wealth_idle - wealth_after,
        50,
        "the wealth cost is never refunded"
    );
    assert_eq!(
        influence_idle - influence_after,
        10,
        "the Influence cost is never refunded"
    );
    assert!(
        aleyn_resolution(&mut host).is_none(),
        "a failed attempt resolves nothing"
    );
    let card = aleyn_card(&mut host).expect("the demand stays live through the failure");
    assert_eq!(
        card.active.occurrence(),
        occurrence,
        "the same lifecycle continues — failure neither resolves nor resets it"
    );

    // A retry is the same ordinary command, accepted on its own merits.
    host.submit(PlayerCommand::StartAssignment {
        assignment: key("muster"),
        leader: edrun,
        target: AssignmentTarget::None,
    })
    .expect("a retry is an ordinary assignment again");
}

// ---------------------------------------------------------------------------
// Torvald's Standing: the third household demand — a live derived-opinion
// predicate over the bound liege head's regard for the house's head.
// ---------------------------------------------------------------------------

use aeon_sim::politics::{OpinionEntry, OrgRecord};

/// The authored derived-opinion target of Torvald's demand.
const TORVALD_OPINION_TARGET: i32 = 0;

/// Casimir's derived opening opinion of Edrun: grasping opposing
/// magnanimous, and nothing else — exactly -15.
const STARTING_LIEGE_OPINION: i32 = -15;

fn torvald_card(host: &mut SimHost) -> Option<SituationCard> {
    active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition == key("torvald-standing"))
}

fn torvald_resolution(host: &mut SimHost) -> Option<aeon_sim::situations::SituationResolution> {
    host.world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .find(|notice| notice.situation.definition == key("torvald-standing"))
        .cloned()
}

fn answer_torvald(
    host: &mut SimHost,
    response: &str,
) -> Result<aeon_sim::CommandEnvelope, CommandRejection> {
    let situation = torvald_card(host).expect("live demand").active.key;
    host.submit(PlayerCommand::AnswerSituation {
        situation,
        response: key(response),
    })
}

/// The live derived opinion of the Veyrin head named Casimir about the
/// current Harrow head.
fn liege_opinion(host: &mut SimHost) -> i32 {
    let casimir = character(host, "casimir-veyrin");
    let harrow = org(host, "harrow");
    let head = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    opinion_between(host.world_mut(), casimir, head)
}

/// Sets (or replaces) one direct test modifier on Casimir's ledger toward
/// the live Harrow head — a stand-in for any legitimate relationship
/// effect, in the spirit of `adjust_order` and `form_fixture_army` above.
/// One stable reason means repeated calls replace rather than stack.
fn set_liege_esteem(host: &mut SimHost, amount: i32) {
    let casimir = character(host, "casimir-veyrin");
    let harrow = org(host, "harrow");
    let head = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    let world = host.world_mut();
    let entity = world.resource::<PoliticsIndex>().characters[&casimir];
    world
        .get_mut::<OpinionLedger>(entity)
        .expect("characters carry opinion ledgers")
        .set(OpinionEntry {
            target: head,
            amount,
            reason: "test-esteem".to_owned(),
            expires: None,
        });
}

#[test]
fn torvalds_demand_opens_after_the_court_with_the_shared_deadline_and_live_metrics() {
    let mut host = scenario_host(342, repository_content());
    let start = start_date(&mut host);
    assert!(
        torvald_card(&mut host).is_none(),
        "the household waits until the court's test is behind the reign"
    );

    open_household_demands(&mut host);
    let card = torvald_card(&mut host).expect("the demand opens with the court answered");
    assert_eq!(card.unavailable, None);
    let torvald = character(&mut host, "torvald-harrow");
    let casimir = character(&mut host, "casimir-veyrin");
    assert_eq!(
        card.active.key.bindings.get("requester"),
        Some(&SituationSubject::Character(torvald)),
        "Torvald himself is the bound requester"
    );
    assert_eq!(
        card.active.key.bindings.get("liege-head"),
        Some(&SituationSubject::Character(casimir)),
        "the exact liege head whose regard is demanded is structurally bound"
    );

    let projection = card.projection.clone().expect("projection");
    // The same shared deadline as the other demands: the day the court's
    // window would have closed, plus the authored 120 days.
    assert_eq!(
        projection.deadline,
        Some(start.add_days(HOUSEHOLD_DEADLINE_DAYS))
    );
    let integer_metric = |label: &str| {
        projection.metrics.iter().find_map(|metric| {
            (metric.label_key == label).then(|| match &metric.value {
                aeon_sim::situations::SituationMetricValue::Integer(value) => *value,
                aeon_sim::situations::SituationMetricValue::Text(text) => {
                    panic!("expected integer metric, got '{text}'")
                }
            })
        })
    };
    assert_eq!(
        integer_metric("situation.metric.opinion-target"),
        Some(i64::from(TORVALD_OPINION_TARGET))
    );
    assert_eq!(
        integer_metric("situation.metric.opinion-shortfall"),
        Some(i64::from(TORVALD_OPINION_TARGET - STARTING_LIEGE_OPINION)),
        "the shortfall reads the exact remaining gap"
    );
    let regard = projection
        .metrics
        .iter()
        .find(|metric| metric.label_key == "situation.metric.liege-opinion")
        .expect("the card names the liege head and his live regard");
    match &regard.value {
        aeon_sim::situations::SituationMetricValue::Text(text) => {
            assert!(
                text.contains("Casimir") && text.ends_with(&STARTING_LIEGE_OPINION.to_string()),
                "the row names the bound man and the live derived value, got '{text}'"
            );
        }
        aeon_sim::situations::SituationMetricValue::Integer(value) => {
            panic!("expected a named text metric, got {value}")
        }
    }
    assert_eq!(
        i64::from(liege_opinion(&mut host)),
        i64::from(STARTING_LIEGE_OPINION),
        "the card reads the authoritative derived opinion"
    );
    for consequence in [
        "situation.metric.on-achieved",
        "situation.metric.on-refused",
        "situation.metric.on-ignored",
        "situation.metric.on-broken",
    ] {
        assert!(
            projection
                .metrics
                .iter()
                .any(|metric| metric.label_key == consequence),
            "the card states the relationship consequence {consequence}"
        );
    }
    // The one honest ordinary route: the head courting the liege's house.
    let veyrin = org(&mut host, "veyrin");
    let harrow = org(&mut host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    let court = projection
        .actions
        .iter()
        .find(|action| action.id == key("court"))
        .expect("the card offers the courting route");
    assert_eq!(court.leader, Some(edrun));
    assert_eq!(court.target, AssignmentTarget::Org(veyrin));
    // The bound liege head is a navigable link while his regard lags.
    assert_eq!(projection.links.len(), 1);
    assert_eq!(
        projection.links[0].kind,
        aeon_data::model::SituationSubjectKind::Character
    );
    assert_eq!(projection.links[0].id, casimir.raw());

    // Activation raised a pausing announcement and permanent tagged history.
    let occurrence = card.active.occurrence();
    assert!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&occurrence))
    );
    let popup = host
        .world_mut()
        .resource::<PendingPopups>()
        .popups
        .iter()
        .find(|popup| popup.assignment == key("torvald-standing"))
        .cloned()
        .expect("activation announcement popup");
    assert!(popup.text.contains("Casimir"));
    assert!(popup.text.contains("120-day"));
}

#[test]
fn all_three_household_demands_open_together_and_expire_on_one_date() {
    let mut host = scenario_host(343, repository_content());
    let start = start_date(&mut host);
    for definition in ["kessarin-order", "aleyn-levies", "torvald-standing"] {
        assert!(
            !active_cards(host.world_mut())
                .iter()
                .any(|card| card.active.key.definition == key(definition)),
            "{definition} waits for the court"
        );
    }

    open_household_demands(&mut host);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let expected = [
        ("kessarin-order", "kessarin-harrow"),
        ("aleyn-levies", "aleyn-harrow"),
        ("torvald-standing", "torvald-harrow"),
    ];
    for (definition, requester) in expected {
        let card = active_cards(host.world_mut())
            .into_iter()
            .find(|card| card.active.key.definition == key(definition))
            .unwrap_or_else(|| panic!("{definition} opens with the others"));
        assert_eq!(
            card.projection.expect("projection").deadline,
            Some(deadline),
            "{definition} shares the one visible deadline"
        );
        let who = character(&mut host, requester);
        assert_eq!(
            card.active.key.bindings.get("requester"),
            Some(&SituationSubject::Character(who)),
            "{definition} is pressed by its own named family member"
        );
    }
}

#[test]
fn unmet_demands_resolve_together_at_the_shared_boundary_in_stable_order() {
    let mut host = scenario_host(344, repository_content());
    let start = start_date(&mut host);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let household = [
        key("aleyn-levies"),
        key("kessarin-order"),
        key("torvald-standing"),
    ];

    // Let the court lapse; all three demands open together, unanswered.
    host.advance_days(7);
    let remaining = host.date().days_until(deadline);
    host.advance_days(remaining as u32 - 1);
    let live: Vec<_> = active_cards(host.world_mut())
        .into_iter()
        .map(|card| card.active.key.definition)
        .filter(|definition| household.contains(definition))
        .collect();
    assert_eq!(
        live.len(),
        3,
        "nothing resolves the day before the shared deadline"
    );
    assert!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .all(|notice| !household.contains(&notice.situation.definition)),
        "no household resolution exists before the boundary"
    );

    // The boundary day: all three resolve in the one evaluate pass, each
    // with its own tier on its own requester's ledger, in stable
    // definition-key order with strictly increasing resolution ids.
    host.advance_days(1);
    let mut notices: Vec<_> = host
        .world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .filter(|notice| household.contains(&notice.situation.definition))
        .cloned()
        .collect();
    assert_eq!(
        notices.len(),
        3,
        "all three demands resolve on the boundary"
    );
    notices.sort_by_key(|notice| notice.id);
    assert!(
        notices.windows(2).all(|pair| pair[0].id < pair[1].id),
        "resolution ids are strictly increasing"
    );
    assert_eq!(
        notices
            .iter()
            .map(|notice| notice.situation.definition.clone())
            .collect::<Vec<_>>(),
        household.to_vec(),
        "one pass resolves the demands in stable definition-key order"
    );
    for notice in &notices {
        assert_eq!(notice.outcome, key("ignored"));
        assert_eq!(notice.resolved, deadline, "the boundary day is exact");
    }

    // Three distinct silence modifiers on three distinct ledgers.
    let harrow = org(&mut host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("head");
    for (who, reason) in [
        ("kessarin-harrow", "kessarin-order-ignored"),
        ("aleyn-harrow", "aleyn-levies-ignored"),
        ("torvald-harrow", "torvald-standing-ignored"),
    ] {
        let requester = character(&mut host, who);
        let entry = opinion_modifier(&mut host, requester, reason)
            .unwrap_or_else(|| panic!("{who} carries {reason}"));
        assert_eq!(entry.target, edrun);
        assert_eq!(entry.amount, -10);
        assert_eq!(entry.expires, Some(deadline.add_days(1440)));
    }
}

#[test]
fn partial_esteem_stays_live_and_the_exact_zero_boundary_achieves() {
    let mut host = scenario_host(345, repository_content());
    open_household_demands(&mut host);
    let torvald = character(&mut host, "torvald-harrow");
    let harrow = org(&mut host, "harrow");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");

    // One point short of the inclusive target: near-regard is not regard.
    set_liege_esteem(
        &mut host,
        TORVALD_OPINION_TARGET - STARTING_LIEGE_OPINION - 1,
    );
    host.advance_days(1);
    assert_eq!(liege_opinion(&mut host), TORVALD_OPINION_TARGET - 1);
    let card = torvald_card(&mut host).expect("a partly met demand stays live");
    assert!(
        torvald_resolution(&mut host).is_none(),
        "no resolution is recorded while the regard lags"
    );
    let projection = card.projection.clone().expect("projection");
    let shortfall = projection.metrics.iter().find_map(|metric| {
        (metric.label_key == "situation.metric.opinion-shortfall").then(|| match &metric.value {
            aeon_sim::situations::SituationMetricValue::Integer(value) => *value,
            aeon_sim::situations::SituationMetricValue::Text(text) => {
                panic!("expected integer metric, got '{text}'")
            }
        })
    });
    assert_eq!(shortfall, Some(1), "the shortfall reads the exact gap");

    // The goal met at exactly the target is achievement — the boundary is
    // inclusive, and any legitimate relationship effect may close it.
    let before = opinion_between(host.world_mut(), torvald, edrun);
    set_liege_esteem(&mut host, TORVALD_OPINION_TARGET - STARTING_LIEGE_OPINION);
    host.advance_days(1);
    assert_eq!(liege_opinion(&mut host), TORVALD_OPINION_TARGET);
    let resolved_on = host.date();
    assert!(torvald_card(&mut host).is_none());
    let notice = torvald_resolution(&mut host).expect("resolution at exactly the target");
    assert_eq!(notice.outcome, key("achieved"));
    let entry = opinion_modifier(&mut host, torvald, "torvald-standing-achieved")
        .expect("achievement modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, 10);
    assert_eq!(entry.expires, Some(resolved_on.add_days(1440)));
    assert_eq!(
        opinion_between(host.world_mut(), torvald, edrun),
        before + 10
    );
}

#[test]
fn esteem_that_never_stands_on_a_settled_day_resolves_nothing() {
    let mut host = scenario_host(346, repository_content());
    let start = start_date(&mut host);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    open_household_demands(&mut host);
    let occurrence = torvald_card(&mut host)
        .expect("live demand")
        .active
        .occurrence();

    // Regard that rises above the target and falls back inside the same
    // day never stands at a settled evaluation, so it resolves nothing:
    // the predicate is genuinely non-monotonic while the demand is open.
    set_liege_esteem(&mut host, 20);
    set_liege_esteem(&mut host, 10);
    host.advance_days(1);
    assert_eq!(liege_opinion(&mut host), STARTING_LIEGE_OPINION + 10);
    assert!(torvald_resolution(&mut host).is_none());
    let card = torvald_card(&mut host).expect("the demand stays live");
    assert_eq!(
        card.active.occurrence(),
        occurrence,
        "the same lifecycle continues through the swing"
    );

    // The partial gain later withdrawn entirely: still the same lifecycle,
    // and the deadline settles it by the silence tier.
    host.advance_days(5);
    set_liege_esteem(&mut host, 0);
    host.advance_days(1);
    assert_eq!(liege_opinion(&mut host), STARTING_LIEGE_OPINION);
    assert!(torvald_card(&mut host).is_some());
    let remaining = host.date().days_until(deadline);
    host.advance_days(remaining as u32);
    let notice = torvald_resolution(&mut host).expect("deadline resolution");
    assert_eq!(notice.outcome, key("ignored"));
    assert_eq!(notice.resolved, deadline);
    let torvald = character(&mut host, "torvald-harrow");
    let entry =
        opinion_modifier(&mut host, torvald, "torvald-standing-ignored").expect("silence modifier");
    assert_eq!(entry.amount, -10);
    assert_eq!(entry.expires, Some(deadline.add_days(1440)));
}

#[test]
fn refusing_torvald_costs_its_stated_tier_and_achievement_still_overrides_it() {
    let content = repository_content();

    // An honest refusal, left to stand: -5 for 1,080 days at the deadline.
    let mut refused = scenario_host(347, Arc::clone(&content));
    open_household_demands(&mut refused);
    answer_torvald(&mut refused, "refuse").expect("refusing is an ordinary command");
    refused.advance_days(1);
    let card = torvald_card(&mut refused).expect("a refused demand stays live");
    assert_eq!(
        card.projection.as_ref().expect("projection").stage,
        key("refused")
    );
    let start = start_date(&mut refused);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let remaining = refused.date().days_until(deadline);
    refused.advance_days(remaining as u32);
    let notice = torvald_resolution(&mut refused).expect("deadline resolution");
    assert_eq!(notice.outcome, key("refused"));
    assert_eq!(notice.resolved, deadline, "the boundary day is exact");
    let torvald = character(&mut refused, "torvald-harrow");
    let harrow = org(&mut refused, "harrow");
    let edrun = aeon_sim::access::org_head(refused.world_mut(), harrow).expect("head");
    let entry = opinion_modifier(&mut refused, torvald, "torvald-standing-refused")
        .expect("refusal modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, -5);
    assert_eq!(entry.expires, Some(deadline.add_days(1080)));

    // The same refusal followed by the regard anyway: achievement counts
    // whatever was said, and only the achieved tier applies.
    let mut anyway = scenario_host(348, Arc::clone(&content));
    open_household_demands(&mut anyway);
    answer_torvald(&mut anyway, "refuse").expect("refusal accepted");
    anyway.advance_days(1);
    set_liege_esteem(&mut anyway, 40);
    anyway.advance_days(1);
    let notice = torvald_resolution(&mut anyway).expect("resolution");
    assert_eq!(notice.outcome, key("achieved"));
    let torvald = character(&mut anyway, "torvald-harrow");
    assert!(opinion_modifier(&mut anyway, torvald, "torvald-standing-achieved").is_some());
    assert!(
        opinion_modifier(&mut anyway, torvald, "torvald-standing-refused").is_none(),
        "tiers never stack on one lifecycle"
    );
}

#[test]
fn silence_and_broken_promises_to_torvald_cost_their_tiers_at_the_exact_deadline() {
    let content = repository_content();

    // Silence: the demand opens when the court window lapses, is never
    // answered, and the regard is never won.
    let mut silent = scenario_host(349, Arc::clone(&content));
    let start = start_date(&mut silent);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    silent.advance_days(7);
    assert!(
        torvald_card(&mut silent).is_some(),
        "a lapsed court still opens the household demands"
    );
    let remaining = silent.date().days_until(deadline);
    silent.advance_days(remaining as u32 - 1);
    assert!(
        torvald_card(&mut silent).is_some(),
        "the demand is still live the day before the deadline"
    );
    silent.advance_days(1);
    let notice = torvald_resolution(&mut silent).expect("deadline resolution");
    assert_eq!(notice.outcome, key("ignored"));
    assert_eq!(notice.resolved, deadline);
    let torvald = character(&mut silent, "torvald-harrow");
    let harrow = org(&mut silent, "harrow");
    let edrun = aeon_sim::access::org_head(silent.world_mut(), harrow).expect("head");
    let entry = opinion_modifier(&mut silent, torvald, "torvald-standing-ignored")
        .expect("silence modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, -10);
    assert_eq!(entry.expires, Some(deadline.add_days(1440)));

    // A promise given and missed: -20 for 1,800 days.
    let mut broken = scenario_host(350, Arc::clone(&content));
    open_household_demands(&mut broken);
    answer_torvald(&mut broken, "promise").expect("promising is an ordinary command");
    broken.advance_days(1);
    assert_eq!(
        torvald_card(&mut broken)
            .expect("live demand")
            .projection
            .expect("projection")
            .stage,
        key("promised")
    );
    let remaining = broken.date().days_until(deadline);
    broken.advance_days(remaining as u32);
    let notice = torvald_resolution(&mut broken).expect("deadline resolution");
    assert_eq!(notice.outcome, key("broken"));
    let torvald = character(&mut broken, "torvald-harrow");
    let entry = opinion_modifier(&mut broken, torvald, "torvald-standing-broken")
        .expect("broken-promise modifier");
    assert_eq!(entry.amount, -20);
    assert_eq!(entry.expires, Some(deadline.add_days(1800)));

    // The regard won exactly on the deadline day still counts as
    // achievement.
    let mut boundary = scenario_host(351, Arc::clone(&content));
    boundary.advance_days(7);
    let remaining = boundary.date().days_until(deadline);
    boundary.advance_days(remaining as u32 - 1);
    assert!(torvald_card(&mut boundary).is_some());
    set_liege_esteem(
        &mut boundary,
        TORVALD_OPINION_TARGET - STARTING_LIEGE_OPINION,
    );
    boundary.advance_days(1);
    let notice = torvald_resolution(&mut boundary).expect("boundary resolution");
    assert_eq!(notice.outcome, key("achieved"));
    assert_eq!(notice.resolved, deadline);
}

#[test]
fn a_veyrin_succession_passes_the_demand_on_and_never_pays_the_achievement_tier() {
    let mut host = scenario_host(352, repository_content());
    open_household_demands(&mut host);
    answer_torvald(&mut host, "promise").expect("promise accepted");
    host.advance_days(1);
    let torvald = character(&mut host, "torvald-harrow");
    let casimir = character(&mut host, "casimir-veyrin");
    let aldric = character(&mut host, "aldric-veyrin");
    let harrow = org(&mut host, "harrow");
    let veyrin = org(&mut host, "veyrin");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");

    let date = host.date();
    process_death(host.world_mut(), casimir, date);
    evaluate(host.world_mut());

    // Succession installs Casimir's heir, whose derived regard for Edrun
    // (shared forthright, no opposed pair) already clears the target. The
    // achieved outcome must judge the BOUND man, not the live liege head:
    // the same evaluation that ends this lifecycle must pass it on, not
    // quietly pay the achievement tier from a warmer successor.
    assert_eq!(
        aeon_sim::access::org_head(host.world_mut(), veyrin),
        Some(aldric),
        "the heir heads House Veyrin"
    );
    assert_eq!(
        opinion_between(host.world_mut(), aldric, edrun),
        15,
        "the hazard is real: the live liege head clears the target at once"
    );
    let notice = torvald_resolution(&mut host).expect("passed-on resolution");
    assert_eq!(notice.outcome, key("passed-on"));
    for reason in [
        "torvald-standing-achieved",
        "torvald-standing-refused",
        "torvald-standing-ignored",
        "torvald-standing-broken",
    ] {
        assert!(
            opinion_modifier(&mut host, torvald, reason).is_none(),
            "a liege succession carries no household tier ({reason})"
        );
    }

    // With the live liege head's regard already at the target, no fresh
    // demand exists to press — the household's concern is settled state,
    // not a checkbox.
    assert!(
        torvald_card(&mut host).is_none(),
        "no lifecycle opens when live state already satisfies the goal"
    );
    assert_eq!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation.definition == key("torvald-standing"))
            .count(),
        1
    );
}

#[test]
fn a_changed_liege_passes_the_demand_on_and_binds_the_new_lieges_head() {
    let mut host = scenario_host(353, repository_content());
    open_household_demands(&mut host);
    answer_torvald(&mut host, "promise").expect("promise accepted");
    host.advance_days(1);
    let torvald = character(&mut host, "torvald-harrow");
    let zorka = character(&mut host, "zorka-draksha");
    let harrow = org(&mut host, "harrow");
    let draksha = org(&mut host, "draksha");

    // Harrow's liege changes: the bound Veyrin head is no longer the head
    // of the house's liege, so the old lifecycle ends without any tier and
    // a new unanswered demand binds the new liege's head.
    {
        let world = host.world_mut();
        let entity = aeon_sim::access::org_entity(world, harrow).expect("indexed organisation");
        world
            .get_mut::<OrgRecord>(entity)
            .expect("organisations carry records")
            .liege = Some(draksha);
    }
    evaluate(host.world_mut());

    let notice = torvald_resolution(&mut host).expect("passed-on resolution");
    assert_eq!(notice.outcome, key("passed-on"));
    for reason in [
        "torvald-standing-achieved",
        "torvald-standing-refused",
        "torvald-standing-ignored",
        "torvald-standing-broken",
    ] {
        assert!(
            opinion_modifier(&mut host, torvald, reason).is_none(),
            "a changed liege carries no household tier ({reason})"
        );
    }
    let card = torvald_card(&mut host).expect("the concern renews against the new liege");
    assert_eq!(
        card.active.key.bindings.get("liege-head"),
        Some(&SituationSubject::Character(zorka)),
        "the new liege's head is the newly bound judge"
    );
    assert_eq!(
        aeon_sim::situations::recorded_answer(host.world_mut(), &card.active.key),
        None,
        "the promise made under the old liege does not transfer"
    );
}

#[test]
fn the_demand_follows_the_office_of_head_of_house() {
    let mut host = scenario_host(354, repository_content());
    open_household_demands(&mut host);
    let torvald = character(&mut host, "torvald-harrow");
    let casimir = character(&mut host, "casimir-veyrin");
    let mikael = character(&mut host, "mikael-harrow");
    let harrow = org(&mut host, "harrow");
    let card = torvald_card(&mut host).expect("live demand");
    assert_eq!(
        card.active.key.bindings.get("liege-head"),
        Some(&SituationSubject::Character(casimir))
    );

    // The house head changes while the same requester and the same bound
    // liege head stand: the demand is about the house's standing, so the
    // goal follows the office. Casimir's derived regard for Mikael carries
    // no opposed pair, so the head swap itself clears the target — and the
    // achieved tier lands toward the live head, through the ordinary
    // owner-head effect role.
    {
        let world = host.world_mut();
        let entity = aeon_sim::access::org_entity(world, harrow).expect("indexed organisation");
        world
            .get_mut::<OrgRecord>(entity)
            .expect("organisations carry records")
            .head = Some(mikael);
    }
    assert_eq!(opinion_between(host.world_mut(), casimir, mikael), 0);
    evaluate(host.world_mut());
    let resolved_on = host.date();

    let notice = torvald_resolution(&mut host).expect("resolution");
    assert_eq!(notice.outcome, key("achieved"));
    let entry = opinion_modifier(&mut host, torvald, "torvald-standing-achieved")
        .expect("achievement modifier");
    assert_eq!(
        entry.target, mikael,
        "the tier falls on the live head of house"
    );
    assert_eq!(entry.amount, 10);
    assert_eq!(entry.expires, Some(resolved_on.add_days(1440)));
    assert!(torvald_card(&mut host).is_none());
}

#[test]
fn the_demand_passes_on_when_torvald_dies_and_the_successor_takes_it_up() {
    let mut host = scenario_host(355, repository_content());
    open_household_demands(&mut host);
    answer_torvald(&mut host, "promise").expect("promise accepted");
    host.advance_days(1);
    let torvald = character(&mut host, "torvald-harrow");
    let aleyn = character(&mut host, "aleyn-harrow");
    let casimir = character(&mut host, "casimir-veyrin");

    let date = host.date();
    process_death(host.world_mut(), torvald, date);
    evaluate(host.world_mut());

    // The dead requester's lifecycle ends without a relationship penalty.
    let notice = torvald_resolution(&mut host).expect("passed-on resolution");
    assert_eq!(notice.outcome, key("passed-on"));
    for reason in [
        "torvald-standing-achieved",
        "torvald-standing-refused",
        "torvald-standing-ignored",
        "torvald-standing-broken",
    ] {
        assert!(
            opinion_modifier(&mut host, torvald, reason).is_none(),
            "death carries no household tier ({reason})"
        );
    }

    // The replacement is the authored pure rule: the first living adult
    // non-head member in stable ID order — Aleyn, who now presses two
    // demands at once. Her lifecycle is a new occurrence bound to the same
    // liege head, so the promise made by Torvald does not transfer.
    let card = torvald_card(&mut host).expect("the successor presses the demand");
    assert_eq!(
        card.active.key.bindings.get("requester"),
        Some(&SituationSubject::Character(aleyn))
    );
    assert_eq!(
        card.active.key.bindings.get("liege-head"),
        Some(&SituationSubject::Character(casimir)),
        "the bound judge of the demand is unchanged"
    );
    assert_eq!(
        aeon_sim::situations::recorded_answer(host.world_mut(), &card.active.key),
        None,
        "a reactivation starts unanswered"
    );
}

#[test]
fn an_early_achievement_resolves_independently_of_the_other_demands() {
    let content = repository_content();
    let mut host = scenario_host(356, Arc::clone(&content));
    open_household_demands(&mut host);
    let start = start_date(&mut host);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);

    // Torvald alone is satisfied early; the other two run to the boundary.
    set_liege_esteem(&mut host, 40);
    host.advance_days(1);
    let torvald_notice = torvald_resolution(&mut host).expect("early resolution");
    assert_eq!(torvald_notice.outcome, key("achieved"));
    assert!(torvald_notice.resolved < deadline);
    assert!(
        kessarin_card(&mut host).is_some() && aleyn_card(&mut host).is_some(),
        "the sibling demands stay live and unresolved"
    );

    let remaining = host.date().days_until(deadline);
    host.advance_days(remaining as u32);
    assert_eq!(
        kessarin_resolution(&mut host)
            .expect("boundary tier")
            .outcome,
        key("ignored")
    );
    assert_eq!(
        aleyn_resolution(&mut host).expect("boundary tier").outcome,
        key("ignored")
    );

    // Completion is history while the metric keeps moving: the regard
    // later collapses, and nothing reopens or retracts past the window.
    set_liege_esteem(&mut host, -40);
    host.advance_days(2);
    assert!(liege_opinion(&mut host) < TORVALD_OPINION_TARGET);
    assert!(torvald_card(&mut host).is_none());
    assert_eq!(
        host.world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| notice.situation.definition == key("torvald-standing"))
            .count(),
        1
    );
    let torvald = character(&mut host, "torvald-harrow");
    assert!(
        opinion_modifier(&mut host, torvald, "torvald-standing-achieved").is_some(),
        "the achieved tier still stands while the liege's regard moves on"
    );

    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert!(
        restored
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .any(|notice| notice.situation.definition == key("torvald-standing")),
        "the resolution is durable across save and load"
    );
}

#[test]
fn torvald_answers_validate_apply_once_and_survive_snapshots() {
    let content = repository_content();
    let mut host = scenario_host(357, Arc::clone(&content));
    open_household_demands(&mut host);
    let situation = torvald_card(&mut host).expect("live demand").active.key;

    // Spectators and other houses cannot answer, and only declared
    // responses exist.
    host.world_mut().resource_mut::<PlayerHouse>().0 = None;
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Assignment(
            AssignmentRejection::NoPlayerOrg
        ))
    ));
    let veyrin = org(&mut host, "veyrin");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(veyrin);
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Situation(_))
    ));
    let harrow = org(&mut host, "harrow");
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(harrow);
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("dither"),
        }),
        Err(CommandRejection::Situation(_))
    ));

    // Two answers queued the same day: the first applies, the second is
    // dropped by the same re-validation every delayed command runs.
    answer_torvald(&mut host, "refuse").expect("first answer accepted");
    answer_torvald(&mut host, "promise").expect("second accepted at submission");
    host.advance_days(1);
    assert_eq!(
        aeon_sim::situations::recorded_answer(host.world_mut(), &situation),
        Some(key("refuse")),
        "the first recorded answer is final"
    );
    assert!(matches!(
        host.submit(PlayerCommand::AnswerSituation {
            situation: situation.clone(),
            response: key("promise"),
        }),
        Err(CommandRejection::Situation(_))
    ));

    // The recorded answer is tagged permanent history and durable state.
    let occurrence = torvald_card(&mut host)
        .expect("live demand")
        .active
        .occurrence();
    assert!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&occurrence) && entry.text.contains("Refuse")),
        "the answer wrote a tagged history line"
    );
    let hash = host.state_hash();
    let mut restored = SimHost::restore_with_content(host.snapshot(), content).unwrap();
    assert_eq!(restored.state_hash(), hash);
    assert_eq!(
        aeon_sim::situations::recorded_answer(restored.world_mut(), &situation),
        Some(key("refuse")),
        "the answer survives save and load"
    );
}

#[test]
fn torvald_lifecycles_snapshot_and_replay_across_their_resolutions() {
    let content = repository_content();

    // Path one: an early achievement. The direct esteem fixture lands
    // before the resolution day is settled, so every checkpoint is a
    // settled state and every replay is command-driven from there.
    let mut achieved = scenario_host(358, Arc::clone(&content));
    open_household_demands(&mut achieved);
    achieved.advance_days(12);
    set_liege_esteem(&mut achieved, 40);
    achieved.advance_days(1);
    let mut achieved_checkpoints = vec![achieved.snapshot()];
    achieved.advance_days(10);
    achieved_checkpoints.push(achieved.snapshot());

    // Path two: a promise left to break, checkpointed before, exactly on,
    // and after the shared deadline. The middle checkpoint lands on the
    // triple-resolution day itself: the two unanswered sibling demands and
    // the broken promise all resolve in that one evaluate pass.
    let mut broken = scenario_host(359, Arc::clone(&content));
    open_household_demands(&mut broken);
    answer_torvald(&mut broken, "promise").expect("promise accepted");
    broken.advance_days(1);
    let start = start_date(&mut broken);
    let deadline = start.add_days(HOUSEHOLD_DEADLINE_DAYS);
    let remaining = broken.date().days_until(deadline);
    broken.advance_days(remaining as u32 - 3);
    let mid = broken.snapshot();
    assert!(
        !mid.state.situations.answers.is_empty(),
        "the promise is snapshotted authoritative state"
    );
    let mut broken_checkpoints = vec![mid];
    broken.advance_days(3);
    assert_eq!(broken.date(), deadline);
    assert_eq!(
        broken
            .world_mut()
            .resource::<SituationState>()
            .resolutions
            .iter()
            .filter(|notice| {
                [
                    key("aleyn-levies"),
                    key("kessarin-order"),
                    key("torvald-standing"),
                ]
                .contains(&notice.situation.definition)
            })
            .count(),
        3,
        "the checkpoint day carries all three boundary resolutions"
    );
    broken_checkpoints.push(broken.snapshot());
    broken.advance_days(3);
    broken_checkpoints.push(broken.snapshot());

    for (index, (mut host, checkpoints)) in [
        (achieved, achieved_checkpoints),
        (broken, broken_checkpoints),
    ]
    .into_iter()
    .enumerate()
    {
        let final_day = start_date(&mut host).add_days(150);
        let remaining = host.date().days_until(final_day);
        host.advance_days(remaining as u32);
        let final_hash = host.state_hash();
        for snapshot in checkpoints {
            let expected_mid = snapshot.state_hash;
            let mut replayed =
                SimHost::restore_with_content(snapshot, Arc::clone(&content)).unwrap();
            assert_eq!(replayed.state_hash(), expected_mid, "restore is exact");
            let remaining = replayed.date().days_until(final_day);
            replayed.advance_days(remaining as u32);
            assert_eq!(
                replayed.state_hash(),
                final_hash,
                "every checkpoint replays to the same final state (path {index})"
            );
        }
    }
}

/// A campaign seed under which Edrun's courting of House Veyrin rolls a
/// plain success and nothing else moves Casimir's opinion of him first.
const COURT_SUCCESS_SEED: u64 = 360;

#[test]
fn courting_the_liege_is_the_authored_route_and_its_success_achieves_the_demand() {
    let mut host = scenario_host(COURT_SUCCESS_SEED, repository_content());

    // Let the court lapse so the head is free and the demands are open.
    host.advance_days(7);
    let harrow = org(&mut host, "harrow");
    let veyrin = org(&mut host, "veyrin");
    let edrun = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    let casimir = character(&mut host, "casimir-veyrin");
    let torvald = character(&mut host, "torvald-harrow");
    assert!(torvald_card(&mut host).is_some());

    // The card's own route, issued as the ordinary org-targeted command it
    // launches: Edrun personally courting House Veyrin.
    let envelope = host
        .submit(PlayerCommand::StartAssignment {
            assignment: key("court"),
            leader: edrun,
            target: AssignmentTarget::Org(veyrin),
        })
        .expect("courting the liege is an ordinary valid command");
    while host.date() < envelope.day {
        host.advance_days(1);
    }
    host.advance_days(46);

    // A plain success led by the head lands both authored modifiers on
    // Casimir — his regard for Edrun the man and for the house Edrun
    // heads — and their sum lifts the derived opinion over the target.
    let courted = opinion_modifier(&mut host, casimir, "courted").expect("personal regard");
    assert_eq!(courted.target, edrun);
    assert_eq!(courted.amount, 10);
    let courted_house =
        opinion_modifier(&mut host, casimir, "courted-house").expect("house regard");
    assert_eq!(courted_house.target, edrun);
    assert_eq!(courted_house.amount, 10);
    assert_eq!(
        liege_opinion(&mut host),
        STARTING_LIEGE_OPINION + 20,
        "the two modifiers are the only movement on the pair"
    );

    // The live predicate resolves the demand with the achieved tier.
    let notice = torvald_resolution(&mut host).expect("achievement resolution");
    assert_eq!(notice.outcome, key("achieved"));
    assert!(torvald_card(&mut host).is_none());
    let entry = opinion_modifier(&mut host, torvald, "torvald-standing-achieved")
        .expect("achievement modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, 10);
}

// ---------------------------------------------------------------------------
// The Liege's Visit: the first-year windowed hosted Situation whose live
// forecast reads the liege head's current opinion, and whose lifecycle is
// wholly subject to the simulation.
// ---------------------------------------------------------------------------

use aeon_sim::presence::{CharacterLocation, Location};

/// The authored deterministic window, in days from the campaign start.
const VISIT_OPENS: i64 = 140;
const VISIT_CLOSES: i64 = 180;

/// The authored hospitality tiers: action id, assignment key, wealth cost.
const VISIT_TIERS: [(&str, &str, i64); 3] = [
    ("host-restrained", "host-visit-restrained", 10),
    ("host-proper", "host-visit-proper", 30),
    ("host-lavish", "host-visit-lavish", 60),
];

fn visit_card(host: &mut SimHost) -> Option<SituationCard> {
    active_cards(host.world_mut())
        .into_iter()
        .find(|card| card.active.key.definition == key("casimir-visit"))
}

fn visit_resolutions(host: &mut SimHost) -> Vec<aeon_sim::situations::SituationResolution> {
    host.world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .filter(|notice| notice.situation.definition == key("casimir-visit"))
        .cloned()
        .collect()
}

/// Places a character at a concrete location, for making the liege's head
/// reachable or not through the same presence facts real travel uses.
fn place_character(host: &mut SimHost, character: CharacterId, location: Location) {
    let world = host.world_mut();
    let entity = world.resource::<PoliticsIndex>().characters[&character];
    world.entity_mut(entity).insert(CharacterLocation(location));
}

fn any_ship(host: &mut SimHost) -> aeon_sim::ShipId {
    *host
        .world_mut()
        .resource::<ForcesIndex>()
        .ships
        .keys()
        .next()
        .expect("the scenario fields ships")
}

fn harrow_wealth(host: &mut SimHost) -> i64 {
    let harrow = org(host, "harrow");
    let world = host.world_mut();
    let entity = aeon_sim::access::org_entity(world, harrow).expect("indexed organisation");
    world
        .get::<aeon_sim::OrgResources>(entity)
        .expect("organisations carry resources")
        .wealth
}

fn visit_forecast(
    host: &mut SimHost,
    action: &str,
    leader: CharacterId,
) -> aeon_sim::forecast::AssignmentForecast {
    let situation = visit_card(host).expect("live visit").active.key;
    aeon_sim::situations::forecast_for_action(
        host.world_mut(),
        &situation,
        &key(action),
        leader,
        AssignmentTarget::None,
    )
    .expect("the projected tier forecasts")
}

#[test]
fn the_visit_opens_exactly_with_its_window_and_reads_the_liege_heads_live_regard() {
    let mut host = scenario_host(371, repository_content());
    let start = start_date(&mut host);
    host.advance_days(VISIT_OPENS as u32 - 1);
    assert!(
        visit_card(&mut host).is_none(),
        "the visit does not exist the day before its window"
    );
    assert!(visit_resolutions(&mut host).is_empty());

    host.advance_days(1);
    let card = visit_card(&mut host).expect("the visit opens on the exact opening day");
    assert_eq!(card.unavailable, None);
    let casimir = character(&mut host, "casimir-veyrin");
    let harrow = org(&mut host, "harrow");
    assert_eq!(
        card.active.key.bindings.get("liege-head"),
        Some(&SituationSubject::Character(casimir)),
        "the live liege head is structurally bound"
    );
    assert_eq!(
        card.active.key.bindings.get("house"),
        Some(&SituationSubject::Organisation(harrow))
    );
    assert_eq!(card.active.activated, start.add_days(VISIT_OPENS));

    let projection = card.projection.clone().expect("projection");
    assert_eq!(projection.deadline, Some(start.add_days(VISIT_CLOSES)));
    let days_left = projection.metrics.iter().find_map(|metric| {
        (metric.label_key == "situation.metric.days-left").then(|| match &metric.value {
            aeon_sim::situations::SituationMetricValue::Integer(value) => *value,
            aeon_sim::situations::SituationMetricValue::Text(text) => {
                panic!("expected integer metric, got '{text}'")
            }
        })
    });
    assert_eq!(days_left, Some(VISIT_CLOSES - VISIT_OPENS));
    let live = liege_opinion(&mut host);
    let regard = projection
        .metrics
        .iter()
        .find(|metric| metric.label_key == "situation.metric.liege-opinion")
        .expect("the card names the liege head and his live regard");
    match &regard.value {
        aeon_sim::situations::SituationMetricValue::Text(text) => {
            assert!(
                text.contains("Casimir") && text.ends_with(&live.to_string()),
                "the row names the bound man and the live derived value, got '{text}'"
            );
        }
        aeon_sim::situations::SituationMetricValue::Integer(value) => {
            panic!("expected a named text metric, got {value}")
        }
    }

    // Three hospitality tiers, each deliberately without a pinned leader:
    // the host is the player's choice, and any eligible member may serve.
    assert_eq!(
        projection
            .actions
            .iter()
            .map(|action| action.id.as_str().to_owned())
            .collect::<Vec<_>>(),
        ["host-restrained", "host-proper", "host-lavish"]
    );
    for action in &projection.actions {
        assert_eq!(
            action.leader, None,
            "{}: the host is a free choice",
            action.id
        );
        assert_eq!(action.target, AssignmentTarget::None);
    }

    // Activation raised a pausing announcement and permanent tagged history.
    let occurrence = card.active.occurrence();
    assert!(
        host.world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&occurrence))
    );
    assert!(
        host.world_mut()
            .resource::<PendingPopups>()
            .popups
            .iter()
            .any(|popup| popup.assignment == key("casimir-visit")),
        "activation announces through the ordinary pausing popup channel"
    );
}

/// The nth currently free adult of the house, in stable ID order: any of
/// them is a legal host, which is the point of a leaderless action.
fn free_household_host(host: &mut SimHost, index: usize) -> CharacterId {
    let harrow = org(host, "harrow");
    let date = host.date();
    let world = host.world_mut();
    let free: Vec<CharacterId> = world
        .resource::<PoliticsIndex>()
        .characters
        .keys()
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|id| {
            aeon_sim::leader_availability(world, harrow, *id, date)
                .blocks_assignment(AssignmentTarget::None)
                .is_none()
        })
        .collect();
    assert!(!free.is_empty(), "the household has a free host");
    free[index % free.len()]
}

#[test]
fn every_hospitality_tier_starts_an_ordinary_costed_assignment_and_resolves_the_visit_hosted() {
    for (index, (action, assignment, wealth_cost)) in VISIT_TIERS.into_iter().enumerate() {
        let content = repository_content();
        let seed = 372 + index as u64;
        // A twin campaign on the same seed that never hosts: derived RNG
        // streams are isolated, so every difference between the two is
        // exactly the hosting order's own consequence.
        let mut hosted = scenario_host(seed, Arc::clone(&content));
        let mut idle = scenario_host(seed, Arc::clone(&content));
        hosted.advance_days(VISIT_OPENS as u32);
        idle.advance_days(VISIT_OPENS as u32);

        let situation = visit_card(&mut hosted).expect("live visit").active.key;
        let occurrence = visit_card(&mut hosted)
            .expect("live visit")
            .active
            .occurrence();
        // A different eligible host per tier proves the choice is
        // genuinely free: whoever the house has to spare may serve.
        let leader = free_household_host(&mut hosted, index);
        let envelope = hosted
            .submit(PlayerCommand::StartSituationAssignment {
                situation: situation.clone(),
                action: key(action),
                leader,
                target: AssignmentTarget::None,
                war: None,
            })
            .expect("a projected tier with an eligible host is a valid command");
        while hosted.date() < envelope.day {
            hosted.advance_days(1);
            idle.advance_days(1);
        }

        // The ordinary assignment stands, owned by the house, led by the
        // chosen host, tagged to the exact visit lifecycle, with the
        // authored cost charged on acceptance.
        let active = {
            let world = hosted.world_mut();
            world
                .resource::<AssignmentsIndex>()
                .assignments
                .values()
                .find_map(|entity| {
                    world
                        .get::<ActiveAssignment>(*entity)
                        .filter(|work| work.def == key(assignment))
                        .cloned()
                })
                .unwrap_or_else(|| panic!("{assignment} is running"))
        };
        assert_eq!(active.leader, leader);
        assert_eq!(active.origin_situation, Some(occurrence));
        assert_eq!(
            harrow_wealth(&mut idle) - harrow_wealth(&mut hosted),
            wealth_cost,
            "{assignment} charged exactly its authored wealth on acceptance"
        );

        // Acceptance inside the window resolves the visit hosted, once,
        // durably; the assignment's own results carry the consequences.
        let notices = visit_resolutions(&mut hosted);
        assert_eq!(notices.len(), 1, "{assignment}: one resolution");
        assert_eq!(notices[0].outcome, key("hosted"));
        assert!(visit_card(&mut hosted).is_none());

        // The charge stands in the following days: no refund path exists,
        // and a failed reception refunds nothing either — the same
        // ordinary no-protection rule the muster test proves for costed
        // assignments. (The twins soon diverge legitimately — a household
        // member busy hosting is a member the simulation cannot send
        // elsewhere — so the exact comparison deliberately stays short.)
        hosted.advance_days(2);
        idle.advance_days(2);
        assert_eq!(
            harrow_wealth(&mut idle) - harrow_wealth(&mut hosted),
            wealth_cost,
            "{assignment}: nothing hands the price back"
        );

        // The window can never re-ask: one lifecycle, one resolution.
        let past_completion = hosted.date().days_until(active.completes) + 20;
        hosted.advance_days(past_completion as u32);
        assert_eq!(visit_resolutions(&mut hosted).len(), 1);
        assert!(visit_card(&mut hosted).is_none());
    }
}

#[test]
fn current_opinion_strongly_shifts_the_odds_while_spending_and_a_capable_host_mitigate() {
    let mut host = scenario_host(375, repository_content());
    host.advance_days(VISIT_OPENS as u32);
    let edrun = {
        let harrow = org(&mut host, "harrow");
        aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head")
    };
    let reyn = character(&mut host, "reyn-harrow");

    // The forecast is authoritative about what it read: the live opinion,
    // the clamped shift it produced, and the effectiveness they feed.
    set_liege_esteem(&mut host, -20);
    let cold = visit_forecast(&mut host, "host-proper", edrun);
    assert_eq!(cold.opinion_value, Some(liege_opinion(&mut host)));
    assert_eq!(
        cold.effectiveness,
        cold.skill_value - cold.difficulty + cold.opinion_shift,
        "the shift lives inside the one effectiveness number"
    );
    assert!(cold.opinion_shift < 0, "ill will reads as a penalty");

    // The same tier and host at three relationships: the odds order with
    // the regard, and the swing between the extremes is dramatic.
    let cold_chance = cold.success_chance();
    set_liege_esteem(&mut host, 0);
    let base_chance = visit_forecast(&mut host, "host-proper", edrun).success_chance();
    set_liege_esteem(&mut host, 40);
    let warm = visit_forecast(&mut host, "host-proper", edrun);
    let warm_chance = warm.success_chance();
    assert!(
        cold_chance < base_chance && base_chance < warm_chance,
        "opinion orders the odds: {cold_chance} < {base_chance} < {warm_chance}"
    );
    assert!(
        warm_chance - cold_chance >= 200,
        "current opinion strongly shifts the outcome: {cold_chance} vs {warm_chance}"
    );
    assert!(warm.opinion_shift > 0);

    // Mitigation without determination, judged at the same poor
    // relationship: deeper spending is an easier contest, and a more
    // capable host raises the same tier's odds — while neither erases the
    // relationship's weight.
    set_liege_esteem(&mut host, -20);
    let restrained = visit_forecast(&mut host, "host-restrained", edrun);
    let proper = visit_forecast(&mut host, "host-proper", edrun);
    let lavish = visit_forecast(&mut host, "host-lavish", edrun);
    assert!(
        restrained.success_chance() < proper.success_chance()
            && proper.success_chance() < lavish.success_chance(),
        "spending mitigates a poor relationship"
    );
    assert!(
        lavish.success_chance() < warm_chance,
        "money mitigates the relationship's weight without erasing it: the \
         warmly regarded proper host still outperforms cold lavishness"
    );
    let capable = visit_forecast(&mut host, "host-proper", reyn);
    assert!(
        capable.success_chance() > proper.success_chance(),
        "a better diplomat mitigates the same tier"
    );
    assert_eq!(
        capable.opinion_value, proper.opinion_value,
        "the relationship read is the house's, not the host's"
    );

    // Distinct authored costs, durations, and difficulties are exposed on
    // the authoritative forecast the tiers are compared by.
    assert_eq!(
        (
            restrained.wealth_cost,
            proper.wealth_cost,
            lavish.wealth_cost
        ),
        (10, 30, 60)
    );
    assert_eq!(
        (
            restrained.duration_days,
            proper.duration_days,
            lavish.duration_days
        ),
        (40, 45, 50)
    );
    assert!(restrained.difficulty > proper.difficulty && proper.difficulty > lavish.difficulty);
}

#[test]
fn a_dead_liege_head_passes_the_visit_on_and_the_successors_own_visit_takes_over() {
    let mut host = scenario_host(376, repository_content());
    host.advance_days(VISIT_OPENS as u32 + 1);
    let casimir = character(&mut host, "casimir-veyrin");
    let aldric = character(&mut host, "aldric-veyrin");
    assert_eq!(
        visit_card(&mut host)
            .expect("live visit")
            .active
            .key
            .bindings
            .get("liege-head"),
        Some(&SituationSubject::Character(casimir))
    );
    let wealth_before = harrow_wealth(&mut host);

    let date = host.date();
    process_death(host.world_mut(), casimir, date);
    evaluate(host.world_mut());

    // The bound man's death ends his visit with no tier and no penalty —
    // the same evaluation must never pay hospitality's dues to the dead.
    let notices = visit_resolutions(&mut host);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].outcome, key("passed-on"));
    assert!(
        opinion_modifier(&mut host, casimir, "casimir-visit-slighted").is_none(),
        "a cancelled visit is not a slight"
    );
    assert_eq!(harrow_wealth(&mut host), wealth_before, "no tier was paid");

    // The window still stands and the succession installed a living,
    // reachable liege head, so his own visit binds him: content adapts to
    // the simulation rather than protecting a named man.
    let card = visit_card(&mut host).expect("the successor's visit takes over");
    assert_eq!(
        card.active.key.bindings.get("liege-head"),
        Some(&SituationSubject::Character(aldric))
    );
}

#[test]
fn a_changed_liege_passes_the_visit_on_and_binds_the_new_lieges_head() {
    let mut host = scenario_host(377, repository_content());
    host.advance_days(VISIT_OPENS as u32 + 1);
    let zorka = character(&mut host, "zorka-draksha");
    let harrow = org(&mut host, "harrow");
    let draksha = org(&mut host, "draksha");
    assert!(visit_card(&mut host).is_some());

    {
        let world = host.world_mut();
        let entity = aeon_sim::access::org_entity(world, harrow).expect("indexed organisation");
        world
            .get_mut::<OrgRecord>(entity)
            .expect("organisations carry records")
            .liege = Some(draksha);
    }
    evaluate(host.world_mut());

    let notices = visit_resolutions(&mut host);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].outcome, key("passed-on"));
    let card = visit_card(&mut host).expect("the new liege's own visit stands");
    assert_eq!(
        card.active.key.bindings.get("liege-head"),
        Some(&SituationSubject::Character(zorka))
    );
}

#[test]
fn impossible_travel_delays_the_visit_and_cancels_it_without_penalty_mid_window() {
    let mut host = scenario_host(378, repository_content());
    let casimir = character(&mut host, "casimir-veyrin");
    let ship = any_ship(&mut host);

    // A liege head with no province to set out from cannot come: the
    // window opens and no visit exists, because the trigger asks the same
    // route facts real travel uses.
    host.advance_days(VISIT_OPENS as u32 - 1);
    for _ in 0..3 {
        place_character(&mut host, casimir, Location::Aboard(ship));
        host.advance_days(1);
        assert!(
            visit_card(&mut host).is_none(),
            "an unreachable liege never announces a visit"
        );
    }
    assert!(visit_resolutions(&mut host).is_empty());

    // Back on solid ground inside the window, the visit arrives late
    // rather than never: the arc is delayed by the simulation, not lost.
    let redwater = host.world_mut().resource::<MapIndex>().province_keys[&key("redwater")];
    place_character(&mut host, casimir, Location::Province(redwater));
    host.advance_days(1);
    let card = visit_card(&mut host).expect("the delayed visit activates once travel is possible");
    assert_eq!(
        card.active.activated,
        host.date(),
        "activation is the first reachable settled day"
    );

    // Travel becoming impossible mid-lifecycle cancels the bound visit
    // without any penalty; renewed reachability binds a fresh lifecycle.
    place_character(&mut host, casimir, Location::Aboard(ship));
    host.advance_days(1);
    let notices = visit_resolutions(&mut host);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].outcome, key("passed-on"));
    assert!(
        opinion_modifier(&mut host, casimir, "casimir-visit-slighted").is_none(),
        "a visit that cannot happen slights nobody"
    );
    place_character(&mut host, casimir, Location::Province(redwater));
    host.advance_days(1);
    assert!(visit_card(&mut host).is_some());
}

#[test]
fn a_landless_house_receives_no_visit() {
    let mut host = scenario_host(384, repository_content());
    host.advance_days(VISIT_OPENS as u32 - 1);

    // Strip every Harrow holding: a liege has no seat to visit, so the
    // window opens on nothing — same route facts, no protected arc.
    let harrow = org(&mut host, "harrow");
    let veyrin = org(&mut host, "veyrin");
    let held = held_provinces(host.world_mut(), harrow);
    assert!(!held.is_empty());
    {
        let world = host.world_mut();
        for province in held {
            let (entity, _) = {
                let index = world.resource::<PoliticsIndex>();
                let title = index.province_titles[&province];
                (index.titles[&title], title)
            };
            world
                .get_mut::<aeon_sim::politics::TitleRecord>(entity)
                .expect("titles carry records")
                .holder = aeon_sim::politics::TitleHolder::Org(veyrin);
        }
    }
    host.advance_days(3);
    assert!(visit_card(&mut host).is_none(), "no seat, no visit");
    assert!(visit_resolutions(&mut host).is_empty());
}

#[test]
fn an_unanswered_window_charges_the_stated_slight_exactly_once() {
    let content = repository_content();

    // An order given on the window's last open day applies on the boundary
    // itself, and the boundary reads the court's deadline-day way: the
    // acceptance still answers the visit.
    let mut punctual = scenario_host(379, Arc::clone(&content));
    let close = start_date(&mut punctual).add_days(VISIT_CLOSES);
    punctual.advance_days(VISIT_CLOSES as u32 - 1);
    let card = visit_card(&mut punctual).expect("the visit is live the day before the close");
    assert!(
        card.projection.expect("projection").warning,
        "the closing window raises the shared attention warning"
    );
    let situation = visit_card(&mut punctual).expect("live visit").active.key;
    let edrun = {
        let harrow = org(&mut punctual, "harrow");
        aeon_sim::access::org_head(punctual.world_mut(), harrow).expect("harrow head")
    };
    let envelope = punctual
        .submit(PlayerCommand::StartSituationAssignment {
            situation: situation.clone(),
            action: key("host-proper"),
            leader: edrun,
            target: AssignmentTarget::None,
            war: None,
        })
        .expect("the last-day submission is valid");
    assert_eq!(
        envelope.day, close,
        "the head's order lands on the boundary"
    );
    punctual.advance_days(1);
    let notices = visit_resolutions(&mut punctual);
    assert_eq!(notices.len(), 1);
    assert_eq!(
        notices[0].outcome,
        key("hosted"),
        "deadline-day acceptance answers the visit"
    );
    let started = {
        let world = punctual.world_mut();
        world
            .resource::<AssignmentsIndex>()
            .assignments
            .values()
            .filter_map(|entity| world.get::<ActiveAssignment>(*entity))
            .find(|work| work.def == key("host-visit-proper"))
            .map(|work| work.started)
    };
    assert_eq!(started, Some(close));

    // The same window left wholly unanswered: the slight, exactly once,
    // on the exact boundary day, for the authored term — and a submission
    // after the close is refused as any stale order is.
    let mut slighted = scenario_host(383, Arc::clone(&content));
    let close = start_date(&mut slighted).add_days(VISIT_CLOSES);
    slighted.advance_days(VISIT_CLOSES as u32 - 1);
    let situation = visit_card(&mut slighted).expect("live visit").active.key;
    slighted.advance_days(1);
    let notices = visit_resolutions(&mut slighted);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].outcome, key("slighted"));
    assert_eq!(notices[0].resolved, close, "the boundary day is exact");
    let edrun = {
        let harrow = org(&mut slighted, "harrow");
        aeon_sim::access::org_head(slighted.world_mut(), harrow).expect("harrow head")
    };
    assert!(matches!(
        slighted.submit(PlayerCommand::StartSituationAssignment {
            situation,
            action: key("host-proper"),
            leader: edrun,
            target: AssignmentTarget::None,
            war: None,
        }),
        Err(CommandRejection::Situation(_))
    ));
    let casimir = character(&mut slighted, "casimir-veyrin");
    let entry = opinion_modifier(&mut slighted, casimir, "casimir-visit-slighted")
        .expect("the slight is a durable opinion modifier");
    assert_eq!(entry.target, edrun);
    assert_eq!(entry.amount, -20);
    assert_eq!(entry.expires, Some(close.add_days(1440)));

    // Nothing reopens after the window: no card, no second resolution.
    slighted.advance_days(15);
    assert!(visit_card(&mut slighted).is_none());
    assert_eq!(visit_resolutions(&mut slighted).len(), 1);
}

#[test]
fn visit_lifecycles_snapshot_and_replay_across_their_resolutions() {
    let content = repository_content();
    let final_offset = 230i64;

    // Path one: hosted. Checkpoints once the hosting order is in the
    // snapshotted state — pending on the acceptance eve, mid-visit with
    // the assignment in flight, and after its completion. (A checkpoint
    // taken before a command was submitted describes a campaign where it
    // never happens, which is a different campaign.)
    let mut hosted = scenario_host(380, Arc::clone(&content));
    hosted.advance_days(140);
    let situation = visit_card(&mut hosted).expect("live visit").active.key;
    let host_leader = free_household_host(&mut hosted, 0);
    hosted
        .submit(PlayerCommand::StartSituationAssignment {
            situation,
            action: key("host-lavish"),
            leader: host_leader,
            target: AssignmentTarget::None,
            war: None,
        })
        .expect("hosting is an ordinary valid command");
    let pending = hosted.snapshot();
    assert!(
        !pending.state.pending_commands.is_empty(),
        "the hosting order is snapshotted authoritative state"
    );
    let mut hosted_checkpoints = vec![pending];
    hosted.advance_days(10);
    assert_eq!(
        visit_resolutions(&mut hosted)
            .first()
            .map(|notice| notice.outcome.clone()),
        Some(key("hosted"))
    );
    hosted_checkpoints.push(hosted.snapshot());
    hosted.advance_days(55);
    hosted_checkpoints.push(hosted.snapshot());

    // Path two: slighted, checkpointed before the window, before the
    // boundary, exactly on it, and after it.
    let mut slighted = scenario_host(381, Arc::clone(&content));
    slighted.advance_days(130);
    let mut slighted_checkpoints = vec![slighted.snapshot()];
    slighted.advance_days(VISIT_CLOSES as u32 - 131);
    slighted_checkpoints.push(slighted.snapshot());
    slighted.advance_days(1);
    assert_eq!(
        visit_resolutions(&mut slighted)
            .first()
            .map(|notice| notice.outcome.clone()),
        Some(key("slighted"))
    );
    slighted_checkpoints.push(slighted.snapshot());
    slighted.advance_days(3);
    slighted_checkpoints.push(slighted.snapshot());

    for (index, (mut host, checkpoints)) in [
        (hosted, hosted_checkpoints),
        (slighted, slighted_checkpoints),
    ]
    .into_iter()
    .enumerate()
    {
        let final_day = start_date(&mut host).add_days(final_offset);
        let remaining = host.date().days_until(final_day);
        host.advance_days(remaining as u32);
        let final_hash = host.state_hash();
        for snapshot in checkpoints {
            let expected_mid = snapshot.state_hash;
            let mut replayed =
                SimHost::restore_with_content(snapshot, Arc::clone(&content)).unwrap();
            assert_eq!(replayed.state_hash(), expected_mid, "restore is exact");
            let remaining = replayed.date().days_until(final_day);
            replayed.advance_days(remaining as u32);
            assert_eq!(
                replayed.state_hash(),
                final_hash,
                "every checkpoint replays to the same final state (path {index})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Unquiet Holdings: the covert first-year operation. A hostile, capable
// house mounts deniable sabotage against a border province unaided; the
// targeted holder sees the province, its live Order, the resistance that
// Order applies, and the time remaining — and never the hand behind it —
// while spectators and replay retain complete authoritative provenance.
// ---------------------------------------------------------------------------

/// A seed on which Vantar mounts the operation unaided on the first
/// window pulse (goal and plan on day 180, sabotage accepted on day 181).
const SHADOW_SEED: u64 = 404;
/// A settled day with the seed's operation reliably in flight.
const SHADOW_LIVE_DAY: u32 = 182;

/// Text fragments that would betray the covert provenance if any surface
/// an ordinary player reads carried them: the covert plan's title, the
/// covert goal's title, the sabotage assignment's title, and the open
/// hostile-plan rumour naming the culprit house.
const SHADOW_TELLS: [&str; 4] = [
    "Deniable Pressure",
    "Undermine a Neighbour",
    "Foment Unrest",
    "whispers that House Vantar",
];

fn unquiet_card_for(host: &mut SimHost, house: OrgId) -> Option<SituationCard> {
    active_cards(host.world_mut()).into_iter().find(|card| {
        card.active.key.definition == key("unquiet-holdings")
            && card.active.key.bindings.get("house") == Some(&SituationSubject::Organisation(house))
    })
}

fn vantar_operation(host: &mut SimHost) -> Option<ActiveAssignment> {
    let vantar = org(host, "vantar");
    let world = host.world_mut();
    world
        .resource::<AssignmentsIndex>()
        .assignments
        .values()
        .find_map(|entity| {
            world
                .get::<ActiveAssignment>(*entity)
                .filter(|work| work.owner == vantar && work.def == key("foment-unrest"))
                .cloned()
        })
}

fn integer_metric(
    projection: &aeon_sim::situations::SituationProjection,
    label: &str,
) -> Option<i64> {
    projection.metrics.iter().find_map(|metric| {
        (metric.label_key == label).then(|| match &metric.value {
            aeon_sim::situations::SituationMetricValue::Integer(value) => *value,
            aeon_sim::situations::SituationMetricValue::Text(text) => {
                panic!("expected integer metric for {label}, got '{text}'")
            }
        })
    })
}

#[test]
fn the_covert_operation_mounts_unaided_inside_the_authored_window() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];

    // Before the authored window opens, nothing of the arc exists.
    host.advance_days(179);
    assert!(
        !host
            .world_mut()
            .resource::<aeon_sim::goals::Goals>()
            .active
            .contains_key(&vantar),
        "the covert ambition cannot be adopted before its authored window"
    );
    assert!(vantar_operation(&mut host).is_none());
    assert!(unquiet_card_for(&mut host, harrow).is_none());

    // Inside the window the whole chain mounts with no scripted nudge:
    // goal, plan, sabotage, and the holder's card.
    host.advance_days(SHADOW_LIVE_DAY - 179);
    let goal = host
        .world_mut()
        .resource::<aeon_sim::goals::Goals>()
        .active
        .get(&vantar)
        .cloned()
        .expect("Vantar adopts the covert ambition unaided");
    assert_eq!(goal.def, key("undermine-a-neighbour"));
    assert_eq!(
        goal.target,
        AssignmentTarget::Org(harrow),
        "the hostile border neighbour resolves to Harrow"
    );
    let plan = host
        .world_mut()
        .resource::<aeon_sim::plans::Plans>()
        .active
        .get(&perrin)
        .cloned()
        .expect("the head leads the covert campaign");
    assert_eq!(plan.def, key("deniable-pressure"));
    assert_eq!(
        plan.method, "from-ill-will",
        "hostility is the authored gate that opened"
    );
    assert_eq!(plan.target, AssignmentTarget::Org(harrow));
    let work = vantar_operation(&mut host).expect("the sabotage is in flight");
    assert_eq!(work.leader, perrin);
    assert_eq!(
        work.target,
        AssignmentTarget::Province(vhorruk),
        "the border selector resolves to the one shared border province"
    );

    // The holder's card binds the province, the house, and — structurally,
    // never visibly — the culprit.
    let card = unquiet_card_for(&mut host, harrow).expect("the holder's card is live");
    assert_eq!(card.unavailable, None);
    assert_eq!(
        card.active.key.bindings.get("province"),
        Some(&SituationSubject::Province(vhorruk))
    );
    assert_eq!(
        card.active.key.bindings.get("actor"),
        Some(&SituationSubject::Organisation(vantar)),
        "authoritative provenance is structurally bound for investigation and replay"
    );
    let projection = card.projection.clone().expect("projection");
    assert_eq!(
        projection.deadline,
        Some(work.completes),
        "remaining time is the work's own clock"
    );
    assert_eq!(
        integer_metric(&projection, "situation.metric.days-left"),
        Some(host.date().days_until(work.completes)),
    );

    // The resistance row quotes the exact live number the resolution roll
    // will use, from the one shared calculation.
    let def = host
        .world_mut()
        .resource::<aeon_sim::state::ContentDb>()
        .0
        .assignments[&key("foment-unrest")]
        .clone();
    let reading = aeon_sim::forecast::order_modifier_reading(host.world_mut(), work.target, &def)
        .expect("the sabotage authors an Order modifier");
    assert_eq!(
        reading.0, 800,
        "Vhorruk stands at the settled opening Order"
    );
    assert_eq!(
        integer_metric(&projection, "situation.metric.order-resistance"),
        Some(i64::from(reading.1)),
    );

    // Strengthening the ground mid-flight changes the same live number on
    // the same card: forecast, card, and resolution share one reading.
    aeon_sim::order::adjust_order(host.world_mut(), vhorruk, 150);
    let strengthened = unquiet_card_for(&mut host, harrow)
        .and_then(|card| card.projection)
        .expect("projection");
    assert_eq!(
        integer_metric(&strengthened, "situation.metric.order-resistance"),
        Some(-6),
        "Order 950 against reference 800 resists at the authored scale"
    );
    assert_eq!(
        aeon_sim::forecast::effectiveness(host.world_mut(), vantar, perrin, work.target, &def),
        7 - 10 - 6,
        "the resolution-side effectiveness carries the same live resistance"
    );
}

#[test]
fn no_player_surface_leaks_the_covert_hand_before_exposure() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let edrun = character(&mut host, "edrun-harrow");
    host.advance_days(SHADOW_LIVE_DAY);
    let work = vantar_operation(&mut host).expect("the operation is live");
    let card = unquiet_card_for(&mut host, harrow).expect("the holder's card is live");
    let exact = card.active.occurrence();
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];

    // The projection names the province and nothing else: no culprit
    // organisation, leader, or source plan in participants, groups,
    // links, or actions.
    let projection = card.projection.clone().expect("projection");
    assert_eq!(
        projection.participants,
        vec![aeon_sim::situations::SituationLink {
            kind: aeon_data::model::SituationSubjectKind::Province,
            id: vhorruk.raw(),
            label_key: None,
        }]
    );
    assert!(projection.participant_groups.is_empty());
    assert!(projection.links.is_empty());
    for action in &projection.actions {
        match action.id.as_str() {
            // Steadying the ground is the head's own work, pinned to him
            // and aimed at nothing in particular.
            "hold-court" | "tour-holdings" => {
                assert_eq!(
                    action.leader,
                    Some(edrun),
                    "counter-play is the holder's own work"
                );
                assert_eq!(action.target, AssignmentTarget::None);
            }
            // Investigation deliberately pins NO leader — the player
            // compares investigators — and is aimed at the holder's own
            // troubled ground, never at a suspect.
            "investigate" => {
                assert_eq!(
                    action.leader, None,
                    "the investigation lets the player choose its leader"
                );
                assert_eq!(action.target, AssignmentTarget::Province(vhorruk));
            }
            other => panic!("unexpected projected action '{other}'"),
        }
        assert!(action.context.is_empty());
    }

    // Run the operation to its end so the result and campaign-end lines
    // exist — and then on past the covert AMBITION's own expiry, because
    // the goal's end line is written by the monthly pass long after the
    // sabotage finished and would otherwise fall outside the sweep.
    let vantar = org(&mut host, "vantar");
    while host.date() < work.completes {
        host.advance_days(1);
    }
    host.advance_days(1);
    let ambition = key("undermine-a-neighbour");
    let mut ambition_ended = false;
    for _ in 0..180 {
        if host
            .world_mut()
            .resource::<aeon_sim::goals::Goals>()
            .active
            .get(&vantar)
            .is_none_or(|goal| goal.def != ambition)
        {
            ambition_ended = true;
            break;
        }
        host.advance_days(1);
    }
    assert!(
        ambition_ended,
        "the covert ambition ends inside the swept horizon"
    );

    // Now sweep the ENTIRE log the player can read.
    let log = host.world_mut().resource::<MessageLog>().clone();
    for entry in log
        .entries
        .iter()
        .filter(|entry| entry.audience.visible_to(Some(harrow)))
    {
        for tell in SHADOW_TELLS {
            assert!(
                !entry.text.contains(tell),
                "a player-visible line leaks covert provenance: '{}'",
                entry.text
            );
        }
    }
    // The withheld lines exist — history is complete, not rewritten —
    // and every one of them remains open to spectators and replay.
    let withheld: Vec<_> = log
        .entries
        .iter()
        .filter(|entry| !entry.audience.visible_to(Some(harrow)))
        .collect();
    assert!(
        withheld
            .iter()
            .any(|entry| entry.text.contains("Deniable Pressure")),
        "the covert campaign's own history is written, owner-confided"
    );
    assert!(withheld.iter().all(|entry| entry.audience.visible_to(None)));

    // The ambition's own end line — the last covert writer in the arc,
    // and the one furthest from the operation that produced it — is
    // confided to its owner like every other: withheld from the targeted
    // house, open to spectators and replay.
    let goal_end = log
        .entries
        .iter()
        .find(|entry| {
            entry.text.contains("House Vantar")
                && entry.text.contains("Undermine a Neighbour")
                && (entry.text.contains("set aside") || entry.text.contains("achieved"))
        })
        .expect("the covert ambition's end is written, not swallowed");
    assert!(
        !goal_end.audience.visible_to(Some(harrow)),
        "the ambition's end line leaks the covert goal: '{}'",
        goal_end.text
    );
    assert!(goal_end.audience.visible_to(None));

    // Notifications: the activation announcement reached the player
    // through the ordinary pausing channel, no popup belongs to the
    // sabotage itself, and no pending popup names the hand.
    let popups = host.world_mut().resource::<PendingPopups>().clone();
    assert!(
        popups
            .popups
            .iter()
            .any(|popup| popup.assignment == key("unquiet-holdings")),
        "the alarm announces itself to its audience"
    );
    for popup in &popups.popups {
        assert_ne!(popup.assignment, key("foment-unrest"));
        for tell in SHADOW_TELLS {
            assert!(
                !popup.text.contains(tell),
                "a popup leaks: '{}'",
                popup.text
            );
        }
    }

    // The frozen resolution notice reads the province, not the hand.
    let state = host.world_mut().resource::<SituationState>().clone();
    let notice = state
        .resolutions
        .iter()
        .find(|notice| notice.occurrence() == exact)
        .expect("the ended operation resolved its card");
    assert!(
        matches!(notice.outcome.as_str(), "struck" | "weathered"),
        "the operation's end is judged from the province, got '{}'",
        notice.outcome
    );
    for tell in SHADOW_TELLS {
        assert!(!notice.text.contains(tell));
    }
    assert_eq!(notice.participants.len(), 1);
    assert_eq!(
        notice.participants[0].kind,
        aeon_data::model::SituationSubjectKind::Province
    );
}

#[test]
fn spectators_and_replay_retain_complete_covert_provenance() {
    let content = repository_content();
    let mut original = scenario_host(SHADOW_SEED, Arc::clone(&content));
    let mut twin = scenario_host(SHADOW_SEED, Arc::clone(&content));
    original.advance_days(SHADOW_LIVE_DAY);
    twin.advance_days(SHADOW_LIVE_DAY);
    assert_eq!(
        original.state_hash(),
        twin.state_hash(),
        "equal seeds replay to one hash with the covert operation live"
    );

    let harrow = org(&mut original, "harrow");
    let draksha = org(&mut original, "draksha");
    let card_key = unquiet_card_for(&mut original, harrow)
        .expect("live card")
        .active
        .key;

    // The card is the bound holder's and the spectator's; an outsider
    // sees nothing.
    assert!(aeon_sim::situations::visible_to_player(
        original.world_mut(),
        &card_key
    ));
    original.world_mut().resource_mut::<PlayerHouse>().0 = Some(draksha);
    assert!(!aeon_sim::situations::visible_to_player(
        original.world_mut(),
        &card_key
    ));
    original.world_mut().resource_mut::<PlayerHouse>().0 = None;
    assert!(aeon_sim::situations::visible_to_player(
        original.world_mut(),
        &card_key
    ));
    original.world_mut().resource_mut::<PlayerHouse>().0 = Some(harrow);

    // Every line the player cannot read, the spectator can: secrecy is an
    // audience, never a second history.
    let lines = original
        .world_mut()
        .resource::<MessageLog>()
        .clone()
        .entries;
    let withheld: Vec<_> = lines
        .iter()
        .filter(|entry| !entry.audience.visible_to(Some(harrow)))
        .cloned()
        .collect();
    assert!(
        !withheld.is_empty(),
        "the covert operation left real history"
    );
    assert!(withheld.iter().all(|entry| entry.audience.visible_to(None)));

    // Snapshot and restore keep every line and every recorded audience,
    // and the campaign continues identically afterwards.
    let mut restored = SimHost::restore_with_content(original.snapshot(), content).unwrap();
    assert_eq!(
        restored
            .world_mut()
            .resource::<MessageLog>()
            .clone()
            .entries,
        lines,
        "covert lines and their audiences round-trip through the snapshot"
    );
    assert_eq!(restored.state_hash(), original.state_hash());
    original.advance_days(40);
    restored.advance_days(40);
    assert_eq!(restored.state_hash(), original.state_hash());
}

#[test]
fn live_order_materially_shifts_the_hostile_odds_through_the_one_calculation() {
    let mut host = scenario_host(440, repository_content());
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    let sabotage = key("foment-unrest");
    let target = AssignmentTarget::Province(vhorruk);
    let view = |host: &mut SimHost| {
        aeon_sim::forecast::forecast(host.world_mut(), vantar, &sabotage, perrin, target)
            .expect("the sabotage is defined")
    };

    // At the settled opening Order the modifier reads zero: the authored
    // reference is the neutral point.
    let settled = view(&mut host);
    assert_eq!(settled.order_value, Some(800));
    assert_eq!(settled.order_shift, 0);

    // A province held at the cap resists at the authored clamp, and the
    // resistance moves the same odds the resolution roll obeys.
    aeon_sim::order::adjust_order(host.world_mut(), vhorruk, 200);
    let resisted = view(&mut host);
    assert_eq!(resisted.order_value, Some(1000));
    assert_eq!(resisted.order_shift, -8, "the authored clamp holds");
    assert_eq!(resisted.effectiveness, settled.effectiveness - 8);
    assert!(
        resisted.success_chance() < settled.success_chance(),
        "high Order materially worsens the hostile work's odds"
    );

    // Disorder never helps beyond neutral: max is authored at zero.
    aeon_sim::order::adjust_order(host.world_mut(), vhorruk, -600);
    let lax = view(&mut host);
    assert_eq!(lax.order_value, Some(400));
    assert_eq!(lax.order_shift, 0);
    assert_eq!(lax.success_chance(), settled.success_chance());

    // The forecast number IS the resolution number: both read the one
    // shared effectiveness calculation.
    let def = host
        .world_mut()
        .resource::<aeon_sim::state::ContentDb>()
        .0
        .assignments[&sabotage]
        .clone();
    assert_eq!(
        aeon_sim::forecast::effectiveness(host.world_mut(), vantar, perrin, target, &def),
        lax.effectiveness
    );
}

#[test]
fn a_transferred_target_passes_the_unquiet_card_on_through_ordinary_rules() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let veyrin = org(&mut host, "veyrin");
    host.advance_days(SHADOW_LIVE_DAY);
    let exact = unquiet_card_for(&mut host, harrow)
        .expect("live card")
        .active
        .occurrence();

    // Vhorruk changes hands while the operation is still in flight.
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    {
        let world = host.world_mut();
        let entity = {
            let index = world.resource::<PoliticsIndex>();
            index.titles[&index.province_titles[&vhorruk]]
        };
        world
            .get_mut::<aeon_sim::politics::TitleRecord>(entity)
            .expect("province title")
            .holder = aeon_sim::TitleHolder::Org(veyrin);
    }
    evaluate(host.world_mut());

    // The old lifecycle ends passed-on — old bindings judged against the
    // live world — with no orphaned card and no unearned effects, and the
    // new holder's own lifecycle recomputes from live state.
    let state = host.world_mut().resource::<SituationState>().clone();
    assert!(!state.active.contains_key(&exact.situation));
    let notice = state
        .resolutions
        .iter()
        .find(|notice| notice.occurrence() == exact)
        .expect("the ended lifecycle resolved");
    assert_eq!(notice.outcome, key("passed-on"));
    for tell in SHADOW_TELLS {
        assert!(!notice.text.contains(tell));
    }
    assert!(
        unquiet_card_for(&mut host, veyrin).is_some(),
        "the alarm recomputes for the province's new holder"
    );
}

#[test]
fn a_dead_agent_abandons_the_operation_through_the_ordinary_quiet_rules() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    host.advance_days(SHADOW_LIVE_DAY);
    let work = vantar_operation(&mut host).expect("the operation is live");

    // The agent dies mid-operation; the ordinary dead-leader rule
    // abandons the work when it comes due, and the plan with it. The
    // house's AMBITION legitimately survives him — a successor may mount
    // a fresh operation of their own — so what this proves is that HIS
    // work and HIS campaign ended, through the same rules any assignment
    // and plan obey.
    let date = host.date();
    process_death(host.world_mut(), work.leader, date);
    while host.date() < work.completes {
        host.advance_days(1);
    }
    host.advance_days(1);
    assert!(
        aeon_sim::access::assignment(host.world_mut(), work.id).is_none(),
        "the dead agent's own sabotage did not survive him"
    );
    assert!(
        !host
            .world_mut()
            .resource::<aeon_sim::plans::Plans>()
            .active
            .contains_key(&work.leader),
        "the dead agent's covert campaign was abandoned"
    );

    // The abandonment is logged — and stays owner-confided, because even
    // a failed covert operation must not name its hand on the way out.
    let log = host.world_mut().resource::<MessageLog>().clone();
    let abandoned = log
        .entries
        .iter()
        .find(|entry| entry.text.contains("Foment Unrest") && entry.text.contains("abandoned"))
        .expect("the ordinary dead-leader abandonment is logged");
    assert!(!abandoned.audience.visible_to(Some(harrow)));
    assert!(abandoned.audience.visible_to(None));
}

#[test]
fn holdings_kept_in_high_order_weather_the_operation_whatever_it_rolled() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    host.advance_days(SHADOW_LIVE_DAY);
    vantar_operation(&mut host).expect("the operation is live");
    let exact = unquiet_card_for(&mut host, harrow)
        .expect("live card")
        .active
        .occurrence();

    // The holder answers the alarm the intended way: the province is
    // held at the cap for as long as the campaign persists (the authored
    // plan may retry a failed attempt once, and the card rightly follows
    // the whole campaign). Even a sabotage that LANDS cannot drag Order
    // from the cap below the authored struck line, so however the rolls
    // fall the outcome is weathered — high Order is the resistance, read
    // at resolution from the live province.
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    let mut resolved = None;
    for _ in 0..150 {
        host.advance_days(1);
        let current = aeon_sim::order::province_order(host.world_mut(), vhorruk).order;
        if current < 1000 {
            aeon_sim::order::adjust_order(host.world_mut(), vhorruk, 1000 - current);
        }
        let state = host.world_mut().resource::<SituationState>().clone();
        if let Some(notice) = state
            .resolutions
            .iter()
            .find(|notice| notice.occurrence() == exact)
        {
            resolved = Some(notice.clone());
            break;
        }
    }
    let notice = resolved.expect("the ended operation resolved its card");
    assert_eq!(notice.outcome, key("weathered"));
}

#[test]
fn holdings_left_unsteady_are_struck_when_the_operation_runs_its_course() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    host.advance_days(SHADOW_LIVE_DAY);
    vantar_operation(&mut host).expect("the operation is live");
    let exact = unquiet_card_for(&mut host, harrow)
        .expect("live card")
        .active
        .occurrence();

    // The mirror of weathering, and the other half of the same rule: the
    // holder leaves the ground unsteady for as long as the campaign
    // persists. Because the outcome is a pure live-state reading of the
    // province rather than a peek at the hidden roll, ground held below
    // the authored struck line is struck however the rolls fell.
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    let unsteady = aeon_sim::order::adjust_order(host.world_mut(), vhorruk, -400);
    assert!(
        unsteady < 700,
        "the ground starts below the authored struck line, got {unsteady}"
    );
    let mut resolved = None;
    for _ in 0..150 {
        host.advance_days(1);
        let current = aeon_sim::order::province_order(host.world_mut(), vhorruk).order;
        if current > unsteady {
            aeon_sim::order::adjust_order(host.world_mut(), vhorruk, unsteady - current);
        }
        let state = host.world_mut().resource::<SituationState>().clone();
        if let Some(notice) = state
            .resolutions
            .iter()
            .find(|notice| notice.occurrence() == exact)
        {
            resolved = Some(notice.clone());
            break;
        }
    }
    let notice = resolved.expect("the ended operation resolved its card");
    assert_eq!(notice.outcome, key("struck"));

    // The struck resolution renders its own authored text, naming the
    // province and — still, at the very end — never the hand.
    assert!(
        notice.text.contains("Vhorruk") && notice.text.contains("the ground gave"),
        "the struck resolution text renders, got '{}'",
        notice.text
    );
    for tell in SHADOW_TELLS {
        assert!(!notice.text.contains(tell));
    }
}

// ---------------------------------------------------------------------------
// Tracing the hand: the ordinary investigation answer to Unquiet Holdings.
// An enquiry either proves the organisation the lifecycle already bound —
// the true hand, read from a structural binding rather than chosen — or
// proves nothing at all. There is no third result, and no result names
// anybody else. Discovery is durable, per knower, and survives every
// snapshot, restore, and replay of every epistemic stage.
// ---------------------------------------------------------------------------

/// Offsets from [`SHADOW_LIVE_DAY`] at which the same household
/// investigator's enquiry proves the hand, and at which it comes back
/// cold. Both are ordinary draws of the existing resolution stream on the
/// pinned seed: the outcome is the simulation's, and the test only picks
/// which day the order was given.
const SHADOW_PROVED_OFFSET: u32 = 2;
const SHADOW_COLD_OFFSET: u32 = 0;
/// Authored duration of `trace-the-hand`.
const ENQUIRY_DAYS: i64 = 25;

fn exposure_of(host: &mut SimHost) -> aeon_sim::covert::Exposure {
    host.world_mut()
        .resource::<aeon_sim::covert::Exposure>()
        .clone()
}

/// Orders the ordinary investigation off the live card, exactly as the
/// client's action button does: the projected action, the projected
/// target, and a leader the player chose.
fn order_the_enquiry(
    host: &mut SimHost,
    leader: CharacterId,
) -> aeon_sim::situations::SituationInstanceKey {
    let harrow = org(host, "harrow");
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    let situation = unquiet_card_for(host, harrow)
        .expect("the holder's card is live")
        .active
        .key;
    host.submit(PlayerCommand::StartSituationAssignment {
        situation: situation.clone(),
        action: key("investigate"),
        leader,
        target: AssignmentTarget::Province(vhorruk),
        war: None,
    })
    .expect("the enquiry is an ordinary valid command");
    situation
}

fn enquiry_in_flight(host: &mut SimHost) -> Option<ActiveAssignment> {
    let world = host.world_mut();
    world
        .resource::<AssignmentsIndex>()
        .assignments
        .values()
        .find_map(|entity| {
            world
                .get::<ActiveAssignment>(*entity)
                .filter(|work| work.def == key("trace-the-hand"))
                .cloned()
        })
}

fn text_metric(
    projection: &aeon_sim::situations::SituationProjection,
    label: &str,
) -> Option<String> {
    projection.metrics.iter().find_map(|metric| {
        (metric.label_key == label).then(|| match &metric.value {
            aeon_sim::situations::SituationMetricValue::Text(text) => text.clone(),
            aeon_sim::situations::SituationMetricValue::Integer(value) => {
                panic!("expected text metric for {label}, got {value}")
            }
        })
    })
}

/// Runs one campaign to the point where the enquiry ordered `offset` days
/// into the live operation has resolved, and hands back the card's exact
/// occurrence.
fn campaign_after_an_enquiry(
    seed: u64,
    content: Arc<ContentSet>,
    offset: u32,
) -> (SimHost, aeon_sim::situations::SituationOccurrence) {
    let mut host = scenario_host(seed, content);
    host.advance_days(SHADOW_LIVE_DAY + offset);
    let leader = free_household_host(&mut host, 0);
    let situation = order_the_enquiry(&mut host, leader);
    let occurrence = host
        .world_mut()
        .resource::<SituationState>()
        .active
        .get(&situation)
        .expect("the lifecycle is live")
        .occurrence();
    // Run past the enquiry's own completion, whatever day the ordinary
    // order delay actually started it on.
    while enquiry_in_flight(&mut host).is_none() {
        host.advance_days(1);
    }
    let completes = enquiry_in_flight(&mut host).expect("in flight").completes;
    while host.date() < completes {
        host.advance_days(1);
    }
    (host, occurrence)
}

#[test]
fn the_player_compares_investigators_on_one_authoritative_forecast() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let edrun = character(&mut host, "edrun-harrow");
    host.advance_days(SHADOW_LIVE_DAY);
    let card = unquiet_card_for(&mut host, harrow).expect("the holder's card is live");
    let situation = card.active.key.clone();
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    let projection = card.projection.clone().expect("projection");

    // The enquiry is offered as an ordinary projected action, aimed at the
    // holder's own troubled ground, and deliberately pins no leader: a
    // leaderless action is exactly how content says "the player chooses".
    let enquiry = projection
        .actions
        .iter()
        .find(|action| action.id == key("investigate"))
        .expect("the card offers the enquiry");
    assert_eq!(enquiry.leader, None);
    assert_eq!(enquiry.target, AssignmentTarget::Province(vhorruk));

    // Every eligible investigator is forecast through the one
    // authoritative path the order itself will take.
    let forecast_for = |host: &mut SimHost, candidate: CharacterId| {
        aeon_sim::situations::forecast_for_action(
            host.world_mut(),
            &situation,
            &key("investigate"),
            candidate,
            AssignmentTarget::Province(vhorruk),
        )
        .expect("every eligible investigator forecasts")
    };
    let date = host.date();
    let eligible: Vec<CharacterId> = {
        let world = host.world_mut();
        world
            .resource::<PoliticsIndex>()
            .characters
            .keys()
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .filter(|id| {
                aeon_sim::leader_availability(world, harrow, *id, date)
                    .blocks_assignment(AssignmentTarget::Province(vhorruk))
                    .is_none()
            })
            .collect()
    };
    assert!(
        eligible.len() >= 2,
        "the house has investigators to compare, got {eligible:?}"
    );
    for candidate in &eligible {
        let view = forecast_for(&mut host, *candidate);
        assert_eq!(view.leader, *candidate);
        assert!(view.blocked.is_none(), "an eligible investigator is free");
    }

    // And the comparison is genuinely per-candidate: a better intriguer
    // forecasts better odds, by exactly the skill the contest reads, and
    // nobody else's number moves.
    let (sharper, other) = (eligible[0], eligible[1]);
    let before_sharper = forecast_for(&mut host, sharper);
    let before_other = forecast_for(&mut host, other);
    {
        let world = host.world_mut();
        let entity = aeon_sim::access::character_entity(world, sharper).expect("indexed");
        world
            .get_mut::<aeon_sim::politics::CharacterSkills>(entity)
            .expect("characters carry skills")
            .0
            .intrigue += 6;
    }
    let after_sharper = forecast_for(&mut host, sharper);
    let after_other = forecast_for(&mut host, other);
    assert_eq!(
        after_sharper.effectiveness,
        before_sharper.effectiveness + 6,
        "the enquiry is contested on the investigator's own intrigue"
    );
    assert!(
        after_sharper.success_chance() > before_sharper.success_chance(),
        "the sharper investigator quotes better odds"
    );
    assert_eq!(
        after_other.success_chance(),
        before_other.success_chance(),
        "one candidate's forecast is nobody else's"
    );

    // The forecast is the ordinary assignment forecast for the authored
    // enquiry, not a Situation-only invention.
    let direct = aeon_sim::forecast::forecast(
        host.world_mut(),
        harrow,
        &key("trace-the-hand"),
        edrun,
        AssignmentTarget::Province(vhorruk),
    )
    .expect("the enquiry is an ordinary defined assignment");
    let through_card = aeon_sim::situations::forecast_for_action(
        host.world_mut(),
        &situation,
        &key("investigate"),
        edrun,
        AssignmentTarget::Province(vhorruk),
    )
    .expect("the card forecasts the same order");
    assert_eq!(direct.success_chance(), through_card.success_chance());
    assert_eq!(direct.effectiveness, through_card.effectiveness);

    // Ordering it is the ordinary command, and the work it starts carries
    // the card's exact provenance.
    let leader = free_household_host(&mut host, 0);
    order_the_enquiry(&mut host, leader);
    while enquiry_in_flight(&mut host).is_none() {
        host.advance_days(1);
    }
    let work = enquiry_in_flight(&mut host).expect("the enquiry is under way");
    assert_eq!(work.owner, harrow);
    assert_eq!(work.leader, leader);
    assert_eq!(work.target, AssignmentTarget::Province(vhorruk));
    assert_eq!(
        work.origin_situation.as_ref(),
        Some(&card.active.occurrence()),
        "the enquiry is tagged with the exact lifecycle that offered it"
    );
}

#[test]
fn a_proved_enquiry_names_the_true_hand_and_opens_the_card() {
    let (mut host, exact) =
        campaign_after_an_enquiry(SHADOW_SEED, repository_content(), SHADOW_PROVED_OFFSET);
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];

    // The durable record: the culprit is the organisation the lifecycle
    // bound, the knower is the house that paid for the enquiry, and the
    // discovery is tied to the exact activation that produced it.
    let records: Vec<_> = exposure_of(&mut host).records.into_iter().collect();
    assert_eq!(records.len(), 1, "one enquiry, one discovery");
    assert_eq!(records[0].culprit, vantar);
    assert_eq!(records[0].knower, harrow);
    assert_eq!(
        records[0].occurrence, exact,
        "the evidence stays tied to the lifecycle it was found through"
    );

    // The card now discloses the operation's true provenance: the
    // organisation behind it, the head who ordered it, and the very work
    // still in flight — plus its own stage and metric.
    let projection = unquiet_card_for(&mut host, harrow)
        .expect("the card is still live")
        .projection
        .expect("projection");
    assert_eq!(projection.stage, key("traced"));
    assert!(
        projection
            .participants
            .contains(&aeon_sim::situations::SituationLink {
                kind: aeon_data::model::SituationSubjectKind::Organisation,
                id: vantar.raw(),
                label_key: None,
            })
    );
    assert!(
        projection.participants.iter().any(|link| link.kind
            == aeon_data::model::SituationSubjectKind::Province
            && link.id == vhorruk.raw()),
        "the ground it was aimed at stays on the card"
    );
    assert!(
        projection.links.iter().any(|link| link.kind
            == aeon_data::model::SituationSubjectKind::Character
            && link.id == perrin.raw()),
        "the head who ordered it is navigable"
    );
    let sabotage = vantar_operation(&mut host).expect("the operation is still running");
    assert!(
        projection.links.iter().any(|link| link.kind
            == aeon_data::model::SituationSubjectKind::Assignment
            && link.id == sabotage.id.raw()),
        "the covert work itself is navigable once proved"
    );
    assert!(
        text_metric(&projection, "situation.metric.proved-hand")
            .expect("the card names the proved hand")
            .contains("House Vantar")
    );
    assert!(
        !projection
            .actions
            .iter()
            .any(|action| action.id == key("investigate")),
        "nothing is left to trace once the hand is proved"
    );

    // New history is written naming the hand, tagged with this exact
    // lifecycle so it lands in the card's own history.
    let log = host.world_mut().resource::<MessageLog>().clone();
    let revelation = log
        .entries
        .iter()
        .find(|entry| entry.text.contains("House Vantar") && entry.text.contains("The trail holds"))
        .expect("the discovery is written into history");
    assert!(revelation.audience.visible_to(Some(harrow)));
    assert!(revelation.situations.contains(&exact));

    // And the lines already written stay exactly as they were stamped: a
    // discovery reveals by writing new history, never by reopening old.
    let adoption = log
        .entries
        .iter()
        .find(|entry| entry.text.contains("Deniable Pressure"))
        .expect("the covert campaign's adoption was written owner-confided");
    assert!(
        !adoption.audience.visible_to(Some(harrow)),
        "an audience stamped at write time is never re-widened: '{}'",
        adoption.text
    );
    assert!(adoption.audience.visible_to(None));
}

#[test]
fn a_cold_enquiry_names_nobody_and_leaves_the_card_exactly_as_it_was() {
    let (mut host, _) =
        campaign_after_an_enquiry(SHADOW_SEED, repository_content(), SHADOW_COLD_OFFSET);
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];

    assert!(
        exposure_of(&mut host).records.is_empty(),
        "a cold trail proves nothing"
    );

    // The card reads exactly as it did before the enquiry: the province,
    // and only the province.
    let projection = unquiet_card_for(&mut host, harrow)
        .expect("the card is still live")
        .projection
        .expect("projection");
    assert_eq!(projection.stage, key("unrest"));
    assert_eq!(
        projection.participants,
        vec![aeon_sim::situations::SituationLink {
            kind: aeon_data::model::SituationSubjectKind::Province,
            id: vhorruk.raw(),
            label_key: None,
        }]
    );
    assert!(projection.links.is_empty());
    assert!(projection.participant_groups.is_empty());
    assert!(text_metric(&projection, "situation.metric.proved-hand").is_none());
    assert!(
        projection
            .actions
            .iter()
            .any(|action| action.id == key("investigate")),
        "a cold trail may be picked up again"
    );

    // Nothing the player can read names the hand, and the enquiry's own
    // failure line is written without naming anybody.
    let log = host.world_mut().resource::<MessageLog>().clone();
    for entry in log
        .entries
        .iter()
        .filter(|entry| entry.audience.visible_to(Some(harrow)))
    {
        for tell in SHADOW_TELLS {
            assert!(
                !entry.text.contains(tell),
                "a failed enquiry leaked covert provenance: '{}'",
                entry.text
            );
        }
        assert!(
            !entry.text.contains("The trail holds"),
            "a failed enquiry wrote a revelation: '{}'",
            entry.text
        );
    }
    assert!(
        log.entries
            .iter()
            .any(|entry| entry.text.contains("every trail ends")
                || entry.text.contains("nothing whatever was learned")),
        "the enquiry still reports that it found nothing"
    );
    // The card's own tagged history — everything the Situations panel
    // shows under this lifecycle — names no house whatever.
    let exact = unquiet_card_for(&mut host, harrow)
        .expect("the card is live")
        .active
        .occurrence();
    for entry in log
        .entries
        .iter()
        .filter(|entry| entry.situations.contains(&exact))
    {
        assert!(
            !entry.text.contains("House Vantar"),
            "the card's own history named the hand after a cold trail: '{}'",
            entry.text
        );
    }

    // And no accusation of any kind: a cold trail creates no grievance,
    // no favour, no obligation between the two houses.
    let ledger = host
        .world_mut()
        .resource::<aeon_sim::obligations::Obligations>()
        .clone();
    assert!(
        !ledger.entries.iter().any(|entry| {
            (entry.debtor == vantar && entry.creditor == harrow)
                || (entry.debtor == harrow && entry.creditor == vantar)
        }),
        "an investigation is not an accusation"
    );
}

#[test]
fn no_enquiry_of_any_result_can_name_a_house_that_did_not_do_it() {
    let content = repository_content();
    // Every ordering day across the operation's life, proved and cold
    // alike. Whatever the draw, the only organisation any surface may
    // ever name is the one the lifecycle bound.
    for offset in 0..6u32 {
        let (mut host, exact) =
            campaign_after_an_enquiry(SHADOW_SEED, Arc::clone(&content), offset);
        let harrow = org(&mut host, "harrow");
        let vantar = org(&mut host, "vantar");
        let bound = match exact.situation.bindings.get("actor") {
            Some(SituationSubject::Organisation(actor)) => *actor,
            other => panic!("the culprit is always structurally bound, got {other:?}"),
        };
        assert_eq!(bound, vantar);

        for record in &exposure_of(&mut host).records {
            assert_eq!(
                record.culprit, bound,
                "a discovery can only ever name the bound actor (offset {offset})"
            );
            assert_eq!(record.knower, harrow);
        }

        if let Some(projection) =
            unquiet_card_for(&mut host, harrow).and_then(|card| card.projection)
        {
            for link in projection
                .participants
                .iter()
                .chain(projection.links.iter())
                .chain(
                    projection
                        .participant_groups
                        .iter()
                        .flat_map(|group| group.participants.iter()),
                )
            {
                if link.kind == aeon_data::model::SituationSubjectKind::Organisation {
                    assert_eq!(
                        link.id,
                        bound.raw(),
                        "no projection may name an organisation that did not do it \
                         (offset {offset})"
                    );
                }
            }
        }
    }
}

#[test]
fn an_enquiry_and_the_sabotage_resolve_independently_when_they_fall_due_together() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    host.advance_days(SHADOW_LIVE_DAY + SHADOW_PROVED_OFFSET);
    let leader = free_household_host(&mut host, 0);
    let situation = order_the_enquiry(&mut host, leader);
    while enquiry_in_flight(&mut host).is_none() {
        host.advance_days(1);
    }
    let enquiry = enquiry_in_flight(&mut host).expect("in flight");
    let sabotage = vantar_operation(&mut host).expect("the operation is live");
    assert_eq!(
        enquiry.completes,
        enquiry.started.add_days(ENQUIRY_DAYS),
        "the enquiry runs its authored course"
    );
    assert!(
        sabotage.id < enquiry.id,
        "the operation was accepted first, so it resolves first in stable-ID order"
    );

    // Bring the covert operation due on the very day the enquiry
    // finishes. Nothing else about either is touched: they share a day,
    // a province, and nothing whatever else.
    {
        let world = host.world_mut();
        let entity = aeon_sim::access::assignment_entity(world, sabotage.id).expect("indexed");
        world
            .get_mut::<ActiveAssignment>(entity)
            .expect("the operation is running")
            .completes = enquiry.completes;
    }

    let shared_day = enquiry.completes;
    while host.date() < shared_day {
        host.advance_days(1);
    }

    // Both resolved on the shared day, each through its own ordinary path.
    assert!(
        enquiry_in_flight(&mut host).is_none(),
        "the enquiry resolved on its own day"
    );
    assert!(
        vantar_operation(&mut host).is_none_or(|retry| retry.started >= shared_day),
        "the operation resolved on the shared day, whatever its plan did next"
    );
    let records = exposure_of(&mut host).records;
    assert_eq!(records.len(), 1, "the enquiry still proved its own case");
    assert_eq!(
        records.iter().next().expect("record").culprit,
        vantar,
        "sharing a resolution day changes nothing about what was proved"
    );

    // Both wrote their own history on that day: the operation's line,
    // still confided to its owner, and the enquiry's revelation.
    let log = host.world_mut().resource::<MessageLog>().clone();
    let day_lines: Vec<_> = log
        .entries
        .iter()
        .filter(|entry| entry.date == shared_day)
        .collect();
    assert!(
        day_lines
            .iter()
            .any(|entry| entry.text.contains("The trail holds")),
        "the enquiry wrote its own result on the shared day"
    );
    assert!(
        day_lines
            .iter()
            .any(|entry| !entry.audience.visible_to(Some(harrow))),
        "the operation wrote its own owner-confided result on the shared day"
    );

    // And the card's own outcome, when it ends, is still the pure
    // live-Order reading — now able to say who paid for it.
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    let mut resolved = None;
    for _ in 0..200 {
        host.advance_days(1);
        let state = host.world_mut().resource::<SituationState>().clone();
        if let Some(notice) = state
            .resolutions
            .iter()
            .find(|notice| notice.situation == situation)
        {
            resolved = Some(notice.clone());
            break;
        }
    }
    let notice = resolved.expect("the operation's card resolves");
    let order = aeon_sim::order::province_order(host.world_mut(), vhorruk).order;
    let expected = if order < 700 {
        "struck-traced"
    } else {
        "weathered-traced"
    };
    assert_eq!(
        notice.outcome.as_str(),
        expected,
        "the card still reads the ground; proof changes only what it may say"
    );
    assert!(
        notice.text.contains("House Vantar"),
        "the frozen sentence names the proved hand, got '{}'",
        notice.text
    );
}

#[test]
fn every_epistemic_stage_survives_save_load_and_replay() {
    let content = repository_content();

    // Five stages, each a genuinely different state of knowledge:
    // before the operation exists; covert work in flight and unknown;
    // an enquiry in flight and still unknown; the hand proved; and the
    // whole matter closed with the discovery outliving the lifecycle.
    let mut host = scenario_host(SHADOW_SEED, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");

    /// One stage as the live campaign had it, taken the instant before it
    /// was snapshotted: the history exactly as written, so the restored
    /// history can be held against it line for line.
    struct Checkpoint {
        stage: &'static str,
        /// Whether nothing Harrow may read can yet carry covert provenance.
        /// Discovery legitimately widens the lines written after it, so
        /// this stops holding once the hand is proved.
        tells_withheld: bool,
        snapshot: aeon_sim::CampaignSnapshot,
        live_log: Vec<LogEntry>,
    }
    let checkpoint = |host: &mut SimHost, stage: &'static str, tells_withheld: bool| Checkpoint {
        stage,
        tells_withheld,
        live_log: host.world_mut().resource::<MessageLog>().entries.clone(),
        snapshot: host.snapshot(),
    };
    let mut checkpoints = Vec::new();

    host.advance_days(170);
    checkpoints.push(checkpoint(&mut host, "pre-operation", true));

    host.advance_days(SHADOW_LIVE_DAY + SHADOW_PROVED_OFFSET - 170);
    assert!(vantar_operation(&mut host).is_some());
    assert!(exposure_of(&mut host).records.is_empty());
    checkpoints.push(checkpoint(&mut host, "covert-in-flight", true));

    let leader = free_household_host(&mut host, 0);
    order_the_enquiry(&mut host, leader);
    while enquiry_in_flight(&mut host).is_none() {
        host.advance_days(1);
    }
    assert!(
        exposure_of(&mut host).records.is_empty(),
        "an enquiry in flight has proved nothing yet"
    );
    checkpoints.push(checkpoint(&mut host, "enquiry-in-flight", true));

    let completes = enquiry_in_flight(&mut host).expect("in flight").completes;
    while host.date() < completes {
        host.advance_days(1);
    }
    assert!(
        exposure_of(&mut host).knows(harrow, vantar),
        "the enquiry proved the hand"
    );
    checkpoints.push(checkpoint(&mut host, "proved", false));

    // Past the operation, past the card's own resolution, past the covert
    // ambition's expiry: the discovery outlives every lifecycle involved.
    host.advance_days(200);
    assert!(
        !host
            .world_mut()
            .resource::<SituationState>()
            .active
            .keys()
            .any(|instance| instance.definition == key("unquiet-holdings")),
        "the lifecycle that produced the evidence has long ended"
    );
    assert!(
        exposure_of(&mut host).knows(harrow, vantar),
        "durable evidence outlives the Situation that found it"
    );
    checkpoints.push(checkpoint(&mut host, "closed", false));

    for Checkpoint {
        stage,
        tells_withheld,
        snapshot,
        live_log,
    } in checkpoints
    {
        assert_eq!(
            snapshot.format_version,
            aeon_sim::SNAPSHOT_FORMAT_VERSION,
            "{stage}"
        );
        let recorded = snapshot.state.exposure.clone();
        let mut restored = SimHost::restore_with_content(snapshot, Arc::clone(&content))
            .unwrap_or_else(|error| panic!("{stage} restores: {error}"));
        assert_eq!(
            exposure_of(&mut restored),
            recorded,
            "{stage}: the discovery record round-trips exactly"
        );

        // Ordinary player views gain nothing from the round trip. Proved
        // before-against-after rather than by a blanket sweep, because at
        // the proved and closed stages discovery has legitimately widened
        // the lines written after it to Harrow: what must never happen is
        // that restore changes who may read any line at all.
        let restored_log = restored
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .clone();
        assert_eq!(
            restored_log.len(),
            live_log.len(),
            "{stage}: restore neither drops nor invents history"
        );
        for (index, (before, after)) in live_log.iter().zip(&restored_log).enumerate() {
            assert_eq!(
                after.text, before.text,
                "{stage}: line {index} reads differently after restore"
            );
            assert_eq!(
                after.audience, before.audience,
                "{stage}: restore changed who may read line {index}: '{}'",
                after.text
            );
            assert_eq!(
                after.audience.visible_to(Some(harrow)),
                before.audience.visible_to(Some(harrow)),
                "{stage}: restore changed whether Harrow may read line {index}: '{}'",
                after.text
            );
            assert!(
                after.audience.visible_to(None),
                "{stage}: every line stays open to spectators and replay: '{}'",
                after.text
            );
        }

        if tells_withheld {
            // Before the hand is proved, nothing Harrow may read — as
            // written, or as restored — carries covert provenance.
            for entry in live_log
                .iter()
                .chain(&restored_log)
                .filter(|entry| entry.audience.visible_to(Some(harrow)))
            {
                for tell in SHADOW_TELLS {
                    assert!(
                        !entry.text.contains(tell),
                        "{stage}: a line Harrow may read carries covert provenance: '{}'",
                        entry.text
                    );
                }
            }
        } else {
            // Once proved, the revelation is Harrow's to read, and the
            // round trip keeps it so.
            assert!(
                restored_log.iter().any(|entry| {
                    entry.text.contains("The trail holds")
                        && entry.audience.visible_to(Some(harrow))
                }),
                "{stage}: the revelation survives restore for the house that made it"
            );
        }

        // Continuing from the restore is the same campaign.
        let mut twin =
            SimHost::restore_with_content(restored.snapshot(), Arc::clone(&content)).unwrap();
        assert_eq!(restored.state_hash(), twin.state_hash(), "{stage}");
        restored.advance_days(40);
        twin.advance_days(40);
        assert_eq!(
            restored.state_hash(),
            twin.state_hash(),
            "{stage}: replay after restore stays identical"
        );
    }
}

#[test]
fn one_houses_discovery_teaches_no_other_house_and_no_stage_hides_from_a_spectator() {
    let content = repository_content();
    let mut host = scenario_host(SHADOW_SEED, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let draksha = org(&mut host, "draksha");

    // Before the operation, with it live, and with an enquiry in flight:
    // nothing about the arc ever reaches an uninvolved house, and
    // everything always reaches a spectator.
    let sweep = |host: &mut SimHost| {
        for entry in &host.world_mut().resource::<MessageLog>().entries.clone() {
            assert!(
                entry.audience.visible_to(None),
                "a spectator reads every line at every stage"
            );
            if !entry.audience.visible_to(Some(harrow)) {
                assert!(
                    !entry.audience.visible_to(Some(draksha)),
                    "an owner-confided line reached an uninvolved house: '{}'",
                    entry.text
                );
            }
        }
    };
    host.advance_days(170);
    sweep(&mut host);
    host.advance_days(SHADOW_LIVE_DAY + SHADOW_PROVED_OFFSET - 170);
    sweep(&mut host);

    let leader = free_household_host(&mut host, 0);
    order_the_enquiry(&mut host, leader);
    while enquiry_in_flight(&mut host).is_none() {
        host.advance_days(1);
    }
    sweep(&mut host);
    let completes = enquiry_in_flight(&mut host).expect("in flight").completes;
    while host.date() < completes {
        host.advance_days(1);
    }
    let discovered = exposure_of(&mut host);
    assert!(discovered.knows(harrow, vantar));
    assert!(
        !discovered.knows(draksha, vantar),
        "one house's investigation is not published to the world"
    );

    // The proved card is still the bound holder's and the spectator's
    // alone; an outsider sees no card at any stage.
    let card_key = unquiet_card_for(&mut host, harrow)
        .expect("the card is live")
        .active
        .key;
    assert!(aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &card_key
    ));
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(draksha);
    assert!(!aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &card_key
    ));
    host.world_mut().resource_mut::<PlayerHouse>().0 = None;
    assert!(aeon_sim::situations::visible_to_player(
        host.world_mut(),
        &card_key
    ));
    host.world_mut().resource_mut::<PlayerHouse>().0 = Some(harrow);

    // And every line the discovery went on to widen reached the house that
    // found out — never the bystander.
    for entry in &host.world_mut().resource::<MessageLog>().entries.clone() {
        if entry.text.contains("House Vantar") {
            assert!(
                !entry.audience.visible_to(Some(draksha))
                    || entry.audience == aeon_sim::assignments::LogAudience::Public,
                "an uninvolved house read covert provenance: '{}'",
                entry.text
            );
        }
    }
}

#[test]
fn an_expose_effect_outside_a_situation_lifecycle_names_nobody() {
    // The effect proves a structural binding on an exact lifecycle. Fired
    // where no such lifecycle exists — an event, a popup answer, a
    // miswritten script — it has no culprit to read, and refuses loudly
    // rather than inventing one. Loudly, but as a diagnostic: a
    // string-table line confided to the acting house and open to the
    // spectator, never raw engine text in everybody's history.
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let draksha = org(&mut host, "draksha");
    host.advance_days(SHADOW_LIVE_DAY);
    let before = host.world_mut().resource::<MessageLog>().entries.len();
    let roles = aeon_sim::assignments::AssignmentRoles::default();
    aeon_sim::assignments::apply_effects(
        host.world_mut(),
        &[aeon_data::ScriptEffect::Expose {
            binding: "actor".to_owned(),
        }],
        &roles,
        Some(harrow),
    );
    assert!(
        exposure_of(&mut host).records.is_empty(),
        "no lifecycle, no culprit, no discovery"
    );
    let complaint = host.world_mut().resource::<MessageLog>().entries[before..]
        .iter()
        .find(|entry| entry.text.contains("outside a Situation lifecycle"))
        .cloned()
        .expect("the refusal is loud, not silent");
    let expected = host.world_mut().resource::<aeon_sim::TextDb>().format(
        "sim.covert.expose-refused",
        &[("reason", "it was fired outside a Situation lifecycle")],
    );
    assert_eq!(
        complaint.text, expected,
        "the refusal is the string table's diagnostic, not raw engine text"
    );
    assert_eq!(complaint.channel, LogChannel::Events);
    assert_eq!(
        complaint.audience,
        aeon_sim::assignments::LogAudience::organisations([harrow]),
        "an authoring fault is confided to the house whose work fired it"
    );
    assert!(
        !complaint.audience.visible_to(Some(draksha)),
        "and reaches no bystander's history"
    );
    assert!(
        complaint.audience.visible_to(None),
        "while the spectator, and replay, read it"
    );
}

#[test]
fn the_enquiry_is_an_answer_to_the_alarm_not_a_free_standing_order() {
    // The investigation is gated on the same fact that raises the card:
    // somebody else's covert work running against that holding. Where
    // there is nothing to trace it is neither offered on the province nor
    // accepted from the household — before the operation exists, on any
    // quiet holding while it runs, and again once the work has ended.
    let content = repository_content();
    let mut host = scenario_host(SHADOW_SEED, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    let enquiry = key("trace-the-hand");

    let holdings_of_harrow = |host: &mut SimHost| -> Vec<aeon_sim::ProvinceId> {
        let world = &*host.world_mut();
        world
            .resource::<MapIndex>()
            .provinces
            .keys()
            .copied()
            .filter(|province| aeon_sim::warfare::province_holder(world, *province) == Some(harrow))
            .collect()
    };
    let offered_on = |host: &mut SimHost, province: aeon_sim::ProvinceId| {
        aeon_sim::target_allowed(
            host.world_mut(),
            &enquiry,
            harrow,
            AssignmentTarget::Province(province),
        )
    };
    let ordered_on = |host: &mut SimHost, province: aeon_sim::ProvinceId| {
        let leader = free_household_host(host, 0);
        host.submit(PlayerCommand::StartAssignment {
            assignment: enquiry.clone(),
            leader,
            target: AssignmentTarget::Province(province),
        })
    };
    let worked_against = |host: &mut SimHost, province: aeon_sim::ProvinceId| {
        // Judged from the record and the authored flag directly, so the
        // stage is settled independently of the gate under test.
        let world = host.world_mut();
        let content = world.resource::<aeon_sim::state::ContentDb>().0.clone();
        world
            .resource::<AssignmentsIndex>()
            .assignments
            .values()
            .any(|entity| {
                world.get::<ActiveAssignment>(*entity).is_some_and(|work| {
                    work.owner != harrow
                        && work.target == AssignmentTarget::Province(province)
                        && content
                            .assignments
                            .get(&work.def)
                            .is_some_and(|def| def.covert)
                })
            })
    };

    // Before the operation exists: quiet ground everywhere.
    host.advance_days(170);
    assert!(!worked_against(&mut host, vhorruk));
    let holdings = holdings_of_harrow(&mut host);
    assert!(holdings.contains(&vhorruk), "Vhorruk is Harrow's to hold");
    for province in &holdings {
        assert!(
            !offered_on(&mut host, *province),
            "with nothing to trace, the enquiry is on offer nowhere"
        );
    }
    assert!(
        matches!(
            ordered_on(&mut host, vhorruk),
            Err(CommandRejection::Assignment(AssignmentRejection::BadTarget))
        ),
        "and ordering it anyway is refused at the gate every start path shares"
    );
    assert!(enquiry_in_flight(&mut host).is_none());

    // With the operation live: offered on the worked holding, and there
    // alone.
    host.advance_days(SHADOW_LIVE_DAY - 170);
    assert!(worked_against(&mut host, vhorruk));
    for province in holdings_of_harrow(&mut host) {
        assert_eq!(
            offered_on(&mut host, province),
            province == vhorruk,
            "the enquiry is offered exactly where covert work is running"
        );
    }

    // Once the work has ended and no hand is moving against the province,
    // the offer withdraws with it.
    let mut quiet = false;
    for _ in 0..600 {
        host.advance_days(1);
        if !worked_against(&mut host, vhorruk) {
            quiet = true;
            break;
        }
    }
    assert!(quiet, "the operation ends inside the swept horizon");
    assert!(
        !offered_on(&mut host, vhorruk),
        "with the work over there is nothing left to trace"
    );
    assert!(matches!(
        ordered_on(&mut host, vhorruk),
        Err(CommandRejection::Assignment(AssignmentRejection::BadTarget))
    ));
}

#[test]
fn an_enquiry_ordered_from_the_province_inherits_the_card_and_proves_the_hand() {
    // The province panel and the household list issue the ordinary
    // StartAssignment command, naming no Situation. The forecast they show
    // is the card's forecast, so the order they place must be the card's
    // order — origin included — or the promise of a proved hand would be
    // one the ordinary path could never keep. Same seed, same day, and the
    // same investigator as the card-launched proved enquiry: the outcome is
    // the simulation's own draw, and the only thing that changes is which
    // button was pressed.
    let content = repository_content();
    let mut host = scenario_host(SHADOW_SEED, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    host.advance_days(SHADOW_LIVE_DAY + SHADOW_PROVED_OFFSET);
    let card = unquiet_card_for(&mut host, harrow).expect("the holder's card is live");
    let exact = card.active.occurrence();
    let leader = free_household_host(&mut host, 0);

    // The ordinary forecast the province panel shows is the card's forecast:
    // one number, one path, and nothing blocking it.
    let direct = aeon_sim::forecast::forecast(
        host.world_mut(),
        harrow,
        &key("trace-the-hand"),
        leader,
        AssignmentTarget::Province(vhorruk),
    )
    .expect("the enquiry is an ordinary defined assignment");
    let through_card = aeon_sim::situations::forecast_for_action(
        host.world_mut(),
        &card.active.key,
        &key("investigate"),
        leader,
        AssignmentTarget::Province(vhorruk),
    )
    .expect("the card forecasts the same order");
    assert!(direct.blocked.is_none(), "the ordinary order is open");
    assert_eq!(direct.success_chance(), through_card.success_chance());
    assert_eq!(direct.effectiveness, through_card.effectiveness);

    // Ordered through the ordinary path — no Situation named at all.
    host.submit(PlayerCommand::StartAssignment {
        assignment: key("trace-the-hand"),
        leader,
        target: AssignmentTarget::Province(vhorruk),
    })
    .expect("the enquiry is an ordinary valid command");
    while enquiry_in_flight(&mut host).is_none() {
        host.advance_days(1);
    }
    let work = enquiry_in_flight(&mut host).expect("the enquiry is under way");
    assert_eq!(work.owner, harrow);
    assert_eq!(work.leader, leader);
    assert_eq!(work.target, AssignmentTarget::Province(vhorruk));
    assert_eq!(
        work.origin_situation.as_ref(),
        Some(&exact),
        "an ordinary order the live card would have launched inherits the card's provenance"
    );
    assert_eq!(
        work.war, None,
        "and carries exactly the formal-war context it was forecast in"
    );

    // ...and an unrelated order given the same day, which no live card
    // offers, starts exactly as it always has: with no origin at all.
    let steward = free_household_host(&mut host, 1);
    assert_ne!(
        steward, leader,
        "a second free host stands in the household"
    );
    host.submit(PlayerCommand::StartAssignment {
        assignment: key("collect-tithes"),
        leader: steward,
        target: AssignmentTarget::None,
    })
    .expect("routine work is an ordinary valid command");
    let tithes_started = |host: &mut SimHost| {
        let world = host.world_mut();
        world
            .resource::<AssignmentsIndex>()
            .assignments
            .values()
            .find_map(|entity| {
                world
                    .get::<ActiveAssignment>(*entity)
                    .filter(|routine| {
                        routine.owner == harrow
                            && routine.leader == steward
                            && routine.def == key("collect-tithes")
                    })
                    .cloned()
            })
    };
    while tithes_started(&mut host).is_none() {
        host.advance_days(1);
    }
    let routine = tithes_started(&mut host).expect("the routine work is under way");
    assert_eq!(
        routine.origin_situation, None,
        "an order no live card offers starts without a Situation origin, as before"
    );

    // Run the enquiry to its own completion: the hand is proved, exactly
    // as the forecast promised, and the card opens on it.
    let completes = work.completes;
    while host.date() < completes {
        host.advance_days(1);
    }
    assert!(
        exposure_of(&mut host).knows(harrow, vantar),
        "the ordinary order proves the hand the card bound"
    );
    let records: Vec<_> = exposure_of(&mut host).records.into_iter().collect();
    assert_eq!(records.len(), 1, "one enquiry, one discovery");
    assert_eq!(
        records[0].occurrence, exact,
        "the evidence stays tied to the lifecycle that offered the order"
    );
    let projection = unquiet_card_for(&mut host, harrow)
        .expect("the card is still live")
        .projection
        .expect("projection");
    assert_eq!(projection.stage, key("traced"));

    // The refusal diagnostic never fires: nothing about the ordinary path
    // left the effect without a lifecycle to read.
    let refused = host.world_mut().resource::<aeon_sim::TextDb>().format(
        "sim.covert.expose-refused",
        &[("reason", "it was fired outside a Situation lifecycle")],
    );
    let log = host.world_mut().resource::<MessageLog>().clone();
    assert!(
        !log.entries.iter().any(|entry| entry.text == refused),
        "the ordinary order never reaches the no-origin refusal"
    );
}

#[test]
fn a_proved_hand_withdraws_the_ordinary_enquiry_while_the_operation_still_runs() {
    // Between the day the hand is proved and the day the operation ends,
    // the card is still live but offers no investigate action: there is
    // nothing left to prove. The ordinary province and household paths
    // must agree with it exactly, because an enquiry accepted in that
    // window would attach to no lifecycle, and a successful one would
    // spend the player's wealth and days on a "proved hand" popup
    // followed by the no-origin refusal. Same seed and same proved enquiry
    // as the card tests; only what is attempted afterwards differs.
    let (mut host, _) =
        campaign_after_an_enquiry(SHADOW_SEED, repository_content(), SHADOW_PROVED_OFFSET);
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let vhorruk = host.world_mut().resource::<MapIndex>().province_keys[&key("vhorruk")];
    let enquiry = key("trace-the-hand");

    // The window under test: the hand is proved, and the operation is
    // still running against the same ground — every fact the pre-proof
    // gate keys on is unchanged.
    assert!(
        exposure_of(&mut host).knows(harrow, vantar),
        "the hand is proved"
    );
    let sabotage = vantar_operation(&mut host).expect("the operation is still running");
    assert_eq!(sabotage.target, AssignmentTarget::Province(vhorruk));
    assert!(
        enquiry_in_flight(&mut host).is_none(),
        "the enquiry that proved it has resolved"
    );
    let projection = unquiet_card_for(&mut host, harrow)
        .expect("the card is still live")
        .projection
        .expect("projection");
    assert_eq!(projection.stage, key("traced"));
    assert!(
        !projection
            .actions
            .iter()
            .any(|action| action.id == key("investigate")),
        "the card offers nothing to trace"
    );

    // Neither does the province: the ordinary offer withdraws with the
    // card's action, and the forecast the panel would quote is blocked at
    // the one gate every start path shares.
    assert!(
        !aeon_sim::target_allowed(
            host.world_mut(),
            &enquiry,
            harrow,
            AssignmentTarget::Province(vhorruk),
        ),
        "an enquiry into a hand already proved is offered nowhere"
    );
    let leader = free_household_host(&mut host, 0);
    let forecast = aeon_sim::forecast::forecast(
        host.world_mut(),
        harrow,
        &enquiry,
        leader,
        AssignmentTarget::Province(vhorruk),
    )
    .expect("the enquiry is still an ordinary defined assignment");
    assert_eq!(
        forecast.blocked,
        Some(AssignmentRejection::BadTarget),
        "and the ordinary forecast says so"
    );

    // Ordering it anyway through the ordinary path — naming no Situation
    // — is refused outright, exactly as on quiet ground.
    assert!(
        matches!(
            host.submit(PlayerCommand::StartAssignment {
                assignment: enquiry.clone(),
                leader,
                target: AssignmentTarget::Province(vhorruk),
            }),
            Err(CommandRejection::Assignment(AssignmentRejection::BadTarget))
        ),
        "the ordinary order is refused at the gate, not accepted without a lifecycle"
    );

    // Run past where a second enquiry would have resolved had one slipped
    // through: none ever starts, nothing further is proved, and the
    // no-origin refusal never fires.
    for _ in 0..(ENQUIRY_DAYS + 30) {
        host.advance_days(1);
        assert!(
            enquiry_in_flight(&mut host).is_none(),
            "no enquiry starts against a hand already proved"
        );
    }
    let records: Vec<_> = exposure_of(&mut host).records.into_iter().collect();
    assert_eq!(records.len(), 1, "one enquiry, one discovery, and no more");
    assert_eq!(records[0].culprit, vantar);
    assert_eq!(records[0].knower, harrow);
    let refused = host.world_mut().resource::<aeon_sim::TextDb>().format(
        "sim.covert.expose-refused",
        &[("reason", "it was fired outside a Situation lifecycle")],
    );
    let log = host.world_mut().resource::<MessageLog>().clone();
    assert!(
        !log.entries.iter().any(|entry| entry.text == refused),
        "no ordinary order reaches the no-origin refusal"
    );
}

// ---------------------------------------------------------------------------
// Reconciliation and derailment: changing Vantar's live relationship
// suppresses or abandons only uncommitted hostile planning, while an
// operation or a war already underway runs to its ordinary end, a
// succession re-reads the relationship from the successor's own regard,
// and the open Cold Border card explains the reasoning without ever
// naming a hand.
// ---------------------------------------------------------------------------

/// The authored hostility floor and reconciliation line, as the shadow
/// content states them and the Cold Border card shows them.
const SHADOW_FLOOR: i64 = -10;
const SHADOW_LINE: i64 = 20;
/// Vantar's authored opening regard for Harrow's head: grasping against
/// magnanimous.
const SHADOW_OPENING_REGARD: i32 = -15;

/// Sets (or replaces) one direct test modifier on the live Vantar head's
/// ledger toward the live Harrow head — the relationship every shadow
/// predicate and the Cold Border card read. One stable reason means
/// repeated calls replace rather than stack.
fn set_vantar_esteem(host: &mut SimHost, amount: i32) {
    let vantar = org(host, "vantar");
    let harrow = org(host, "harrow");
    let theirs = aeon_sim::access::org_head(host.world_mut(), vantar).expect("vantar head");
    let own = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    let world = host.world_mut();
    let entity = world.resource::<PoliticsIndex>().characters[&theirs];
    world
        .get_mut::<OpinionLedger>(entity)
        .expect("characters carry opinion ledgers")
        .set(OpinionEntry {
            target: own,
            amount,
            reason: "test-thaw".to_owned(),
            expires: None,
        });
}

/// The live Vantar head's derived opinion of the live Harrow head.
fn vantar_regard(host: &mut SimHost) -> i32 {
    let vantar = org(host, "vantar");
    let harrow = org(host, "harrow");
    let theirs = aeon_sim::access::org_head(host.world_mut(), vantar).expect("vantar head");
    let own = aeon_sim::access::org_head(host.world_mut(), harrow).expect("harrow head");
    opinion_between(host.world_mut(), theirs, own)
}

fn vantar_ambition(host: &mut SimHost) -> Option<aeon_sim::goals::ActiveGoal> {
    let vantar = org(host, "vantar");
    host.world_mut()
        .resource::<aeon_sim::goals::Goals>()
        .active
        .get(&vantar)
        .cloned()
}

/// The covert campaign the live Vantar head is pursuing, if any.
fn vantar_campaign(host: &mut SimHost) -> Option<aeon_sim::plans::ActivePlan> {
    let vantar = org(host, "vantar");
    let head = aeon_sim::access::org_head(host.world_mut(), vantar)?;
    host.world_mut()
        .resource::<aeon_sim::plans::Plans>()
        .active
        .get(&head)
        .cloned()
}

/// The Cold Border card `house` holds about `neighbour`. Harrow opens the
/// reign with two cold neighbours — Draksha's head regards Edrun as
/// coldly as Vantar's — so the card under test is always named by both
/// bindings.
fn cold_border_card_for(
    host: &mut SimHost,
    house: OrgId,
    neighbour: OrgId,
) -> Option<SituationCard> {
    active_cards(host.world_mut()).into_iter().find(|card| {
        card.active.key.definition == key("cold-border")
            && card.active.key.bindings.get("house") == Some(&SituationSubject::Organisation(house))
            && card.active.key.bindings.get("neighbour")
                == Some(&SituationSubject::Organisation(neighbour))
    })
}

/// Vantar's ambition, when it is aimed at Harrow. A reconciled Vantar may
/// legitimately turn the same ambition on another cold neighbour later in
/// the window; what these tests hold is that Harrow is not its target.
fn vantar_ambition_against_harrow(host: &mut SimHost) -> Option<aeon_sim::goals::ActiveGoal> {
    let harrow = org(host, "harrow");
    vantar_ambition(host).filter(|goal| goal.target == AssignmentTarget::Org(harrow))
}

/// Vantar's sabotage against ground Harrow holds, if any is running.
fn vantar_operation_against_harrow(host: &mut SimHost) -> Option<ActiveAssignment> {
    let harrow = org(host, "harrow");
    let work = vantar_operation(host)?;
    let AssignmentTarget::Province(province) = work.target else {
        return None;
    };
    (aeon_sim::warfare::province_holder(host.world_mut(), province) == Some(harrow)).then_some(work)
}

fn last_cold_border_resolution(
    host: &mut SimHost,
) -> Option<aeon_sim::situations::SituationResolution> {
    host.world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .rev()
        .find(|notice| notice.situation.definition == key("cold-border"))
        .cloned()
}

/// A line Vantar's covert work wrote — on record, confided to Vantar and
/// never to Harrow, open to spectators and replay.
fn assert_confided_to_vantar(host: &mut SimHost, fragment: &str) {
    let harrow = org(host, "harrow");
    let vantar = org(host, "vantar");
    let log = host.world_mut().resource::<MessageLog>().clone();
    let line = log
        .entries
        .iter()
        .rev()
        .find(|entry| entry.org == Some(vantar) && entry.text.contains(fragment))
        .unwrap_or_else(|| panic!("a Vantar line carrying '{fragment}' is on record"));
    assert!(
        !line.audience.visible_to(Some(harrow)),
        "'{}' must not reach the house it concerns",
        line.text
    );
    assert!(line.audience.visible_to(Some(vantar)));
    assert!(line.audience.visible_to(None));
}

#[test]
fn regard_lifted_above_the_floor_before_the_window_keeps_the_year_quiet() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    assert_eq!(
        vantar_regard(&mut host),
        SHADOW_OPENING_REGARD,
        "the authored opening regard sits below the floor"
    );
    assert!(
        cold_border_card_for(&mut host, harrow, vantar).is_some(),
        "day one shows the cold border for what it is"
    );

    // A thaw to one point above the floor, months before the window:
    // enough to suppress new escalation, well short of the line.
    set_vantar_esteem(
        &mut host,
        SHADOW_OPENING_REGARD.abs() + SHADOW_FLOOR as i32 + 1,
    );
    assert_eq!(vantar_regard(&mut host), SHADOW_FLOOR as i32 + 1);
    host.advance_days(1);
    assert!(cold_border_card_for(&mut host, harrow, vantar).is_none());
    assert_eq!(
        last_cold_border_resolution(&mut host)
            .expect("the card resolved")
            .outcome,
        key("eased"),
        "above the floor but short of the line is eased, not reconciled"
    );

    // The whole window elapses with nothing of the arc aimed at Harrow:
    // no ambition against it, no operation on its ground, no alarm.
    for _ in 1..=260 {
        host.advance_days(1);
        assert!(
            vantar_ambition_against_harrow(&mut host).is_none(),
            "no hostile ambition forms against a house regarded above the floor"
        );
        assert!(vantar_operation_against_harrow(&mut host).is_none());
    }
    assert!(unquiet_card_for(&mut host, harrow).is_none());
    let log = host.world_mut().resource::<MessageLog>().clone();
    assert!(
        !log.entries.iter().any(|entry| {
            entry.org == Some(vantar)
                && entry.subject == Some(aeon_sim::LogSubject::Org(harrow))
                && entry.text.contains("Undermine a Neighbour")
        }),
        "not even the spectator's history carries an ambition against Harrow that never formed"
    );
}

#[test]
fn reconciliation_at_the_line_lets_an_uncommitted_campaign_and_its_ambition_go() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    host.advance_days(180);
    let ambition =
        vantar_ambition(&mut host).expect("the ambition forms on the window's first pulse");
    assert_eq!(ambition.target, AssignmentTarget::Org(harrow));
    let campaign =
        vantar_campaign(&mut host).expect("the head takes up the campaign the same pulse");
    assert_eq!(campaign.def, key("deniable-pressure"));
    assert!(
        campaign.current_assignment.is_none(),
        "nothing is committed on the day of adoption"
    );

    // The regard reaches the line — no grievance owed, no war — before
    // any step commits. The campaign is let go the next day, before its
    // step could begin, and the reason is confided to Vantar alone.
    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    assert_eq!(vantar_regard(&mut host), SHADOW_LINE as i32);
    host.advance_days(1);
    assert!(
        vantar_campaign(&mut host).is_none(),
        "the uncommitted campaign is let go before any step"
    );
    assert!(
        vantar_operation(&mut host).is_none(),
        "no sabotage was ever accepted"
    );
    assert_confided_to_vantar(&mut host, "the grounds for it no longer hold");
    assert!(
        vantar_ambition(&mut host).is_some(),
        "the ambition stands until its own monthly pulse"
    );

    // On the pulse the ambition is set aside — with no cooldown to lock
    // the house out should its grounds return — and again the line is
    // Vantar's alone to read.
    host.advance_days(29);
    assert!(
        vantar_ambition(&mut host).is_none(),
        "at the line, with no grievance and no war, the ambition is set aside"
    );
    assert!(
        !host
            .world_mut()
            .resource::<aeon_sim::goals::Goals>()
            .cooldowns
            .contains_key(&(vantar, key("undermine-a-neighbour"))),
        "lost grounds start no cooldown"
    );
    assert_confided_to_vantar(&mut host, "Undermine a Neighbour");

    // Nothing of the arc mounts against Harrow for the rest of the
    // window, whatever else the house may set its mind to.
    for _ in 0..51 {
        host.advance_days(1);
        assert!(vantar_ambition_against_harrow(&mut host).is_none());
        assert!(vantar_operation_against_harrow(&mut host).is_none());
    }
    assert!(unquiet_card_for(&mut host, harrow).is_none());

    // The Cold Border card reflected the thaw: resolved reconciled,
    // naming the neighbour as a neighbour and nothing as a hand.
    let notice = last_cold_border_resolution(&mut host).expect("the card resolved");
    assert_eq!(notice.outcome, key("reconciled"));
    assert!(
        notice.text.contains("House Vantar"),
        "got '{}'",
        notice.text
    );
    for tell in SHADOW_TELLS {
        assert!(!notice.text.contains(tell));
    }
}

#[test]
fn reconciliation_at_the_line_leaves_the_operation_in_flight_to_its_ordinary_end() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    host.advance_days(SHADOW_LIVE_DAY);
    let work = vantar_operation(&mut host).expect("the operation is live");
    let exact = unquiet_card_for(&mut host, harrow)
        .expect("the holder's card is live")
        .active
        .occurrence();
    assert_eq!(
        vantar_campaign(&mut host)
            .expect("the campaign stands")
            .current_assignment,
        Some(work.id)
    );

    // The regard reaches the line with the sabotage in flight. The
    // accepted work is untouched: it runs to the day it was always going
    // to resolve on, and the campaign stands behind it the whole way —
    // across the pulse that sets the ambition above it aside.
    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    while host.date().add_days(1) < work.completes {
        host.advance_days(1);
        assert!(
            aeon_sim::access::assignment(host.world_mut(), work.id).is_some(),
            "work already accepted is untouched by the change of heart"
        );
        assert_eq!(
            vantar_campaign(&mut host)
                .expect("the campaign stands behind its work")
                .current_assignment,
            Some(work.id)
        );
    }
    assert!(
        vantar_ambition_against_harrow(&mut host).is_none(),
        "the ambition was set aside on the pulse before the work resolved"
    );
    host.advance_days(1);
    assert_eq!(host.date(), work.completes);
    assert!(
        aeon_sim::access::assignment(host.world_mut(), work.id).is_none(),
        "the sabotage resolved on its ordinary day"
    );

    // Resolved through the ordinary Situation, not erased: the holder's
    // card read the ground and closed struck or weathered, never
    // passed-on, and without naming the hand.
    let state = host.world_mut().resource::<SituationState>().clone();
    let notice = state
        .resolutions
        .iter()
        .find(|notice| notice.occurrence() == exact)
        .expect("the alarm resolved");
    assert!(
        matches!(notice.outcome.as_str(), "struck" | "weathered"),
        "the operation ran its course: {}",
        notice.outcome
    );
    for tell in SHADOW_TELLS {
        assert!(!notice.text.contains(tell));
    }
    // With the work done there is nothing left to commit, and the
    // campaign ends by its ordinary rules within days.
    host.advance_days(2);
    assert!(vantar_campaign(&mut host).is_none());
    assert!(vantar_operation_against_harrow(&mut host).is_none());
}

#[test]
fn a_war_already_declared_keeps_the_ambition_until_peace_is_made_through_ordinary_means() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    host.advance_days(SHADOW_LIVE_DAY);
    let work = vantar_operation(&mut host).expect("the operation is live");
    let war = declare_war(host.world_mut(), harrow, vantar, key("test-border-war"))
        .expect("an ordinary formal war");
    evaluate(host.world_mut());

    // The open card reads the war as a public fact.
    let projection = cold_border_card_for(&mut host, harrow, vantar)
        .expect("the border is still cold")
        .projection
        .expect("projection");
    assert_eq!(
        integer_metric(&projection, "situation.metric.war-days"),
        Some(0)
    );
    assert!(projection.links.iter().any(|link| {
        link.kind == aeon_data::model::SituationSubjectKind::War && link.id == war.raw()
    }));

    // The regard reaches the line. The operation runs to its end and the
    // ambition keeps its grounds: a war already declared is a deed, and
    // it ends only through ordinary negotiation.
    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    while host.date() < work.completes {
        host.advance_days(1);
    }
    host.advance_days(1);
    assert!(aeon_sim::access::assignment(host.world_mut(), work.id).is_none());
    assert!(
        vantar_ambition(&mut host).is_some(),
        "at war, the ambition keeps its grounds whatever the regard"
    );
    assert!(
        aeon_sim::wars::is_active_war(host.world_mut(), war),
        "no opinion threshold ends a war"
    );

    // Peace is made the ordinary way; on the next pulse the ambition is
    // set aside for lost grounds.
    conclude_war(host.world_mut(), war, WarConclusionKind::NegotiatedPeace)
        .expect("peace is negotiated");
    host.advance_days(30);
    assert!(
        vantar_ambition_against_harrow(&mut host).is_none(),
        "with peace made and the regard at the line, the ambition is set aside"
    );
    assert_confided_to_vantar(&mut host, "Undermine a Neighbour");
}

#[test]
fn a_successor_reads_the_relationship_afresh_with_no_protected_or_inherited_hostility() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let valka = character(&mut host, "valka-vantar");
    let edrun = character(&mut host, "edrun-harrow");

    // Perrin sets the house on the ambition inside the window and takes up
    // the campaign himself on the same pulse.
    host.advance_days(180);
    let goal = vantar_ambition_against_harrow(&mut host).expect("the ambition forms");
    assert_eq!(goal.adopted_by, perrin);
    assert_eq!(
        vantar_campaign(&mut host)
            .expect("the head takes up the campaign")
            .def,
        key("deniable-pressure")
    );

    // Perrin is reconciled to the line. The uncommitted campaign is let go
    // the next day — and, as every ended plan does, it leaves ITS ACTOR's
    // cooldown on record — and on the pulse the ambition is set aside for
    // lost grounds, which by design starts no cooldown at all.
    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    host.advance_days(1);
    assert!(vantar_campaign(&mut host).is_none());
    assert!(
        host.world_mut()
            .resource::<aeon_sim::plans::Plans>()
            .cooldowns
            .contains_key(&(perrin, key("deniable-pressure"))),
        "a campaign let go leaves its actor's plan cooldown behind"
    );
    host.advance_days(29);
    assert!(
        vantar_ambition(&mut host).is_none(),
        "at the line the ambition is set aside on the pulse"
    );
    assert!(
        !host
            .world_mut()
            .resource::<aeon_sim::goals::Goals>()
            .cooldowns
            .contains_key(&(vantar, key("undermine-a-neighbour"))),
        "a lost-grounds set-aside starts no ambition cooldown"
    );
    assert!(cold_border_card_for(&mut host, harrow, vantar).is_none());
    let ledger_before = host
        .world_mut()
        .resource::<aeon_sim::obligations::Obligations>()
        .clone();

    // Perrin dies the same day; Valka succeeds, and her own regard for
    // Edrun — never touched by her husband's reconciliation — sits below
    // the floor.
    let date = host.date();
    process_death(host.world_mut(), perrin, date);
    assert_eq!(
        aeon_sim::access::org_head(host.world_mut(), vantar),
        Some(valka)
    );
    assert!(
        opinion_between(host.world_mut(), valka, edrun) <= SHADOW_FLOOR as i32,
        "the successor's own regard, read afresh"
    );
    assert_eq!(
        host.world_mut()
            .resource::<aeon_sim::obligations::Obligations>()
            .clone(),
        ledger_before,
        "organisation-level obligations pass through the succession untouched"
    );
    host.advance_days(1);
    let projection = cold_border_card_for(&mut host, harrow, vantar)
        .expect("the border runs cold again under the successor")
        .projection
        .expect("projection");
    assert_eq!(
        integer_metric(&projection, "situation.metric.neighbour-regard"),
        Some(i64::from(opinion_between(host.world_mut(), valka, edrun)))
    );

    // Inside the window the house re-arms on the next ordinary pulse,
    // under the successor, from her own regard — nothing protected,
    // nothing inherited but the relationship as it now stands. Had the
    // set-aside started the ambition's 720-day cooldown, or had Perrin's
    // plan cooldown passed to his widow, neither could happen here.
    let mut rearmed = None;
    while host.date() < start_date(&mut host).add_days(261) {
        host.advance_days(1);
        if let Some(goal) = vantar_ambition_against_harrow(&mut host) {
            rearmed = Some(goal);
            break;
        }
    }
    let goal = rearmed.expect("the successor's own regard re-arms the ambition inside the window");
    assert_eq!(goal.def, key("undermine-a-neighbour"));
    assert_eq!(goal.adopted_by, valka, "the successor set the house on it");
    assert_eq!(goal.target, AssignmentTarget::Org(harrow));
    let plan_cooldowns = host
        .world_mut()
        .resource::<aeon_sim::plans::Plans>()
        .cooldowns
        .clone();
    assert!(
        plan_cooldowns.contains_key(&(perrin, key("deniable-pressure"))),
        "the dead head's plan cooldown stands, and stays his"
    );
    assert!(
        !plan_cooldowns.keys().any(|(who, _)| *who == valka),
        "a plan cooldown is the dead head's, never the successor's"
    );
    assert!(
        !host
            .world_mut()
            .resource::<aeon_sim::goals::Goals>()
            .cooldowns
            .contains_key(&(vantar, key("undermine-a-neighbour"))),
        "the set-aside left no ambition cooldown for the successor to wait out"
    );
}

#[test]
fn a_grievance_owed_opens_the_arc_above_the_floor_through_the_grievance_gate() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");

    // The regard is lifted to zero — above the floor, short of the line —
    // and Harrow comes to owe Vantar an open grievance. The open card
    // reads exactly that: an aggrieved border, one grievance owed, and a
    // regard that on its own would open nothing.
    set_vantar_esteem(&mut host, -SHADOW_OPENING_REGARD);
    assert_eq!(vantar_regard(&mut host), 0);
    aeon_sim::obligations::create(
        host.world_mut(),
        ObligationKind::Grievance,
        harrow,
        vantar,
        "a border slight",
        20,
        None,
    );
    evaluate(host.world_mut());
    let projection = cold_border_card_for(&mut host, harrow, vantar)
        .expect("a grievance owed keeps the border cold whatever the regard")
        .projection
        .expect("projection");
    assert_eq!(projection.stage, key("aggrieved"));
    assert_eq!(
        integer_metric(&projection, "situation.metric.neighbour-regard"),
        Some(0)
    );
    assert_eq!(
        integer_metric(&projection, "situation.metric.open-grievances"),
        Some(1)
    );

    // On the window's first pulse the ledger alone resolves the ambition's
    // target, and the head takes up the campaign through its grievance
    // gate — the ill-will gate, one point and more above the floor, is
    // shut. The operation then mounts as it would from ill will.
    host.advance_days(180);
    let goal = vantar_ambition_against_harrow(&mut host)
        .expect("a grievance owed is grounds for the ambition on its own");
    assert_eq!(goal.def, key("undermine-a-neighbour"));
    let campaign = vantar_campaign(&mut host).expect("the head takes up the campaign");
    assert_eq!(campaign.def, key("deniable-pressure"));
    assert_eq!(
        campaign.method, "from-grievance",
        "above the floor, only the grievance gate is open"
    );
    assert_eq!(campaign.target, AssignmentTarget::Org(harrow));
    host.advance_days(2);
    assert!(
        vantar_operation_against_harrow(&mut host).is_some(),
        "the operation mounts on Harrow's ground"
    );

    // Warmth alone settles nothing: at the line, with the grievance still
    // open, the ambition keeps its grounds through the pulse.
    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    assert_eq!(vantar_regard(&mut host), SHADOW_LINE as i32);
    host.advance_days(28);
    assert!(
        vantar_ambition_against_harrow(&mut host).is_some(),
        "a wronged house keeps its grounds whatever the regard"
    );
}

#[test]
fn the_cold_border_card_reads_only_public_relationship_facts() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let edrun = character(&mut host, "edrun-harrow");

    // Day one: the live regard, the two authored numbers, the clear
    // ledger, no war, the neighbour's head as a link, and the two
    // ordinary levers aimed at the neighbour.
    let card =
        cold_border_card_for(&mut host, harrow, vantar).expect("the card is live from day one");
    assert_eq!(card.unavailable, None);
    assert_eq!(
        card.active.key.bindings.get("neighbour"),
        Some(&SituationSubject::Organisation(vantar))
    );
    let projection = card.projection.clone().expect("projection");
    assert_eq!(projection.stage, key("cold"));
    assert!(!projection.warning, "a standing fact raises no alarm");
    assert_eq!(
        integer_metric(&projection, "situation.metric.neighbour-regard"),
        Some(i64::from(SHADOW_OPENING_REGARD))
    );
    assert_eq!(
        integer_metric(&projection, "situation.metric.hostility-floor"),
        Some(SHADOW_FLOOR)
    );
    assert_eq!(
        integer_metric(&projection, "situation.metric.reconciliation-line"),
        Some(SHADOW_LINE)
    );
    assert_eq!(
        integer_metric(&projection, "situation.metric.open-grievances"),
        Some(0)
    );
    assert_eq!(
        integer_metric(&projection, "situation.metric.war-days"),
        None,
        "no war row without a war"
    );
    assert!(projection.links.iter().any(|link| {
        link.kind == aeon_data::model::SituationSubjectKind::Character && link.id == perrin.raw()
    }));
    let court = projection
        .actions
        .iter()
        .find(|action| action.id == key("court"))
        .expect("courtship is offered");
    assert_eq!(
        court.leader,
        Some(edrun),
        "courtship pins the head as envoy"
    );
    assert_eq!(court.target, AssignmentTarget::Org(vantar));
    let gifts = projection
        .actions
        .iter()
        .find(|action| action.id == key("send-gifts"))
        .expect("gifts are offered");
    assert_eq!(gifts.leader, None, "the envoy is a free choice");
    assert_eq!(gifts.target, AssignmentTarget::Org(vantar));

    // Both levers are ordinary open orders, forecast unblocked.
    let envoy = free_household_host(&mut host, 1);
    for (lever, leader) in [(key("court"), edrun), (key("send-gifts"), envoy)] {
        let forecast = aeon_sim::forecast::forecast(
            host.world_mut(),
            harrow,
            &lever,
            leader,
            AssignmentTarget::Org(vantar),
        )
        .expect("an ordinary defined assignment");
        assert_eq!(
            forecast.blocked, None,
            "{lever} is open against the neighbour"
        );
    }

    // With the covert operation live, the card is unchanged in kind: no
    // assignment, no plan, no culprit — the same public facts, and the
    // holder's alarm beside it is the only sign anything is afoot.
    host.advance_days(SHADOW_LIVE_DAY);
    assert!(vantar_operation(&mut host).is_some());
    let card = cold_border_card_for(&mut host, harrow, vantar).expect("still cold");
    let projection = card.projection.clone().expect("projection");
    assert!(
        projection
            .links
            .iter()
            .chain(&projection.participants)
            .all(|link| {
                link.kind != aeon_data::model::SituationSubjectKind::Assignment
                    && link.kind != aeon_data::model::SituationSubjectKind::Province
            }),
        "the card links no work and no ground"
    );
    for text in [&card.title, &card.summary] {
        for tell in SHADOW_TELLS {
            assert!(!text.contains(tell), "the card carries a tell: '{text}'");
        }
    }

    // A grievance Harrow comes to owe Vantar moves the card to its
    // aggrieved stage and counts on it; the regard is unchanged.
    aeon_sim::obligations::create(
        host.world_mut(),
        ObligationKind::Grievance,
        harrow,
        vantar,
        "a border slight",
        20,
        None,
    );
    evaluate(host.world_mut());
    let projection = cold_border_card_for(&mut host, harrow, vantar)
        .expect("still cold")
        .projection
        .expect("projection");
    assert_eq!(projection.stage, key("aggrieved"));
    assert_eq!(
        integer_metric(&projection, "situation.metric.open-grievances"),
        Some(1)
    );
    // Warmth alone does not close an aggrieved border: the card stands
    // at the line while the grievance does.
    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    evaluate(host.world_mut());
    assert!(
        cold_border_card_for(&mut host, harrow, vantar).is_some(),
        "an open grievance keeps the border cold whatever the regard"
    );
}

#[test]
fn save_load_and_replay_hold_across_the_reconciliation_boundary() {
    let content = repository_content();
    let mut host = scenario_host(SHADOW_SEED, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");

    // Four stages either side of the boundary: the ambition and campaign
    // adopted and uncommitted; the campaign let go the day after the
    // thaw; the ambition set aside on its pulse; and the window closed
    // with nothing mounted.
    struct Checkpoint {
        stage: &'static str,
        snapshot: aeon_sim::CampaignSnapshot,
        live_log: Vec<LogEntry>,
    }
    let checkpoint = |host: &mut SimHost, stage: &'static str| Checkpoint {
        stage,
        live_log: host.world_mut().resource::<MessageLog>().entries.clone(),
        snapshot: host.snapshot(),
    };
    let mut checkpoints = Vec::new();

    host.advance_days(180);
    assert!(vantar_ambition(&mut host).is_some());
    assert!(vantar_campaign(&mut host).is_some_and(|plan| plan.current_assignment.is_none()));
    checkpoints.push(checkpoint(&mut host, "adopted-uncommitted"));

    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    host.advance_days(1);
    assert!(vantar_campaign(&mut host).is_none());
    checkpoints.push(checkpoint(&mut host, "campaign-let-go"));

    host.advance_days(29);
    assert!(vantar_ambition(&mut host).is_none());
    checkpoints.push(checkpoint(&mut host, "ambition-set-aside"));

    host.advance_days(51);
    assert!(vantar_operation_against_harrow(&mut host).is_none());
    checkpoints.push(checkpoint(&mut host, "window-closed"));

    for Checkpoint {
        stage,
        snapshot,
        live_log,
    } in checkpoints
    {
        assert_eq!(
            snapshot.format_version,
            aeon_sim::SNAPSHOT_FORMAT_VERSION,
            "{stage}: every checkpoint is written in the current format"
        );
        let mut restored = SimHost::restore_with_content(snapshot, Arc::clone(&content))
            .unwrap_or_else(|error| panic!("{stage} restores: {error}"));

        // History restores line for line, audience for audience, and
        // nothing Harrow may read carries covert provenance at any stage.
        let restored_log = restored
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .clone();
        assert_eq!(restored_log.len(), live_log.len(), "{stage}");
        for (index, (before, after)) in live_log.iter().zip(&restored_log).enumerate() {
            assert_eq!(after.text, before.text, "{stage}: line {index}");
            assert_eq!(
                after.audience, before.audience,
                "{stage}: restore changed who may read line {index}: '{}'",
                after.text
            );
            assert!(after.audience.visible_to(None));
            if after.audience.visible_to(Some(harrow)) {
                for tell in SHADOW_TELLS {
                    assert!(
                        !after.text.contains(tell),
                        "{stage}: a line Harrow may read carries covert provenance: '{}'",
                        after.text
                    );
                }
            }
        }

        // Continuing from the restore is the same campaign.
        let mut twin =
            SimHost::restore_with_content(restored.snapshot(), Arc::clone(&content)).unwrap();
        assert_eq!(restored.state_hash(), twin.state_hash(), "{stage}");
        restored.advance_days(40);
        twin.advance_days(40);
        assert_eq!(
            restored.state_hash(),
            twin.state_hash(),
            "{stage}: replay after restore stays identical"
        );
    }
}

#[test]
fn a_war_between_the_houses_keeps_a_border_closing_at_the_line_from_reading_reconciled() {
    let mut host = scenario_host(SHADOW_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let war = declare_war(host.world_mut(), harrow, vantar, key("test-border-war"))
        .expect("an ordinary formal war");
    evaluate(host.world_mut());
    assert!(
        cold_border_card_for(&mut host, harrow, vantar).is_some(),
        "a war changes nothing about how cold the border is"
    );

    // The regard reaches the line with the war still standing. The border
    // is no longer cold, so the card closes — but it closes eased, never
    // reconciled: the card judges the same three facts the ambition's own
    // contract does, and a war already declared keeps every design its
    // grounds until peace is made.
    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    assert_eq!(vantar_regard(&mut host), SHADOW_LINE as i32);
    host.advance_days(1);
    assert!(cold_border_card_for(&mut host, harrow, vantar).is_none());
    let notice = last_cold_border_resolution(&mut host).expect("the card resolved");
    assert_eq!(
        notice.outcome,
        key("eased"),
        "at the line but at war, the card reads eased, not reconciled"
    );
    assert!(
        aeon_sim::wars::is_active_war(host.world_mut(), war),
        "no opinion threshold ends a war"
    );

    // The border runs cold again and the card returns; peace is then made
    // the ordinary way, and the same line — with no grievance owed and no
    // war between the houses — reads reconciled.
    set_vantar_esteem(&mut host, 0);
    assert_eq!(vantar_regard(&mut host), SHADOW_OPENING_REGARD);
    evaluate(host.world_mut());
    assert!(
        cold_border_card_for(&mut host, harrow, vantar).is_some(),
        "cold again, the border is a live card again"
    );
    conclude_war(host.world_mut(), war, WarConclusionKind::NegotiatedPeace)
        .expect("peace is negotiated");
    set_vantar_esteem(&mut host, SHADOW_LINE as i32 - SHADOW_OPENING_REGARD);
    host.advance_days(1);
    assert!(cold_border_card_for(&mut host, harrow, vantar).is_none());
    let notice = last_cold_border_resolution(&mut host).expect("the card resolved again");
    assert_eq!(
        notice.outcome,
        key("reconciled"),
        "with peace made, the line with a clear ledger is reconciliation"
    );
    assert!(
        notice.text.contains("no war between them"),
        "the resolution states the war clause: '{}'",
        notice.text
    );
    for tell in SHADOW_TELLS {
        assert!(!notice.text.contains(tell));
    }
}

// ---------------------------------------------------------------------------
// The two ordinary levers from the Cold Border card, resolved: gifts carry
// their own authored amounts under their own reason, in the one direction
// that matters, and stack with a head-led courtship to reach the line.
// ---------------------------------------------------------------------------

/// Seeds on which the day-one levers from the card — gifts by a free envoy
/// and courtship led by the head, both aimed at Vantar and submitted in
/// that order — resolve as named. Found by running the scenario and pinned
/// like every other authored seed.
const GIFTS_SUCCESS_SEED: u64 = 4;
const GIFTS_TRIUMPH_SEED: u64 = 2;
const GIFTS_SPURNED_SEED: u64 = 7;

/// Harrow's running assignment of the given definition, if any.
fn harrow_assignment(host: &mut SimHost, def: &str) -> Option<ActiveAssignment> {
    let harrow = org(host, "harrow");
    let world = host.world_mut();
    world
        .resource::<AssignmentsIndex>()
        .assignments
        .values()
        .find_map(|entity| {
            world
                .get::<ActiveAssignment>(*entity)
                .filter(|work| work.owner == harrow && work.def == key(def))
                .cloned()
        })
}

/// Pulls both levers from the live Vantar card on day one — gifts by a
/// free envoy, courtship by the head — and runs until both are accepted,
/// returning the two ordinary assignments (gifts, court).
fn pull_both_levers(host: &mut SimHost) -> (ActiveAssignment, ActiveAssignment) {
    let harrow = org(host, "harrow");
    let vantar = org(host, "vantar");
    let edrun = character(host, "edrun-harrow");
    let card = cold_border_card_for(host, harrow, vantar).expect("the card is live from day one");
    let situation = card.active.key.clone();
    let occurrence = card.active.occurrence();
    let envoy = free_household_host(host, 1);
    assert_ne!(envoy, edrun, "the gifts go by a free envoy, not the head");
    let gifts = host
        .submit(PlayerCommand::StartSituationAssignment {
            situation: situation.clone(),
            action: key("send-gifts"),
            leader: envoy,
            target: AssignmentTarget::Org(vantar),
            war: None,
        })
        .expect("gifts are a valid order from the card");
    let court = host
        .submit(PlayerCommand::StartSituationAssignment {
            situation,
            action: key("court"),
            leader: edrun,
            target: AssignmentTarget::Org(vantar),
            war: None,
        })
        .expect("courtship is a valid order from the card");
    while host.date() < gifts.day.max(court.day) {
        host.advance_days(1);
    }
    let gifts = harrow_assignment(host, "send-gifts").expect("the gifts are on their way");
    let court = harrow_assignment(host, "court").expect("the courtship is under way");
    assert_eq!(gifts.leader, envoy);
    assert_eq!(gifts.target, AssignmentTarget::Org(vantar));
    assert_eq!(gifts.origin_situation, Some(occurrence));
    assert_eq!(court.leader, edrun);
    (gifts, court)
}

/// Runs to the day the work resolves and checks it did so then.
fn run_until_resolved(host: &mut SimHost, work: &ActiveAssignment) {
    while host.date() < work.completes {
        host.advance_days(1);
    }
    assert!(
        aeon_sim::access::assignment(host.world_mut(), work.id).is_none(),
        "the work resolved on its ordinary day"
    );
}

#[test]
fn gifts_and_a_head_led_courtship_stack_to_reach_the_line_from_the_opening_standing() {
    let mut host = scenario_host(GIFTS_SUCCESS_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let edrun = character(&mut host, "edrun-harrow");
    let perrin = character(&mut host, "perrin-vantar");
    assert_eq!(vantar_regard(&mut host), SHADOW_OPENING_REGARD);
    let (gifts, court) = pull_both_levers(&mut host);

    // The gifts resolve first. A success lifts the neighbour head's regard
    // for OUR head by exactly the authored amount, under the lever's own
    // reason, for its own term — on Perrin's ledger, toward Edrun, and
    // nowhere else: the envoy who carried them earns nothing personally.
    run_until_resolved(&mut host, &gifts);
    let gifted = opinion_modifier(&mut host, perrin, "gifted").expect("the success modifier");
    assert_eq!(
        gifted.target, edrun,
        "the target's head regards the owner's head"
    );
    assert_eq!(gifted.amount, 15);
    assert_eq!(gifted.expires, Some(gifts.completes.add_days(1080)));
    assert!(
        opinion_modifier(&mut host, edrun, "gifted").is_none(),
        "nothing moves the other way"
    );
    assert!(
        opinion_modifier(&mut host, perrin, "courted-house").is_none(),
        "the courtship has not resolved yet"
    );
    assert_eq!(vantar_regard(&mut host), SHADOW_OPENING_REGARD + 15);

    // Then the courtship. Led by the head, its success lifts both the
    // personal and the house regard toward the same man, each under a
    // reason of its own. Modifiers replace per reason, so the three
    // distinct reasons stack — and from the opening standing, one ordinary
    // success of each lever lands exactly on the line.
    run_until_resolved(&mut host, &court);
    let courted = opinion_modifier(&mut host, perrin, "courted").expect("the personal modifier");
    assert_eq!(courted.target, edrun);
    assert_eq!(courted.amount, 10);
    let courted_house =
        opinion_modifier(&mut host, perrin, "courted-house").expect("the house modifier");
    assert_eq!(courted_house.target, edrun);
    assert_eq!(courted_house.amount, 10);
    assert_eq!(
        opinion_modifier(&mut host, perrin, "gifted")
            .expect("the gifts still count")
            .amount,
        15,
        "gifts and courtship carry distinct reasons, so neither replaces the other"
    );
    assert_eq!(
        vantar_regard(&mut host),
        SHADOW_LINE as i32,
        "a head-led court success plus a gifts success reaches the line exactly from {SHADOW_OPENING_REGARD}"
    );

    // The card closed the moment the regard first rose above the floor —
    // after the gifts, short of the line — and closed eased, in the
    // ordinary way, without waiting for anything to be reconciled.
    assert!(cold_border_card_for(&mut host, harrow, vantar).is_none());
    assert_eq!(
        last_cold_border_resolution(&mut host)
            .expect("the card resolved")
            .outcome,
        key("eased")
    );
}

#[test]
fn a_triumph_of_gifts_and_a_spurned_gift_carry_their_own_authored_amounts() {
    // A triumph: the larger amount for the longer term, the same direction
    // and the same reason as a plain success.
    let mut host = scenario_host(GIFTS_TRIUMPH_SEED, repository_content());
    let edrun = character(&mut host, "edrun-harrow");
    let perrin = character(&mut host, "perrin-vantar");
    let (gifts, _court) = pull_both_levers(&mut host);
    run_until_resolved(&mut host, &gifts);
    let gifted = opinion_modifier(&mut host, perrin, "gifted").expect("the triumph modifier");
    assert_eq!(gifted.target, edrun);
    assert_eq!(gifted.amount, 25);
    assert_eq!(gifted.expires, Some(gifts.completes.add_days(1800)));
    assert!(opinion_modifier(&mut host, perrin, "gift-spurned").is_none());
    assert_eq!(vantar_regard(&mut host), SHADOW_OPENING_REGARD + 25);

    // A disaster: the gifts come back, nothing is gifted, and the regard
    // falls by the authored amount for its own term under its own reason
    // — the border colder than it began.
    let mut host = scenario_host(GIFTS_SPURNED_SEED, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let edrun = character(&mut host, "edrun-harrow");
    let perrin = character(&mut host, "perrin-vantar");
    let (gifts, _court) = pull_both_levers(&mut host);
    run_until_resolved(&mut host, &gifts);
    assert!(opinion_modifier(&mut host, perrin, "gifted").is_none());
    let spurned =
        opinion_modifier(&mut host, perrin, "gift-spurned").expect("the disaster modifier");
    assert_eq!(spurned.target, edrun);
    assert_eq!(spurned.amount, -10);
    assert_eq!(spurned.expires, Some(gifts.completes.add_days(1440)));
    assert_eq!(vantar_regard(&mut host), SHADOW_OPENING_REGARD - 10);
    assert!(
        cold_border_card_for(&mut host, harrow, vantar).is_some(),
        "the border stays cold"
    );
}
