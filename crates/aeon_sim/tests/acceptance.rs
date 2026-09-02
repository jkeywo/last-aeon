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
    AssignmentTarget, CampaignConfig, CharacterId, OrgId, PendingPopups, PlayerCommand,
    PoliticsIndex, SimHost,
};

fn repository_content() -> Arc<ContentSet> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
    assert!(set.is_some(), "content loads: {:?}", report.findings);
    Arc::new(set.unwrap())
}

/// The authored deck with one deterministic runtime-only Situation fault.
///
/// Content validation can prove that the named function exists, but only a
/// live semantic world can prove that it returns a projection map. Pointing
/// the Consular projection at its (valid) trigger function exercises that
/// boundary without adding test-only content to the production deck.
fn repository_content_with_consular_projection_error() -> Arc<ContentSet> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let mut sources = aeon_data::fs::read_content_dir(&root).expect("assets/content readable");
    let situations = sources
        .iter_mut()
        .find(|source| source.path.ends_with("situations.rhai"))
        .expect("authored Situations source");
    let projection = "projection_fn: \"consular_vacancy_projection\"";
    assert!(
        situations.source.contains(projection),
        "the acceptance fault must replace the authored Consular projection"
    );
    situations.source = situations
        .source
        .replace(projection, "projection_fn: \"consular_vacancy_instances\"");

    let (strings, report) = aeon_data::fs::read_string_table(&root).expect("strings readable");
    assert!(
        !report.has_errors(),
        "string findings: {:?}",
        report.findings
    );
    let (set, report) = load_content(&sources, &strings.expect("valid string table"));
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
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("court"),
        leader: edrun,
        target: AssignmentTarget::Org(veyrin),
    })
    .unwrap();
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("manage-estates"),
        leader: kessarin,
        target: AssignmentTarget::None,
    })
    .unwrap();
    h.advance_days(120);

    // The head curries Sanctora favour, then musters a levy.
    h.submit(PlayerCommand::StartAssignment {
        assignment: key("curry-favour"),
        leader: edrun,
        target: AssignmentTarget::None,
    })
    .unwrap();
    h.advance_days(120);

    h.submit(PlayerCommand::StartAssignment {
        assignment: key("muster"),
        leader: edrun,
        target: AssignmentTarget::None,
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
        .submit(PlayerCommand::StartAssignment {
            assignment: key("court"),
            leader: edrun,
            target: AssignmentTarget::Org(veyrin),
        })
        .unwrap();
    original.advance_days(300);

    // Snapshot here, then continue with more decisions.
    let mid_snapshot = original.snapshot();
    let snapshot_date = original.date();

    original
        .submit(PlayerCommand::StartAssignment {
            assignment: key("muster"),
            leader: edrun,
            target: AssignmentTarget::None,
        })
        .unwrap();
    original.advance_days(150);
    original
        .submit(PlayerCommand::StartAssignment {
            assignment: key("curry-favour"),
            leader: edrun,
            target: AssignmentTarget::None,
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

/// Milestone 2 acceptance: a multi-year playthrough that exercises the
/// systems this milestone added — provincial order moving under pressure,
/// contextual events firing and being answered, obligations settling, and
/// autonomous houses responding to their own pressures — and proves the
/// whole enhanced campaign still replays exactly from a mid-run snapshot.
#[test]
fn the_enhanced_campaign_replays_from_a_mid_campaign_snapshot() {
    use aeon_sim::events::EventState;
    use aeon_sim::obligations::Obligations;
    use aeon_sim::order::{ORDER_MAX, adjust_order, province_order};

    let content = repository_content();
    let mut original = scenario_host(content.clone(), 4242);

    let edrun = char_id(&mut original, "edrun-harrow");
    let veyrin = org_id(&mut original, "veyrin");
    let harrow = org_id(&mut original, "harrow");

    // Year one: the head courts the liege while the realm settles.
    original
        .submit(PlayerCommand::StartAssignment {
            assignment: key("court"),
            leader: edrun,
            target: AssignmentTarget::Org(veyrin),
        })
        .unwrap();
    original.advance_days(200);

    // Knock one of the player's own holdings badly out of order, so the
    // order system, its events, and the AI all have something to react to.
    let hyperions_rest = original
        .world_mut()
        .resource::<aeon_sim::MapIndex>()
        .province_keys[&key("hyperions-rest")];
    adjust_order(original.world_mut(), hyperions_rest, -500);
    original.advance_days(500);

    // Mid-campaign snapshot, taken with events, obligations and order all
    // in mid-flight.
    let snapshot = original.snapshot();
    let bytes = persistence::snapshot_to_ron(&snapshot).expect("snapshot serialises");

    // Answer whatever the world has asked, then play on for two more years.
    let play_on = |h: &mut SimHost| {
        for _ in 0..8 {
            let pending = h.world_mut().resource::<PendingPopups>().clone();
            let Some(popup) = pending.popups.first().cloned() else {
                break;
            };
            let choice = popup.choices[0].0.clone();
            let _ = h.submit(PlayerCommand::AnswerPopup {
                popup: popup.id,
                choice,
            });
            h.advance_days(1);
        }
        h.advance_days(720);
    };
    play_on(&mut original);

    // The replay: restore the snapshot and play the identical continuation.
    let restored_snapshot = persistence::snapshot_from_ron(&bytes).expect("snapshot deserialises");
    let mut replayed =
        SimHost::restore_with_content(restored_snapshot, content).expect("snapshot restores");
    play_on(&mut replayed);

    assert_eq!(
        replayed.state_hash(),
        original.state_hash(),
        "a campaign carrying order, events, obligations and reactive houses \
         must replay to the identical state"
    );

    // And the milestone's systems must actually have been exercised, or
    // the guarantee above would be vacuous.
    let world = original.world_mut();
    let events = world.resource::<EventState>();
    assert!(
        !events.history.is_empty(),
        "the playthrough should have drawn contextual events"
    );
    let ledger = world.resource::<Obligations>();
    assert!(
        ledger.entries.len() >= 9,
        "the authored obligations should be on the books"
    );
    let order = province_order(world, hyperions_rest).order;
    assert!(
        order < ORDER_MAX,
        "the disordered holding should still bear the marks of it"
    );
    let acted = world
        .resource::<aeon_sim::MessageLog>()
        .entries
        .iter()
        .any(|entry| entry.org != Some(harrow) && entry.text.contains("began '"));
    assert!(
        acted,
        "at least one autonomous house should have acted on a pressure and said why"
    );
}

/// Snapshot 20 acceptance for the connected Situation and formal-war slice.
///
/// Focused tests own the individual rules. This test keeps one deliberately
/// dense campaign state alive across the same serialise, restore, and continue
/// path as acceptance: the complete authored Situation deck, a runtime fault,
/// simultaneous wars, internal-war adoption, a concluded occurrence and its
/// resolution, exact provenance, and a still-running war-bound operation.
#[test]
fn snapshot_20_replays_connected_situations_and_formal_wars() {
    use std::collections::BTreeSet;

    use aeon_sim::assignments::{ActiveAssignment, AssignmentsIndex, MessageLog};
    use aeon_sim::situations::{
        SituationInstanceKey, SituationOccurrence, SituationState, SituationSubject, active_cards,
        evaluate,
    };
    use aeon_sim::wars::{
        WarConclusionKind, WarSideId, Wars, adopt_side, conclude_war, declare_war,
    };

    fn war_situation(
        host: &mut SimHost,
        war: aeon_sim::WarId,
    ) -> (SituationInstanceKey, SituationOccurrence) {
        let active = host
            .world_mut()
            .resource::<SituationState>()
            .active
            .values()
            .find(|active| {
                active.key.definition == key("formal-war")
                    && active.key.bindings.get("war") == Some(&SituationSubject::War(war))
            })
            .cloned()
            .expect("formal war has one active Situation");
        (active.key.clone(), active.occurrence())
    }

    let content = repository_content_with_consular_projection_error();
    assert_eq!(
        content.situations.keys().cloned().collect::<BTreeSet<_>>(),
        [
            key("consular-vacancy"),
            key("court-awaits"),
            key("favour-debt"),
            key("formal-war"),
            key("kessarin-order"),
            key("planetary-succession"),
        ]
        .into_iter()
        .collect(),
        "acceptance runs the complete six-definition authored deck"
    );

    let mut original = scenario_host(Arc::clone(&content), 18_1818);
    let opening_definitions: BTreeSet<_> = original
        .world_mut()
        .resource::<SituationState>()
        .active
        .keys()
        .map(|instance| instance.definition.clone())
        .collect();
    assert_eq!(
        opening_definitions,
        [
            key("planetary-succession"),
            key("favour-debt"),
            key("court-awaits"),
        ]
        .into_iter()
        .collect(),
        "every applicable non-war Situation is live on day one"
    );

    // The authored Consul begins in office. Exercise the same vacancy path
    // that a campaign reaches when that character dies, completing the
    // non-war deck while the campaign is still on its opening date.
    let consul = {
        let world = original.world_mut();
        let title = world.resource::<PoliticsIndex>().title_keys[&key("consul-of-the-sector")];
        match aeon_sim::access::title(world, title)
            .expect("authored Consular title")
            .holder
        {
            aeon_sim::politics::TitleHolder::Character(holder) => holder,
            holder => panic!("the authored Consul should begin in office, got {holder:?}"),
        }
    };
    let opening_date = original.date();
    aeon_sim::politics::process_death(original.world_mut(), consul, opening_date);
    evaluate(original.world_mut());
    let consul_occurrence = {
        let state = original.world_mut().resource::<SituationState>();
        let (instance, active) = state
            .active
            .iter()
            .find(|(instance, _)| instance.definition == key("consular-vacancy"))
            .expect("Consular Vacancy is attached on day one");
        assert!(
            state.runtime_errors.contains_key(instance),
            "the malformed live projection is unavailable"
        );
        active.occurrence()
    };
    assert!(
        original
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.situations.contains(&consul_occurrence)),
        "the runtime fault is permanently tagged to its exact lifecycle"
    );

    let harrow = org_id(&mut original, "harrow");
    let vantar = org_id(&mut original, "vantar");
    let veyrin = org_id(&mut original, "veyrin");
    let draksha = org_id(&mut original, "draksha");

    // The first occurrence is an internal sibling conflict. Its common liege
    // explicitly adopts Harrow's side, freezing that decision in the ledger.
    let internal = declare_war(
        original.world_mut(),
        harrow,
        vantar,
        key("acceptance-internal-war"),
    )
    .expect("the sibling branches may fight");
    evaluate(original.world_mut());
    let (internal_situation, internal_occurrence) = war_situation(&mut original, internal);
    adopt_side(original.world_mut(), internal, veyrin, WarSideId::Attacker)
        .expect("the common liege may explicitly adopt Harrow's side");

    // A second occurrence overlaps the first and remains active across the
    // snapshot, carrying an operation launched from its Situation card.
    let active_war = declare_war(
        original.world_mut(),
        harrow,
        draksha,
        key("acceptance-simultaneous-war"),
    )
    .expect("a distinct opposing branch permits a simultaneous war");
    evaluate(original.world_mut());
    let (active_situation, active_occurrence) = war_situation(&mut original, active_war);
    assert_eq!(
        original.world_mut().resource::<Wars>().active().count(),
        2,
        "both formal-war occurrences were simultaneously active"
    );

    conclude_war(
        original.world_mut(),
        internal,
        WarConclusionKind::NegotiatedPeace,
    )
    .expect("the adopted internal war may conclude as a whole");
    evaluate(original.world_mut());
    let resolution = original
        .world_mut()
        .resource::<SituationState>()
        .resolutions
        .iter()
        .find(|notice| notice.situation == internal_situation)
        .cloned()
        .expect("the concluded war leaves a resolution");
    assert_eq!(resolution.occurrence(), internal_occurrence);
    assert!(
        original
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| {
                entry.war == Some(internal) && entry.situations.contains(&internal_occurrence)
            }),
        "the concluded occurrence retains exact war and Situation provenance"
    );

    let siege = active_cards(original.world_mut())
        .into_iter()
        .find(|card| card.active.key == active_situation)
        .and_then(|card| card.projection)
        .and_then(|projection| {
            projection
                .actions
                .into_iter()
                .find(|action| action.id == key("besiege"))
        })
        .expect("the player's active formal war offers a concrete siege");
    let start = original
        .submit(PlayerCommand::StartSituationAssignment {
            situation: active_situation.clone(),
            action: siege.id,
            leader: siege.leader.expect("the army general is projected"),
            target: siege.target,
            war: Some(active_war),
        })
        .expect("the projected war action is authoritatively valid");
    while original.date() < start.day {
        original.advance_days(1);
    }
    let operation = {
        let world = original.world_mut();
        world
            .resource::<AssignmentsIndex>()
            .assignments
            .values()
            .filter_map(|entity| world.get::<ActiveAssignment>(*entity))
            .find(|assignment| assignment.origin_situation.as_ref() == Some(&active_occurrence))
            .cloned()
            .expect("the Situation action starts one exact war-bound operation")
    };
    assert_eq!(operation.war, Some(active_war));
    assert_eq!(operation.def, key("besiege"));

    // Leave a meaningful player command pending in the snapshot. Both copies
    // will apply that exact envelope during continuation.
    let dismissal = original
        .submit(PlayerCommand::DismissSituationResolution {
            resolution: resolution.id,
        })
        .expect("the resolution may be dismissed");
    assert!(dismissal.day > original.date());

    let snapshot = original.snapshot();
    assert_eq!(snapshot.format_version, 20);
    assert_eq!(
        snapshot
            .state
            .situations
            .active
            .keys()
            .map(|instance| instance.definition.clone())
            .collect::<BTreeSet<_>>(),
        [
            key("consular-vacancy"),
            key("favour-debt"),
            key("formal-war"),
            // The accepted siege answered the court, so the household's
            // first demand is live by the midpoint.
            key("kessarin-order"),
            key("planetary-succession"),
        ]
        .into_iter()
        .collect(),
        "all authored non-one-shot Situation kinds are connected at the midpoint"
    );
    assert_eq!(snapshot.state.wars.records.len(), 2);
    let internal_record = &snapshot.state.wars.records[&internal];
    assert_eq!(internal_record.adoption_history.len(), 1);
    assert_eq!(
        internal_record.adoption_history[0].side,
        WarSideId::Attacker
    );
    assert_eq!(
        internal_record.conclusion.expect("concluded war").kind,
        WarConclusionKind::NegotiatedPeace
    );
    assert!(snapshot.state.wars.records[&active_war].active());
    assert!(
        snapshot
            .state
            .assignments
            .assignments
            .iter()
            .any(|assignment| {
                assignment.id == operation.id
                    && assignment.war == Some(active_war)
                    && assignment.origin_situation.as_ref() == Some(&active_occurrence)
            })
    );
    assert!(
        snapshot
            .state
            .situations
            .resolutions
            .iter()
            .any(|notice| notice.id == resolution.id)
    );
    assert!(
        snapshot
            .state
            .situations
            .runtime_errors
            .keys()
            .any(|instance| instance.definition == key("consular-vacancy"))
    );
    assert!(
        snapshot
            .state
            .pending_commands
            .iter()
            .any(|envelope| envelope == &dismissal)
    );

    let midpoint_hash = original.state_hash();
    let bytes = persistence::snapshot_to_ron(&snapshot).expect("Snapshot 20 serialises");
    let decoded = persistence::snapshot_from_ron(&bytes).expect("Snapshot 20 deserialises");
    let mut replayed =
        SimHost::restore_with_content(decoded, content).expect("Snapshot 20 restores");
    assert_eq!(
        replayed.state_hash(),
        midpoint_hash,
        "restore reproduces the complete connected midpoint"
    );

    original.advance_days(10);
    replayed.advance_days(10);
    assert_eq!(
        replayed.state_hash(),
        original.state_hash(),
        "the Snapshot-20 Situation and formal-war state continues identically"
    );
    for host in [&mut original, &mut replayed] {
        assert!(
            host.applied_commands().contains(&dismissal),
            "the snapshotted dismissal was applied"
        );
        assert!(
            host.world_mut()
                .resource::<SituationState>()
                .resolutions
                .iter()
                .all(|notice| notice.id != resolution.id)
        );
        assert!(
            host.world_mut()
                .resource::<SituationState>()
                .runtime_errors
                .keys()
                .any(|instance| instance.definition == key("consular-vacancy"))
        );
        let world = host.world_mut();
        assert!(
            world
                .resource::<AssignmentsIndex>()
                .assignments
                .values()
                .filter_map(|entity| world.get::<ActiveAssignment>(*entity))
                .any(|assignment| {
                    assignment.id == operation.id
                        && assignment.war == Some(active_war)
                        && assignment.origin_situation.as_ref() == Some(&active_occurrence)
                }),
            "the exact war-bound siege remains in flight"
        );
    }
}

/// On the real authored scenario, an eligible autonomous head can adopt the
/// planetary ambition immediately, declare a personal claim through the
/// ordinary one-day assignment, and press it through the ordinary long-running
/// assignment once their complete realm is dominant. The press remains
/// probabilistic; acceptance proves the autonomous attempt, not that this seed
/// must win the title.
#[test]
fn an_autonomous_house_pursues_the_claim_as_a_campaign() {
    use aeon_sim::goals::Goals;
    use aeon_sim::plans::Plans;

    let content = repository_content();
    let mut h = scenario_host(content, 31337);
    let (veyrin, veyrin_head, title, body) = {
        let world = h.world_mut();
        let veyrin = world.resource::<PoliticsIndex>().org_keys[&key("veyrin")];
        let veyrin_head = aeon_sim::access::org_head(world, veyrin).unwrap();
        let (title, body) = aeon_sim::crisis::paramountcy(world).expect("paramountcy");
        (veyrin, veyrin_head, title, body)
    };
    assert_eq!(
        aeon_sim::crisis::dominant_claimant(h.world_mut(), body),
        Some(veyrin),
        "the authored complete realm makes Veyrin the initial leader"
    );

    let ambition = key("take-the-planet");
    let campaign = key("press-the-claim");
    let declaration = key("declare-paramount-claim");
    let press = key("press-claim");
    let mut adopted_ambition = false;
    let mut chose_declare_method = false;
    let mut chose_press_method = false;
    let mut ran_declaration = false;
    let mut held_personal_claim = false;
    let mut ran_press = false;

    for _ in 0..720 {
        h.advance_days(1);
        let world = h.world_mut();
        adopted_ambition |= world
            .resource::<Goals>()
            .active
            .get(&veyrin)
            .is_some_and(|goal| goal.def == ambition);
        if let Some(plan) = world.resource::<Plans>().active.get(&veyrin_head)
            && plan.def == campaign
        {
            chose_declare_method |= plan.method == "declare";
            chose_press_method |= plan.method == "press-after-peace";
        }
        let active_assignments: Vec<_> = world
            .resource::<aeon_sim::AssignmentsIndex>()
            .assignments
            .values()
            .filter_map(|entity| world.get::<aeon_sim::ActiveAssignment>(*entity))
            .filter(|assignment| assignment.owner == veyrin)
            .map(|assignment| assignment.def.clone())
            .collect();
        ran_declaration |= active_assignments.contains(&declaration);
        ran_press |= active_assignments.contains(&press);
        held_personal_claim |= world
            .resource::<aeon_sim::crisis::ParamountClaims>()
            .entries
            .contains_key(&(title, veyrin_head));
        if adopted_ambition
            && chose_declare_method
            && chose_press_method
            && ran_declaration
            && held_personal_claim
            && ran_press
        {
            break;
        }
    }

    assert!(
        adopted_ambition,
        "Veyrin should adopt the planetary ambition"
    );
    assert!(
        chose_declare_method && ran_declaration && held_personal_claim,
        "the autonomous loop should declare a personal claim through its authored method"
    );
    assert!(
        chose_press_method && ran_press,
        "the autonomous loop should return through the zero-cooldown press method"
    );
}

/// Milestone 8 acceptance: on the real authored scenario, a great house
/// forms a grand ambition unprompted and presses an advisory directive on
/// a house that answers to it — with no scripted nudge.
#[test]
fn a_great_house_forms_an_ambition_and_directs_a_vassal() {
    use aeon_sim::goals::Goals;

    let content = repository_content();
    let mut h = scenario_host(content, 90210);

    let mut directed_vassal = false;
    let mut ambitious_house = None;
    for _ in 0..48 {
        h.advance_days(30);
        let active: Vec<OrgId> = h
            .world_mut()
            .resource::<Goals>()
            .active
            .keys()
            .copied()
            .collect();
        for house in active {
            ambitious_house = Some(house);
            // Does any house receive a directive from an ambition?
            for vassal in aeon_sim::politics::vassals_of(h.world_mut(), house) {
                if !aeon_sim::goals::directives_on(h.world_mut(), vassal).is_empty() {
                    directed_vassal = true;
                }
            }
        }
        if directed_vassal {
            break;
        }
    }

    assert!(
        ambitious_house.is_some(),
        "some autonomous house should form a grand ambition on the real scenario"
    );
    assert!(
        directed_vassal,
        "a house pursuing a directive-pressing ambition should press it on a vassal"
    );
}

/// Milestone 9 acceptance: on the real authored scenario the worlds are
/// economically interdependent — the moon cannot feed itself while the
/// planet grows a surplus — and a transport takes up the route that
/// answers the want, unbidden. A blockade cuts the line.
#[test]
fn the_scenario_worlds_trade_grain_across_the_gulf() {
    use aeon_sim::MapIndex;

    let content = repository_content();
    let mut h = scenario_host(content, 24680);

    let vesk = h.world_mut().resource::<MapIndex>().body_keys[&key("vesk")];
    let ashkarr = h.world_mut().resource::<MapIndex>().body_keys[&key("ashkarr")];
    let grain = key("grain");

    // The planet grows a grain surplus; the moon runs a grain deficit.
    assert!(
        aeon_sim::trade::body_balance(h.world_mut(), ashkarr)[&grain] > 0,
        "the planet feeds itself and more"
    );
    assert!(
        aeon_sim::trade::body_balance(h.world_mut(), vesk)[&grain] < 0,
        "the moon cannot grow the grain it eats"
    );
    assert!(
        aeon_sim::trade::body_in_want(h.world_mut(), vesk),
        "so, untraded, the moon is in want"
    );

    // Left to run, a transport takes up the route that answers it.
    let hauler =
        h.world_mut().resource::<aeon_sim::ForcesIndex>().ship_keys[&key("karvess-hauler")];
    let mut routed = false;
    for _ in 0..24 {
        h.advance_days(30);
        let has_route = {
            let entity = h.world_mut().resource::<aeon_sim::ForcesIndex>().ships[&hauler];
            h.world_mut()
                .get::<aeon_sim::ShipRecord>(entity)
                .and_then(|s| s.route.clone())
        };
        if let Some(route) = has_route {
            assert_eq!(route.good, grain, "the route carries grain");
            routed = true;
            break;
        }
    }
    assert!(
        routed,
        "an idle transport should take up the surplus-to-deficit route unbidden"
    );
    assert!(
        aeon_sim::trade::route_relief(h.world_mut(), vesk, &grain) > 0,
        "and the grain now reaches the moon"
    );

    // Blockade the delivery dock and the line is cut.
    let route = {
        let entity = h.world_mut().resource::<aeon_sim::ForcesIndex>().ships[&hauler];
        h.world_mut()
            .get::<aeon_sim::ShipRecord>(entity)
            .unwrap()
            .route
            .clone()
            .unwrap()
    };
    let picket = h.world_mut().resource::<aeon_sim::ForcesIndex>().ship_keys[&key("pale-lantern")];
    let harrow = h.world_mut().resource::<aeon_sim::PoliticsIndex>().org_keys[&key("harrow")];
    let defender = aeon_sim::warfare::province_holder(h.world_mut(), route.sink)
        .expect("the delivery dock has a holder");
    let war =
        aeon_sim::wars::active_war_between(h.world_mut(), harrow, defender).unwrap_or_else(|| {
            aeon_sim::wars::declare_war(h.world_mut(), harrow, defender, key("trade-blockade"))
                .expect("the fixture may create an exact blockade war")
        });
    {
        let entity = h.world_mut().resource::<aeon_sim::ForcesIndex>().ships[&picket];
        let mut ship = h
            .world_mut()
            .get_mut::<aeon_sim::ShipRecord>(entity)
            .unwrap();
        ship.owner = harrow;
        ship.location = aeon_sim::forces::ShipLocation::Docked(route.sink);
        ship.blockading = Some(aeon_sim::forces::Blockade {
            province: route.sink,
            war,
        });
    }
    assert_eq!(
        aeon_sim::trade::route_relief(h.world_mut(), vesk, &grain),
        0,
        "a blockade at the dock stops the grain"
    );
}
