//! The political obligation ledger: favours, promises, and grievances.
//!
//! These are explicit bilateral facts between organisations, deliberately
//! kept separate from character opinion. A house can dislike you and still
//! owe you a favour; it can like you and still nurse a grievance. Opinion
//! says how someone feels, the ledger says what they are bound by, and the
//! two are read independently by events, jobs, and autonomous agency.
//!
//! Every entry names its parties, where it came from, what it is worth,
//! and how it ends — by fulfilment, by being broken, or by expiry — so the
//! player can always see why a house is behaving as it is.

use bevy::app::App;
use bevy::prelude::{IntoScheduleConfigs, Resource, World};
use serde::{Deserialize, Serialize};

use aeon_core::calendar::GameDate;

use crate::clock::{CampaignClock, DailyTick, TickSet};
use crate::ids::OrgId;
use crate::jobs::{LogChannel, LogEntry, LogSubject, MessageLog};

/// What kind of political fact an entry records.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObligationKind {
    /// The debtor owes the creditor a good turn.
    Favour,
    /// The debtor has undertaken something for the creditor, by a date.
    Promise,
    /// The creditor holds a grudge against the debtor.
    Grievance,
}

impl ObligationKind {
    /// A short player-facing name.
    pub fn label(self) -> &'static str {
        match self {
            ObligationKind::Favour => "Favour",
            ObligationKind::Promise => "Promise",
            ObligationKind::Grievance => "Grievance",
        }
    }

    /// Whether this kind counts in the debtor's favour or against it.
    pub fn is_positive(self) -> bool {
        matches!(self, ObligationKind::Favour | ObligationKind::Promise)
    }
}

/// How an entry ended, if it has.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObligationStatus {
    /// Still in force.
    #[default]
    Open,
    /// Honoured.
    Fulfilled,
    /// Repudiated.
    Broken,
    /// Ran out of time.
    Expired,
}

impl ObligationStatus {
    /// A short player-facing name.
    pub fn label(self) -> &'static str {
        match self {
            ObligationStatus::Open => "open",
            ObligationStatus::Fulfilled => "fulfilled",
            ObligationStatus::Broken => "broken",
            ObligationStatus::Expired => "expired",
        }
    }
}

/// One political obligation between two organisations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationRecord {
    /// Stable id within the ledger.
    pub id: u64,
    /// What kind of fact this is.
    pub kind: ObligationKind,
    /// The house that owes, or that is resented.
    pub debtor: OrgId,
    /// The house that is owed, or that resents.
    pub creditor: OrgId,
    /// Where it came from, in plain words.
    pub origin: String,
    /// The day it came into being.
    pub created: GameDate,
    /// The day it lapses, if it does.
    pub expires: Option<GameDate>,
    /// How much it weighs, for agency and event eligibility.
    pub weight: i32,
    /// Whether it is still in force.
    pub status: ObligationStatus,
}

impl ObligationRecord {
    /// Whether this entry still binds.
    pub fn is_open(&self) -> bool {
        self.status == ObligationStatus::Open
    }

    /// A one-line description for the inspector.
    pub fn summary(&self, name_of: impl Fn(OrgId) -> String) -> String {
        let debtor = name_of(self.debtor);
        let creditor = name_of(self.creditor);
        match self.kind {
            ObligationKind::Favour => format!("{debtor} owes {creditor} a favour"),
            ObligationKind::Promise => format!("{debtor} has promised {creditor}"),
            ObligationKind::Grievance => format!("{creditor} resents {debtor}"),
        }
    }
}

/// The whole ledger, in creation order.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligations {
    /// Next id to hand out.
    pub next_id: u64,
    /// Every entry ever created, including settled ones, so the history
    /// stays inspectable.
    pub entries: Vec<ObligationRecord>,
}

impl Obligations {
    /// Every entry still in force, oldest first.
    pub fn open(&self) -> impl Iterator<Item = &ObligationRecord> {
        self.entries.iter().filter(|entry| entry.is_open())
    }

    /// Open entries where `org` is a party, either way round.
    pub fn involving(&self, org: OrgId) -> impl Iterator<Item = &ObligationRecord> {
        self.open()
            .filter(move |entry| entry.debtor == org || entry.creditor == org)
    }

    /// Open entries `debtor` owes `creditor`.
    pub fn owed(&self, debtor: OrgId, creditor: OrgId) -> impl Iterator<Item = &ObligationRecord> {
        self.open()
            .filter(move |entry| entry.debtor == debtor && entry.creditor == creditor)
    }

    /// The net standing of `debtor` toward `creditor`: favours and
    /// promises count for, grievances against.
    ///
    /// This is a summary for agency and display, never a substitute for
    /// the individual entries, which remain separately inspectable.
    pub fn standing(&self, debtor: OrgId, creditor: OrgId) -> i32 {
        self.open()
            .filter(|entry| entry.debtor == debtor && entry.creditor == creditor)
            .map(|entry| {
                if entry.kind.is_positive() {
                    entry.weight
                } else {
                    -entry.weight
                }
            })
            .sum()
    }
}

/// Records a new obligation and returns its id.
pub fn create(
    world: &mut World,
    kind: ObligationKind,
    debtor: OrgId,
    creditor: OrgId,
    origin: impl Into<String>,
    weight: i32,
    days: Option<i64>,
) -> u64 {
    if debtor == creditor {
        return 0;
    }
    let date = world
        .get_resource::<CampaignClock>()
        .map(|clock| clock.date)
        .unwrap_or_else(|| GameDate::from_days(0));
    let origin = origin.into();
    let mut ledger = world.get_resource_or_insert_with(Obligations::default);
    let id = ledger.next_id + 1;
    ledger.next_id = id;
    ledger.entries.push(ObligationRecord {
        id,
        kind,
        debtor,
        creditor,
        origin,
        created: date,
        expires: days.map(|days| date.add_days(days)),
        weight: weight.max(0),
        status: ObligationStatus::Open,
    });
    id
}

/// Settles the oldest open obligation of `kind` that `debtor` owes
/// `creditor`, returning whether one was found.
pub fn settle(
    world: &mut World,
    kind: ObligationKind,
    debtor: OrgId,
    creditor: OrgId,
    status: ObligationStatus,
) -> bool {
    let Some(mut ledger) = world.get_resource_mut::<Obligations>() else {
        return false;
    };
    let Some(entry) = ledger.entries.iter_mut().find(|entry| {
        entry.is_open()
            && entry.kind == kind
            && entry.debtor == debtor
            && entry.creditor == creditor
    }) else {
        return false;
    };
    entry.status = status;
    true
}

/// Expires obligations whose day has passed.
///
/// Entries are visited in ledger order and only ever settled, never
/// removed, so the ledger replays identically and keeps its history.
pub fn expire_due(world: &mut World) {
    let Some(date) = world
        .get_resource::<CampaignClock>()
        .map(|clock| clock.date)
    else {
        return;
    };
    let lapsed: Vec<(u64, ObligationKind, OrgId, OrgId, String)> = {
        let Some(mut ledger) = world.get_resource_mut::<Obligations>() else {
            return;
        };
        let mut lapsed = Vec::new();
        for entry in ledger.entries.iter_mut() {
            if entry.is_open() && entry.expires.is_some_and(|expiry| expiry <= date) {
                entry.status = ObligationStatus::Expired;
                lapsed.push((
                    entry.id,
                    entry.kind,
                    entry.debtor,
                    entry.creditor,
                    entry.origin.clone(),
                ));
            }
        }
        lapsed
    };

    for (_, kind, debtor, _, origin) in lapsed {
        // A promise that runs out is a political event in its own right;
        // a grievance simply fades.
        if kind == ObligationKind::Promise {
            let name = crate::crisis::org_display_name(world, debtor);
            world.resource_mut::<MessageLog>().entries.push(
                LogEntry::new(
                    date,
                    format!("{name} let a promise lapse: {origin}."),
                    LogChannel::Politics,
                )
                .by(Some(debtor))
                .about(LogSubject::Org(debtor)),
            );
        }
    }
}

/// Seeds the ledger from authored content at campaign start.
///
/// Entries are created in content-key order, so the opening political
/// situation is identical across runs of the same scenario.
pub fn seed_from_content(world: &mut World, content: &aeon_data::ContentSet) {
    let politics = world.resource::<crate::politics::PoliticsIndex>().clone();
    let seeds: Vec<(ObligationKind, OrgId, OrgId, String, i32, Option<i64>)> = content
        .obligations
        .values()
        .filter_map(|def| {
            let kind = match def.kind.as_str() {
                "favour" => ObligationKind::Favour,
                "promise" => ObligationKind::Promise,
                _ => ObligationKind::Grievance,
            };
            let debtor = *politics.org_keys.get(&def.debtor)?;
            let creditor = *politics.org_keys.get(&def.creditor)?;
            Some((
                kind,
                debtor,
                creditor,
                def.origin.clone(),
                def.weight,
                def.days,
            ))
        })
        .collect();
    for (kind, debtor, creditor, origin, weight, days) in seeds {
        create(world, kind, debtor, creditor, origin, weight, days);
    }
}

/// Captures the ledger for a snapshot.
pub fn capture(world: &World) -> Obligations {
    world
        .get_resource::<Obligations>()
        .cloned()
        .unwrap_or_default()
}

/// Restores the ledger from a snapshot.
pub fn restore(world: &mut World, state: &Obligations) {
    world.insert_resource(state.clone());
}

/// Installs the daily expiry sweep.
pub fn install(app: &mut App) {
    app.add_systems(DailyTick, expire_due.in_set(TickSet::Cleanup));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org(raw: u64) -> OrgId {
        OrgId::from_raw(raw).expect("test ids are valid")
    }

    #[test]
    fn standing_weighs_favours_against_grievances() {
        let mut ledger = Obligations::default();
        let date = GameDate::from_days(0);
        let mut push = |kind, weight| {
            ledger.next_id += 1;
            ledger.entries.push(ObligationRecord {
                id: ledger.next_id,
                kind,
                debtor: org(1),
                creditor: org(2),
                origin: "test".to_owned(),
                created: date,
                expires: None,
                weight,
                status: ObligationStatus::Open,
            });
        };
        push(ObligationKind::Favour, 30);
        push(ObligationKind::Grievance, 10);
        assert_eq!(ledger.standing(org(1), org(2)), 20);
        assert_eq!(
            ledger.standing(org(2), org(1)),
            0,
            "standing is directional"
        );
    }

    #[test]
    fn settled_entries_stop_counting_but_stay_on_the_record() {
        let mut ledger = Obligations {
            next_id: 1,
            entries: Vec::new(),
        };
        ledger.entries.push(ObligationRecord {
            id: 1,
            kind: ObligationKind::Favour,
            debtor: org(1),
            creditor: org(2),
            origin: "test".to_owned(),
            created: GameDate::from_days(0),
            expires: None,
            weight: 25,
            status: ObligationStatus::Fulfilled,
        });
        assert_eq!(ledger.standing(org(1), org(2)), 0);
        assert_eq!(ledger.open().count(), 0);
        assert_eq!(ledger.entries.len(), 1, "history is kept");
    }
}
