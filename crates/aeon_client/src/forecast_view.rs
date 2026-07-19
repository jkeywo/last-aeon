//! Client-side forecast plumbing.
//!
//! The client never computes odds, costs, or durations itself: an
//! exclusive system asks the simulation for a [`JobForecast`] for whatever
//! job the inspector currently has expanded, plus the same forecast for
//! every leader who could take it on. Panels only render what the
//! simulation reported, so what the player is shown cannot drift from what
//! the simulation will do.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use aeon_sim::forecast::{JobForecast, Permille, forecast};
use aeon_sim::politics::ADULT_AGE;
use aeon_sim::{
    CampaignClock, CharacterId, CharacterRecord, JobTarget, LeaderAvailability, PlayerHouse,
    PoliticsIndex, leader_availability,
};
use bevy::prelude::*;

use crate::jobs_ui::JobForm;

/// One candidate leader for the expanded job.
#[derive(Clone, Debug)]
pub struct LeaderOption {
    /// The candidate.
    pub id: CharacterId,
    /// Their name.
    pub name: String,
    /// What the simulation says this job would do in their hands.
    ///
    /// The whole forecast is kept, not a summary of it, so the breakdown
    /// shown when comparing candidates is the very object that would
    /// resolve the job — there is no second, simpler calculation that
    /// could drift away from the real one.
    pub forecast: JobForecast,
    /// What they are committed to, in the simulation's own words. Every
    /// candidate carries this, available or not, so the interface can say
    /// where someone is rather than silently omitting them.
    pub availability: LeaderAvailability,
    /// A standing command they hold, if any, for showing beside the name.
    pub assignment: Option<String>,
}

impl LeaderOption {
    /// Their value in the job's governing skill.
    pub fn skill_value(&self) -> i32 {
        self.forecast.skill_value
    }

    /// Their combined chance of success or better, in permille.
    pub fn success(&self) -> Permille {
        self.forecast.success_chance()
    }

    /// Why they cannot take this job on now, if they cannot.
    pub fn blocked(&self) -> Option<String> {
        self.forecast.blocked.as_ref().map(|r| r.to_string())
    }
}

/// What the inspector currently has expanded, and what the simulation says
/// about it. Recomputed only when the choice or the campaign day changes.
#[derive(Resource, Default)]
pub struct ForecastCache {
    key: Option<(ContentKey, Option<CharacterId>, Option<JobTarget>, GameDate)>,
    /// The forecast for the chosen leader, once one is chosen.
    pub forecast: Option<JobForecast>,
    /// Every house member who could lead this job, best chance first.
    pub leaders: Vec<LeaderOption>,
}

/// What every member of the player's house is committed to today.
///
/// Kept as its own resource because the question — "where is this person,
/// and can they act" — is asked in several places that have no job
/// expanded: the inspector's action list, and the character picker. One
/// exclusive system answers it once a day from the simulation, so no part
/// of the interface has to work it out for itself.
#[derive(Resource, Default)]
pub struct AvailabilityView {
    day: Option<GameDate>,
    entries: BTreeMap<CharacterId, LeaderAvailability>,
}

impl AvailabilityView {
    /// What this character is committed to, if they are one of ours.
    pub fn of(&self, character: CharacterId) -> Option<&LeaderAvailability> {
        self.entries.get(&character)
    }
}

/// Refreshes the availability of every house member once a day.
pub fn refresh_availability(world: &mut World) {
    let (Some(date), Some(org)) = (
        world.get_resource::<CampaignClock>().map(|c| c.date),
        world.get_resource::<PlayerHouse>().and_then(|p| p.0),
    ) else {
        return;
    };
    if world
        .get_resource::<AvailabilityView>()
        .is_some_and(|view| view.day == Some(date))
    {
        return;
    }

    let members: Vec<CharacterId> = world
        .get_resource::<PoliticsIndex>()
        .map(|index| {
            index
                .characters
                .iter()
                .filter(|(_, entity)| {
                    world
                        .get::<CharacterRecord>(**entity)
                        .is_some_and(|record| record.alive() && record.organisation == Some(org))
                })
                .map(|(id, _)| *id)
                .collect()
        })
        .unwrap_or_default();

    let entries: BTreeMap<CharacterId, LeaderAvailability> = members
        .into_iter()
        .map(|id| (id, leader_availability(world, org, id, date)))
        .collect();

    let mut view = world.resource_mut::<AvailabilityView>();
    view.day = Some(date);
    view.entries = entries;
}

/// Recomputes the cached forecast when the expanded job, its chosen leader
/// or target, or the campaign day changes.
pub fn refresh_forecast(world: &mut World) {
    let Some(job) = world.get_resource::<JobForm>().and_then(|f| f.job.clone()) else {
        // Nothing expanded; drop any stale forecast.
        if let Some(mut cache) = world.get_resource_mut::<ForecastCache>()
            && cache.key.is_some()
        {
            *cache = ForecastCache::default();
        }
        return;
    };
    let (leader, target) = {
        let form = world.resource::<JobForm>();
        (form.leader, form.target)
    };
    let Some(date) = world.get_resource::<CampaignClock>().map(|c| c.date) else {
        return;
    };
    let Some(org) = world.get_resource::<PlayerHouse>().and_then(|p| p.0) else {
        return;
    };

    let key = (job.clone(), leader, target, date);
    if world
        .get_resource::<ForecastCache>()
        .is_some_and(|cache| cache.key.as_ref() == Some(&key))
    {
        return;
    }

    // Candidate leaders: living adult members of the player's house. The
    // target a candidate would act on is the one already chosen, except
    // for army operations, which are always led by the army's general.
    let candidates: Vec<CharacterId> = {
        let Some(index) = world.get_resource::<PoliticsIndex>() else {
            return;
        };
        index
            .characters
            .iter()
            .filter(|(_, entity)| {
                world
                    .get::<CharacterRecord>(**entity)
                    .is_some_and(|record| {
                        record.alive()
                            && record.organisation == Some(org)
                            && record.age_years(date) >= ADULT_AGE
                    })
            })
            .map(|(id, _)| *id)
            .collect()
    };

    // Every adult of the house is a candidate, including those who cannot
    // act: the interface shows them with the reason rather than leaving
    // the player wondering where half their household went.
    let mut leaders: Vec<LeaderOption> = Vec::new();
    if let Some(job_target) = target {
        for candidate in candidates {
            let Some(view) = forecast(world, org, &job, candidate, job_target) else {
                continue;
            };
            let availability = leader_availability(world, org, candidate, date);
            let assignment = match &availability {
                LeaderAvailability::Assigned(assignment) => Some(assignment.describe()),
                _ => None,
            };
            let name = name_of(world, candidate);
            leaders.push(LeaderOption {
                id: candidate,
                name,
                forecast: view,
                availability,
                assignment,
            });
        }
        // Available first, then best prospects; ties broken by name so the
        // order never wobbles between frames.
        leaders.sort_by(|a, b| {
            a.blocked()
                .is_some()
                .cmp(&b.blocked().is_some())
                .then_with(|| b.success().cmp(&a.success()))
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    let current = match (leader, target) {
        (Some(leader), Some(target)) => forecast(world, org, &job, leader, target),
        _ => None,
    };

    let mut cache = world.resource_mut::<ForecastCache>();
    cache.key = Some(key);
    cache.forecast = current;
    cache.leaders = leaders;
}

fn name_of(world: &World, character: CharacterId) -> String {
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.characters.get(&character).copied())
        .and_then(|entity| world.get::<CharacterRecord>(entity))
        .map(|record| record.name.clone())
        .unwrap_or_default()
}
