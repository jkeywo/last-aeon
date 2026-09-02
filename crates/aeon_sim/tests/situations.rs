use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_data::{ContentKey, ContentSet, load_content};
use aeon_sim::assignments::{
    ActiveAssignment, AssignmentRejection, AssignmentTarget, AssignmentsIndex, LogChannel,
    MessageLog, validate_start,
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
