//! Pure, simulation-owned assignment forecasting.
//!
//! Everything the player is told about a assignment before committing to it is
//! derived here, by the same code that later resolves it: [`result_weights`]
//! is the single source of truth for outcome weighting, and
//! [`assignment_duration_days`] for how long a assignment takes. A forecast therefore
//! cannot drift from the simulation that fulfils it.
//!
//! Forecasts report the distribution *at order time*. Where an outcome is
//! settled by a later contest — a military operation resolved in the field
//! — that is reported separately rather than folded into a single number
//! the model does not actually possess.

use aeon_data::ContentKey;
use aeon_data::model::{AssignmentDef, GoverningSkill, MilitaryOp, OutcomeKind, RiskTag};
use bevy::prelude::*;

use crate::assignments::{AssignmentRejection, AssignmentTarget};
use crate::politics::{CharacterSkills, PoliticsIndex};
use crate::{CharacterId, OrgId};

/// A probability in parts per thousand.
pub type Permille = u32;

/// One possible result of a assignment with its exact current chance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForecastResult {
    /// Which graded outcome this is.
    pub kind: OutcomeKind,
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

/// Everything known about a assignment before it is ordered.
#[derive(Clone, Debug)]
pub struct AssignmentForecast {
    /// The assignment definition forecast.
    pub assignment: ContentKey,
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
    /// The skill that governs this assignment.
    pub skill: GoverningSkill,
    /// The leader's value in that skill.
    pub skill_value: i32,
    /// The assignment's authored difficulty.
    pub difficulty: i32,
    /// Skill minus difficulty; drives the weight shift.
    pub effectiveness: i32,
    /// Every possible outcome, most favourable first; chances sum to 1000.
    pub results: Vec<ForecastResult>,
    /// Personal risks run on a bad outcome.
    pub risks: Vec<ForecastRisk>,
    /// A conditional field contest settled *after* a successful roll, if any.
    pub military_op: Option<MilitaryOp>,
    /// Why this assignment cannot be started now, if it cannot.
    pub blocked: Option<AssignmentRejection>,
}

impl AssignmentForecast {
    /// The combined chance of a success or better.
    pub fn success_chance(&self) -> Permille {
        self.results
            .iter()
            .filter(|r| matches!(r.kind, OutcomeKind::Success | OutcomeKind::CriticalSuccess))
            .map(|r| r.chance)
            .sum()
    }

    /// Whether the assignment can be ordered as forecast.
    pub fn startable(&self) -> bool {
        self.blocked.is_none()
    }
}

/// Shifts a result weight by skill-versus-difficulty effectiveness.
///
/// Positive outcomes scale up with effectiveness, negative outcomes scale
/// down, both floored at a fifth of the authored weight.
pub fn shifted_weight(base: u32, kind: OutcomeKind, effectiveness: i32) -> u64 {
    let swing = 40i64 * i64::from(effectiveness);
    let factor = match kind {
        OutcomeKind::CriticalSuccess | OutcomeKind::Success => 1000 + swing,
        OutcomeKind::Failure | OutcomeKind::Disaster => 1000 - swing,
    }
    .max(200);
    (u64::from(base) * factor as u64) / 1000
}

/// The weighted outcome table for a assignment at a given effectiveness.
///
/// This is the single source of truth: assignment resolution rolls against exactly
/// this table, so a forecast can never disagree with the outcome.
pub fn result_weights(def: &AssignmentDef, effectiveness: i32) -> Vec<(OutcomeKind, u64)> {
    OutcomeKind::ALL
        .iter()
        .filter_map(|kind| {
            def.results
                .get(kind)
                .map(|r| (*kind, shifted_weight(r.weight, *kind, effectiveness)))
        })
        .collect()
}

/// Draws one outcome from the same weighted table the forecast reports.
///
/// This is the only sampler assignment resolution has: it consumes exactly one
/// roll against [`result_weights`], so the odds shown to the player are
/// the odds the roll obeys — by construction, not by two call sites
/// agreeing to stay in step.
pub fn resolve_outcome(
    def: &AssignmentDef,
    effectiveness: i32,
    rng: &mut aeon_core::rng::DeterministicRng,
) -> OutcomeKind {
    let weights = result_weights(def, effectiveness);
    let total: u64 = weights.iter().map(|(_, w)| w).sum();
    let mut roll = rng.roll(total.max(1));
    let mut outcome = weights
        .last()
        .map(|(k, _)| *k)
        .unwrap_or(OutcomeKind::Failure);
    for (kind, weight) in &weights {
        if roll < *weight {
            outcome = *kind;
            break;
        }
        roll -= *weight;
    }
    outcome
}

/// Converts the weight table to permille chances that sum to exactly 1000.
///
/// Uses largest-remainder apportionment so the reported chances are exact
/// integers with no drift, and remains pure integer arithmetic.
pub fn result_odds(def: &AssignmentDef, effectiveness: i32) -> Vec<(OutcomeKind, Permille)> {
    let weights = result_weights(def, effectiveness);
    let total: u64 = weights.iter().map(|(_, w)| *w).sum();
    if total == 0 {
        return weights.into_iter().map(|(kind, _)| (kind, 0)).collect();
    }

    // Floor each share, then hand out the remaining units to the largest
    // remainders (ties broken by the fixed OutcomeKind order).
    let mut shares: Vec<(OutcomeKind, u64, u64)> = weights
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

/// The leader's effectiveness on a assignment: governing skill minus difficulty.
pub fn effectiveness(world: &World, leader: CharacterId, def: &AssignmentDef) -> i32 {
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

/// How long a assignment will take, including a march's travel time.
///
/// Shared by [`crate::assignments::start_assignment`] and the forecast so the quoted
/// duration is the duration actually used.
pub fn assignment_duration_days(
    world: &World,
    def: &AssignmentDef,
    target: AssignmentTarget,
) -> i64 {
    let base = i64::from(def.duration_days);
    // Marches take at least the army's round travel time to the objective.
    let march_days = match target {
        AssignmentTarget::ArmyToProvince(army, destination) => world
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

/// Builds the full forecast for a prospective assignment.
///
/// Returns `None` only when the assignment definition is unknown; an otherwise
/// illegal start is reported through [`AssignmentForecast::blocked`] so the player
/// still sees what the assignment would cost and do.
pub fn forecast(
    world: &World,
    org: OrgId,
    def_key: &ContentKey,
    leader: CharacterId,
    target: AssignmentTarget,
) -> Option<AssignmentForecast> {
    let content = world.get_resource::<crate::state::ContentDb>()?;
    let def = content.0.assignments.get(def_key)?.clone();

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

    Some(AssignmentForecast {
        assignment: def_key.clone(),
        title: def.title.clone(),
        summary: def.summary.clone(),
        leader,
        duration_days: assignment_duration_days(world, &def, target),
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
        blocked: crate::assignments::validate_start(world, org, def_key, leader, target).err(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeon_data::model::{AssignmentCategory, AssignmentTargetKind, OutcomeDef};
    use std::collections::BTreeMap;

    fn def_with(weights: &[(OutcomeKind, u32)]) -> AssignmentDef {
        let mut results = BTreeMap::new();
        for (kind, weight) in weights {
            results.insert(
                *kind,
                OutcomeDef {
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
        AssignmentDef {
            key: ContentKey::new("test-assignment").expect("kebab-case"),
            title: "Test Assignment".to_owned(),
            summary: String::new(),
            category: AssignmentCategory::Routine,
            duration_days: 30,
            skill: GoverningSkill::Stewardship,
            difficulty: 0,
            target: AssignmentTargetKind::None,
            risks: Vec::new(),
            military_op: None,
            ai_available: false,
            ai_intent: aeon_data::model::AiIntent::Routine,
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
            (OutcomeKind::CriticalSuccess, 1),
            (OutcomeKind::Success, 7),
            (OutcomeKind::Failure, 3),
            (OutcomeKind::Disaster, 2),
        ]);
        for effectiveness in -12..=12 {
            let odds = result_odds(&def, effectiveness);
            let total: u32 = odds.iter().map(|(_, p)| *p).sum();
            assert_eq!(total, 1000, "effectiveness {effectiveness} did not sum");
        }
    }

    #[test]
    fn skill_advantage_moves_probability_toward_success() {
        let def = def_with(&[(OutcomeKind::Success, 5), (OutcomeKind::Failure, 5)]);
        let success = |eff: i32| -> Permille {
            result_odds(&def, eff)
                .into_iter()
                .find(|(k, _)| *k == OutcomeKind::Success)
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
        let def = def_with(&[(OutcomeKind::Success, 5), (OutcomeKind::Failure, 5)]);
        for effectiveness in [-100, 100] {
            for (_, chance) in result_odds(&def, effectiveness) {
                assert!(chance > 0, "a tail vanished at {effectiveness}");
            }
        }
    }

    #[test]
    fn the_sampler_obeys_the_odds_the_forecast_reports() {
        // The invariant the whole forecast system promises: the roll uses
        // exactly the distribution the player was shown. Sample the real
        // sampler across many derived streams and compare the empirical
        // frequencies against result_odds.
        let def = def_with(&[
            (OutcomeKind::CriticalSuccess, 1),
            (OutcomeKind::Success, 7),
            (OutcomeKind::Failure, 3),
            (OutcomeKind::Disaster, 2),
        ]);
        for effectiveness in [-6, 0, 6] {
            let odds = result_odds(&def, effectiveness);
            let mut counts: BTreeMap<OutcomeKind, u64> = BTreeMap::new();
            const SAMPLES: u64 = 100_000;
            for seed in 0..SAMPLES {
                let mut rng =
                    aeon_core::rng::DeterministicRng::derive(seed, "sampler-test", &[seed]);
                *counts
                    .entry(resolve_outcome(&def, effectiveness, &mut rng))
                    .or_default() += 1;
            }
            for (kind, permille) in odds {
                let observed = counts.get(&kind).copied().unwrap_or(0) * 1000 / SAMPLES;
                let drift = observed.abs_diff(u64::from(permille));
                assert!(
                    drift <= 10,
                    "at effectiveness {effectiveness}, {kind:?} was forecast \
                     {permille}permille but sampled {observed}permille"
                );
            }
        }
    }

    #[test]
    fn odds_are_stable_for_the_same_inputs() {
        let def = def_with(&[
            (OutcomeKind::CriticalSuccess, 2),
            (OutcomeKind::Success, 6),
            (OutcomeKind::Failure, 4),
        ]);
        assert_eq!(result_odds(&def, 3), result_odds(&def, 3));
    }
}
