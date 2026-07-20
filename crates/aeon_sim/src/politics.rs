//! The authoritative political model: characters, organisations, titles,
//! offices, opinion, succession, and the contested Consular appointment.
//!
//! Everything here follows the simulation's determinism rules: iteration
//! in stable-ID order, integer arithmetic, and randomness only through
//! derived named streams. Characters born during play get stable IDs from
//! the campaign allocator; authored entities keep their content keys so
//! saves stay legible.

use std::collections::BTreeMap;

use aeon_core::calendar::{DAYS_PER_YEAR, GameDate};
use aeon_core::rng::DeterministicRng;
use aeon_data::model::{Gender, HouseTier, OrgKind, SkillsDef, TitleHolderDef, TitleKindDef};
use aeon_data::{ContentKey, ContentSet};
use bevy::app::App;
use bevy::prelude::{Component, Entity, IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use crate::clock::{CampaignClock, DailyTick, TickSet, YearlyPulse};
use crate::ids::{BodyId, CharacterId, OfficeId, OrgId, ProvinceId, TitleId};
use crate::map::MapIndex;
use crate::state::{CampaignIds, ContentDb};

/// Age of legal adulthood, in years.
pub const ADULT_AGE: i64 = 18;

/// Days a Consular vacancy stays open before the Tsar's appointment
/// arrives.
pub const CONSUL_CONTEST_DAYS: i64 = 120;

/// Days a starbase-commander vacancy waits before the Consul fills it.
pub const COMMANDER_VACANCY_DAYS: i64 = 30;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// A character's identity and vital state.
#[derive(Component, Clone, Debug)]
pub struct CharacterRecord {
    /// Stable ID.
    pub id: CharacterId,
    /// Authored content key; `None` for characters born during play.
    pub key: Option<ContentKey>,
    /// Full display name.
    pub name: String,
    /// Biological sex.
    pub gender: Gender,
    /// Birth date.
    pub birth: GameDate,
    /// Death date; `None` while alive.
    pub death: Option<GameDate>,
    /// Organisation this character belongs to.
    pub organisation: Option<OrgId>,
}

impl CharacterRecord {
    /// Whether the character is alive.
    pub fn alive(&self) -> bool {
        self.death.is_none()
    }

    /// Whole years of age at `date`.
    pub fn age_years(&self, date: GameDate) -> i64 {
        self.birth.days_until(date).max(0) / DAYS_PER_YEAR
    }
}

/// A character's base skills.
#[derive(Component, Copy, Clone, Debug, Default)]
pub struct CharacterSkills(pub SkillsDef);

/// A character's traits, sorted by key.
#[derive(Component, Clone, Debug, Default)]
pub struct CharacterTraits(pub Vec<ContentKey>);

/// Family bonds. Children are derived by scanning parents.
#[derive(Component, Clone, Debug, Default)]
pub struct Lineage {
    /// Up to two parents.
    pub parents: Vec<CharacterId>,
    /// Current spouse.
    pub spouse: Option<CharacterId>,
}

/// Directional opinion modifiers *from* this character toward others.
///
/// Sorted by (target, reason) for deterministic iteration and hashing.
#[derive(Component, Clone, Debug, Default)]
pub struct OpinionLedger(pub Vec<OpinionEntry>);

/// One stored directional opinion modifier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpinionEntry {
    /// The character this opinion is about.
    pub target: CharacterId,
    /// Signed opinion amount.
    pub amount: i32,
    /// Stable reason tag; one modifier per (target, reason).
    pub reason: String,
    /// Expiry date, if temporary.
    pub expires: Option<GameDate>,
}

impl OpinionLedger {
    /// Adds or replaces the modifier with this target and reason.
    pub fn set(&mut self, entry: OpinionEntry) {
        match self
            .0
            .binary_search_by(|e| (e.target, e.reason.as_str()).cmp(&(entry.target, &entry.reason)))
        {
            Ok(index) => self.0[index] = entry,
            Err(index) => self.0.insert(index, entry),
        }
    }
}

/// An organisation's identity and mutable politics.
#[derive(Component, Clone, Debug)]
pub struct OrgRecord {
    /// Stable ID.
    pub id: OrgId,
    /// Authored content key.
    pub key: ContentKey,
    /// What kind of organisation this is.
    pub kind: OrgKind,
    /// Hierarchy tier; houses only.
    pub tier: Option<HouseTier>,
    /// Current liege organisation, for vassal houses.
    pub liege: Option<OrgId>,
    /// Current head; `None` while leaderless.
    pub head: Option<CharacterId>,
    /// Whether the organisation has failed (no succession possible).
    pub defunct: bool,
}

/// What a title covers.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TitleKind {
    /// The legal title over one province.
    Province(ProvinceId),
    /// Paramount title over a body's provinces.
    Paramount(BodyId),
    /// The Tsar-appointed Consul title.
    Consul,
}

/// Who holds a title.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TitleHolder {
    /// Held by an organisation.
    Org(OrgId),
    /// Held personally by a character.
    Character(CharacterId),
    /// Vacant or contested.
    Vacant,
}

/// A legal title.
#[derive(Component, Clone, Debug)]
pub struct TitleRecord {
    /// Stable ID.
    pub id: TitleId,
    /// Authored content key; `None` for implicit province titles.
    pub key: Option<ContentKey>,
    /// Display name.
    pub name: String,
    /// What the title covers.
    pub kind: TitleKind,
    /// Current holder.
    pub holder: TitleHolder,
}

/// A revocable office held by a character.
#[derive(Component, Clone, Debug)]
pub struct OfficeRecord {
    /// Stable ID.
    pub id: OfficeId,
    /// Authored content key.
    pub key: ContentKey,
    /// Display name.
    pub name: String,
    /// The organisation whose authority this office carries.
    pub organisation: OrgId,
    /// The province this office administers, if any.
    pub province: Option<ProvinceId>,
    /// Current holder.
    pub holder: Option<CharacterId>,
    /// The day the office fell vacant, for appointment timers.
    pub vacant_since: Option<GameDate>,
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Lookup between political stable IDs, content keys, and entities.
#[derive(Resource, Clone, Debug, Default)]
pub struct PoliticsIndex {
    /// Characters by stable ID.
    pub characters: BTreeMap<CharacterId, Entity>,
    /// Organisations by stable ID.
    pub orgs: BTreeMap<OrgId, Entity>,
    /// Titles by stable ID.
    pub titles: BTreeMap<TitleId, Entity>,
    /// Offices by stable ID.
    pub offices: BTreeMap<OfficeId, Entity>,
    /// Character IDs by authored key.
    pub character_keys: BTreeMap<ContentKey, CharacterId>,
    /// Organisation IDs by authored key.
    pub org_keys: BTreeMap<ContentKey, OrgId>,
    /// Title IDs by authored key (authored titles only).
    pub title_keys: BTreeMap<ContentKey, TitleId>,
    /// Office IDs by authored key.
    pub office_keys: BTreeMap<ContentKey, OfficeId>,
    /// Province title IDs by province.
    pub province_titles: BTreeMap<ProvinceId, TitleId>,
}

/// The organisation the player leads, when the scenario names one.
#[derive(Resource, Copy, Clone, Debug, Default)]
pub struct PlayerHouse(pub Option<OrgId>);

/// An open contest for the vacant Consul title.
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsulContest {
    /// The Consul title being contested.
    pub title: TitleId,
    /// The day the vacancy opened.
    pub opened: GameDate,
    /// Declared candidates, in stable-ID order.
    pub candidates: Vec<CharacterId>,
}

/// Set when the campaign has ended.
///
/// The MVP has no victory condition; the campaign ends only on terminal
/// failure of the player house.
#[derive(Resource, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignOver {
    /// The day the campaign ended.
    pub date: GameDate,
    /// Human-readable reason.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Spawning from content
// ---------------------------------------------------------------------------

/// Spawns the political world for a fresh campaign.
///
/// Allocation order (characters, organisations, province titles, authored
/// titles, offices — each in key/ID order) is part of the determinism
/// contract: same content, same IDs.
pub fn spawn_from_content(world: &mut World, content: &ContentSet) {
    let mut index = PoliticsIndex::default();

    // Allocate character and organisation IDs up front so cross-references
    // resolve regardless of definition order.
    for key in content.characters.keys() {
        let id: CharacterId = world.resource_mut::<CampaignIds>().0.allocate();
        index.character_keys.insert(key.clone(), id);
    }
    for key in content.organisations.keys() {
        let id: OrgId = world.resource_mut::<CampaignIds>().0.allocate();
        index.org_keys.insert(key.clone(), id);
    }

    for (key, def) in &content.characters {
        let id = index.character_keys[key];
        let organisation = def.organisation.as_ref().map(|o| index.org_keys[o]);
        let birth = aeon_core::calendar::CalendarDate {
            year: def.birth_year,
            month: def.birth_month,
            day: def.birth_day,
        }
        .to_date()
        .expect("content validation guarantees birth dates");
        let entity = world
            .spawn((
                CharacterRecord {
                    id,
                    key: Some(key.clone()),
                    name: def.name.clone(),
                    gender: def.gender,
                    birth,
                    death: None,
                    organisation,
                },
                CharacterSkills(def.skills),
                CharacterTraits({
                    let mut traits = def.traits.clone();
                    traits.sort();
                    traits
                }),
                Lineage {
                    parents: def
                        .parents
                        .iter()
                        .map(|p| index.character_keys[p])
                        .collect(),
                    spouse: def.spouse.as_ref().map(|s| index.character_keys[s]),
                },
                OpinionLedger::default(),
                crate::assignments::CharacterCondition::default(),
            ))
            .id();
        index.characters.insert(id, entity);
    }

    // Character locations: members start at their organisation's first
    // held province; everyone else at the map's first province.
    let map_index = world.resource::<crate::map::MapIndex>().clone();
    let fallback_province = map_index.province_ids.keys().next().copied();
    let mut org_homes: BTreeMap<OrgId, crate::ids::ProvinceId> = BTreeMap::new();
    for (key, def) in &content.organisations {
        let org_id = index.org_keys[key];
        let home = def
            .provinces
            .iter()
            .map(|p| map_index.province_keys[p])
            .min()
            .or(fallback_province);
        if let Some(home) = home {
            org_homes.insert(org_id, home);
        }
    }
    for (key, def) in &content.characters {
        let id = index.character_keys[key];
        let entity = index.characters[&id];
        let home = def
            .organisation
            .as_ref()
            .map(|o| index.org_keys[o])
            .and_then(|org| org_homes.get(&org).copied())
            .or(fallback_province);
        if let Some(home) = home {
            world
                .entity_mut(entity)
                .insert(crate::presence::CharacterLocation(
                    crate::presence::Location::Province(home),
                ));
        }
    }

    for (key, def) in &content.organisations {
        let id = index.org_keys[key];
        let entity = world
            .spawn((
                OrgRecord {
                    id,
                    key: key.clone(),
                    kind: def.kind,
                    tier: def.tier,
                    liege: def.liege.as_ref().map(|l| index.org_keys[l]),
                    head: def.head.as_ref().map(|h| index.character_keys[h]),
                    defunct: false,
                },
                crate::economy::OrgResources {
                    wealth: def.wealth,
                    manpower: def.manpower,
                    supplies: def.supplies,
                    influence: i64::from(def.legitimacy),
                    legitimacy: def.legitimacy,
                },
            ))
            .id();
        index.orgs.insert(id, entity);
    }

    // Implicit province titles, in province-ID order. Holdings come from
    // organisation province lists.
    let map_index = world.resource::<MapIndex>().clone();
    let mut province_holders: BTreeMap<ProvinceId, OrgId> = BTreeMap::new();
    for (key, def) in &content.organisations {
        let org_id = index.org_keys[key];
        for province_key in &def.provinces {
            let province_id = map_index.province_keys[province_key];
            province_holders.insert(province_id, org_id);
        }
    }
    for (province_id, province_key) in &map_index.province_ids {
        let id: TitleId = world.resource_mut::<CampaignIds>().0.allocate();
        let name = content.provinces[province_key].name.clone();
        let holder = province_holders
            .get(province_id)
            .map(|org| TitleHolder::Org(*org))
            .unwrap_or(TitleHolder::Vacant);
        let entity = world
            .spawn(TitleRecord {
                id,
                key: None,
                name,
                kind: TitleKind::Province(*province_id),
                holder,
            })
            .id();
        index.titles.insert(id, entity);
        index.province_titles.insert(*province_id, id);
    }

    for (key, def) in &content.titles {
        let id: TitleId = world.resource_mut::<CampaignIds>().0.allocate();
        let kind = match &def.kind {
            TitleKindDef::Paramount { body } => TitleKind::Paramount(map_index.body_keys[body]),
            TitleKindDef::Consul => TitleKind::Consul,
        };
        let holder = match &def.holder {
            TitleHolderDef::Organisation(org) => TitleHolder::Org(index.org_keys[org]),
            TitleHolderDef::Character(character) => {
                TitleHolder::Character(index.character_keys[character])
            }
            TitleHolderDef::Vacant => TitleHolder::Vacant,
        };
        let entity = world
            .spawn(TitleRecord {
                id,
                key: Some(key.clone()),
                name: def.name.clone(),
                kind,
                holder,
            })
            .id();
        index.titles.insert(id, entity);
        index.title_keys.insert(key.clone(), id);
    }

    for (key, def) in &content.offices {
        let id: OfficeId = world.resource_mut::<CampaignIds>().0.allocate();
        let entity = world
            .spawn(OfficeRecord {
                id,
                key: key.clone(),
                name: def.name.clone(),
                organisation: index.org_keys[&def.organisation],
                province: def.province.as_ref().map(|p| map_index.province_keys[p]),
                holder: def.holder.as_ref().map(|h| index.character_keys[h]),
                vacant_since: None,
            })
            .id();
        index.offices.insert(id, entity);
        index.office_keys.insert(key.clone(), id);
    }

    // Marriages are symmetric: mirror spouse links authored in one
    // direction only. Conflicting declarations are content errors.
    let spouse_pairs: Vec<(CharacterId, CharacterId)> = index
        .characters
        .iter()
        .filter_map(|(id, entity)| {
            world
                .get::<Lineage>(*entity)
                .and_then(|l| l.spouse)
                .map(|spouse| (*id, spouse))
        })
        .collect();
    for (a, b) in spouse_pairs {
        let b_entity = index.characters[&b];
        let mut lineage = world.get_mut::<Lineage>(b_entity).expect("indexed");
        if lineage.spouse.is_none() {
            lineage.spouse = Some(a);
        }
    }

    let player = content
        .scenario
        .as_ref()
        .and_then(|s| s.player_house.as_ref())
        .map(|key| index.org_keys[key]);

    world.insert_resource(index);
    world.insert_resource(PlayerHouse(player));
}

// ---------------------------------------------------------------------------
// Snapshot state
// ---------------------------------------------------------------------------

/// Serialised character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterState {
    /// Stable ID.
    pub id: CharacterId,
    /// Authored key, if authored.
    pub key: Option<ContentKey>,
    /// Display name.
    pub name: String,
    /// Biological sex.
    pub gender: Gender,
    /// Birth date.
    pub birth: GameDate,
    /// Death date.
    pub death: Option<GameDate>,
    /// Organisation membership.
    pub organisation: Option<OrgId>,
    /// Base skills.
    pub skills: SkillsDef,
    /// Traits, sorted.
    pub traits: Vec<ContentKey>,
    /// Parents.
    pub parents: Vec<CharacterId>,
    /// Spouse.
    pub spouse: Option<CharacterId>,
    /// Stored opinion modifiers from this character.
    pub opinions: Vec<OpinionEntry>,
    /// Temporary incapacitating conditions.
    #[serde(default)]
    pub condition: crate::assignments::CharacterCondition,
    /// Physical location, if tracked.
    #[serde(default)]
    pub location: Option<crate::presence::Location>,
}

/// Serialised organisation (mutable facts only; the rest is content).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgState {
    /// Stable ID.
    pub id: OrgId,
    /// Authored key.
    pub key: ContentKey,
    /// Current liege.
    pub liege: Option<OrgId>,
    /// Current head.
    pub head: Option<CharacterId>,
    /// Whether the organisation has failed.
    pub defunct: bool,
    /// Strategic resources.
    #[serde(default)]
    pub resources: crate::economy::OrgResources,
}

/// Serialised title.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TitleState {
    /// Stable ID.
    pub id: TitleId,
    /// Authored key, if authored.
    pub key: Option<ContentKey>,
    /// Display name (needed for implicit titles).
    pub name: String,
    /// What the title covers.
    pub kind: TitleKind,
    /// Current holder.
    pub holder: TitleHolder,
}

/// Serialised office.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfficeState {
    /// Stable ID.
    pub id: OfficeId,
    /// Authored key.
    pub key: ContentKey,
    /// Display name.
    pub name: String,
    /// Owning organisation.
    pub organisation: OrgId,
    /// Administered province.
    pub province: Option<ProvinceId>,
    /// Current holder.
    pub holder: Option<CharacterId>,
    /// Vacancy start, for appointment timers.
    pub vacant_since: Option<GameDate>,
}

/// The complete serialised political world.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoliticsState {
    /// Characters in ID order.
    pub characters: Vec<CharacterState>,
    /// Organisations in ID order.
    pub orgs: Vec<OrgState>,
    /// Titles in ID order.
    pub titles: Vec<TitleState>,
    /// Offices in ID order.
    pub offices: Vec<OfficeState>,
    /// The player's organisation.
    pub player_house: Option<OrgId>,
    /// An open Consular contest, if any.
    pub consul_contest: Option<ConsulContest>,
    /// Campaign end state, if the campaign has ended.
    pub campaign_over: Option<CampaignOver>,
}

/// Captures the political world for a snapshot.
pub fn capture_politics(world: &World) -> PoliticsState {
    let Some(index) = world.get_resource::<PoliticsIndex>() else {
        return PoliticsState::default();
    };

    let mut characters = Vec::with_capacity(index.characters.len());
    for entity in index.characters.values() {
        let record = world.get::<CharacterRecord>(*entity).expect("indexed");
        let skills = world.get::<CharacterSkills>(*entity).expect("indexed");
        let traits = world.get::<CharacterTraits>(*entity).expect("indexed");
        let lineage = world.get::<Lineage>(*entity).expect("indexed");
        let opinions = world.get::<OpinionLedger>(*entity).expect("indexed");
        let condition = world
            .get::<crate::assignments::CharacterCondition>(*entity)
            .copied()
            .unwrap_or_default();
        let location = world
            .get::<crate::presence::CharacterLocation>(*entity)
            .map(|l| l.0);
        characters.push(CharacterState {
            id: record.id,
            key: record.key.clone(),
            name: record.name.clone(),
            gender: record.gender,
            birth: record.birth,
            death: record.death,
            organisation: record.organisation,
            skills: skills.0,
            traits: traits.0.clone(),
            parents: lineage.parents.clone(),
            spouse: lineage.spouse,
            opinions: opinions.0.clone(),
            condition,
            location,
        });
    }

    let orgs = index
        .orgs
        .values()
        .map(|entity| {
            let record = world.get::<OrgRecord>(*entity).expect("indexed");
            OrgState {
                id: record.id,
                key: record.key.clone(),
                liege: record.liege,
                head: record.head,
                defunct: record.defunct,
                resources: world
                    .get::<crate::economy::OrgResources>(*entity)
                    .copied()
                    .unwrap_or_default(),
            }
        })
        .collect();

    let titles = index
        .titles
        .values()
        .map(|entity| {
            let record = world.get::<TitleRecord>(*entity).expect("indexed");
            TitleState {
                id: record.id,
                key: record.key.clone(),
                name: record.name.clone(),
                kind: record.kind,
                holder: record.holder,
            }
        })
        .collect();

    let offices = index
        .offices
        .values()
        .map(|entity| {
            let record = world.get::<OfficeRecord>(*entity).expect("indexed");
            OfficeState {
                id: record.id,
                key: record.key.clone(),
                name: record.name.clone(),
                organisation: record.organisation,
                province: record.province,
                holder: record.holder,
                vacant_since: record.vacant_since,
            }
        })
        .collect();

    PoliticsState {
        characters,
        orgs,
        titles,
        offices,
        player_house: world.get_resource::<PlayerHouse>().and_then(|p| p.0),
        consul_contest: world.get_resource::<ConsulContest>().cloned(),
        campaign_over: world.get_resource::<CampaignOver>().cloned(),
    }
}

/// Respawns the political world from a snapshot against hash-verified
/// content.
pub fn restore_politics(world: &mut World, state: &PoliticsState, content: &ContentSet) {
    let mut index = PoliticsIndex::default();

    for character in &state.characters {
        let entity = world
            .spawn((
                CharacterRecord {
                    id: character.id,
                    key: character.key.clone(),
                    name: character.name.clone(),
                    gender: character.gender,
                    birth: character.birth,
                    death: character.death,
                    organisation: character.organisation,
                },
                CharacterSkills(character.skills),
                CharacterTraits(character.traits.clone()),
                Lineage {
                    parents: character.parents.clone(),
                    spouse: character.spouse,
                },
                OpinionLedger(character.opinions.clone()),
                character.condition,
            ))
            .id();
        if let Some(location) = character.location {
            world
                .entity_mut(entity)
                .insert(crate::presence::CharacterLocation(location));
        }
        index.characters.insert(character.id, entity);
        if let Some(key) = &character.key {
            index.character_keys.insert(key.clone(), character.id);
        }
    }

    for org in &state.orgs {
        let def = content
            .organisations
            .get(&org.key)
            .expect("hash-verified content defines every persisted organisation");
        let entity = world
            .spawn((
                OrgRecord {
                    id: org.id,
                    key: org.key.clone(),
                    kind: def.kind,
                    tier: def.tier,
                    liege: org.liege,
                    head: org.head,
                    defunct: org.defunct,
                },
                org.resources,
            ))
            .id();
        index.orgs.insert(org.id, entity);
        index.org_keys.insert(org.key.clone(), org.id);
    }

    for title in &state.titles {
        let entity = world
            .spawn(TitleRecord {
                id: title.id,
                key: title.key.clone(),
                name: title.name.clone(),
                kind: title.kind,
                holder: title.holder,
            })
            .id();
        index.titles.insert(title.id, entity);
        if let Some(key) = &title.key {
            index.title_keys.insert(key.clone(), title.id);
        }
        if let TitleKind::Province(province) = title.kind {
            index.province_titles.insert(province, title.id);
        }
    }

    for office in &state.offices {
        let entity = world
            .spawn(OfficeRecord {
                id: office.id,
                key: office.key.clone(),
                name: office.name.clone(),
                organisation: office.organisation,
                province: office.province,
                holder: office.holder,
                vacant_since: office.vacant_since,
            })
            .id();
        index.offices.insert(office.id, entity);
        index.office_keys.insert(office.key.clone(), office.id);
    }

    world.insert_resource(index);
    world.insert_resource(PlayerHouse(state.player_house));
    if let Some(contest) = &state.consul_contest {
        world.insert_resource(contest.clone());
    }
    if let Some(over) = &state.campaign_over {
        world.insert_resource(over.clone());
    }
}

// ---------------------------------------------------------------------------
// Opinion
// ---------------------------------------------------------------------------

/// Situational opinion bonuses.
const OPINION_SAME_ORG: i32 = 10;
const OPINION_SPOUSE: i32 = 30;
const OPINION_PARENT_CHILD: i32 = 25;
const OPINION_SIBLING: i32 = 15;

/// One character's opinion-relevant facts, borrowed from components.
///
/// Lets presentation code compute opinions from query data without
/// exclusive world access.
#[derive(Copy, Clone)]
pub struct CharacterView<'a> {
    /// Identity and vitals.
    pub record: &'a CharacterRecord,
    /// Traits.
    pub traits: &'a CharacterTraits,
    /// Family bonds.
    pub lineage: &'a Lineage,
    /// Stored opinion modifiers from this character.
    pub ledger: &'a OpinionLedger,
}

/// Derived opinion of `from` about `to` given both characters' facts:
/// trait compatibility plus situational bonds plus stored directional
/// modifiers, clamped to -100..=100.
pub fn opinion_of(
    content: &ContentSet,
    date: GameDate,
    from: CharacterView<'_>,
    to: CharacterView<'_>,
) -> i32 {
    if from.record.id == to.record.id {
        return 100;
    }
    let mut opinion = 0i32;

    // Trait affinity.
    for trait_key in &from.traits.0 {
        let Some(def) = content.traits.get(trait_key) else {
            continue;
        };
        if to.traits.0.contains(trait_key) {
            opinion += def.opinion_same;
        }
        for opposite in &def.opposites {
            if to.traits.0.contains(opposite) {
                opinion -= def.opinion_opposed;
            }
        }
    }

    // Situational bonds.
    if from.record.organisation.is_some() && from.record.organisation == to.record.organisation {
        opinion += OPINION_SAME_ORG;
    }
    if from.lineage.spouse == Some(to.record.id) {
        opinion += OPINION_SPOUSE;
    }
    if from.lineage.parents.contains(&to.record.id) || to.lineage.parents.contains(&from.record.id)
    {
        opinion += OPINION_PARENT_CHILD;
    } else if !from.lineage.parents.is_empty()
        && from
            .lineage
            .parents
            .iter()
            .any(|p| to.lineage.parents.contains(p))
    {
        opinion += OPINION_SIBLING;
    }

    // Stored modifiers.
    for entry in &from.ledger.0 {
        if entry.target == to.record.id && entry.expires.is_none_or(|e| e > date) {
            opinion += entry.amount;
        }
    }

    opinion.clamp(-100, 100)
}

/// Derived opinion of `from` about `to`, looked up from the world.
/// How far `start` sits below `liege` in the chain of vassalage, if it sits
/// below it at all.
///
/// `Some(0)` means they are the same house — ground held directly.
/// `Some(1)` is a direct vassal, `Some(2)` a vassal's vassal, and so on;
/// `None` means `start` does not answer to `liege` by any path.
///
/// This answers "is this mine, and how directly" for a house anywhere in
/// the hierarchy, which walking to the top of the chain cannot: a vassal
/// house is not the great house above it, and its own holdings are still
/// its own.
/// The great house at the top of an organisation's chain of vassalage.
///
/// This answers the Great House map's question — whose banner this ground
/// ultimately marches under — not whether ground is *yours*: that is
/// [`answers_to`]'s hop-distance question, which a top-of-chain walk gets
/// wrong for any house that is itself a vassal.
pub fn great_house_of(world: &World, start: OrgId) -> OrgId {
    let mut current = start;
    // Bounded so a cycle in authored content cannot hang the campaign.
    for _ in 0..16 {
        let Some(record) = crate::access::org(world, current) else {
            break;
        };
        match (record.tier, record.liege) {
            (Some(HouseTier::Vassal), Some(liege)) => current = liege,
            _ => break,
        }
    }
    current
}

pub fn answers_to(world: &World, start: OrgId, liege: OrgId) -> Option<u32> {
    let index = world.get_resource::<PoliticsIndex>()?;
    let mut current = start;
    // Bounded so a cycle in authored content cannot hang the campaign.
    for hops in 0..16 {
        if current == liege {
            return Some(hops);
        }
        let record = index
            .orgs
            .get(&current)
            .and_then(|entity| world.get::<OrgRecord>(*entity))?;
        match (record.tier, record.liege) {
            (Some(HouseTier::Vassal), Some(above)) => current = above,
            _ => return None,
        }
    }
    None
}

pub fn opinion_between(world: &World, from: CharacterId, to: CharacterId) -> i32 {
    if from == to {
        return 100;
    }
    let index = world.resource::<PoliticsIndex>();
    let content = world.resource::<ContentDb>();
    let (Some(from_entity), Some(to_entity)) =
        (index.characters.get(&from), index.characters.get(&to))
    else {
        return 0;
    };
    let date = world.resource::<CampaignClock>().date;

    let view = |entity: Entity| CharacterView {
        record: world.get::<CharacterRecord>(entity).expect("indexed"),
        traits: world.get::<CharacterTraits>(entity).expect("indexed"),
        lineage: world.get::<Lineage>(entity).expect("indexed"),
        ledger: world.get::<OpinionLedger>(entity).expect("indexed"),
    };
    opinion_of(&content.0, date, view(*from_entity), view(*to_entity))
}

// ---------------------------------------------------------------------------
// Life cycle: mortality, succession, marriage, birth
// ---------------------------------------------------------------------------

/// Yearly mortality permille by decade of age.
fn mortality_permille(age_years: i64) -> u32 {
    match age_years {
        i64::MIN..=4 => 8,
        5..=49 => 2,
        50..=59 => 8,
        60..=69 => 25,
        70..=79 => 70,
        80..=89 => 160,
        _ => 350,
    }
}

fn character_entity(world: &World, id: CharacterId) -> Entity {
    crate::access::character_entity(world, id).expect("indexed")
}

/// Yearly mortality and everything a death sets in motion.
pub fn yearly_mortality(world: &mut World) {
    if world.get_resource::<PoliticsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let year = date.calendar().year;

    for id in crate::access::living_character_ids(world) {
        let age = crate::access::character(world, id)
            .expect("indexed")
            .age_years(date);
        let mut rng = crate::access::derived_rng(world, "mortality", &[id.raw(), year as u64]);
        if rng.check_permille(mortality_permille(age)) {
            process_death(world, id, date);
        }
    }
}

/// Marks a character dead and resolves everything their death touches:
/// widowhood, house succession, Consular vacancy, office vacancies.
pub fn process_death(world: &mut World, id: CharacterId, date: GameDate) {
    let entity = character_entity(world, id);
    {
        let mut record = world.get_mut::<CharacterRecord>(entity).expect("indexed");
        if record.death.is_some() {
            return;
        }
        record.death = Some(date);
    }

    // Widow the spouse.
    let spouse = world.get::<Lineage>(entity).expect("indexed").spouse;
    if let Some(spouse_id) = spouse {
        let spouse_entity = character_entity(world, spouse_id);
        world
            .get_mut::<Lineage>(spouse_entity)
            .expect("indexed")
            .spouse = None;
        world.get_mut::<Lineage>(entity).expect("indexed").spouse = None;
    }

    // A ship whose captain dies is left without one, and says so. It
    // cannot be ordered again until somebody takes command.
    let vacated: Vec<(crate::ids::ShipId, String, OrgId)> = world
        .get_resource::<crate::forces::ForcesIndex>()
        .map(|forces| {
            forces
                .ships
                .values()
                .filter_map(|entity| {
                    let ship = world.get::<crate::forces::ShipRecord>(*entity)?;
                    (ship.captain == Some(id)).then(|| (ship.id, ship.name.clone(), ship.owner))
                })
                .collect()
        })
        .unwrap_or_default();
    for (ship, name, owner) in vacated {
        if let Some(entity) = crate::access::ship_entity(world, ship)
            && let Some(mut record) = world.get_mut::<crate::forces::ShipRecord>(entity)
        {
            record.captain = None;
        }
        let line = world
            .resource::<crate::text::TextDb>()
            .format("sim.politics.ship-without-captain", &[("ship", &name)]);
        crate::access::log(
            world,
            crate::assignments::LogEntry::line(line, crate::assignments::LogChannel::Military)
                .by(Some(owner)),
        );
    }

    // Succession for any organisation the character led.
    let led: Vec<OrgId> = {
        let index = world.resource::<PoliticsIndex>();
        index
            .orgs
            .iter()
            .filter(|(_, org_entity)| {
                world
                    .get::<OrgRecord>(**org_entity)
                    .is_some_and(|o| o.head == Some(id))
            })
            .map(|(org_id, _)| *org_id)
            .collect()
    };
    for org_id in led {
        resolve_succession(world, org_id, id, date);
    }

    // Vacate personally held titles (the Consulate opens a contest).
    let held_titles: Vec<TitleId> = {
        let index = world.resource::<PoliticsIndex>();
        index
            .titles
            .iter()
            .filter(|(_, title_entity)| {
                world
                    .get::<TitleRecord>(**title_entity)
                    .is_some_and(|t| t.holder == TitleHolder::Character(id))
            })
            .map(|(title_id, _)| *title_id)
            .collect()
    };
    for title_id in held_titles {
        vacate_personal_title(world, title_id, date);
    }

    // Vacate offices.
    let held_offices: Vec<OfficeId> = {
        let index = world.resource::<PoliticsIndex>();
        index
            .offices
            .iter()
            .filter(|(_, office_entity)| {
                world
                    .get::<OfficeRecord>(**office_entity)
                    .is_some_and(|o| o.holder == Some(id))
            })
            .map(|(office_id, _)| *office_id)
            .collect()
    };
    for office_id in held_offices {
        let office_entity = world.resource::<PoliticsIndex>().offices[&office_id];
        let mut office = world
            .get_mut::<OfficeRecord>(office_entity)
            .expect("indexed");
        office.holder = None;
        office.vacant_since = Some(date);
    }
}

/// Legal heir order for a dynastic house after `dead` dies: children by
/// age, then siblings by age, then any living member by age. Gender-blind.
pub fn house_heir(world: &World, org: OrgId, dead: CharacterId) -> Option<CharacterId> {
    let index = world.resource::<PoliticsIndex>();
    let dead_entity = index.characters[&dead];
    let dead_lineage = world.get::<Lineage>(dead_entity).expect("indexed");
    let dead_parents = dead_lineage.parents.clone();

    let mut candidates: Vec<(u8, GameDate, CharacterId)> = Vec::new();
    for (id, entity) in &index.characters {
        if *id == dead {
            continue;
        }
        let record = world.get::<CharacterRecord>(*entity).expect("indexed");
        if !record.alive() || record.organisation != Some(org) {
            continue;
        }
        let lineage = world.get::<Lineage>(*entity).expect("indexed");
        let rank = if lineage.parents.contains(&dead) {
            0 // child
        } else if !dead_parents.is_empty()
            && lineage.parents.iter().any(|p| dead_parents.contains(p))
        {
            1 // sibling
        } else {
            2 // any member
        };
        candidates.push((rank, record.birth, *id));
    }
    candidates.sort();
    candidates.first().map(|(_, _, id)| *id)
}

/// Resolves the change of head after a death, per the organisation's
/// succession rules.
fn resolve_succession(world: &mut World, org_id: OrgId, dead: CharacterId, date: GameDate) {
    let org_entity = crate::access::org_entity(world, org_id).expect("indexed");
    let kind = world.get::<OrgRecord>(org_entity).expect("indexed").kind;

    match kind {
        OrgKind::DynasticHouse => {
            let heir = house_heir(world, org_id, dead);
            let mut org = world.get_mut::<OrgRecord>(org_entity).expect("indexed");
            match heir {
                Some(heir_id) => org.head = Some(heir_id),
                None => {
                    let key = org.key.clone();
                    org.head = None;
                    org.defunct = true;
                    let is_player = world.resource::<PlayerHouse>().0 == Some(org_id);
                    if is_player && world.get_resource::<CampaignOver>().is_none() {
                        let strings = world.resource::<crate::text::TextDb>();
                        let name = world
                            .resource::<ContentDb>()
                            .0
                            .organisations
                            .get(&key)
                            .map(|def| def.name.clone())
                            .unwrap_or_else(|| {
                                strings.text("sim.politics.the-player-house").to_owned()
                            });
                        let reason = world
                            .resource::<crate::text::TextDb>()
                            .format("sim.politics.no-successor", &[("house", &name)]);
                        world.insert_resource(CampaignOver { date, reason });
                    }
                }
            }
        }
        OrgKind::SanctoraImperim => {
            // The Sanctora's practical head is the Consul; the vacancy
            // opens a contest handled by the daily contest system.
            let mut org = world.get_mut::<OrgRecord>(org_entity).expect("indexed");
            org.head = None;
        }
    }
}

/// Vacates a personally held title; a Consul vacancy opens the contest.
fn vacate_personal_title(world: &mut World, title_id: TitleId, date: GameDate) {
    let title_entity = crate::access::title_entity(world, title_id).expect("indexed");
    let kind = {
        let mut title = world.get_mut::<TitleRecord>(title_entity).expect("indexed");
        title.holder = TitleHolder::Vacant;
        title.kind
    };
    if kind == TitleKind::Consul && world.get_resource::<ConsulContest>().is_none() {
        let candidates = consul_candidates(world);
        world.insert_resource(ConsulContest {
            title: title_id,
            opened: date,
            candidates,
        });
    }
}

/// Everyone who puts themselves forward for a vacant Consulate: living
/// adult heads of organisations and adult Sanctora members, in ID order.
fn consul_candidates(world: &World) -> Vec<CharacterId> {
    let index = world.resource::<PoliticsIndex>();
    let date = world.resource::<CampaignClock>().date;
    let sanctora = crate::access::sanctora_org(world);
    let heads: Vec<CharacterId> = index
        .orgs
        .values()
        .filter_map(|entity| world.get::<OrgRecord>(*entity).expect("indexed").head)
        .collect();

    let mut candidates: Vec<CharacterId> = Vec::new();
    for (id, entity) in &index.characters {
        let record = world.get::<CharacterRecord>(*entity).expect("indexed");
        if !record.alive() || record.age_years(date) < ADULT_AGE {
            continue;
        }
        let is_head = heads.contains(id);
        let is_sanctora = sanctora.is_some() && record.organisation == sanctora;
        if is_head || is_sanctora {
            candidates.push(*id);
        }
    }
    candidates
}

/// A candidate's standing in the Consular contest. Opinion terms let
/// political assignments move the outcome.
fn consul_score(world: &World, candidate: CharacterId, jitter: i64) -> i64 {
    let skills = crate::access::on_character::<CharacterSkills>(world, candidate)
        .expect("indexed")
        .0;

    let sanctora = crate::access::sanctora_org(world);
    let sanctora_members: Vec<CharacterId> = crate::access::living_character_ids(world)
        .into_iter()
        .filter(|id| {
            *id != candidate
                && sanctora.is_some()
                && crate::access::organisation_of(world, *id) == sanctora
        })
        .collect();

    let opinion_sum: i64 = sanctora_members
        .iter()
        .map(|member| i64::from(opinion_between(world, *member, candidate)))
        .sum();

    i64::from(skills.diplomacy) * 2 + i64::from(skills.stewardship) + opinion_sum + jitter
}

/// Daily: resolves an open Consular contest once the Tsar's appointment
/// arrives, and fills vacant offices whose timers have run out.
pub fn daily_appointments(world: &mut World) {
    if world.get_resource::<PoliticsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;

    // Consular contest.
    if let Some(contest) = world.get_resource::<ConsulContest>().cloned()
        && date.days_until(contest.opened.add_days(CONSUL_CONTEST_DAYS)) <= 0
    {
        let mut best: Option<(i64, CharacterId)> = None;
        for candidate in &contest.candidates {
            let alive = crate::access::character(world, *candidate).is_some_and(|r| r.alive());
            if !alive {
                continue;
            }
            let mut rng = crate::access::derived_rng(
                world,
                "consul-appointment",
                &[contest.title.raw(), candidate.raw()],
            );
            let jitter = rng.roll_range(-10, 10);
            let score = consul_score(world, *candidate, jitter);
            // Ties resolve to the lower stable ID (strict comparison).
            if best.is_none_or(|(best_score, best_id)| {
                score > best_score || (score == best_score && *candidate < best_id)
            }) {
                best = Some((score, *candidate));
            }
        }

        if let Some((_, winner)) = best {
            let title_entity = crate::access::title_entity(world, contest.title).expect("indexed");
            world
                .get_mut::<TitleRecord>(title_entity)
                .expect("indexed")
                .holder = TitleHolder::Character(winner);
            // The new Consul heads the Sanctora Imperim.
            let sanctora = crate::access::sanctora_org(world)
                .and_then(|org| crate::access::org_entity(world, org));
            if let Some(sanctora_entity) = sanctora {
                world
                    .get_mut::<OrgRecord>(sanctora_entity)
                    .expect("indexed")
                    .head = Some(winner);
            }
        }
        // Contest closes even if no candidate survived; a later death
        // reopens it.
        world.remove_resource::<ConsulContest>();
    }

    // Office appointments: the Consul fills vacant Sanctora offices.
    let consul = crate::access::consul(world);
    let vacant_offices: Vec<OfficeId> = {
        let index = world.resource::<PoliticsIndex>();
        index
            .offices
            .iter()
            .filter(|(_, entity)| {
                world.get::<OfficeRecord>(**entity).is_some_and(|o| {
                    o.holder.is_none()
                        && o.vacant_since.is_some_and(|v| {
                            date.days_until(v.add_days(COMMANDER_VACANCY_DAYS)) <= 0
                        })
                })
            })
            .map(|(id, _)| *id)
            .collect()
    };
    if consul.is_some() {
        for office_id in vacant_offices {
            let org = crate::access::office(world, office_id)
                .expect("indexed")
                .organisation;
            // Best living member of the office's organisation by
            // command + stewardship; ties to the lower ID.
            let candidates: Vec<(i64, CharacterId)> = {
                let index = world.resource::<PoliticsIndex>();
                index
                    .characters
                    .iter()
                    .filter(|(_, entity)| {
                        world
                            .get::<CharacterRecord>(**entity)
                            .is_some_and(|r| r.alive() && r.organisation == Some(org))
                    })
                    .map(|(id, entity)| {
                        let skills = world.get::<CharacterSkills>(*entity).expect("indexed").0;
                        (
                            -(i64::from(skills.command) + i64::from(skills.stewardship)),
                            *id,
                        )
                    })
                    .collect()
            };
            let appointee = candidates.iter().min().map(|(_, id)| *id);
            if let Some(appointee) = appointee {
                let entity = crate::access::office_entity(world, office_id).expect("indexed");
                let mut office = world.get_mut::<OfficeRecord>(entity).expect("indexed");
                office.holder = Some(appointee);
                office.vacant_since = None;
            }
        }
    }
}

/// Yearly: unmarried adults in houses seek marriages.
pub fn yearly_marriages(world: &mut World) {
    if world.get_resource::<PoliticsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let year = date.calendar().year;

    for id in crate::access::living_character_ids(world) {
        let record = crate::access::character(world, id).expect("indexed");
        let lineage = crate::access::on_character::<Lineage>(world, id).expect("indexed");
        if lineage.spouse.is_some()
            || record.organisation.is_none()
            || record.age_years(date) < ADULT_AGE
        {
            continue;
        }
        let gender = record.gender;
        let my_parents = lineage.parents.clone();

        // A marriage prospect this year at all?
        let mut rng = crate::access::derived_rng(world, "marriage", &[id.raw(), year as u64]);
        if !rng.check_permille(300) {
            continue;
        }

        // Best mutual match among unmarried adults of the other gender.
        let mut best: Option<(i64, CharacterId)> = None;
        for other in crate::access::living_character_ids(world) {
            if other <= id {
                continue; // each pair considered once, initiator = lower ID
            }
            let other_record = crate::access::character(world, other).expect("indexed");
            let other_lineage =
                crate::access::on_character::<Lineage>(world, other).expect("indexed");
            if other_record.gender == gender
                || other_lineage.spouse.is_some()
                || other_record.organisation.is_none()
                || other_record.age_years(date) < ADULT_AGE
            {
                continue;
            }
            // No parents, children, or siblings.
            let related = other_lineage.parents.contains(&id)
                || my_parents.contains(&other)
                || (!my_parents.is_empty()
                    && other_lineage.parents.iter().any(|p| my_parents.contains(p)));
            if related {
                continue;
            }
            let mutual = i64::from(opinion_between(world, id, other))
                + i64::from(opinion_between(world, other, id));
            if mutual < 0 {
                continue;
            }
            if best.is_none_or(|(score, best_id)| {
                mutual > score || (mutual == score && other < best_id)
            }) {
                best = Some((mutual, other));
            }
        }

        if let Some((_, partner)) = best {
            let entity = character_entity(world, id);
            let partner_entity = character_entity(world, partner);
            world.get_mut::<Lineage>(entity).expect("indexed").spouse = Some(partner);
            world
                .get_mut::<Lineage>(partner_entity)
                .expect("indexed")
                .spouse = Some(id);
        }
    }
}

/// Yearly: married couples may have children.
pub fn yearly_births(world: &mut World) {
    if world.get_resource::<PoliticsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let year = date.calendar().year;

    for id in crate::access::living_character_ids(world) {
        let record = crate::access::character(world, id).expect("indexed");
        // Iterate mothers only so each couple rolls once.
        if record.gender != Gender::Female {
            continue;
        }
        let age = record.age_years(date);
        if !(ADULT_AGE..=45).contains(&age) {
            continue;
        }
        let Some(father) = crate::access::on_character::<Lineage>(world, id)
            .expect("indexed")
            .spouse
        else {
            continue;
        };
        if !crate::access::character(world, father)
            .expect("indexed")
            .alive()
        {
            continue;
        }

        // Existing children of this mother.
        let children = {
            let index = world.resource::<PoliticsIndex>();
            index
                .characters
                .values()
                .filter(|e| {
                    world
                        .get::<Lineage>(**e)
                        .is_some_and(|l| l.parents.contains(&id))
                })
                .count() as i64
        };

        let chance = (250 - children * 40).max(50) as u32;
        let mut rng = crate::access::derived_rng(world, "birth", &[id.raw(), year as u64]);
        if !rng.check_permille(chance) {
            continue;
        }

        spawn_child(world, id, father, date, &mut rng);
    }
}

/// Creates a newborn character of a couple.
fn spawn_child(
    world: &mut World,
    mother: CharacterId,
    father: CharacterId,
    date: GameDate,
    rng: &mut DeterministicRng,
) {
    let gender = if rng.check_permille(500) {
        Gender::Male
    } else {
        Gender::Female
    };

    // House: the head's line wins, then the lower-ID parent's house.
    let mother_org = crate::access::organisation_of(world, mother);
    let father_org = crate::access::organisation_of(world, father);
    let org = {
        let is_head = |c: CharacterId, o: Option<OrgId>| {
            o.is_some_and(|org_id| crate::access::org_head(world, org_id) == Some(c))
        };
        if is_head(mother, mother_org) {
            mother_org
        } else if is_head(father, father_org) {
            father_org
        } else if mother < father {
            mother_org.or(father_org)
        } else {
            father_org.or(mother_org)
        }
    };

    // Name from the first name pool plus the house surname.
    let content = world.resource::<ContentDb>().0.clone();
    let pool = content
        .name_pools
        .values()
        .next()
        .expect("content validation requires a name pool");
    let given = match gender {
        Gender::Male => pool.male[rng.roll(pool.male.len() as u64) as usize].clone(),
        Gender::Female => pool.female[rng.roll(pool.female.len() as u64) as usize].clone(),
    };
    let surname = org.and_then(|org_id| {
        let key = &crate::access::org(world, org_id).expect("indexed").key;
        content
            .organisations
            .get(key)
            .and_then(|def| def.surname.clone())
    });
    let name = match surname {
        Some(surname) => format!("{given} {surname}"),
        None => given,
    };

    let id: CharacterId = world.resource_mut::<CampaignIds>().0.allocate();
    let entity = world
        .spawn((
            CharacterRecord {
                id,
                key: None,
                name,
                gender,
                birth: date,
                death: None,
                organisation: org,
            },
            CharacterSkills::default(),
            CharacterTraits::default(),
            Lineage {
                parents: vec![mother.min(father), mother.max(father)],
                spouse: None,
            },
            OpinionLedger::default(),
            crate::assignments::CharacterCondition::default(),
        ))
        .id();
    let mother_home =
        crate::access::on_character::<crate::presence::CharacterLocation>(world, mother)
            .map(|l| l.0);
    if let Some(crate::presence::Location::Province(province)) = mother_home {
        world
            .entity_mut(entity)
            .insert(crate::presence::CharacterLocation(
                crate::presence::Location::Province(province),
            ));
    }
    world
        .resource_mut::<PoliticsIndex>()
        .characters
        .insert(id, entity);
}

/// Monthly: drop expired opinion modifiers.
pub fn expire_opinion_modifiers(world: &mut World) {
    if world.get_resource::<PoliticsIndex>().is_none() {
        return;
    }
    let date = world.resource::<CampaignClock>().date;
    let entities: Vec<Entity> = world
        .resource::<PoliticsIndex>()
        .characters
        .values()
        .copied()
        .collect();
    for entity in entities {
        if let Some(mut ledger) = world.get_mut::<OpinionLedger>(entity) {
            ledger
                .0
                .retain(|entry| entry.expires.is_none_or(|e| e > date));
        }
    }
}

// ---------------------------------------------------------------------------
// Plugin wiring
// ---------------------------------------------------------------------------

pub(crate) fn install(app: &mut App) {
    app.add_systems(DailyTick, daily_appointments.in_set(TickSet::Simulation));
    app.add_systems(crate::clock::MonthlyPulse, expire_opinion_modifiers);
    app.add_systems(
        YearlyPulse,
        (yearly_mortality, yearly_marriages, yearly_births).chain(),
    );
}
