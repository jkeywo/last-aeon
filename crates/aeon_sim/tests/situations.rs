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
