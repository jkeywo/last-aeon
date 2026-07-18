//! Client-side forecast plumbing.
//!
//! The client never computes odds, costs, or durations itself: an
//! exclusive system asks the simulation for a [`JobForecast`] for whatever
//! job the inspector currently has expanded, plus the same forecast for
//! every leader who could take it on. Panels only render what the
//! simulation reported, so what the player is shown cannot drift from what
//! the simulation will do.

use aeon_core::calendar::GameDate;
use aeon_data::ContentKey;
use aeon_sim::forecast::{JobForecast, Permille, forecast};
use aeon_sim::politics::ADULT_AGE;
use aeon_sim::{
    CampaignClock, CharacterId, CharacterRecord, JobTarget, PlayerHouse, PoliticsIndex,
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
    /// Their value in the job's governing skill.
    pub skill_value: i32,
    /// Their combined chance of success or better, in permille.
    pub success: Permille,
    /// Why they cannot take it on now, if they cannot.
    pub blocked: Option<String>,
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

    let army_led = matches!(
        target,
        Some(JobTarget::OwnArmy(_)) | Some(JobTarget::ArmyToProvince(_, _))
    );
    let mut leaders: Vec<LeaderOption> = Vec::new();
    if let Some(job_target) = target
        && !army_led
    {
        for candidate in candidates {
            let Some(view) = forecast(world, org, &job, candidate, job_target) else {
                continue;
            };
            let name = name_of(world, candidate);
            leaders.push(LeaderOption {
                id: candidate,
                name,
                skill_value: view.skill_value,
                success: view.success_chance(),
                blocked: view.blocked.as_ref().map(|r| r.to_string()),
            });
        }
        // Best prospects first; ties broken by name for a stable order.
        leaders.sort_by(|a, b| {
            a.blocked
                .is_some()
                .cmp(&b.blocked.is_some())
                .then_with(|| b.success.cmp(&a.success))
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
