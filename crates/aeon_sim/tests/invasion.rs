//! The border in arms: Vantar's conditional one-holding invasion of Harrow
//! as an ordinary, data-authored formal war.
//!
//! What these tests hold is that the arc is simulation-native from end to
//! end — the host is the standing levy reinforced by an ordinary
//! assignment, the declaration an ordinary seven-day assignment, the
//! sides the ordinary frozen branches, the siege the ordinary engine
//! operation with its field engagement and title transfer, the peace the
//! ordinary negotiation — and that the balance the design asks for
//! follows from the accepted engagement formula rather than from any
//! tuning of it. Every campaign here is a fresh `SimHost` over the shipped
//! Ashkarr content; nothing is scripted that a player could not have
//! ordered by hand.

use std::sync::Arc;

use aeon_core::calendar::CalendarDate;
use aeon_core::rng::DeterministicRng;
use aeon_data::effect::ScriptEffect;
use aeon_data::model::{AiIntent, OutcomeKind, PlanRequires};
use aeon_data::{ContentKey, ContentSet, load_content};
use aeon_sim::agency::ScoredIntent;
use aeon_sim::assignments::{
    ActiveAssignment, AssignmentRejection, AssignmentRoles, AssignmentsIndex, MessageLog,
    apply_effects, commanded_army, validate_start,
};
use aeon_sim::forces::{ArmyLocation, ArmyRecord, ForcesIndex, form_army};
use aeon_sim::order::{adjust_order, defence_factor_permille, province_order};
use aeon_sim::plans::{
    Plans, enemy_border_provinces_in_war, requires_met, requires_met_by, try_adopt,
};
use aeon_sim::politics::{TitleHolder, TitleRecord, process_death};
use aeon_sim::warfare::{EngagementInputs, army_strength, decide_engagement, province_holder};
use aeon_sim::wars::{WarSideId, conclude_war, declare_war, negotiate_peace, war};
use aeon_sim::{
    ArmyId, AssignmentTarget, CampaignConfig, CampaignOver, CharacterId, MapIndex, OrgId,
    OrgResources, PoliticsIndex, ProvinceId, SimHost, WarConclusionKind, WarId,
};

fn key(text: &str) -> ContentKey {
    ContentKey::new(text).unwrap()
}

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

fn character(host: &mut SimHost, name: &str) -> CharacterId {
    host.world_mut().resource::<PoliticsIndex>().character_keys[&key(name)]
}

fn province(host: &mut SimHost, name: &str) -> ProvinceId {
    host.world_mut().resource::<MapIndex>().province_keys[&key(name)]
}

/// Every army an organisation fields, in stable ID order.
fn armies_of(host: &mut SimHost, owner: OrgId) -> Vec<ArmyRecord> {
    let world = host.world_mut();
    let forces = world.resource::<ForcesIndex>();
    forces
        .armies
        .values()
        .filter_map(|entity| world.get::<ArmyRecord>(*entity))
        .filter(|army| army.owner == owner)
        .cloned()
        .collect()
}

/// The one army a house fields at the opening: Harrow's 600-strong Guard
/// under Edrun at Ostragard, or Vantar's 450-strong levy under Perrin at
/// Cindral.
fn only_army(host: &mut SimHost, owner: &str) -> ArmyRecord {
    let owner = org(host, owner);
    let mut armies = armies_of(host, owner);
    assert_eq!(armies.len(), 1, "the house opens with one army");
    armies.remove(0)
}

fn army_record(host: &mut SimHost, army: ArmyId) -> ArmyRecord {
    aeon_sim::access::army(host.world_mut(), army)
        .expect("army indexed")
        .clone()
}

/// Stands an army in a province directly — the fixture's placement of a
/// defence, standing in for the marches a player would have ordered.
fn stand_army_in(host: &mut SimHost, army: ArmyId, at: ProvinceId) {
    let world = host.world_mut();
    let entity = aeon_sim::access::army_entity(world, army).expect("army indexed");
    world
        .get_mut::<ArmyRecord>(entity)
        .expect("army record")
        .location = ArmyLocation::Province(at);
}

fn set_army_manpower(host: &mut SimHost, army: ArmyId, manpower: i64) {
    let world = host.world_mut();
    let entity = aeon_sim::access::army_entity(world, army).expect("army indexed");
    world
        .get_mut::<ArmyRecord>(entity)
        .expect("army record")
        .manpower = manpower;
}

fn resources_mut(host: &mut SimHost, owner: OrgId, edit: impl FnOnce(&mut OrgResources)) {
    let world = host.world_mut();
    let entity = aeon_sim::access::org_entity(world, owner).expect("organisation indexed");
    let mut resources = world.get_mut::<OrgResources>(entity).expect("resources");
    edit(&mut resources);
}

/// Hands a province's title to another organisation directly — the
/// fixture's stand-in for a conquest or grant that happened elsewhere.
fn hand_province_to(host: &mut SimHost, province: ProvinceId, holder: OrgId) {
    let world = host.world_mut();
    let entity = {
        let index = world.resource::<PoliticsIndex>();
        let title = index.province_titles[&province];
        index.titles[&title]
    };
    world
        .get_mut::<TitleRecord>(entity)
        .expect("title record")
        .holder = TitleHolder::Org(holder);
}

fn vantar_assignments(host: &mut SimHost) -> Vec<ActiveAssignment> {
    let vantar = org(host, "vantar");
    let world = host.world_mut();
    world
        .resource::<AssignmentsIndex>()
        .assignments
        .values()
        .filter_map(|entity| world.get::<ActiveAssignment>(*entity))
        .filter(|work| work.owner == vantar)
        .cloned()
        .collect()
}

fn plan_of(host: &mut SimHost, who: CharacterId) -> Option<aeon_sim::plans::ActivePlan> {
    host.world_mut()
        .resource::<Plans>()
        .active
        .get(&who)
        .cloned()
}

/// The invade pressure the head-only scorer raises while the ambition
/// stands, built by hand so a test can drive adoption on the day it
/// chooses rather than waiting on the monthly pulse and its roll.
fn invade_pressure(target: AssignmentTarget) -> ScoredIntent {
    ScoredIntent {
        intent: AiIntent::Invade,
        assignment: key("raise-the-host"),
        target,
        score: 100,
        reason: String::new(),
        subject: None,
        explains: false,
    }
}

/// Advances day by day until the condition holds, or panics after the
/// budget. Returns the number of days advanced.
fn advance_until(
    host: &mut SimHost,
    budget: u32,
    what: &str,
    mut done: impl FnMut(&mut SimHost) -> bool,
) -> u32 {
    for day in 1..=budget {
        host.advance_days(1);
        if done(host) {
            return day;
        }
    }
    panic!("{what} did not happen within {budget} days");
}

/// Waits until the head is free to lead the given assignment: a routine
/// pulse may have given him something else to do.
fn wait_until_free(
    host: &mut SimHost,
    owner: OrgId,
    leader: CharacterId,
    assignment: &str,
    target: AssignmentTarget,
    war_id: Option<WarId>,
) {
    advance_until(host, 120, "the leader becomes free", |host| {
        aeon_sim::assignments::validate_start_in_war(
            host.world_mut(),
            owner,
            &key(assignment),
            leader,
            target,
            war_id,
        )
        .is_ok()
    });
}

/// Raises Vantar's host through the ordinary `raise-the-host` assignment
/// and lets its roll decide the size: the levy grows to 800 on a success,
/// to 900 on a triumph, and not at all on a failure, in which case the
/// call is repeated up to the budget. Returns the host's manpower, or
/// `None` when every attempt failed.
fn raise_the_host(host: &mut SimHost, attempts: u32) -> Option<i64> {
    let vantar = org(host, "vantar");
    let perrin = character(host, "perrin-vantar");
    let levy = only_army(host, "vantar");
    assert_eq!(levy.general, Some(perrin), "the head commands the levy");
    for _ in 0..attempts {
        wait_until_free(
            host,
            vantar,
            perrin,
            "raise-the-host",
            AssignmentTarget::None,
            None,
        );
        let id = aeon_sim::assignments::start_assignment(
            host.world_mut(),
            vantar,
            &key("raise-the-host"),
            perrin,
            AssignmentTarget::None,
        );
        advance_until(host, 90, "the muster resolves", |host| {
            aeon_sim::access::assignment(host.world_mut(), id).is_none()
        });
        let strength = army_record(host, levy.id).manpower;
        if strength > levy.manpower {
            return Some(strength);
        }
    }
    None
}

/// Starts a war-bound assignment through the shared gate.
fn start_in_war(
    host: &mut SimHost,
    owner: OrgId,
    assignment: &str,
    leader: CharacterId,
    target: AssignmentTarget,
    war_id: WarId,
) -> aeon_sim::AssignmentId {
    aeon_sim::assignments::validate_start_in_war(
        host.world_mut(),
        owner,
        &key(assignment),
        leader,
        target,
        Some(war_id),
    )
    .unwrap_or_else(|why| panic!("{assignment} is valid in its war: {why:?}"));
    aeon_sim::assignments::start_assignment_in_war(
        host.world_mut(),
        owner,
        &key(assignment),
        leader,
        target,
        Some(war_id),
    )
}

/// Every siege Vantar has ever started, by target province.
fn vantar_siege_targets(seen: &mut Vec<ProvinceId>, host: &mut SimHost) {
    for work in vantar_assignments(host) {
        if work.def == key("besiege")
            && let AssignmentTarget::ArmyToProvince(_, target) = work.target
            && !seen.contains(&target)
        {
            seen.push(target);
        }
    }
}

/// The sibling war is exactly `{vantar}` against `{harrow}`, whoever
/// declared it, with no liege on either side and no adoption on record.
fn assert_sides_are_the_two_houses(host: &mut SimHost, war_id: WarId) {
    let harrow = org(host, "harrow");
    let vantar = org(host, "vantar");
    let veyrin = org(host, "veyrin");
    let record = war(host.world_mut(), war_id)
        .expect("war on record")
        .clone();
    let members: Vec<_> = WarSideId::ALL
        .into_iter()
        .map(|side| record.side(side).members.clone())
        .collect();
    assert!(
        members
            .iter()
            .all(|side| side.len() == 1 && (side.contains(&vantar) || side.contains(&harrow))),
        "the sides are the two sibling houses alone: {members:?}"
    );
    assert_eq!(
        record.side_of(veyrin),
        None,
        "the common liege is on neither side"
    );
    assert!(
        record.adoption_history.is_empty(),
        "nobody adopted a side: {:?}",
        record.adoption_history
    );
}

// ---------------------------------------------------------------------------
// Balance, layer one: the accepted engagement formula, fed the arc's exact
// strategic inputs, with no tuning of its own.
// ---------------------------------------------------------------------------

#[test]
fn the_engagement_formula_makes_six_hundred_dangerous_and_a_thousand_decisive_on_settled_ground() {
    let mut host = scenario_host(1, repository_content());
    let vhorruk = province(&mut host, "vhorruk");
    let guard = only_army(&mut host, "harrow");
    let levy = only_army(&mut host, "vantar");
    assert_eq!(
        guard.manpower, 600,
        "Harrow's opening defence is the 600 Guard"
    );
    assert_eq!(
        levy.manpower, 450,
        "Vantar's levy before it is raised to a host"
    );

    // The authored host and its triumph, under Perrin's command of 6, and
    // the Guard under Edrun's 7 on home ground at the settled opening Order.
    let mut host_army = levy.clone();
    host_army.manpower = 800;
    let mut eager_army = levy.clone();
    eager_army.manpower = 900;
    let mut thousand = guard.clone();
    thousand.manpower = 1000;
    let world = host.world_mut();
    let attack = army_strength(world, &host_army, false);
    let eager = army_strength(world, &eager_army, false);
    let six_hundred = army_strength(world, &guard, true);
    let one_thousand = army_strength(world, &thousand, true);
    let order_factor = defence_factor_permille(province_order(world, vhorruk).order);
    assert_eq!(attack, 1040, "800 at +5% per point of command 6");
    assert_eq!(
        six_hundred, 972,
        "600 at command 7, lifted 20% for home ground"
    );
    assert_eq!(one_thousand, 1620);
    assert_eq!(
        order_factor, 1000,
        "settled ground scales the defence by nothing"
    );

    let sample = |attack_strength: i64, attacker_manpower: i64, defence: &ArmyRecord| -> usize {
        let defence_strength = army_strength(world, defence, true);
        (0..500u64)
            .filter(|seed| {
                let inputs = EngagementInputs {
                    attack_strength,
                    defence_strength,
                    order_factor,
                    attacker_manpower,
                    defender_manpower: defence.manpower,
                };
                let mut rng = DeterministicRng::derive(*seed, "invasion-balance", &[*seed]);
                decide_engagement(inputs, &mut rng).attacker_won
            })
            .count()
    };

    let against_six_hundred = sample(attack, 800, &guard);
    assert!(
        against_six_hundred > 250,
        "the host carries the field against 600 in a majority of engagements ({against_six_hundred}/500)"
    );
    assert!(
        against_six_hundred < 500,
        "600 is dangerous, not hopeless ({against_six_hundred}/500)"
    );
    assert_eq!(
        sample(attack, 800, &thousand),
        0,
        "a thousand standing together on settled ground holds every field the bounded swing allows"
    );
    assert_eq!(
        sample(eager, 900, &thousand),
        0,
        "even the 900-strong triumph host cannot reach a thousand on settled ground"
    );
}

// ---------------------------------------------------------------------------
// Balance, layer two: deterministic campaign samples through the ordinary
// muster, declaration, pressing plan, siege, and engagement.
// ---------------------------------------------------------------------------

/// How the defence of Vhorruk is fielded in a sample arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Defence {
    /// The opening Guard, marched into Vhorruk: 600 in the holding.
    GuardInTheHolding,
    /// Aleyn's thousand as a house naturally reaches it — the Guard in
    /// Vhorruk and a second stack of 400 raised at Ostragard. A thousand
    /// under arms; 600 where it matters.
    ThousandSplit,
    /// A thousand standing together in Vhorruk.
    ThousandInTheHolding,
}

const ARMS: [Defence; 3] = [
    Defence::GuardInTheHolding,
    Defence::ThousandSplit,
    Defence::ThousandInTheHolding,
];

/// Seeds per arm. Each sampled campaign raises its host once by the
/// muster's own roll, then plays the war out from that snapshot under
/// each defence.
const SAMPLE_SEEDS: u64 = 6;

/// One sampled war from a prepared host: the defence is fielded, the war
/// declared, and the AI's own pressing plan runs its orders and siege.
/// Returns whether Vhorruk fell.
fn sample_war(seed: u64, mut host: SimHost, defence: Defence) -> bool {
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let aleyn = character(&mut host, "aleyn-harrow");
    let vhorruk = province(&mut host, "vhorruk");
    let ostragard = province(&mut host, "ostragard");
    let guard = only_army(&mut host, "harrow").id;

    stand_army_in(&mut host, guard, vhorruk);
    match defence {
        Defence::GuardInTheHolding => {}
        Defence::ThousandSplit => {
            form_army(host.world_mut(), harrow, aleyn, 400, 80, ostragard);
        }
        Defence::ThousandInTheHolding => set_army_manpower(&mut host, guard, 1000),
    }
    let war_id = declare_war(host.world_mut(), vantar, harrow, key("border-war"))
        .expect("the sibling houses may go to war");

    // The AI's own pressing plan does the rest: doctrine on the host, one
    // siege of the holding across the border, then it is done.
    assert!(try_adopt(
        host.world_mut(),
        perrin,
        vantar,
        &[invade_pressure(AssignmentTarget::War(war_id))]
    ));
    assert_eq!(
        plan_of(&mut host, perrin).expect("adopted").def,
        key("press-the-border")
    );
    let mut sieges = Vec::new();
    advance_until(
        &mut host,
        220,
        "the pressing plan runs its course",
        |host| {
            vantar_siege_targets(&mut sieges, host);
            plan_of(host, perrin).is_none()
        },
    );
    assert_eq!(
        sieges,
        vec![vhorruk],
        "seed {seed}: the pressing plan besieges the one holding across the border only"
    );
    assert!(
        host.world_mut().get_resource::<CampaignOver>().is_none(),
        "seed {seed}: losing one holding never ends the campaign"
    );
    assert_sides_are_the_two_houses(&mut host, war_id);
    province_holder(host.world_mut(), vhorruk) == Some(vantar)
}

#[test]
fn deterministic_samples_show_six_hundred_dangerous_and_a_thousand_favourable_but_not_certain() {
    let content = repository_content();
    let mut lost: std::collections::BTreeMap<String, u64> =
        ARMS.iter().map(|arm| (format!("{arm:?}"), 0)).collect();
    let mut sizes = std::collections::BTreeSet::new();
    for seed in 1..=SAMPLE_SEEDS {
        let mut host = scenario_host(seed, Arc::clone(&content));
        let Some(size) = raise_the_host(&mut host, 3) else {
            // A house whose call to arms fails three times invades nobody
            // this year: a legitimate quiet outcome, counted as held.
            continue;
        };
        sizes.insert(size);
        let prepared = host.snapshot();
        for arm in ARMS {
            let host = SimHost::restore_with_content(prepared.clone(), Arc::clone(&content))
                .expect("the prepared campaign restores");
            if sample_war(seed, host, arm) {
                *lost.get_mut(&format!("{arm:?}")).unwrap() += 1;
            }
        }
    }
    let guard = lost["GuardInTheHolding"];
    let split = lost["ThousandSplit"];
    let together = lost["ThousandInTheHolding"];
    assert!(
        !sizes.is_empty() && sizes.iter().all(|size| [800, 900].contains(size)),
        "the muster's own roll sizes the host: {sizes:?}"
    );
    assert!(
        guard * 3 >= SAMPLE_SEEDS,
        "600 in the holding is dangerous: Vhorruk fell in only {guard} of {SAMPLE_SEEDS} seeds"
    );
    assert!(
        split > 0,
        "a thousand under arms guarantees nothing when 600 stand where it matters"
    );
    assert!(
        together < guard,
        "a thousand standing together is favourable: {together} losses against {guard}"
    );
    assert!(
        together * 4 <= SAMPLE_SEEDS,
        "a thousand standing together is favourable: Vhorruk fell in {together} of {SAMPLE_SEEDS} seeds"
    );
}

// ---------------------------------------------------------------------------
// The mechanisms the campaigns are built from.
// ---------------------------------------------------------------------------

#[test]
fn the_pressing_plan_marches_the_host_on_the_one_holding_across_the_border() {
    let mut host = scenario_host(7, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let vhorruk = province(&mut host, "vhorruk");
    let levy = only_army(&mut host, "vantar").id;
    set_army_manpower(&mut host, levy, 800);
    let war_id = declare_war(host.world_mut(), vantar, harrow, key("border-war")).unwrap();
    assert_eq!(
        enemy_border_provinces_in_war(host.world_mut(), vantar, war_id),
        vec![vhorruk],
        "Vhorruk is the one Harrow holding across the Ulmgorn border"
    );

    wait_until_free(
        &mut host,
        vantar,
        perrin,
        "besiege",
        AssignmentTarget::ArmyToProvince(levy, vhorruk),
        Some(war_id),
    );
    assert!(try_adopt(
        host.world_mut(),
        perrin,
        vantar,
        &[invade_pressure(AssignmentTarget::War(war_id))]
    ));
    assert_eq!(
        plan_of(&mut host, perrin).unwrap().def,
        key("press-the-border")
    );
    advance_until(&mut host, 60, "the siege starts", |host| {
        vantar_assignments(host)
            .iter()
            .any(|work| work.def == key("besiege"))
    });
    let siege = vantar_assignments(&mut host)
        .into_iter()
        .find(|work| work.def == key("besiege"))
        .expect("siege");
    assert_eq!(
        siege.target,
        AssignmentTarget::ArmyToProvince(levy, vhorruk),
        "the host marches on the holding across the border"
    );
    assert_eq!(siege.war, Some(war_id), "the siege carries the exact war");
    assert_eq!(siege.leader, perrin);
    assert_eq!(
        army_record(&mut host, levy).standing_order.0,
        vec![key("respond"), key("patrol")],
        "the host carries the doctrine"
    );
}

#[test]
fn the_strongest_army_selector_gives_the_doctrine_to_the_largest_force_the_head_commands() {
    let mut host = scenario_host(8, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let cindral = province(&mut host, "cindral");
    let levy = only_army(&mut host, "vantar").id;
    // A second, larger force under the same general, higher stable ID.
    let larger = form_army(host.world_mut(), vantar, perrin, 800, 160, cindral);
    assert!(levy < larger);
    let war_id = declare_war(host.world_mut(), vantar, harrow, key("border-war")).unwrap();

    assert!(try_adopt(
        host.world_mut(),
        perrin,
        vantar,
        &[invade_pressure(AssignmentTarget::War(war_id))]
    ));
    host.advance_days(1);
    assert_eq!(
        army_record(&mut host, larger).standing_order.0,
        vec![key("respond"), key("patrol")],
        "the strongest army carries the doctrine, not the lowest ID"
    );
    assert!(
        army_record(&mut host, levy).standing_order.is_empty(),
        "the smaller force is left as it was"
    );
}

#[test]
fn with_no_enemy_holding_across_the_border_the_head_sues_for_peace_instead() {
    let mut host = scenario_host(8, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let veyrin = org(&mut host, "veyrin");
    let perrin = character(&mut host, "perrin-vantar");
    let vhorruk = province(&mut host, "vhorruk");
    let levy = only_army(&mut host, "vantar").id;
    set_army_manpower(&mut host, levy, 800);
    let war_id = declare_war(host.world_mut(), vantar, harrow, key("border-war")).unwrap();

    // The geography changes under the war: the one border holding is
    // Veyrin's now, so there is nothing across the border to press.
    hand_province_to(&mut host, vhorruk, veyrin);
    assert!(enemy_border_provinces_in_war(host.world_mut(), vantar, war_id).is_empty());
    let pressing = &repository_content().plans[&key("press-the-border")].methods[0].requires;
    assert!(
        !requires_met(
            host.world_mut(),
            vantar,
            AssignmentTarget::War(war_id),
            pressing
        ),
        "the pressing gate needs an enemy holding across the border"
    );

    assert!(try_adopt(
        host.world_mut(),
        perrin,
        vantar,
        &[invade_pressure(AssignmentTarget::War(war_id))]
    ));
    assert_eq!(
        plan_of(&mut host, perrin).unwrap().def,
        key("settle-the-border"),
        "with nothing to press the head reaches for peace"
    );
    advance_until(&mut host, 120, "the peace overture starts", |host| {
        vantar_assignments(host)
            .iter()
            .any(|work| work.def == key("negotiate") && work.war == Some(war_id))
    });
    assert!(
        !vantar_assignments(&mut host)
            .iter()
            .any(|work| work.def == key("besiege")),
        "no siege is aimed anywhere"
    );
}

#[test]
fn preparation_declaration_and_their_derailments_are_authored_gates_over_live_state() {
    let content = repository_content();
    let mut host = scenario_host(9, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let veyrin = org(&mut host, "veyrin");
    let draksha = org(&mut host, "draksha");
    let koszev = org(&mut host, "koszev");
    let perrin = character(&mut host, "perrin-vantar");
    let target = AssignmentTarget::Org(harrow);
    let prepare = content.plans[&key("prepare-the-host")].methods[0]
        .requires
        .clone();
    let declare = content.plans[&key("declare-the-border-war")].methods[0]
        .requires
        .clone();
    let met = |host: &mut SimHost, req: &PlanRequires| {
        requires_met(host.world_mut(), vantar, target, req)
    };

    // The opening: Perrin regards Edrun at -15 and the levy stands at 450,
    // short of the host's authored 800 — the house prepares and does not
    // yet declare.
    assert!(
        met(&mut host, &prepare),
        "a hostile, capable house prepares"
    );
    assert!(
        !met(&mut host, &declare),
        "and does not declare from weakness"
    );

    // Depleted resources hold the preparation: a purse or a pool that
    // cannot bear the host.
    resources_mut(&mut host, vantar, |r| r.wealth = 10);
    assert!(!met(&mut host, &prepare), "no coin, no muster");
    resources_mut(&mut host, vantar, |r| r.wealth = 110);
    resources_mut(&mut host, vantar, |r| r.manpower = 300);
    assert!(!met(&mut host, &prepare), "no pool, no host");
    resources_mut(&mut host, vantar, |r| r.manpower = 900);
    assert!(met(&mut host, &prepare));

    // The host raised: 800 against 600 clears the three-quarters floor.
    // Preparation is done and the declaration opens.
    let levy = only_army(&mut host, "vantar").id;
    set_army_manpower(&mut host, levy, 800);
    assert!(
        !met(&mut host, &prepare),
        "a house at its host's strength raises no more"
    );
    assert!(met(&mut host, &declare), "a prepared house declares");

    // A war of the house's own delays the declaration for exactly as long
    // as it stands, whoever it is against; being swept into the liege's
    // war as a branch member is not a war of the house's own.
    let elsewhere = declare_war(host.world_mut(), vantar, koszev, key("elsewhere")).unwrap();
    assert!(
        !met(&mut host, &declare),
        "a house already at war opens no second front"
    );
    conclude_war(
        host.world_mut(),
        elsewhere,
        WarConclusionKind::NegotiatedPeace,
    )
    .unwrap();
    assert!(
        met(&mut host, &declare),
        "peace elsewhere reopens the declaration"
    );
    // The reading is "leads a side", not "declared": a war declared against
    // the house, which it leads the defending side of, holds the
    // declaration exactly as its own would.
    let against = declare_war(host.world_mut(), koszev, vantar, key("against-vantar")).unwrap();
    assert_eq!(
        war(host.world_mut(), against)
            .unwrap()
            .side(WarSideId::Defender)
            .leader,
        vantar,
        "Vantar leads the defending side"
    );
    assert!(
        !met(&mut host, &declare),
        "a house defending a war of its own opens no second front either"
    );
    conclude_war(
        host.world_mut(),
        against,
        WarConclusionKind::NegotiatedPeace,
    )
    .unwrap();
    assert!(met(&mut host, &declare), "and peace there reopens it too");
    let lieges_war = declare_war(host.world_mut(), veyrin, draksha, key("the-lieges-war")).unwrap();
    assert!(
        aeon_sim::wars::side_of(host.world_mut(), lieges_war, vantar).is_some(),
        "Vantar rides in its liege's war"
    );
    assert!(
        met(&mut host, &declare),
        "a liege's war is not the house's own and holds nothing"
    );
    conclude_war(
        host.world_mut(),
        lieges_war,
        WarConclusionKind::NegotiatedPeace,
    )
    .unwrap();

    // A Harrow that has answered Aleyn with a thousand — in any number of
    // stacks — still faces the declaration: 800 against 1,000 is 800
    // permille. A Harrow at 1,100 deters it outright.
    let guard = only_army(&mut host, "harrow").id;
    set_army_manpower(&mut host, guard, 1000);
    assert!(met(&mut host, &declare));
    set_army_manpower(&mut host, guard, 1100);
    assert!(
        !met(&mut host, &declare),
        "a neighbour more than a third stronger deters the declaration"
    );
    assert!(
        !met(&mut host, &prepare),
        "and the host is already at its strength: the house simply waits"
    );
    set_army_manpower(&mut host, guard, 600);

    // Friendship: the same reconciliation predicates as the shadows. Regard
    // lifted above the floor shuts both gates; the line lets the campaigns
    // and the ambition go.
    let edrun = character(&mut host, "edrun-harrow");
    {
        use aeon_sim::politics::{OpinionEntry, OpinionLedger};
        let world = host.world_mut();
        let entity = world.resource::<PoliticsIndex>().characters[&perrin];
        world
            .get_mut::<OpinionLedger>(entity)
            .unwrap()
            .set(OpinionEntry {
                target: edrun,
                amount: 35,
                reason: "test-thaw".to_owned(),
                expires: None,
            });
    }
    assert_eq!(
        aeon_sim::opinion_between(host.world_mut(), perrin, edrun),
        20
    );
    assert!(
        !met(&mut host, &declare),
        "above the floor there is no declaration"
    );
    set_army_manpower(&mut host, levy, 450);
    assert!(!met(&mut host, &prepare), "and no muster");
    let reconciled = content.plans[&key("declare-the-border-war")]
        .abandon_when
        .clone()
        .expect("the declaration carries the reconciliation gate");
    assert!(
        met(&mut host, &reconciled),
        "at the line the uncommitted campaign is let go"
    );
    assert_eq!(
        content.plans[&key("prepare-the-host")].abandon_when,
        Some(reconciled.clone())
    );
    assert_eq!(
        content.goals[&key("take-the-border")].set_aside_when,
        Some(reconciled),
        "the ambition is set aside by the same predicates"
    );
}

// ---------------------------------------------------------------------------
// The war itself: ordinary from declaration to peace, one holding at most,
// genuine and recoverable.
// ---------------------------------------------------------------------------

/// Runs the scripted arc on one seed: the host, the ordinary declaration
/// through the plan, the pressing plan's siege of an undefended Vhorruk,
/// and — when the siege carries — Harrow's recovery through the same war
/// and the negotiated peace. Returns `false` when this seed's siege did
/// not carry, so the caller can try the next.
fn loss_and_recovery(seed: u64, content: Arc<ContentSet>) -> bool {
    let mut host = scenario_host(seed, content);
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let veyrin = org(&mut host, "veyrin");
    let perrin = character(&mut host, "perrin-vantar");
    let aleyn = character(&mut host, "aleyn-harrow");
    let vhorruk = province(&mut host, "vhorruk");
    let tolmaz = province(&mut host, "tolmaz");
    let vhorruk_name = aeon_sim::access::province_name(host.world_mut(), vhorruk);

    // The declaration is the plan's own seven-day assignment, aimed at
    // the ambition's target organisation.
    let levy = only_army(&mut host, "vantar").id;
    set_army_manpower(&mut host, levy, 800);
    wait_until_free(
        &mut host,
        vantar,
        perrin,
        "declare-formal-war",
        AssignmentTarget::Org(harrow),
        None,
    );
    assert!(try_adopt(
        host.world_mut(),
        perrin,
        vantar,
        &[invade_pressure(AssignmentTarget::Org(harrow))]
    ));
    assert_eq!(
        plan_of(&mut host, perrin).unwrap().def,
        key("declare-the-border-war")
    );
    advance_until(&mut host, 60, "the war is declared", |host| {
        aeon_sim::wars::active_war_between(host.world_mut(), vantar, harrow).is_some()
    });
    let war_id = aeon_sim::wars::active_war_between(host.world_mut(), vantar, harrow).unwrap();
    let record = war(host.world_mut(), war_id).unwrap().clone();
    assert_eq!(record.side(WarSideId::Attacker).leader, vantar);
    assert_eq!(record.side(WarSideId::Defender).leader, harrow);
    assert_eq!(
        record.cause,
        key("declare-formal-war"),
        "the ordinary declaration is the cause on record"
    );
    assert_sides_are_the_two_houses(&mut host, war_id);

    // The pressing plan besieges the undefended holding across the border.
    advance_until(&mut host, 40, "the head is free again", |host| {
        plan_of(host, perrin).is_none()
    });
    assert!(try_adopt(
        host.world_mut(),
        perrin,
        vantar,
        &[invade_pressure(AssignmentTarget::War(war_id))]
    ));
    let mut sieges = Vec::new();
    advance_until(&mut host, 220, "the pressing plan ends", |host| {
        vantar_siege_targets(&mut sieges, host);
        plan_of(host, perrin).is_none()
    });
    assert_eq!(sieges, vec![vhorruk], "one holding, across the border");
    if province_holder(host.world_mut(), vhorruk) != Some(vantar) {
        return false;
    }

    // Genuine: the title has passed, the ground is disordered, the fall is
    // public history tagged to the exact war — and the campaign goes on.
    assert_eq!(province_order(host.world_mut(), vhorruk).order, 350);
    assert!(host.world_mut().get_resource::<CampaignOver>().is_none());
    assert_eq!(
        aeon_sim::order::held_provinces(host.world_mut(), harrow).len(),
        3
    );
    {
        let log = host.world_mut().resource::<MessageLog>().clone();
        let fell = log
            .entries
            .iter()
            .find(|entry| {
                entry.war == Some(war_id)
                    && entry.text.contains(&vhorruk_name)
                    && entry.text.contains("fallen")
            })
            .expect("the fall is on record against its war");
        assert!(
            fell.audience.visible_to(Some(harrow)),
            "an open war hides nothing"
        );
    }
    // The AI's pressing is spent: with the holding taken it sues for
    // peace rather than pressing on to Tolmaz, now the border.
    assert_eq!(
        enemy_border_provinces_in_war(host.world_mut(), vantar, war_id),
        vec![tolmaz]
    );
    assert!(try_adopt(
        host.world_mut(),
        perrin,
        vantar,
        &[invade_pressure(AssignmentTarget::War(war_id))]
    ));
    assert_eq!(
        plan_of(&mut host, perrin).unwrap().def,
        key("settle-the-border")
    );
    assert_sides_are_the_two_houses(&mut host, war_id);

    // Recoverable, through the same war: Harrow marches a stack in and
    // besieges its own lost holding from the war's card. The fixture
    // lends Harrow the Influence a longer war would have recharged.
    let stack = form_army(host.world_mut(), harrow, aleyn, 1200, 240, tolmaz);
    resources_mut(&mut host, harrow, |r| r.influence = 100);
    let mut retaken = false;
    for _ in 0..3 {
        // The household may have given Aleyn free work of her own; the
        // siege waits for her exactly as a player's order would.
        let target = AssignmentTarget::ArmyToProvince(stack, vhorruk);
        wait_until_free(&mut host, harrow, aleyn, "besiege", target, Some(war_id));
        let siege = start_in_war(&mut host, harrow, "besiege", aleyn, target, war_id);
        advance_until(&mut host, 90, "Harrow's siege resolves", |host| {
            aeon_sim::access::assignment(host.world_mut(), siege).is_none()
        });
        if province_holder(host.world_mut(), vhorruk) == Some(harrow) {
            retaken = true;
            break;
        }
    }
    if !retaken {
        return false;
    }
    assert_eq!(
        aeon_sim::order::held_provinces(host.world_mut(), harrow).len(),
        4
    );
    assert_eq!(
        aeon_sim::order::held_provinces(host.world_mut(), vantar).len(),
        2
    );
    assert_sides_are_the_two_houses(&mut host, war_id);

    // Peace is the ordinary whole-war settlement by a side leader, and
    // what each side holds at the peace is what it keeps.
    negotiate_peace(host.world_mut(), war_id, harrow).expect("the defender's leader may settle");
    let record = war(host.world_mut(), war_id).unwrap().clone();
    assert_eq!(
        record.conclusion.map(|c| c.kind),
        Some(WarConclusionKind::NegotiatedPeace)
    );
    host.advance_days(2);
    assert_eq!(province_holder(host.world_mut(), vhorruk), Some(harrow));
    assert!(host.world_mut().get_resource::<CampaignOver>().is_none());
    assert_eq!(
        aeon_sim::wars::side_of(host.world_mut(), war_id, veyrin),
        None,
        "Veyrin was never part of it"
    );
    assert!(
        !host
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.org == Some(veyrin) && entry.war == Some(war_id)),
        "no liege wrote a line into this war"
    );
    true
}

#[test]
fn defeat_transfers_one_holding_without_ending_the_campaign_and_harrow_retakes_it_in_the_same_war()
{
    let content = repository_content();
    // The siege's own authored contest decides whether the assault is
    // pressed at all, so a given seed's undefended holding may still hold;
    // the first seed on which it falls carries the walk.
    let carried = (1..=12u64).find(|seed| loss_and_recovery(*seed, Arc::clone(&content)));
    assert!(
        carried.is_some(),
        "an undefended holding falls to the ordinary siege within a dozen seeds"
    );
}

#[test]
fn a_besieged_field_settles_by_the_ordinary_engagement_and_the_outcome_is_on_the_roll() {
    // The scripted case where a battle is actually fought: the Guard stands
    // in Vhorruk, the host attacks. Whether the province falls is the
    // engagement's business; what the test holds is that either way the
    // ordinary systems did the work.
    let content = repository_content();
    let mut host = scenario_host(11, Arc::clone(&content));
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let vhorruk = province(&mut host, "vhorruk");
    let guard = only_army(&mut host, "harrow").id;
    let levy = only_army(&mut host, "vantar").id;
    stand_army_in(&mut host, guard, vhorruk);
    set_army_manpower(&mut host, levy, 800);
    let war_id = declare_war(host.world_mut(), vantar, harrow, key("border-war")).unwrap();
    let target = AssignmentTarget::ArmyToProvince(levy, vhorruk);
    wait_until_free(&mut host, vantar, perrin, "besiege", target, Some(war_id));
    let siege = start_in_war(&mut host, vantar, "besiege", perrin, target, war_id);
    let before = army_record(&mut host, guard).manpower;
    advance_until(&mut host, 90, "the siege resolves", |host| {
        aeon_sim::access::assignment(host.world_mut(), siege).is_none()
    });
    let holder = province_holder(host.world_mut(), vhorruk);
    let guard_now = aeon_sim::access::army(host.world_mut(), guard).map(|a| a.manpower);
    let host_now = aeon_sim::access::army(host.world_mut(), levy).map(|a| a.manpower);
    let log = host.world_mut().resource::<MessageLog>().clone();
    let siege_line = log
        .entries
        .iter()
        .rev()
        .find(|entry| entry.war == Some(war_id) && entry.org == Some(vantar))
        .expect("the siege wrote its result against the war");
    match holder {
        Some(h) if h == vantar => {
            assert!(siege_line.text.contains("fell") || siege_line.text.contains("fallen"));
            assert!(
                guard_now.is_none_or(|men| men < before),
                "a defender that lost the field lost men or broke"
            );
        }
        Some(h) if h == harrow => {
            assert!(siege_line.text.contains("broken") || siege_line.text.contains("disaster"));
            assert_eq!(
                host_now.map(|men| men <= 800),
                Some(true),
                "the host is no larger than it marched"
            );
        }
        other => panic!("Vhorruk is held by one of the two houses, not {other:?}"),
    }
    assert!(host.world_mut().get_resource::<CampaignOver>().is_none());
    assert_sides_are_the_two_houses(&mut host, war_id);
}

#[test]
fn the_border_ambition_is_open_and_authored_in_data() {
    // The shipped contract, read back: a plain (not covert) ambition
    // windowed to the invasion stretch, resolved against the same hostile
    // border neighbour the shadows resolve, favouring the invade pressure,
    // and set aside by the reconciliation line.
    let content = repository_content();
    let ambition = &content.goals[&key("take-the-border")];
    assert!(!ambition.covert, "an invasion is open");
    assert_eq!(ambition.favours, vec![AiIntent::Invade]);
    assert_eq!(ambition.trigger.min_campaign_day, Some(260));
    assert_eq!(ambition.trigger.max_campaign_day, Some(360));
    assert_eq!(ambition.trigger.is_vassal, Some(true));
    assert_eq!(ambition.trigger.has_army, Some(true));
    assert_eq!(ambition.trigger.min_manpower, Some(400));
    assert_eq!(
        ambition.target_selector,
        aeon_data::model::GoalTargetSelector::HostileBorderNeighbour {
            max_head_opinion: -10,
            with_grievance: true,
        }
    );
    assert_eq!(
        ambition.max_days, 517,
        "the horizon covers the whole chain the ambition drives, peace included"
    );
    let raise = &content.assignments[&key("raise-the-host")];
    assert_eq!(raise.ai_intent, AiIntent::Invade);
    assert!(
        !raise.ai_available,
        "the host is raised only through the plan"
    );
    assert!(!raise.covert);
    assert!(raise.results.contains_key(&OutcomeKind::CriticalSuccess));
    assert!(
        raise.requires.leader_commands_army,
        "the muster is refused to a leader with no command to grow"
    );
    let prepare = &content.plans[&key("prepare-the-host")];
    for method in &prepare.methods {
        assert_eq!(
            method.steps[0].skip_if,
            Some(PlanRequires {
                leader_commands_army: Some(true),
                ..Default::default()
            }),
            "the levy is skipped on the head's own command, not on any house army"
        );
    }
    let press = &content.plans[&key("press-the-border")];
    assert_eq!(press.methods.len(), 1, "one way to press: one holding");
    assert_eq!(
        press.methods[0].requires.war_has_enemy_border_province,
        Some(true)
    );
    assert!(press.cooldown_days >= 360, "pressed once per war");
    let settle = &content.plans[&key("settle-the-border")];
    assert_eq!(
        settle.cooldown_days, 0,
        "peace is sued for as often as it takes"
    );
    let declare = &content.plans[&key("declare-the-border-war")];
    for method in &declare.methods {
        assert_eq!(method.requires.at_war, Some(false));
        assert_eq!(method.requires.min_branch_manpower, Some(800));
        assert_eq!(
            method.requires.min_target_branch_manpower_permille,
            Some(750)
        );
    }
}

// ---------------------------------------------------------------------------
// The reinforce-army effect at runtime: exact amounts, the pool clamp, the
// command it lands on, and the command it needs before anything is spent.
// ---------------------------------------------------------------------------

fn vantar_resources(host: &mut SimHost) -> OrgResources {
    let vantar = org(host, "vantar");
    let world = host.world_mut();
    let entity = aeon_sim::access::org_entity(world, vantar).expect("organisation indexed");
    *world.get::<OrgResources>(entity).expect("resources")
}

/// Applies a reinforcement as the muster's resolution would, with `leader`
/// in the leader's role — the effect on its own, so the amounts can be
/// read exactly without a month's production or a day's supply draw
/// between the reading and the deed.
fn reinforce(host: &mut SimHost, leader: CharacterId, manpower: i64, supplies: i64) {
    let vantar = org(host, "vantar");
    let roles = AssignmentRoles {
        assignment: Some(key("raise-the-host")),
        leader: Some(leader),
        ..Default::default()
    };
    apply_effects(
        host.world_mut(),
        &[ScriptEffect::ReinforceArmy { manpower, supplies }],
        &roles,
        Some(vantar),
    );
}

#[test]
fn reinforcing_draws_exactly_the_authored_soldiers_and_supplies_from_the_pool() {
    let mut host = scenario_host(1, repository_content());
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let levy = only_army(&mut host, "vantar");
    let before = vantar_resources(&mut host);
    assert_eq!(
        (before.manpower, before.supplies),
        (900, 180),
        "Vantar's opening pool"
    );

    // The muster's success: 350 soldiers and 60 supplies, from the pool
    // into the levy the head commands, and nowhere else.
    reinforce(&mut host, perrin, 350, 60);
    let after = vantar_resources(&mut host);
    let host_army = army_record(&mut host, levy.id);
    assert_eq!(after.manpower, 550, "900 less the authored 350");
    assert_eq!(after.supplies, 120, "180 less the authored 60");
    assert_eq!(host_army.manpower, 800, "the levy is the host now");
    assert_eq!(host_army.supplies, levy.supplies + 60);
    assert_eq!(host_army.general, Some(perrin), "under the same command");
    assert_eq!(
        armies_of(&mut host, vantar).len(),
        1,
        "one command, one stack: nothing was formed"
    );
    assert_eq!(after.wealth, before.wealth, "the effect touches no coin");
    assert_eq!(after.influence, before.influence);

    // The triumph: 450 and 80, likewise.
    reinforce(&mut host, perrin, 450, 80);
    let after = vantar_resources(&mut host);
    assert_eq!(after.manpower, 100, "550 less the eager 450");
    assert_eq!(after.supplies, 40, "120 less the eager 80");
    assert_eq!(army_record(&mut host, levy.id).manpower, 1250);
    assert_eq!(
        army_record(&mut host, levy.id).supplies,
        levy.supplies + 60 + 80
    );
}

#[test]
fn reinforcing_is_clamped_to_what_the_pool_holds() {
    let mut host = scenario_host(1, repository_content());
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let levy = only_army(&mut host, "vantar");

    // A pool short of the call: the army grows by what there is, and the
    // pool is emptied rather than overdrawn.
    resources_mut(&mut host, vantar, |r| {
        r.manpower = 200;
        r.supplies = 30;
    });
    reinforce(&mut host, perrin, 350, 60);
    let after = vantar_resources(&mut host);
    assert_eq!(
        (after.manpower, after.supplies),
        (0, 0),
        "emptied, never negative"
    );
    let army = army_record(&mut host, levy.id);
    assert_eq!(army.manpower, levy.manpower + 200, "the pool's remainder");
    assert_eq!(army.supplies, levy.supplies + 30);

    // An empty pool reinforces nothing at all.
    reinforce(&mut host, perrin, 350, 60);
    let after = vantar_resources(&mut host);
    assert_eq!((after.manpower, after.supplies), (0, 0));
    assert_eq!(
        army_record(&mut host, levy.id).manpower,
        levy.manpower + 200
    );
    assert_eq!(army_record(&mut host, levy.id).supplies, levy.supplies + 30);
}

#[test]
fn reinforcing_grows_the_lowest_stable_army_the_leader_generals() {
    let mut host = scenario_host(1, repository_content());
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let cindral = province(&mut host, "cindral");
    let levy = only_army(&mut host, "vantar").id;
    // A second force under the same general, higher stable ID, larger:
    // the reinforcement reads the command by ID, not by size.
    let second = form_army(host.world_mut(), vantar, perrin, 600, 120, cindral);
    assert!(levy < second, "the levy has the lower stable ID");
    let pool = vantar_resources(&mut host);

    reinforce(&mut host, perrin, 350, 60);
    assert_eq!(
        army_record(&mut host, levy).manpower,
        450 + 350,
        "the lowest stable ID grows"
    );
    assert_eq!(
        army_record(&mut host, second).manpower,
        600,
        "the higher ID is left as it was"
    );
    let after = vantar_resources(&mut host);
    assert_eq!(
        after.manpower,
        pool.manpower - 350,
        "drawn once, not per army"
    );
    assert_eq!(after.supplies, pool.supplies - 60);
    assert_eq!(
        commanded_army(host.world_mut(), vantar, perrin),
        Some(levy),
        "the same reading the start gate uses"
    );
}

#[test]
fn reinforcing_with_no_command_changes_nothing() {
    // The effect's own guard: reached with a leader who generals nothing,
    // it draws nothing and forms nothing. The rule that keeps the muster
    // from being accepted at all is the start gate, tested below.
    let mut host = scenario_host(1, repository_content());
    let vantar = org(&mut host, "vantar");
    let valka = character(&mut host, "valka-vantar");
    let levy = only_army(&mut host, "vantar");
    let pool = vantar_resources(&mut host);
    assert_eq!(commanded_army(host.world_mut(), vantar, valka), None);

    reinforce(&mut host, valka, 350, 60);
    assert_eq!(vantar_resources(&mut host), pool, "the pool is untouched");
    assert_eq!(army_record(&mut host, levy.id).manpower, levy.manpower);
    assert_eq!(army_record(&mut host, levy.id).supplies, levy.supplies);
    assert_eq!(armies_of(&mut host, vantar).len(), 1, "nothing was formed");
}

#[test]
fn the_muster_spends_the_purse_on_acceptance_and_the_pool_only_when_the_call_is_answered() {
    let content = repository_content();
    let mut host = scenario_host(2, Arc::clone(&content));
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let levy = only_army(&mut host, "vantar");

    for _ in 0..6 {
        wait_until_free(
            &mut host,
            vantar,
            perrin,
            "raise-the-host",
            AssignmentTarget::None,
            None,
        );
        let prepared = host.snapshot();
        let purse = vantar_resources(&mut host);
        let id = aeon_sim::assignments::start_assignment(
            host.world_mut(),
            vantar,
            &key("raise-the-host"),
            perrin,
            AssignmentTarget::None,
        );
        // The coin and Influence go on acceptance; no soldier moves until
        // the call is answered.
        let accepted = vantar_resources(&mut host);
        assert_eq!(accepted.wealth, purse.wealth - 50);
        assert_eq!(accepted.influence, purse.influence - 10);
        assert_eq!(accepted.manpower, purse.manpower);
        assert_eq!(accepted.supplies, purse.supplies);

        let days = advance_until(&mut host, 90, "the muster resolves", |host| {
            aeon_sim::access::assignment(host.world_mut(), id).is_none()
        });
        // A twin that never mustered, advanced the same days, isolates the
        // effect from the month's production and the daily supply draw the
        // pool sees either way.
        let mut twin = SimHost::restore_with_content(prepared, Arc::clone(&content))
            .expect("the prepared campaign restores");
        twin.advance_days(days);
        let real = vantar_resources(&mut host);
        let idle = vantar_resources(&mut twin);
        let grown = army_record(&mut host, levy.id).manpower - levy.manpower;
        let (men, stores) = match grown {
            0 => {
                assert_eq!(
                    real.manpower, idle.manpower,
                    "an unanswered call draws no soldiers"
                );
                assert_eq!(real.supplies, idle.supplies, "nor supplies");
                continue;
            }
            350 => (350, 60),
            450 => (450, 80),
            other => panic!("the host grew by an unauthored {other}"),
        };
        assert_eq!(
            idle.manpower - real.manpower,
            men,
            "the pool fell by exactly the soldiers the host gained"
        );
        assert_eq!(
            idle.supplies - real.supplies,
            stores,
            "and by exactly the supplies committed"
        );
        // The host's own train is read against the idle twin's, not against
        // the opening figure: an army eats while the muster runs, and the
        // reinforced host — larger by the soldiers it just gained — eats
        // more. The exact transfer is pinned by the effect's own test; what
        // this holds is that the stores the pool committed reached the
        // command and nowhere else.
        let idle_train = army_record(&mut twin, levy.id).supplies;
        let host_train = army_record(&mut host, levy.id).supplies;
        assert!(
            host_train > idle_train && host_train - idle_train <= stores,
            "the committed stores reached the host: {host_train} against an idle {idle_train}, {stores} committed"
        );
        assert_eq!(armies_of(&mut host, vantar).len(), 1, "no second army");
        return;
    }
    panic!("the call to arms was not answered in six attempts");
}

// ---------------------------------------------------------------------------
// The command the muster needs: refused at the start, spending nothing.
// ---------------------------------------------------------------------------

#[test]
fn a_leader_with_no_command_is_refused_the_muster_before_anything_is_spent() {
    let mut host = scenario_host(1, repository_content());
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let valka = character(&mut host, "valka-vantar");
    let pool = vantar_resources(&mut host);
    assert_eq!(commanded_army(host.world_mut(), vantar, valka), None);

    // The one shared gate refuses the order, and the forecast shows why.
    assert_eq!(
        validate_start(
            host.world_mut(),
            vantar,
            &key("raise-the-host"),
            valka,
            AssignmentTarget::None,
        ),
        Err(AssignmentRejection::NoCommand)
    );
    let forecast = aeon_sim::forecast::forecast(
        host.world_mut(),
        vantar,
        &key("raise-the-host"),
        valka,
        AssignmentTarget::None,
    )
    .expect("a known assignment forecasts");
    assert_eq!(forecast.blocked, Some(AssignmentRejection::NoCommand));
    assert_eq!(
        forecast.wealth_cost, 50,
        "the player still sees what it would cost"
    );
    assert_eq!(vantar_resources(&mut host), pool, "nothing is spent");
    assert!(
        vantar_assignments(&mut host).is_empty(),
        "nothing is started"
    );
    assert!(
        !host
            .world_mut()
            .resource::<MessageLog>()
            .entries
            .iter()
            .any(|entry| entry.text.contains("host")),
        "nothing is logged"
    );

    // The head who generals the levy passes the same gate.
    assert!(
        validate_start(
            host.world_mut(),
            vantar,
            &key("raise-the-host"),
            perrin,
            AssignmentTarget::None,
        )
        .is_ok()
    );
}

#[test]
fn a_successor_whose_levy_answers_to_the_dead_musters_afresh_instead_of_reinforcing_nothing() {
    let mut host = scenario_host(1, repository_content());
    let harrow = org(&mut host, "harrow");
    let vantar = org(&mut host, "vantar");
    let perrin = character(&mut host, "perrin-vantar");
    let edrun = character(&mut host, "edrun-harrow");
    let levy = only_army(&mut host, "vantar");
    let target = AssignmentTarget::Org(harrow);

    // Perrin dies. The house passes to a successor; the levy does not: it
    // still answers to the dead, so the house fields an army its head does
    // not command.
    let date = host.date();
    process_death(host.world_mut(), perrin, date);
    let successor =
        aeon_sim::access::org_head(host.world_mut(), vantar).expect("Vantar has a successor");
    assert_ne!(successor, perrin);
    assert_eq!(army_record(&mut host, levy.id).general, Some(perrin));
    assert_eq!(commanded_army(host.world_mut(), vantar, successor), None);
    let commands = PlanRequires {
        leader_commands_army: Some(true),
        ..Default::default()
    };
    let fields = PlanRequires {
        has_army: Some(true),
        ..Default::default()
    };
    assert!(
        requires_met(host.world_mut(), vantar, target, &fields),
        "the house still fields an army"
    );
    assert!(
        !requires_met(host.world_mut(), vantar, target, &commands),
        "but its head commands none — the ambition's reading"
    );
    assert!(
        !requires_met_by(host.world_mut(), Some(successor), vantar, target, &commands),
        "nor does the successor as a plan's actor"
    );

    // The muster is refused to the successor at the start, spending nothing.
    let pool = vantar_resources(&mut host);
    assert_eq!(
        validate_start(
            host.world_mut(),
            vantar,
            &key("raise-the-host"),
            successor,
            AssignmentTarget::None,
        ),
        Err(AssignmentRejection::NoCommand)
    );
    assert_eq!(vantar_resources(&mut host), pool);

    // The preparation plan, taken up by the successor on the ambition's
    // ill will, does not skip the levy: it musters a fresh army for the
    // head instead of reaching for a reinforcement nobody could receive.
    {
        use aeon_sim::politics::{OpinionEntry, OpinionLedger};
        let world = host.world_mut();
        let entity = world.resource::<PoliticsIndex>().characters[&successor];
        world
            .get_mut::<OpinionLedger>(entity)
            .unwrap()
            .set(OpinionEntry {
                target: edrun,
                amount: -40,
                reason: "test-ill-will".to_owned(),
                expires: None,
            });
    }
    assert!(aeon_sim::opinion_between(host.world_mut(), successor, edrun) <= -10);
    wait_until_free(
        &mut host,
        vantar,
        successor,
        "muster",
        AssignmentTarget::None,
        None,
    );
    assert!(try_adopt(
        host.world_mut(),
        successor,
        vantar,
        &[invade_pressure(target)]
    ));
    assert_eq!(
        plan_of(&mut host, successor).expect("adopted").def,
        key("prepare-the-host")
    );
    advance_until(
        &mut host,
        60,
        "the successor musters a fresh levy",
        |host| {
            vantar_assignments(host)
                .iter()
                .any(|work| work.def == key("muster") && work.leader == successor)
        },
    );
    assert!(
        !vantar_assignments(&mut host)
            .iter()
            .any(|work| work.def == key("raise-the-host")),
        "no reinforcement is attempted while the head commands nothing"
    );
    assert_eq!(
        army_record(&mut host, levy.id).manpower,
        levy.manpower,
        "the dead man's levy is left as it was"
    );
}

// ---------------------------------------------------------------------------
// The border selector with more than one holding across the border.
// ---------------------------------------------------------------------------

#[test]
fn the_border_selector_prefers_the_most_disordered_holding_and_breaks_ties_by_stable_id() {
    let content = repository_content();
    for tie in [false, true] {
        let mut host = scenario_host(7, Arc::clone(&content));
        let harrow = org(&mut host, "harrow");
        let vantar = org(&mut host, "vantar");
        let perrin = character(&mut host, "perrin-vantar");
        let vhorruk = province(&mut host, "vhorruk");
        let tolmaz = province(&mut host, "tolmaz");
        let karvessa = province(&mut host, "karvessa");
        let levy = only_army(&mut host, "vantar").id;
        set_army_manpower(&mut host, levy, 800);

        // A second Harrow holding across the border: Karvessa is Vantar's
        // now, and Tolmaz shares a route with it.
        hand_province_to(&mut host, karvessa, vantar);
        let war_id = declare_war(host.world_mut(), vantar, harrow, key("border-war")).unwrap();
        let (lower, higher) = (tolmaz.min(vhorruk), tolmaz.max(vhorruk));
        assert_eq!(
            enemy_border_provinces_in_war(host.world_mut(), vantar, war_id),
            vec![lower, higher],
            "both holdings are across the border, in stable ID order"
        );

        let lower_order = province_order(host.world_mut(), lower).order;
        let higher_order = province_order(host.world_mut(), higher).order;
        let expected = if tie {
            // Equal Order: the lowest stable ID.
            adjust_order(host.world_mut(), higher, lower_order - higher_order);
            assert_eq!(
                province_order(host.world_mut(), higher).order,
                province_order(host.world_mut(), lower).order
            );
            lower
        } else {
            // The higher ID made the more disordered, so the Order rule and
            // not the tie-break decides.
            adjust_order(host.world_mut(), higher, lower_order - 50 - higher_order);
            assert!(
                province_order(host.world_mut(), higher).order
                    < province_order(host.world_mut(), lower).order
            );
            higher
        };

        wait_until_free(
            &mut host,
            vantar,
            perrin,
            "besiege",
            AssignmentTarget::ArmyToProvince(levy, expected),
            Some(war_id),
        );
        assert!(try_adopt(
            host.world_mut(),
            perrin,
            vantar,
            &[invade_pressure(AssignmentTarget::War(war_id))]
        ));
        assert_eq!(
            plan_of(&mut host, perrin).unwrap().def,
            key("press-the-border")
        );
        advance_until(&mut host, 60, "the siege starts", |host| {
            vantar_assignments(host)
                .iter()
                .any(|work| work.def == key("besiege"))
        });
        let siege = vantar_assignments(&mut host)
            .into_iter()
            .find(|work| work.def == key("besiege"))
            .expect("siege");
        assert_eq!(
            siege.target,
            AssignmentTarget::ArmyToProvince(levy, expected),
            "tie {tie}: the most disordered holding across the border, lowest stable ID on a tie"
        );
    }
}
