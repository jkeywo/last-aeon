//! Intent-driven organisation agency.
//!
//! Autonomous houses choose from the same job catalogue the player uses.
//! Rather than picking at random, each house scores a small set of
//! concrete pressures it is actually under — holdings slipping or under
//! occupation, political weakness, obligations outstanding, resources
//! run down, and a claim worth pressing — and acts on one of the best.
//!
//! This is a priority system, not a separate AI action model: every
//! choice still goes through [`crate::jobs::validate_start`] and
//! [`crate::jobs::start_job`], so an autonomous house can do nothing the
//! player could not, and is bound by the same eligibility rules.
//!
//! Scoring is integer arithmetic over state the player can also see, and
//! candidates are built in stable ID order, so agency replays identically.
//! When a house acts on a pressure it says so in the log, which is how the
//! player learns why — without exposing the roll that chose between
//! near-equal options as though it were a certainty.

use aeon_core::rng::DeterministicRng;
use aeon_data::ContentKey;
use aeon_data::model::{AiIntent, JobTargetKind};
use bevy::prelude::World;

use crate::clock::CampaignClock;
use crate::economy::OrgResources;
use crate::ids::{CharacterId, OrgId, ProvinceId};
use crate::jobs::{
    JobTarget, JobsIndex, LogChannel, LogEntry, LogSubject, MessageLog, start_job, validate_start,
};
use crate::obligations::{ObligationKind, Obligations};
use crate::order::{ORDER_START, held_provinces, province_order};
use crate::politics::{CampaignOver, OrgRecord, PlayerHouse, PoliticsIndex};
use crate::state::{CampaignSeed, ContentDb};

/// How many of the best-scoring intents a house will consider.
const SHORTLIST: usize = 3;
/// A score below which a pressure is not worth acting on.
const THRESHOLD: i64 = 20;

/// One thing a house might do, and why.
#[derive(Clone, Debug)]
pub struct ScoredIntent {
    /// The job it would start.
    pub job: ContentKey,
    /// What that job would act on.
    pub target: JobTarget,
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

/// The AI-available jobs that answer a pressure, in content-key order.
///
/// The mapping from pressure to job is authored on the job itself, so the
/// simulation never names a piece of content: a new job that declares an
/// intent joins the AI's repertoire with no engine change.
fn jobs_for(world: &World, intent: AiIntent, expected: JobTargetKind) -> Vec<ContentKey> {
    let Some(content) = world.get_resource::<ContentDb>() else {
        return Vec::new();
    };
    content
        .0
        .jobs
        .values()
        .filter(|def| def.ai_available && def.ai_intent == intent && def.target == expected)
        .map(|def| def.key.clone())
        .collect()
}

/// The first job answering a pressure, if content offers one.
fn job_for(world: &World, intent: AiIntent, expected: JobTargetKind) -> Option<ContentKey> {
    jobs_for(world, intent, expected).into_iter().next()
}

/// A house's resource position.
fn resources(world: &World, org: OrgId) -> Option<OrgResources> {
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.orgs.get(&org).copied())
        .and_then(|entity| world.get::<OrgResources>(entity))
        .copied()
}

/// The head of an organisation, if it has a living one.
fn head_of(world: &World, org: OrgId) -> Option<CharacterId> {
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.orgs.get(&org).copied())
        .and_then(|entity| world.get::<OrgRecord>(entity))
        .and_then(|record| record.head)
}

/// Scores every pressure a house is under, best first.
///
/// Deterministic throughout: holdings and organisations are visited in
/// stable ID order, and every score is integer arithmetic over visible
/// state.
pub fn score_intents(world: &World, org: OrgId) -> Vec<ScoredIntent> {
    let mut intents: Vec<ScoredIntent> = Vec::new();

    // ---- Holdings that are slipping, or that someone is standing on ----
    let holdings = held_provinces(world, org);
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
        let name = crate::order::province_display_name(world, province);
        // Troops answer an occupation; administration answers disorder.
        if occupied && let Some(job) = job_for(world, AiIntent::Muster, JobTargetKind::None) {
            intents.push(ScoredIntent {
                job,
                target: JobTarget::None,
                score: score + 10,
                reason: format!("{name} is occupied"),
                subject: Some(LogSubject::Province(province)),
                explains: true,
            });
        }
        if let Some(job) = job_for(world, AiIntent::Order, JobTargetKind::None) {
            intents.push(ScoredIntent {
                job,
                target: JobTarget::None,
                score,
                reason: format!("{name} is restive"),
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
            if entry.creditor == org
                && entry.kind == ObligationKind::Favour
                && heaviest(best_favour)
            {
                best_favour = Some((entry.debtor, entry.weight));
            } else if entry.debtor == org
                && entry.kind == ObligationKind::Grievance
                && heaviest(worst_grievance)
            {
                worst_grievance = Some((entry.creditor, entry.weight));
            }
        }
        if let Some((debtor, weight)) = best_favour
            && let Some(job) = job_for(world, AiIntent::Obligation, JobTargetKind::Organisation)
        {
            intents.push(ScoredIntent {
                job,
                target: JobTarget::Org(debtor),
                score: i64::from(weight) + 20,
                reason: format!(
                    "{} owes us a favour",
                    crate::crisis::org_display_name(world, debtor)
                ),
                subject: Some(LogSubject::Org(debtor)),
                explains: true,
            });
        }
        // A house we have wronged is worth courting before it acts.
        if let Some((aggrieved, weight)) = worst_grievance
            && let Some(job) = jobs_for(world, AiIntent::Standing, JobTargetKind::Organisation)
                .into_iter()
                .next()
        {
            intents.push(ScoredIntent {
                job,
                target: JobTarget::Org(aggrieved),
                score: i64::from(weight),
                reason: format!(
                    "{} holds a grievance against us",
                    crate::crisis::org_display_name(world, aggrieved)
                ),
                subject: Some(LogSubject::Org(aggrieved)),
                explains: true,
            });
        }
    }

    // ---- Resources run down ----
    if let Some(resources) = resources(world, org) {
        if resources.wealth < 100
            && let Some(job) = job_for(world, AiIntent::Resources, JobTargetKind::None)
        {
            intents.push(ScoredIntent {
                job,
                target: JobTarget::None,
                score: (100 - resources.wealth).max(0) / 2 + 20,
                reason: "the treasury is low".to_owned(),
                subject: Some(LogSubject::Org(org)),
                explains: true,
            });
        }
        // Political weakness: little standing to spend.
        let legitimacy = crate::economy::effective_legitimacy(world, org);
        if legitimacy < 50
            && let Some(job) = job_for(world, AiIntent::Standing, JobTargetKind::None)
        {
            intents.push(ScoredIntent {
                job,
                target: JobTarget::None,
                score: i64::from(50 - legitimacy) + 15,
                reason: "our standing is thin".to_owned(),
                subject: Some(LogSubject::Org(org)),
                explains: true,
            });
        }
    }

    // ---- A claim worth pressing ----
    if let Some(body) = world
        .get_resource::<crate::map::MapIndex>()
        .and_then(|index| index.provinces.values().next().copied())
        .and_then(|entity| world.get::<crate::map::ProvinceRecord>(entity))
        .map(|record| record.body)
        && crate::crisis::dominant_claimant(world, body) == Some(org)
        && let Some(job) = job_for(world, AiIntent::Claim, JobTargetKind::None)
    {
        intents.push(ScoredIntent {
            job,
            target: JobTarget::None,
            score: 120,
            reason: "we hold more of the planet than any rival".to_owned(),
            subject: Some(LogSubject::Org(org)),
            explains: true,
        });
    }

    // With nothing pressing, a house still attends to ordinary business.
    for job in jobs_for(world, AiIntent::Routine, JobTargetKind::None) {
        intents.push(ScoredIntent {
            job,
            target: JobTarget::None,
            score: THRESHOLD + 5,
            reason: "ordinary business".to_owned(),
            subject: Some(LogSubject::Org(org)),
            explains: false,
        });
    }

    // Best first; ties broken by job key so the order never wobbles.
    intents.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.job.cmp(&b.job)));
    intents
}

/// Autonomous houses act on their most pressing business.
pub fn ai_start_jobs(world: &mut World) {
    if world.get_resource::<JobsIndex>().is_none() || world.get_resource::<CampaignOver>().is_some()
    {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let seed = world.resource::<CampaignSeed>().0;
    let player = world.get_resource::<PlayerHouse>().and_then(|p| p.0);

    let orgs: Vec<OrgId> = {
        let index = world.resource::<PoliticsIndex>();
        index
            .orgs
            .iter()
            .filter(|(org, entity)| {
                Some(**org) != player
                    && world.get::<OrgRecord>(**entity).is_some_and(|r| !r.defunct)
            })
            .map(|(org, _)| *org)
            .collect()
    };

    for org in orgs {
        let Some(head) = head_of(world, org) else {
            continue;
        };
        let month = date.days_since_epoch() as u64 / 30;
        let mut rng = DeterministicRng::derive(seed, "agency-choice", &[org.raw(), month]);
        // A house does not start something new every month.
        if !rng.check_permille(500) {
            continue;
        }

        let intents = score_intents(world, org);
        let shortlist: Vec<&ScoredIntent> = intents
            .iter()
            .filter(|intent| intent.score >= THRESHOLD)
            .take(SHORTLIST)
            .collect();
        if shortlist.is_empty() {
            continue;
        }

        // Weighted by how much each pressure is felt, so the most urgent
        // usually wins without the house becoming wholly predictable.
        let total: u64 = shortlist
            .iter()
            .map(|intent| intent.score.max(1) as u64)
            .sum();
        let mut roll = rng.roll(total.max(1));
        let mut chosen = shortlist[0].clone();
        for intent in &shortlist {
            let weight = intent.score.max(1) as u64;
            if roll < weight {
                chosen = (*intent).clone();
                break;
            }
            roll -= weight;
        }

        if validate_start(world, org, &chosen.job, head, chosen.target).is_ok() {
            start_job(world, org, &chosen.job, head, chosen.target);
            announce(world, org, &chosen);
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
    let date = world.resource::<CampaignClock>().date;
    let content = world.resource::<ContentDb>().0.clone();
    let title = content
        .jobs
        .get(&intent.job)
        .map(|def| def.title.clone())
        .unwrap_or_default();
    let name = crate::crisis::org_display_name(world, org);
    let mut entry = LogEntry::new(
        date,
        format!("{name} began '{title}': {}.", intent.reason),
        LogChannel::Politics,
    )
    .by(Some(org));
    if let Some(subject) = intent.subject {
        entry = entry.about(subject);
    }
    world.resource_mut::<MessageLog>().entries.push(entry);
}
