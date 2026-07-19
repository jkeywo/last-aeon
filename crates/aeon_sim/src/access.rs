//! World-read access: one home for the walks every system repeats.
//!
//! Reading anything out of the simulation used to mean the same
//! open-coded three steps — resource index, entity handle, component
//! fetch — plus a private copy of display-name resolution, log-date
//! stamping, and campaign-seed fetching in whichever file needed them.
//! Each copy was a place the determinism rules (stable-ID iteration,
//! derived random streams) could be silently broken. They live here
//! instead, once.
//!
//! Everything here is read-only except [`log`], which appends to the
//! message log. Nothing here decides anything: these are the questions,
//! not the answers.

use aeon_core::rng::DeterministicRng;
use bevy::prelude::{Component, Entity, World};

use crate::clock::CampaignClock;
use crate::forces::{ArmyRecord, ForcesIndex, ShipRecord};
use crate::ids::{
    ArmyId, BodyId, CharacterId, JobId, OfficeId, OrgId, ProvinceId, ShipId, TitleId,
};
use crate::jobs::{ActiveJob, JobsIndex, LogEntry, MessageLog};
use crate::map::{BodyRecord, DisplayName, MapIndex, ProvinceRecord};
use crate::politics::{
    CharacterRecord, OfficeRecord, OrgRecord, PoliticsIndex, TitleHolder, TitleKind, TitleRecord,
};
use crate::state::{CampaignSeed, ContentDb};

// ---------------------------------------------------------------------------
// Entities by stable ID
// ---------------------------------------------------------------------------

/// The live entity behind a character.
pub fn character_entity(world: &World, id: CharacterId) -> Option<Entity> {
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.characters.get(&id).copied())
}

/// The live entity behind an organisation.
pub fn org_entity(world: &World, id: OrgId) -> Option<Entity> {
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.orgs.get(&id).copied())
}

/// The live entity behind a title.
pub fn title_entity(world: &World, id: TitleId) -> Option<Entity> {
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.titles.get(&id).copied())
}

/// The live entity behind an office.
pub fn office_entity(world: &World, id: OfficeId) -> Option<Entity> {
    world
        .get_resource::<PoliticsIndex>()
        .and_then(|index| index.offices.get(&id).copied())
}

/// The live entity behind a province.
pub fn province_entity(world: &World, id: ProvinceId) -> Option<Entity> {
    world
        .get_resource::<MapIndex>()
        .and_then(|index| index.provinces.get(&id).copied())
}

/// The live entity behind a body.
pub fn body_entity(world: &World, id: BodyId) -> Option<Entity> {
    world
        .get_resource::<MapIndex>()
        .and_then(|index| index.bodies.get(&id).copied())
}

/// The live entity behind an army.
pub fn army_entity(world: &World, id: ArmyId) -> Option<Entity> {
    world
        .get_resource::<ForcesIndex>()
        .and_then(|index| index.armies.get(&id).copied())
}

/// The live entity behind a ship.
pub fn ship_entity(world: &World, id: ShipId) -> Option<Entity> {
    world
        .get_resource::<ForcesIndex>()
        .and_then(|index| index.ships.get(&id).copied())
}

/// The live entity behind an active job.
pub fn job_entity(world: &World, id: JobId) -> Option<Entity> {
    world
        .get_resource::<JobsIndex>()
        .and_then(|index| index.jobs.get(&id).copied())
}

// ---------------------------------------------------------------------------
// Records by stable ID
// ---------------------------------------------------------------------------

/// A character's record.
pub fn character(world: &World, id: CharacterId) -> Option<&CharacterRecord> {
    character_entity(world, id).and_then(|entity| world.get::<CharacterRecord>(entity))
}

/// An organisation's record.
pub fn org(world: &World, id: OrgId) -> Option<&OrgRecord> {
    org_entity(world, id).and_then(|entity| world.get::<OrgRecord>(entity))
}

/// A title's record.
pub fn title(world: &World, id: TitleId) -> Option<&TitleRecord> {
    title_entity(world, id).and_then(|entity| world.get::<TitleRecord>(entity))
}

/// An office's record.
pub fn office(world: &World, id: OfficeId) -> Option<&OfficeRecord> {
    office_entity(world, id).and_then(|entity| world.get::<OfficeRecord>(entity))
}

/// A province's record.
pub fn province(world: &World, id: ProvinceId) -> Option<&ProvinceRecord> {
    province_entity(world, id).and_then(|entity| world.get::<ProvinceRecord>(entity))
}

/// A body's record.
pub fn body(world: &World, id: BodyId) -> Option<&BodyRecord> {
    body_entity(world, id).and_then(|entity| world.get::<BodyRecord>(entity))
}

/// An army's record.
pub fn army(world: &World, id: ArmyId) -> Option<&ArmyRecord> {
    army_entity(world, id).and_then(|entity| world.get::<ArmyRecord>(entity))
}

/// A ship's record.
pub fn ship(world: &World, id: ShipId) -> Option<&ShipRecord> {
    ship_entity(world, id).and_then(|entity| world.get::<ShipRecord>(entity))
}

/// An active job's record.
pub fn job(world: &World, id: JobId) -> Option<&ActiveJob> {
    job_entity(world, id).and_then(|entity| world.get::<ActiveJob>(entity))
}

/// Any other component carried by a character (skills, lineage, opinion
/// ledger, condition, location).
pub fn on_character<T: Component>(world: &World, id: CharacterId) -> Option<&T> {
    character_entity(world, id).and_then(|entity| world.get::<T>(entity))
}

// ---------------------------------------------------------------------------
// Derived political facts
// ---------------------------------------------------------------------------

/// The living head of an organisation, if it has one.
pub fn org_head(world: &World, id: OrgId) -> Option<CharacterId> {
    org(world, id).and_then(|record| record.head)
}

/// The organisation a character belongs to.
pub fn organisation_of(world: &World, id: CharacterId) -> Option<OrgId> {
    character(world, id).and_then(|record| record.organisation)
}

/// The character personally holding the Consul title, if anyone does.
pub fn consul(world: &World) -> Option<CharacterId> {
    let index = world.get_resource::<PoliticsIndex>()?;
    index.titles.values().find_map(|entity| {
        let title = world.get::<TitleRecord>(*entity)?;
        match (title.kind, title.holder) {
            (TitleKind::Consul, TitleHolder::Character(holder)) => Some(holder),
            _ => None,
        }
    })
}

/// The Sanctora Imperim, if the scenario has one.
pub fn sanctora_org(world: &World) -> Option<OrgId> {
    let index = world.get_resource::<PoliticsIndex>()?;
    index.orgs.iter().find_map(|(id, entity)| {
        let record = world.get::<OrgRecord>(*entity)?;
        (record.kind == aeon_data::model::OrgKind::SanctoraImperim).then_some(*id)
    })
}

// ---------------------------------------------------------------------------
// Stable-ID-ordered collections
// ---------------------------------------------------------------------------
//
// Systems that mutate while iterating collect IDs first, then re-fetch
// per ID. These helpers give that borrow-release dance one home and keep
// the iteration order stable by construction.

/// Every character ID, in stable order.
pub fn character_ids(world: &World) -> Vec<CharacterId> {
    world
        .get_resource::<PoliticsIndex>()
        .map(|index| index.characters.keys().copied().collect())
        .unwrap_or_default()
}

/// Every living character ID, in stable order.
pub fn living_character_ids(world: &World) -> Vec<CharacterId> {
    world
        .get_resource::<PoliticsIndex>()
        .map(|index| {
            index
                .characters
                .iter()
                .filter(|(_, entity)| {
                    world
                        .get::<CharacterRecord>(**entity)
                        .is_some_and(|record| record.alive())
                })
                .map(|(id, _)| *id)
                .collect()
        })
        .unwrap_or_default()
}

/// Every organisation ID, in stable order.
pub fn org_ids(world: &World) -> Vec<OrgId> {
    world
        .get_resource::<PoliticsIndex>()
        .map(|index| index.orgs.keys().copied().collect())
        .unwrap_or_default()
}

/// Every province ID, in stable order.
pub fn province_ids(world: &World) -> Vec<ProvinceId> {
    world
        .get_resource::<MapIndex>()
        .map(|index| index.provinces.keys().copied().collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Display names
// ---------------------------------------------------------------------------

/// A character's display name, falling back to the raw ID.
pub fn character_name(world: &World, id: CharacterId) -> String {
    character(world, id)
        .map(|record| record.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// An organisation's display name from content, falling back to the raw ID.
pub fn org_name(world: &World, id: OrgId) -> String {
    org(world, id)
        .and_then(|record| {
            world
                .get_resource::<ContentDb>()?
                .0
                .organisations
                .get(&record.key)
                .map(|def| def.name.clone())
        })
        .unwrap_or_else(|| id.to_string())
}

/// A province's display name, falling back to the raw ID.
pub fn province_name(world: &World, id: ProvinceId) -> String {
    province_entity(world, id)
        .and_then(|entity| world.get::<DisplayName>(entity))
        .map(|name| name.0.clone())
        .unwrap_or_else(|| id.to_string())
}

/// An army's display name, falling back to the raw ID.
pub fn army_name(world: &World, id: ArmyId) -> String {
    army(world, id)
        .map(|record| record.name.clone())
        .unwrap_or_else(|| id.to_string())
}

// ---------------------------------------------------------------------------
// The message log and derived streams
// ---------------------------------------------------------------------------

/// Stamps `entry` with today's date and appends it to the message log.
///
/// Pair with [`LogEntry::line`], which builds an entry without a date so
/// the stamping happens exactly once, here.
pub fn log(world: &mut World, entry: LogEntry) {
    let date = world.resource::<CampaignClock>().date;
    let mut entry = entry;
    entry.date = date;
    world.resource_mut::<MessageLog>().entries.push(entry);
}

/// Derives the random stream for one purpose from the campaign seed.
///
/// Same contract as [`DeterministicRng::derive`]: the purpose label and
/// the subject IDs are the stream's identity, so call sites must keep
/// them stable forever.
pub fn derived_rng(world: &World, purpose: &str, subjects: &[u64]) -> DeterministicRng {
    DeterministicRng::derive(world.resource::<CampaignSeed>().0, purpose, subjects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::LogChannel;
    use aeon_core::calendar::GameDate;

    #[test]
    fn log_stamps_entries_with_the_campaign_date() {
        let mut world = World::new();
        let today = GameDate::from_days(42);
        world.insert_resource(CampaignClock {
            start_date: GameDate::from_days(0),
            date: today,
        });
        world.insert_resource(MessageLog::default());

        log(&mut world, LogEntry::line("it happened", LogChannel::Jobs));

        let entries = &world.resource::<MessageLog>().entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].date, today, "line() dates are stamped on push");
    }

    #[test]
    fn derived_streams_match_direct_derivation() {
        let mut world = World::new();
        world.insert_resource(CampaignSeed(7));
        let mut ours = derived_rng(&world, "purpose", &[1, 2]);
        let mut direct = DeterministicRng::derive(7, "purpose", &[1, 2]);
        assert_eq!(ours.roll(1000), direct.roll(1000));
    }
}
