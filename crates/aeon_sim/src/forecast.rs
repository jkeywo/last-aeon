//! Pure, simulation-owned job forecasting.
//!
//! Everything the player is told about a job before committing to it is
//! derived here, by the same code that later resolves it: [`result_weights`]
//! is the single source of truth for outcome weighting, and
//! [`job_duration_days`] for how long a job takes. A forecast therefore
//! cannot drift from the simulation that fulfils it.
//!
//! Forecasts report the distribution *at order time*. Where an outcome is
//! settled by a later contest — a military operation resolved in the field
//! — that is reported separately rather than folded into a single number
//! the model does not actually possess.

use aeon_data::ContentKey;
use aeon_data::model::{GoverningSkill, JobDef, JobResultKind, MilitaryOp, RiskTag};
use bevy::prelude::*;

use crate::jobs::{JobRejection, JobTarget};
use crate::politics::{CharacterSkills, PoliticsIndex};
use crate::{CharacterId, OrgId};

/// A probability in parts per thousand.
pub type Permille = u32;

/// One possible result of a job with its exact current chance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForecastResult {
    /// Which graded outcome this is.
    pub kind: JobResultKind,
    /// Chance this outcome is rolled, in permille.
    pub chance: Permille,
    /// The authored description of what this outcome does, if any.
    pub text: Option<String>,
    /// Whether this outcome interrupts with a popup.
    pub popup: bool,
}

/// A personal risk the leader runs, and how likely it is to bite.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ForecastRisk {
    /// Which consequence is risked.
    pub tag: RiskTag,
    /// Chance per failure, in permille.
    pub on_failure: Permille,
    /// Chance per disaster, in permille.
    pub on_disaster: Permille,
}

/// Everything known about a job before it is ordered.
#[derive(Clone, Debug)]
pub struct JobForecast {
    /// The job definition forecast.
    pub job: ContentKey,
    /// Display title.
    pub title: String,
    /// Authored one-line summary.
    pub summary: String,
    /// The leader this forecast is for.
    pub leader: CharacterId,
    /// Days from starting to resolution.
    pub duration_days: i64,
    /// Days before the order even reaches the leader.
    pub order_delay_days: i64,
    /// Immediate cost in wealth.
    pub wealth_cost: i64,
    /// Immediate cost in manpower.
    pub manpower_cost: i64,
    /// Immediate cost in supplies.
    pub supplies_cost: i64,
    /// Immediate cost in influence.
    pub influence_cost: i64,
    /// The skill that governs this job.
    pub skill: GoverningSkill,
    /// The leader's value in that skill.
    pub skill_value: i32,
    /// The job's authored difficulty.
    pub difficulty: i32,
    /// Skill minus difficulty; drives the weight shift.
    pub effectiveness: i32,
    /// Every possible outcome, most favourable first; chances sum to 1000.
    pub results: Vec<ForecastResult>,
    /// Personal risks run on a bad outcome.
    pub risks: Vec<ForecastRisk>,
    /// A conditional field contest settled *after* a successful roll, if any.
    pub military_op: Option<MilitaryOp>,
    /// Why this job cannot be started now, if it cannot.
    pub blocked: Option<JobRejection>,
}

impl JobForecast {
    /// The combined chance of a success or better.
    pub fn success_chance(&self) -> Permille {
        self.results
            .iter()
            .filter(|r| {
                matches!(
                    r.kind,
                    JobResultKind::Success | JobResultKind::CriticalSuccess
                )
            })
            .map(|r| r.chance)
            .sum()
    }

    /// Whether the job can be ordered as forecast.
    pub fn startable(&self) -> bool {
        self.blocked.is_none()
    }
}

/// Shifts a result weight by skill-versus-difficulty effectiveness.
///
/// Positive outcomes scale up with effectiveness, negative outcomes scale
/// down, both floored at a fifth of the authored weight.
pub fn shifted_weight(base: u32, kind: JobResultKind, effectiveness: i32) -> u64 {
    let swing = 40i64 * i64::from(effectiveness);
    let factor = match kind {
        JobResultKind::CriticalSuccess | JobResultKind::Success => 1000 + swing,
        JobResultKind::Failure | JobResultKind::Disaster => 1000 - swing,
    }
    .max(200);
    (u64::from(base) * factor as u64) / 1000
}

/// The weighted outcome table for a job at a given effectiveness.
///
/// This is the single source of truth: job resolution rolls against exactly
/// this table, so a forecast can never disagree with the outcome.
pub fn result_weights(def: &JobDef, effectiveness: i32) -> Vec<(JobResultKind, u64)> {
    JobResultKind::ALL
        .iter()
        .filter_map(|kind| {
            def.results
                .get(kind)
                .map(|r| (*kind, shifted_weight(r.weight, *kind, effectiveness)))
        })
        .collect()
}

/// Converts the weight table to permille chances that sum to exactly 1000.
///
/// Uses largest-remainder apportionment so the reported chances are exact
/// integers with no drift, and remains pure integer arithmetic.
pub fn result_odds(def: &JobDef, effectiveness: i32) -> Vec<(JobResultKind, Permille)> {
    let weights = result_weights(def, effectiveness);
    let total: u64 = weights.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return weights.into_iter().map(|(kind, _)| (kind, 0)).collect();
    }

    // Floor each share, then hand out the remaining units to the largest
    // remainders (ties broken by the fixed JobResultKind order).
    let mut shares: Vec<(JobResultKind, u64, u64)> = weights
        .iter()
        .map(|(kind, weight)| {
            let scaled = weight * 1000;
            (*kind, scaled / total, scaled % total)
        })
        .collect();
    let assigned: u64 = shares.iter().map(|(_, floor, _)| *floor).sum();
    let mut remaining = 1000u64.saturating_sub(assigned);

    let mut order: Vec<usize> = (0..shares.len()).collect();
    order.sort_by(|a, b| {
        shares[*b]
            .2
            .cmp(&shares[*a].2)
            .then_with(|| shares[*a].0.cmp(&shares[*b].0))
    });
    for index in order {
        if remaining == 0 {
            break;
        }
        shares[index].1 += 1;
        remaining -= 1;
    }

    shares
        .into_iter()
        .map(|(kind, share, _)| (kind, share as Permille))
        .collect()
}

/// The leader's effectiveness on a job: governing skill minus difficulty.
pub fn effectiveness(world: &World, leader: CharacterId, def: &JobDef) -> i32 {
    governing_skill(world, leader, def.skill) - def.difficulty
}

/// The leader's value in a governing skill; zero when unknown.
pub fn governing_skill(world: &World, leader: CharacterId, skill: GoverningSkill) -> i32 {
    let Some(index) = world.get_resource::<PoliticsIndex>() else {
        return 0;
    };
    let Some(entity) = index.characters.get(&leader) else {
        return 0;
    };
    let skills = world
        .get::<CharacterSkills>(*entity)
        .map(|s| s.0)
        .unwrap_or_default();
    match skill {
        GoverningSkill::Command => skills.command,
        GoverningSkill::Diplomacy => skills.diplomacy,
        GoverningSkill::Intrigue => skills.intrigue,
        GoverningSkill::Stewardship => skills.stewardship,
    }
}

/// How long a job will take, including a march's travel time.
///
/// Shared by [`crate::jobs::start_job`] and the forecast so the quoted
/// duration is the duration actually used.
pub fn job_duration_days(world: &World, def: &JobDef, target: JobTarget) -> i64 {
    let base = i64::from(def.duration_days);
    // Marches take at least the army's round travel time to the objective.
    let march_days = match target {
        JobTarget::ArmyToProvince(army, destination) => world
            .get_resource::<crate::forces::ForcesIndex>()
            .and_then(|forces| forces.armies.get(&army).copied())
            .and_then(|entity| world.get::<crate::forces::ArmyRecord>(entity))
            .map(|record| crate::presence::travel_days(world, record.location, destination) * 2),
        _ => None,
    };
    march_days.map_or(base, |march| base.max(march))
}

/// The chance a personal risk lands, given a bad outcome.
pub fn risk_permille(tag: RiskTag, disaster: bool) -> Permille {
    let base = match tag {
        RiskTag::Injury => 80,
        RiskTag::Capture => 40,
        RiskTag::Scandal => 80,
        RiskTag::Incapacity => 50,
        RiskTag::Death => 25,
    };
    if disaster { base * 2 } else { base }
}

/// Builds the full forecast for a prospective job.
///
/// Returns `None` only when the job definition is unknown; an otherwise
/// illegal start is reported through [`JobForecast::blocked`] so the player
/// still sees what the job would cost and do.
pub fn forecast(
    world: &World,
    org: OrgId,
    def_key: &ContentKey,
    leader: CharacterId,
    target: JobTarget,
) -> Option<JobForecast> {
    let content = world.get_resource::<crate::state::ContentDb>()?;
    let def = content.0.jobs.get(def_key)?.clone();

    let effectiveness = effectiveness(world, leader, &def);
    let odds = result_odds(&def, effectiveness);
    let results = odds
        .into_iter()
        .map(|(kind, chance)| {
            let authored = def.results.get(&kind);
            ForecastResult {
                kind,
                chance,
                text: authored.and_then(|r| r.log_text.clone().or_else(|| r.popup_text.clone())),
                popup: authored.is_some_and(|r| r.popup),
            }
        })
        .collect();

    let risks = def
        .risks
        .iter()
        .map(|tag| ForecastRisk {
            tag: *tag,
            on_failure: risk_permille(*tag, false),
            on_disaster: risk_permille(*tag, true),
        })
        .collect();

    Some(JobForecast {
        job: def_key.clone(),
        title: def.title.clone(),
        summary: def.summary.clone(),
        leader,
        duration_days: job_duration_days(world, &def, target),
        order_delay_days: crate::presence::order_delay(world, Some(leader)),
        wealth_cost: def.wealth_cost,
        manpower_cost: def.manpower_cost,
        supplies_cost: def.supplies_cost,
        influence_cost: def.influence_cost,
        skill: def.skill,
        skill_value: governing_skill(world, leader, def.skill),
        difficulty: def.difficulty,
        effectiveness,
        results,
        risks,
        military_op: def.military_op,
        blocked: crate::jobs::validate_start(world, org, def_key, leader, target).err(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeon_data::model::{JobCategory, JobResultDef, JobTargetKind};
    use std::collections::BTreeMap;

    fn def_with(weights: &[(JobResultKind, u32)]) -> JobDef {
        let mut results = BTreeMap::new();
        for (kind, weight) in weights {
            results.insert(
                *kind,
                JobResultDef {
                    weight: *weight,
                    popup: false,
                    popup_text: None,
                    choices: Vec::new(),
                    log: false,
                    log_text: None,
                    effect_fn: None,
                },
            );
        }
        JobDef {
            key: ContentKey::new("test-job").expect("kebab-case"),
            title: "Test Job".to_owned(),
            summary: String::new(),
            category: JobCategory::Routine,
            duration_days: 30,
            skill: GoverningSkill::Stewardship,
            difficulty: 0,
            target: JobTargetKind::None,
            risks: Vec::new(),
            military_op: None,
            ai_available: false,
            wealth_cost: 0,
            manpower_cost: 0,
            supplies_cost: 0,
            influence_cost: 0,
            results,
        }
    }

    #[test]
    fn odds_always_sum_to_one_thousand() {
        let def = def_with(&[
            (JobResultKind::CriticalSuccess, 1),
            (JobResultKind::Success, 7),
            (JobResultKind::Failure, 3),
            (JobResultKind::Disaster, 2),
        ]);
        for effectiveness in -12..=12 {
            let odds = result_odds(&def, effectiveness);
            let total: u32 = odds.iter().map(|(_, p)| *p).sum();
            assert_eq!(total, 1000, "effectiveness {effectiveness} did not sum");
        }
    }

    #[test]
    fn skill_advantage_moves_probability_toward_success() {
        let def = def_with(&[(JobResultKind::Success, 5), (JobResultKind::Failure, 5)]);
        let success = |eff: i32| -> Permille {
            result_odds(&def, eff)
                .into_iter()
                .find(|(k, _)| *k == JobResultKind::Success)
                .map(|(_, p)| p)
                .unwrap()
        };
        assert!(success(-5) < success(0), "penalty should reduce success");
        assert!(success(0) < success(5), "advantage should raise success");
        assert_eq!(success(0), 500, "even odds at parity");
    }

    #[test]
    fn weights_floor_rather_than_vanish() {
        // A hopeless leader still has a floor chance, and an expert still
        // runs some risk: neither tail collapses to zero.
        let def = def_with(&[(JobResultKind::Success, 5), (JobResultKind::Failure, 5)]);
        for effectiveness in [-100, 100] {
            for (_, chance) in result_odds(&def, effectiveness) {
                assert!(chance > 0, "a tail vanished at {effectiveness}");
            }
        }
    }

    #[test]
    fn odds_are_stable_for_the_same_inputs() {
        let def = def_with(&[
            (JobResultKind::CriticalSuccess, 2),
            (JobResultKind::Success, 6),
            (JobResultKind::Failure, 4),
        ]);
        assert_eq!(result_odds(&def, 3), result_odds(&def, 3));
    }
}
