//! Intent-driven character agency.
//!
//! A house never acts; a person does. Every autonomous choice here is
//! made by a character — in this module, the head of a house — spending
//! the authority of the organisation they command. The head scores the
//! small set of concrete pressures their house is actually under —
//! holdings slipping or under occupation, political weakness,
//! obligations outstanding, resources run down, and a claim worth
//! pressing — and acts on one of the best, choosing from the same
//! assignment catalogue the player uses.
//!
//! This is a priority system, not a separate AI action model: every
//! choice still goes through [`crate::assignments::validate_start`] and
//! [`crate::assignments::start_assignment`], so an autonomous character can do nothing
//! the player could not, and is bound by the same eligibility rules.
//!
//! Scoring is integer arithmetic over state the player can also see, and
//! candidates are built in stable ID order, so agency replays identically.
//! When a character acts on a pressure the house says so in the log, which
//! is how the player learns why — without exposing the roll that chose
//! between near-equal options as though it were a certainty.

use aeon_core::rng::DeterministicRng;
use aeon_data::ContentKey;
use aeon_data::model::{AiIntent, AssignmentTargetKind};
use bevy::prelude::World;

use crate::assignments::{
    AssignmentTarget, AssignmentsIndex, LogChannel, LogEntry, LogSubject, start_assignment,
    validate_start,
};
use crate::clock::CampaignClock;
use crate::economy::OrgResources;
use crate::ids::{CharacterId, OrgId, ProvinceId};
use crate::obligations::{ObligationKind, Obligations};
use crate::order::{ORDER_START, held_provinces, province_order};
use crate::politics::{CampaignOver, PlayerHouse};
use crate::state::ContentDb;
use crate::text::TextDb;

/// How many of the best-scoring intents a house will consider.
const SHORTLIST: usize = 3;
/// A score below which a pressure is not worth acting on.
const THRESHOLD: i64 = 20;

/// One thing a house might do, and why.
#[derive(Clone, Debug)]
pub struct ScoredIntent {
    /// The pressure this answers, for matching plans to occasions.
    pub intent: AiIntent,
    /// The assignment it would start.
    pub assignment: ContentKey,
    /// What that assignment would act on.
    pub target: AssignmentTarget,
    /// How badly it wants to.
    pub score: i64,
    /// The pressure behind it, in words, for the log.
    pub reason: String,
    /// The subject the reason is about, for navigation.
    pub subject: Option<LogSubject>,
    /// Whether this answers a real pressure, and so is worth explaining.
    /// Routine upkeep speaks through its results instead.
    pub explains: bool,
}

/// The AI-available assignments that answer a pressure, in content-key order.
///
/// The mapping from pressure to assignment is authored on the assignment itself, so the
/// simulation never names a piece of content: a new assignment that declares an
/// intent joins the AI's repertoire with no engine change.
fn assignments_for(
    world: &World,
    intent: AiIntent,
    expected: AssignmentTargetKind,
) -> Vec<ContentKey> {
    let Some(content) = world.get_resource::<ContentDb>() else {
        return Vec::new();
    };
    content
        .0
        .assignments
        .values()
        .filter(|def| def.ai_available && def.ai_intent == intent && def.target == expected)
        .map(|def| def.key.clone())
        .collect()
}

/// The first assignment answering a pressure, if content offers one.
fn assignment_for(
    world: &World,
    intent: AiIntent,
    expected: AssignmentTargetKind,
) -> Option<ContentKey> {
    assignments_for(world, intent, expected).into_iter().next()
}

/// A house's resource position.
fn resources(world: &World, org: OrgId) -> Option<OrgResources> {
    crate::access::org_entity(world, org)
        .and_then(|entity| world.get::<OrgResources>(entity))
        .copied()
}

/// Scores every pressure a character feels through the organisation
/// whose authority they wield, best first.
///
/// The pressures are the house's — its holdings, ledger, and standing —
/// but the judgement is a person's: what the actor may act on depends on
/// who they are, and today that means only the head may weigh the
/// paramountcy claim.
///
/// Deterministic throughout: holdings and organisations are visited in
/// stable ID order, and every score is integer arithmetic over visible
/// state.
pub fn score_intents(world: &World, actor: CharacterId, authority: OrgId) -> Vec<ScoredIntent> {
    let strings = world.resource::<TextDb>();
    let mut intents: Vec<ScoredIntent> = Vec::new();

    // ---- Holdings that are slipping, or that someone is standing on ----
    let holdings = held_provinces(world, authority);
    let mut worst: Option<(ProvinceId, i64, bool)> = None;
    for province in &holdings {
        let state = province_order(world, *province);
        let pressures = crate::order::pressures(world, *province);
        let shortfall = i64::from((ORDER_START - state.order).max(0));
        let mut score = shortfall / 8;
        if pressures.occupied {
            score += 60;
        }
        if state.in_unrest() {
            score += 80;
        }
        if score > 0 && worst.is_none_or(|(_, best, _)| score > best) {
            worst = Some((*province, score, pressures.occupied));
        }
    }
    if let Some((province, score, occupied)) = worst {
        let name = crate::access::province_name(world, province);
        // Troops answer an occupation; administration answers disorder.
        if occupied
            && let Some(assignment) =
                assignment_for(world, AiIntent::Muster, AssignmentTargetKind::None)
        {
            intents.push(ScoredIntent {
                intent: AiIntent::Muster,
                assignment,
                target: AssignmentTarget::None,
                score: score + 10,
                reason: strings.format("sim.intent.occupied", &[("province", &name)]),
                subject: Some(LogSubject::Province(province)),
                explains: true,
            });
        }
        if let Some(assignment) = assignment_for(world, AiIntent::Order, AssignmentTargetKind::None)
        {
            intents.push(ScoredIntent {
                intent: AiIntent::Order,
                assignment,
                target: AssignmentTarget::None,
                score,
                reason: strings.format("sim.intent.restive", &[("province", &name)]),
                subject: Some(LogSubject::Province(province)),
                explains: true,
            });
        }
    }

    // ---- Obligations outstanding ----
    if let Some(ledger) = world.get_resource::<Obligations>() {
        // A favour owed to us is worth collecting.
        let mut best_favour: Option<(OrgId, i32)> = None;
        let mut worst_grievance: Option<(OrgId, i32)> = None;
        for entry in ledger.open() {
            let heaviest = |current: Option<(OrgId, i32)>| {
                current.is_none_or(|(_, weight)| entry.weight > weight)
            };
            if entry.creditor == authority
                && entry.kind == ObligationKind::Favour
                && heaviest(best_favour)
            {
                best_favour = Some((entry.debtor, entry.weight));
            } else if entry.debtor == authority
                && entry.kind == ObligationKind::Grievance
                && heaviest(worst_grievance)
            {
                worst_grievance = Some((entry.creditor, entry.weight));
            }
        }
        if let Some((debtor, weight)) = best_favour
            && let Some(assignment) = assignment_for(
                world,
                AiIntent::Obligation,
                AssignmentTargetKind::Organisation,
            )
        {
            intents.push(ScoredIntent {
                intent: AiIntent::Obligation,
                assignment,
                target: AssignmentTarget::Org(debtor),
                score: i64::from(weight) + 20,
                reason: strings.format(
                    "sim.intent.favour-owed",
                    &[("house", &crate::access::org_name(world, debtor))],
                ),
                subject: Some(LogSubject::Org(debtor)),
                explains: true,
            });
        }
        // A house we have wronged is worth courting before it acts.
        if let Some((aggrieved, weight)) = worst_grievance
            && let Some(assignment) = assignments_for(
                world,
                AiIntent::Standing,
                AssignmentTargetKind::Organisation,
            )
            .into_iter()
            .next()
        {
            intents.push(ScoredIntent {
                intent: AiIntent::Standing,
                assignment,
                target: AssignmentTarget::Org(aggrieved),
                score: i64::from(weight),
                reason: strings.format(
                    "sim.intent.grievance-held",
                    &[("house", &crate::access::org_name(world, aggrieved))],
                ),
                subject: Some(LogSubject::Org(aggrieved)),
                explains: true,
            });
        }
    }

    // ---- Resources run down ----
    if let Some(resources) = resources(world, authority) {
        if resources.wealth < 100
            && let Some(assignment) =
                assignment_for(world, AiIntent::Resources, AssignmentTargetKind::None)
        {
            intents.push(ScoredIntent {
                intent: AiIntent::Resources,
                assignment,
                target: AssignmentTarget::None,
                score: (100 - resources.wealth).max(0) / 2 + 20,
                reason: strings.text("sim.intent.treasury-low").to_owned(),
                subject: Some(LogSubject::Org(authority)),
                explains: true,
            });
        }
        // Political weakness: little standing to spend.
        let legitimacy = crate::economy::effective_legitimacy(world, authority);
        if legitimacy < 50
            && let Some(assignment) =
                assignment_for(world, AiIntent::Standing, AssignmentTargetKind::None)
        {
            intents.push(ScoredIntent {
                intent: AiIntent::Standing,
                assignment,
                target: AssignmentTarget::None,
                score: i64::from(50 - legitimacy) + 15,
                reason: strings.text("sim.intent.standing-thin").to_owned(),
                subject: Some(LogSubject::Org(authority)),
                explains: true,
            });
        }
    }

    // ---- A claim worth pressing ----
    // A paramountcy claim is the house's, but only its head may weigh
    // pressing it: anyone else acting here would commit the house to a
    // crisis they have no standing to declare.
    if crate::access::org_head(world, authority) == Some(actor)
        && let Some(body) = world
            .get_resource::<crate::map::MapIndex>()
            .and_then(|index| index.provinces.values().next().copied())
            .and_then(|entity| world.get::<crate::map::ProvinceRecord>(entity))
            .map(|record| record.body)
        && crate::crisis::dominant_claimant(world, body) == Some(authority)
        && let Some(assignment) = assignment_for(world, AiIntent::Claim, AssignmentTargetKind::None)
    {
        intents.push(ScoredIntent {
            intent: AiIntent::Claim,
            assignment,
            target: AssignmentTarget::None,
            score: 120,
            reason: strings.text("sim.intent.claim-ready").to_owned(),
            subject: Some(LogSubject::Org(authority)),
            explains: true,
        });
    }

    // With nothing pressing, a house still attends to ordinary business.
    for assignment in assignments_for(world, AiIntent::Routine, AssignmentTargetKind::None) {
        intents.push(ScoredIntent {
            intent: AiIntent::Routine,
            assignment,
            target: AssignmentTarget::None,
            score: THRESHOLD + 5,
            reason: strings.text("sim.intent.routine").to_owned(),
            subject: Some(LogSubject::Org(authority)),
            explains: false,
        });
    }

    // Best first; ties broken by assignment key so the order never wobbles.
    intents.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.assignment.cmp(&b.assignment))
    });
    intents
}

/// Picks one intent from the shortlist, weighted by how much each
/// pressure is felt, so the most urgent usually wins without the house
/// becoming wholly predictable.
///
/// Pure but for the stream: consumes exactly one roll when the shortlist
/// is non-empty, which is what keeps the choice testable and the replay
/// stable.
pub fn pick_intent<'a>(
    shortlist: &[&'a ScoredIntent],
    rng: &mut DeterministicRng,
) -> Option<&'a ScoredIntent> {
    let first = shortlist.first()?;
    let total: u64 = shortlist
        .iter()
        .map(|intent| intent.score.max(1) as u64)
        .sum();
    let mut roll = rng.roll(total.max(1));
    for intent in shortlist {
        let weight = intent.score.max(1) as u64;
        if roll < weight {
            return Some(intent);
        }
        roll -= weight;
    }
    Some(first)
}

/// Autonomous characters act on their house's most pressing business.
///
/// The acting unit is the person, not the house: characters are walked
/// in stable ID order, and a character acts here only when they head a
/// non-player house that is still standing — a house cannot take
/// independent action, so its business waits on the one person with the
/// authority to conduct it.
pub fn characters_act(world: &mut World) {
    if world.get_resource::<AssignmentsIndex>().is_none()
        || world.get_resource::<CampaignOver>().is_some()
    {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let player = world.get_resource::<PlayerHouse>().and_then(|p| p.0);

    let actors: Vec<(CharacterId, OrgId)> = crate::access::living_character_ids(world)
        .into_iter()
        .filter_map(|who| crate::access::organisation_of(world, who).map(|org| (who, org)))
        .filter(|(who, org)| {
            Some(*org) != player
                && crate::access::org(world, *org).is_some_and(|r| !r.defunct)
                && crate::access::org_head(world, *org) == Some(*who)
        })
        .collect();

    for (actor, authority) in actors {
        // A head pursuing a plan has already decided what their months
        // are for; the reactive pass leaves them to it.
        if world
            .get_resource::<crate::plans::Plans>()
            .is_some_and(|plans| plans.active.contains_key(&actor))
        {
            continue;
        }
        let month = date.days_since_epoch() as u64 / 30;
        // The stream belongs to the character, not the house: agency
        // moved from houses to their heads in M5.1, and the label moved
        // with it. "agency-choice" (subjects [org, month]) is retired —
        // labels are identities, so it must never be reused.
        let mut rng = crate::access::derived_rng(world, "character-agency", &[actor.raw(), month]);
        // A character does not start something new every month.
        if !rng.check_permille(500) {
            continue;
        }

        let intents = score_intents(world, actor, authority);
        // A heavy enough pressure with an authored plan behind it becomes
        // a campaign instead of a single act.
        if crate::plans::try_adopt(world, actor, authority, &intents) {
            continue;
        }
        let shortlist: Vec<&ScoredIntent> = intents
            .iter()
            .filter(|intent| intent.score >= THRESHOLD)
            .take(SHORTLIST)
            .collect();
        let Some(chosen) = pick_intent(&shortlist, &mut rng).cloned() else {
            continue;
        };

        if validate_start(world, authority, &chosen.assignment, actor, chosen.target).is_ok() {
            start_assignment(world, authority, &chosen.assignment, actor, chosen.target);
            announce(world, authority, &chosen);
        }
    }
}

/// Records why a house acted, so the player can see the reasoning behind
/// a significant move rather than only its result.
fn announce(world: &mut World, org: OrgId, intent: &ScoredIntent) {
    // Acting on a pressure is explained; routine upkeep is left to speak
    // through its own results.
    if !intent.explains {
        return;
    }
    let content = world.resource::<ContentDb>().0.clone();
    let title = content
        .assignments
        .get(&intent.assignment)
        .map(|def| def.title.clone())
        .unwrap_or_default();
    let name = crate::access::org_name(world, org);
    let mut entry = LogEntry::line(
        world.resource::<TextDb>().format(
            "sim.agency.began",
            &[
                ("house", &name),
                ("assignment", &title),
                ("reason", &intent.reason),
            ],
        ),
        LogChannel::Politics,
    )
    .by(Some(org));
    if let Some(subject) = intent.subject {
        entry = entry.about(subject);
    }
    crate::access::log(world, entry);
}

/// Monthly: members of a house with nothing to do find something.
///
/// The leash is deliberate and narrow: a character acts only on
/// assignments that cost the house nothing. Touring a holding, holding
/// court where they stand. Anything that spends wealth, manpower,
/// supplies or influence still waits for the person whose stores they
/// are — a household that quietly drains the treasury while the player
/// is looking elsewhere is worse than one that stands idle.
///
/// Everything else follows the rules already in place: the assignment
/// must pass `validate_start`, so nothing can be started unbidden that
/// the head could not have ordered by hand, and candidates are walked in
/// stable ID order so this replays identically.
///
/// This is the household acting within a house. Heads acting with the
/// house's full authority are [`characters_act`], which is a different
/// level and untouched.
pub fn household_acts(world: &mut World) {
    if world.get_resource::<AssignmentsIndex>().is_none()
        || world.get_resource::<CampaignOver>().is_some()
    {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let content = world.resource::<ContentDb>().0.clone();

    // Assignments anyone may pick up unbidden: no cost, and reachable
    // without a target that has to be chosen for them.
    let free: Vec<ContentKey> = {
        let mut keys: Vec<ContentKey> = content
            .assignments
            .iter()
            .filter(|(_, def)| {
                def.wealth_cost == 0
                    && def.manpower_cost == 0
                    && def.supplies_cost == 0
                    && def.influence_cost == 0
                    && def.target == aeon_data::model::AssignmentTargetKind::None
            })
            .map(|(key, _)| key.clone())
            .collect();
        keys.sort();
        keys
    };
    if free.is_empty() {
        return;
    }

    let orgs: Vec<OrgId> = crate::access::org_ids(world)
        .into_iter()
        .filter(|org| crate::access::org(world, *org).is_some_and(|r| !r.defunct))
        .collect();

    for org in orgs {
        let idle: Vec<crate::ids::CharacterId> = crate::access::living_character_ids(world)
            .into_iter()
            .filter(|who| crate::access::organisation_of(world, *who) == Some(org))
            .filter(|who| {
                matches!(
                    crate::assignments::leader_availability(world, org, *who, date),
                    crate::assignments::LeaderAvailability::Available
                )
            })
            .collect();

        for who in idle {
            for assignment in &free {
                if crate::assignments::validate_start(
                    world,
                    org,
                    assignment,
                    who,
                    crate::assignments::AssignmentTarget::None,
                )
                .is_ok()
                {
                    crate::assignments::start_assignment(
                        world,
                        org,
                        assignment,
                        who,
                        crate::assignments::AssignmentTarget::None,
                    );
                    let name = crate::access::character_name(world, who);
                    let title = content
                        .assignments
                        .get(assignment)
                        .map(|def| def.title.clone())
                        .unwrap_or_default();
                    let line = world.resource::<TextDb>().format(
                        "sim.agency.took-it-up",
                        &[("character", &name), ("assignment", &title)],
                    );
                    crate::access::log(
                        world,
                        LogEntry {
                            date,
                            text: line,
                            org: Some(org),
                            subject: Some(LogSubject::Character(who)),
                            channel: LogChannel::Assignments,
                        },
                    );
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(assignment: &str, score: i64) -> ScoredIntent {
        ScoredIntent {
            intent: AiIntent::Routine,
            assignment: ContentKey::new(assignment).expect("kebab-case"),
            target: AssignmentTarget::None,
            score,
            reason: String::new(),
            subject: None,
            explains: false,
        }
    }

    #[test]
    fn an_empty_shortlist_picks_nothing_and_rolls_nothing() {
        let mut rng = DeterministicRng::from_seed(1);
        let before = rng.clone();
        assert!(pick_intent(&[], &mut rng).is_none());
        assert_eq!(rng, before, "an empty shortlist must not consume a roll");
    }

    #[test]
    fn every_pick_comes_from_the_shortlist() {
        let a = intent("hold-court", 80);
        let b = intent("mend-fences", 40);
        let shortlist = [&a, &b];
        for seed in 0..200 {
            let mut rng = DeterministicRng::from_seed(seed);
            let picked = pick_intent(&shortlist, &mut rng).expect("non-empty");
            assert!(shortlist.iter().any(|i| i.assignment == picked.assignment));
        }
    }

    #[test]
    fn heavier_pressures_win_more_often() {
        let heavy = intent("answer-the-crisis", 90);
        let light = intent("ordinary-business", 30);
        let shortlist = [&heavy, &light];
        let mut heavy_picks = 0;
        for seed in 0..1000 {
            let mut rng = DeterministicRng::from_seed(seed);
            if pick_intent(&shortlist, &mut rng)
                .expect("non-empty")
                .assignment
                == heavy.assignment
            {
                heavy_picks += 1;
            }
        }
        // 90:30 weighting: expect roughly three quarters, and certainly
        // a clear majority.
        assert!(
            (650..=850).contains(&heavy_picks),
            "heavy pressure picked {heavy_picks} of 1000"
        );
    }
}
