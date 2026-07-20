//! Plans: authored multi-step campaigns pursued by autonomous characters.
//!
//! Where the reactive scorer in [`crate::agency`] answers a pressure with
//! a single assignment, a plan answers one with an ordered sequence of
//! them: a head under heavy pressure adopts the authored plan whose goal
//! names it, and pursues it daily until it completes, fails, or outlives
//! its welcome. Every step still starts through
//! [`crate::assignments::validate_start`], so a plan can do nothing its
//! owner could not have ordered by hand.
//!
//! Failure policy is abandonment, deliberately: a step that fails more
//! than its authored retry budget, a leader who dies, an authority that
//! falls, or a plan that runs past `max_days` all end the plan with the
//! reason logged. There is no replanning search — a house that wants the
//! goal again waits out the cooldown and adopts afresh, which the player
//! can see and reason about.
//!
//! Determinism: plan state is one resource of `BTreeMap`s serialised into
//! the snapshot like every other section; adoption, advancement and
//! abandonment are integer decisions over visible state; the only roll is
//! choosing among several eligible methods, on its own derived stream.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use aeon_data::model::{
    AssignmentTargetKind, OutcomeKind, PlanDef, PlanRequires, PlanStepAction, PlanTargetSelector,
};
use bevy::prelude::{Resource, World};
use serde::{Deserialize, Serialize};

use crate::agency::ScoredIntent;
use crate::assignments::{
    AssignmentTarget, AssignmentsIndex, LogChannel, LogEntry, LogSubject, start_assignment,
    validate_start,
};
use crate::clock::CampaignClock;
use crate::ids::{AssignmentId, CharacterId, OrgId};
use crate::politics::CampaignOver;
use crate::state::ContentDb;
use crate::text::TextDb;

/// The pressure score above which a head reaches for a plan rather than
/// a single act. Higher than the one-shot threshold on purpose: a
/// campaign is for occasions, not upkeep.
pub const PLAN_THRESHOLD: i64 = 60;

/// One step of an adopted plan, flattened from the authored method with
/// sub-plans expanded.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStepInstance {
    /// The authored step id, for logs and tests.
    pub id: String,
    /// The assignment this step starts.
    pub assignment: ContentKey,
    /// Where the assignment's target comes from.
    pub target: PlanTargetSelector,
    /// Skip the step when these already hold.
    pub skip_if: Option<PlanRequires>,
}

/// A plan a character is pursuing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivePlan {
    /// The authored plan.
    pub def: ContentKey,
    /// The chosen method's id.
    pub method: String,
    /// The steps, flattened at adoption. Sub-plans are expanded here so
    /// a running plan never needs to consult another definition.
    pub steps: Vec<PlanStepInstance>,
    /// What the plan is aimed at.
    pub target: AssignmentTarget,
    /// Index of the current step.
    pub step: usize,
    /// The day the plan was adopted.
    pub started: GameDate,
    /// The assignment currently running for the current step, if any.
    pub current_assignment: Option<AssignmentId>,
    /// Failures of the current step so far.
    pub retries: u32,
    /// The scored pressure that motivated adoption, in words.
    pub reason: String,
}

/// Every active plan and every cooldown, one resource.
///
/// A resource of `BTreeMap`s rather than components: plans are consulted
/// by stable character ID from several systems, and a `BTreeMap` gives
/// the stable iteration order determinism requires for free.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plans {
    /// Active plans by the character pursuing them.
    pub active: BTreeMap<CharacterId, ActivePlan>,
    /// Days before which a character may not adopt a plan again.
    pub cooldowns: BTreeMap<(CharacterId, ContentKey), GameDate>,
}

/// Whether declarative plan conditions hold for an authority and target.
///
/// Integer facts over visible state, mirroring what the player could
/// check on their own screens; evaluated identically on every replay.
pub fn requires_met(
    world: &World,
    authority: OrgId,
    target: AssignmentTarget,
    req: &PlanRequires,
) -> bool {
    let resources = crate::access::org_entity(world, authority)
        .and_then(|e| world.get::<crate::economy::OrgResources>(e))
        .copied()
        .unwrap_or_default();
    if let Some(need) = req.min_wealth
        && resources.wealth < need
    {
        return false;
    }
    if let Some(need) = req.min_manpower
        && resources.manpower < need
    {
        return false;
    }
    if let Some(need) = req.min_influence
        && resources.influence < need
    {
        return false;
    }
    if let Some(need) = req.min_legitimacy
        && crate::economy::effective_legitimacy(world, authority) < need
    {
        return false;
    }
    if let Some(wanted) = req.has_army {
        let has = world
            .get_resource::<crate::forces::ForcesIndex>()
            .is_some_and(|index| {
                index.armies.values().any(|entity| {
                    world
                        .get::<crate::forces::ArmyRecord>(*entity)
                        .is_some_and(|army| army.owner == authority)
                })
            });
        if has != wanted {
            return false;
        }
    }
    if req.target_owes_favour {
        let AssignmentTarget::Org(debtor) = target else {
            return false;
        };
        let owed = world
            .get_resource::<crate::obligations::Obligations>()
            .is_some_and(|ledger| {
                ledger.open().any(|entry| {
                    entry.kind == crate::obligations::ObligationKind::Favour
                        && entry.debtor == debtor
                        && entry.creditor == authority
                })
            });
        if !owed {
            return false;
        }
    }
    if req.dominant_claimant {
        let dominant = world
            .get_resource::<crate::map::MapIndex>()
            .and_then(|index| index.provinces.values().next().copied())
            .and_then(|entity| world.get::<crate::map::ProvinceRecord>(entity))
            .map(|record| record.body)
            .and_then(|body| crate::crisis::dominant_claimant(world, body));
        if dominant != Some(authority) {
            return false;
        }
    }
    true
}

/// Flattens a plan's chosen method into step instances, expanding
/// sub-plans by their first eligible method.
///
/// Deterministic without a roll: sub-plan method choice is "first whose
/// gate passes", so expansion depends only on visible state. Returns
/// `None` when some sub-plan has no eligible method, which makes the
/// whole candidate ineligible rather than adoptable-but-stuck.
fn flatten_steps(
    world: &World,
    content: &aeon_data::ContentSet,
    authority: OrgId,
    target: AssignmentTarget,
    def: &PlanDef,
    method_index: usize,
) -> Option<Vec<PlanStepInstance>> {
    let mut steps = Vec::new();
    for step in &def.methods[method_index].steps {
        match &step.action {
            PlanStepAction::Assignment {
                key,
                target: selector,
            } => steps.push(PlanStepInstance {
                id: step.id.clone(),
                assignment: key.clone(),
                target: *selector,
                skip_if: step.skip_if.clone(),
            }),
            PlanStepAction::SubPlan(sub_key) => {
                let sub = content.plans.get(sub_key)?;
                let chosen = sub
                    .methods
                    .iter()
                    .position(|m| requires_met(world, authority, target, &m.requires))?;
                let mut inner = flatten_steps(world, content, authority, target, sub, chosen)?;
                steps.append(&mut inner);
            }
        }
    }
    Some(steps)
}

/// Offers a head the chance to adopt a plan for their heaviest pressure.
///
/// Called from the monthly agency pass with the intents already scored.
/// Returns `true` when a plan was adopted, in which case the one-shot
/// path is skipped: the head's attention is the campaign now.
///
/// Candidate plans are walked in content-key order; the only roll is
/// choosing among several eligible methods of the chosen plan, drawn
/// from its own derived stream so the agency stream's consumption does
/// not depend on what plans exist.
pub fn try_adopt(
    world: &mut World,
    actor: CharacterId,
    authority: OrgId,
    intents: &[ScoredIntent],
) -> bool {
    let Some(top) = intents.first() else {
        return false;
    };
    let date = world.resource::<CampaignClock>().date;
    let content = world.resource::<ContentDb>().0.clone();

    // Candidates answer the top pressure's intent, aim at its kind of
    // target, are off cooldown, and clear the plan threshold with their
    // authored bonus counted.
    let mut adopted: Option<(ContentKey, usize, AssignmentTarget)> = None;
    for (key, def) in &content.plans {
        if def.goal != top.intent || top.score + def.score_bonus < PLAN_THRESHOLD {
            continue;
        }
        let target = match (def.target, top.target) {
            (AssignmentTargetKind::None, _) => AssignmentTarget::None,
            (AssignmentTargetKind::Organisation, AssignmentTarget::Org(org)) => {
                AssignmentTarget::Org(org)
            }
            (AssignmentTargetKind::Province, AssignmentTarget::Province(province)) => {
                AssignmentTarget::Province(province)
            }
            _ => continue,
        };
        let off_cooldown = world
            .resource::<Plans>()
            .cooldowns
            .get(&(actor, key.clone()))
            .is_none_or(|until| date >= *until);
        if !off_cooldown {
            continue;
        }
        let eligible: Vec<usize> = def
            .methods
            .iter()
            .enumerate()
            .filter(|(_, m)| requires_met(world, authority, target, &m.requires))
            .map(|(index, _)| index)
            .collect();
        let method_index = match eligible.as_slice() {
            [] => continue,
            [only] => *only,
            several => {
                // One roll, own stream, per character and month.
                let month = date.days_since_epoch() as u64 / 30;
                let mut rng =
                    crate::access::derived_rng(world, "plan-method", &[actor.raw(), month]);
                several[rng.roll(several.len() as u64) as usize]
            }
        };
        if flatten_steps(world, &content, authority, target, def, method_index).is_some() {
            adopted = Some((key.clone(), method_index, target));
            break;
        }
    }
    let Some((key, method_index, target)) = adopted else {
        return false;
    };

    let def = &content.plans[&key];
    let steps = flatten_steps(world, &content, authority, target, def, method_index)
        .expect("checked above");
    let plan = ActivePlan {
        def: key.clone(),
        method: def.methods[method_index].id.clone(),
        steps,
        target,
        step: 0,
        started: date,
        current_assignment: None,
        retries: 0,
        reason: top.reason.clone(),
    };
    world.resource_mut::<Plans>().active.insert(actor, plan);

    let text = world.resource::<TextDb>().format(
        "sim.plan.adopted",
        &[
            ("house", &crate::access::org_name(world, authority)),
            ("plan", &def.title),
            ("reason", &top.reason),
        ],
    );
    let mut entry = LogEntry::line(text, LogChannel::Politics).by(Some(authority));
    if let Some(subject) = top.subject {
        entry = entry.about(subject);
    }
    crate::access::log(world, entry);
    true
}

/// Daily: every active plan advances one decision.
///
/// Walked in stable character-ID order. A plan first checks whether it
/// should still exist — leader gone, authority fallen, target fallen, or
/// out of time — then, if idle, tries to start its current step through
/// the same gate the player's own orders pass. A blocked step simply
/// waits; `max_days` is the one safety valve against waiting forever.
pub fn advance_plans(world: &mut World) {
    if world.get_resource::<AssignmentsIndex>().is_none()
        || world.get_resource::<Plans>().is_none()
        || world.get_resource::<CampaignOver>().is_some()
    {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let content = world.resource::<ContentDb>().0.clone();

    let actors: Vec<CharacterId> = world.resource::<Plans>().active.keys().copied().collect();
    for actor in actors {
        let plan = world.resource::<Plans>().active[&actor].clone();
        let def = &content.plans[&plan.def];

        let leader_alive = crate::access::character(world, actor).is_some_and(|r| r.alive());
        let authority = crate::access::organisation_of(world, actor);
        let authority_stands = authority
            .and_then(|org| crate::access::org(world, org))
            .is_some_and(|r| !r.defunct);
        let target_stands = match plan.target {
            AssignmentTarget::Org(org) => {
                crate::access::org(world, org).is_some_and(|r| !r.defunct)
            }
            _ => true,
        };
        let out_of_time =
            date.days_since_epoch() - plan.started.days_since_epoch() > i64::from(def.max_days);
        if !leader_alive || !authority_stands || !target_stands || out_of_time {
            abandon(world, actor, date);
            continue;
        }
        let authority = authority.expect("standing authority");

        if plan.current_assignment.is_some() {
            continue;
        }

        // Skip already-satisfied steps, then try to start the next one.
        // Loops so a run of satisfied steps clears in one day rather
        // than one per day.
        let mut step = plan.step;
        loop {
            if step >= plan.steps.len() {
                complete(world, actor, date);
                break;
            }
            let instance = plan.steps[step].clone();
            if instance
                .skip_if
                .as_ref()
                .is_some_and(|req| requires_met(world, authority, plan.target, req))
            {
                step += 1;
                world
                    .resource_mut::<Plans>()
                    .active
                    .get_mut(&actor)
                    .expect("present")
                    .step = step;
                continue;
            }
            let target = match instance.target {
                PlanTargetSelector::None => AssignmentTarget::None,
                PlanTargetSelector::PlanTarget => plan.target,
            };
            if validate_start(world, authority, &instance.assignment, actor, target).is_ok() {
                let id = start_assignment(world, authority, &instance.assignment, actor, target);
                world
                    .resource_mut::<Plans>()
                    .active
                    .get_mut(&actor)
                    .expect("present")
                    .current_assignment = Some(id);
            }
            // Blocked: wait. The stall counts against max_days and
            // nothing else; per-step timers would be a second policy.
            break;
        }
    }
}

/// Notes that an assignment a plan was waiting on has resolved.
///
/// Called from assignment resolution, once, for assignments that
/// actually complete. Success moves the plan on; failure spends a retry
/// or ends the plan.
pub fn note_resolution(world: &mut World, id: AssignmentId, outcome: OutcomeKind) {
    let Some(plans) = world.get_resource::<Plans>() else {
        return;
    };
    let Some(actor) = plans
        .active
        .iter()
        .find(|(_, plan)| plan.current_assignment == Some(id))
        .map(|(actor, _)| *actor)
    else {
        return;
    };
    let date = world.resource::<CampaignClock>().date;
    let content = world.resource::<ContentDb>().0.clone();

    enum After {
        Nothing,
        Complete,
        Abandon,
    }
    let after = {
        let mut plans = world.resource_mut::<Plans>();
        let plan = plans.active.get_mut(&actor).expect("found above");
        plan.current_assignment = None;
        match outcome {
            OutcomeKind::Success | OutcomeKind::CriticalSuccess => {
                plan.step += 1;
                plan.retries = 0;
                if plan.step >= plan.steps.len() {
                    After::Complete
                } else {
                    After::Nothing
                }
            }
            OutcomeKind::Failure | OutcomeKind::Disaster => {
                plan.retries += 1;
                let budget = content.plans[&plan.def].max_step_retries;
                if plan.retries > budget {
                    After::Abandon
                } else {
                    After::Nothing
                }
            }
        }
    };
    match after {
        After::Complete => complete(world, actor, date),
        After::Abandon => abandon(world, actor, date),
        After::Nothing => {}
    }
}

/// Notes that an assignment was dropped without an outcome (its leader
/// died mid-work). The plan's own abandon checks take it from here.
pub fn note_dropped(world: &mut World, id: AssignmentId) {
    let Some(mut plans) = world.get_resource_mut::<Plans>() else {
        return;
    };
    for plan in plans.active.values_mut() {
        if plan.current_assignment == Some(id) {
            plan.current_assignment = None;
        }
    }
}

/// Removes a finished plan, starts its cooldown, and says so.
fn complete(world: &mut World, actor: CharacterId, date: GameDate) {
    let Some(plan) = world.resource_mut::<Plans>().active.remove(&actor) else {
        return;
    };
    set_cooldown(world, actor, &plan, date);
    announce_end(world, actor, &plan, "sim.plan.completed");
}

/// Removes a failed plan, starts its cooldown, and says so.
fn abandon(world: &mut World, actor: CharacterId, date: GameDate) {
    let Some(plan) = world.resource_mut::<Plans>().active.remove(&actor) else {
        return;
    };
    set_cooldown(world, actor, &plan, date);
    announce_end(world, actor, &plan, "sim.plan.abandoned");
}

fn set_cooldown(world: &mut World, actor: CharacterId, plan: &ActivePlan, date: GameDate) {
    let cooldown_days = world
        .resource::<ContentDb>()
        .0
        .plans
        .get(&plan.def)
        .map(|def| i64::from(def.cooldown_days))
        .unwrap_or(0);
    if cooldown_days > 0 {
        world
            .resource_mut::<Plans>()
            .cooldowns
            .insert((actor, plan.def.clone()), date.add_days(cooldown_days));
    }
}

fn announce_end(world: &mut World, actor: CharacterId, plan: &ActivePlan, key: &str) {
    let Some(authority) = crate::access::organisation_of(world, actor) else {
        return;
    };
    let title = world
        .resource::<ContentDb>()
        .0
        .plans
        .get(&plan.def)
        .map(|def| def.title.clone())
        .unwrap_or_default();
    let text = world.resource::<TextDb>().format(
        key,
        &[
            ("house", &crate::access::org_name(world, authority)),
            ("plan", &title),
        ],
    );
    crate::access::log(
        world,
        LogEntry::line(text, LogChannel::Politics)
            .by(Some(authority))
            .about(LogSubject::Character(actor)),
    );
}

/// Captures plan state for a snapshot.
pub fn capture(world: &World) -> Plans {
    world.get_resource::<Plans>().cloned().unwrap_or_default()
}

/// Restores plan state from a snapshot.
pub fn restore(world: &mut World, state: &Plans) {
    world.insert_resource(state.clone());
}

/// Installs the daily plan advancement.
pub(crate) fn install(app: &mut bevy::prelude::App) {
    use crate::clock::{DailyTick, TickSet};
    use bevy::prelude::IntoScheduleConfigs;
    // After resolutions so a step's outcome is noted before the next
    // step starts, and before standing orders so an army a plan raises
    // today can be ordered today.
    app.add_systems(
        DailyTick,
        advance_plans
            .in_set(TickSet::Simulation)
            .after(crate::assignments::resolve_due_assignments)
            .before(crate::warfare::standing_orders),
    );
}
