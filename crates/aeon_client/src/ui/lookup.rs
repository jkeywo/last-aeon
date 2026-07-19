//! Name, label and hover-summary lookups, in one place.
//!
//! These were closures declared at the top of the panel function, capturing
//! its locals. That made them convenient to call and impossible to move:
//! any panel that wanted a character's name had to stay inside the function
//! that had built the map, so every panel had to live in one function.
//!
//! As a struct built once per frame and passed by reference, the same
//! lookups can be used from anywhere, and the panels become separable. The
//! maps are still built once per frame rather than queried per call, which
//! is what made the closures worth having in the first place.

use std::collections::BTreeMap;

use aeon_core::calendar::GameDate;
use aeon_data::ContentSet;
use aeon_data::model::{HouseTier, OrgKind};
use aeon_sim::presence::{CharacterLocation, Location};
use aeon_sim::{BodyId, CharacterId, OrgId, ProvinceId, TitleHolder};

use crate::ui::data::{CharMap, OrgMap, PanelData};

/// Everything the panels need to turn an id into words.
pub struct Lookup<'a> {
    /// The authored content, for names and definitions.
    pub content: &'a ContentSet,
    /// Today, for ages and durations.
    pub date: GameDate,
    /// Every character, by id.
    pub chars: CharMap<'a>,
    /// Every organisation and its stores, by id.
    pub orgs: OrgMap<'a>,
    /// Body display names, by id.
    pub body_names: BTreeMap<BodyId, &'a str>,
    /// Province display names, by id.
    pub province_names: BTreeMap<ProvinceId, &'a str>,
    /// How many titles each organisation holds.
    ///
    /// Counted once rather than per hover: the summary for a house is built
    /// every time the pointer crosses a link to it.
    pub titles_held: BTreeMap<OrgId, usize>,
}

impl<'a> Lookup<'a> {
    /// Builds the frame's lookups from the panel queries.
    pub fn build(data: &'a PanelData, content: &'a ContentSet, date: GameDate) -> Self {
        let mut titles_held: BTreeMap<OrgId, usize> = BTreeMap::new();
        for title in &data.titles {
            if let TitleHolder::Org(org) = title.holder {
                *titles_held.entry(org).or_default() += 1;
            }
        }
        Self {
            content,
            date,
            chars: data
                .characters
                .iter()
                .map(|parts| (parts.0.id, parts))
                .collect(),
            orgs: data
                .orgs
                .iter()
                .map(|(record, resources)| (record.id, (record, resources)))
                .collect(),
            body_names: data
                .bodies
                .iter()
                .map(|(record, name)| (record.id, name.0.as_str()))
                .collect(),
            province_names: data
                .provinces
                .iter()
                .map(|(record, name, _)| (record.id, name.0.as_str()))
                .collect(),
            titles_held,
        }
    }

    /// An organisation's player-facing name.
    pub fn org_name(&self, id: OrgId) -> String {
        self.orgs
            .get(&id)
            .and_then(|(record, _)| self.content.organisations.get(&record.key))
            .map(|def| def.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// A character's name, or empty if they are unknown.
    pub fn char_name(&self, id: CharacterId) -> String {
        self.chars
            .get(&id)
            .map(|(record, ..)| record.name.clone())
            .unwrap_or_default()
    }

    /// A body's name, or "Unknown" if it is not one we know.
    pub fn body_name(&self, id: BodyId) -> &str {
        self.body_names.get(&id).copied().unwrap_or("Unknown")
    }

    /// A province's name, or empty if it is unknown.
    pub fn province_name(&self, id: ProvinceId) -> String {
        self.province_names
            .get(&id)
            .map(|name| (*name).to_owned())
            .unwrap_or_default()
    }

    /// Where a character is, in words.
    pub fn location_label(&self, location: Option<&CharacterLocation>) -> String {
        match location.map(|l| l.0) {
            Some(Location::Province(province)) => self.province_name(province),
            Some(Location::Transit { to, arrives }) => {
                let dest = self.province_names.get(&to).copied().unwrap_or("...");
                format!("In transit to {dest} (arrives {arrives})")
            }
            None => "Unknown".to_owned(),
        }
    }

    /// The hover summary for a link to a character.
    pub fn char_hover(&self, id: CharacterId) -> String {
        let Some((record, skills, traits, ..)) = self.chars.get(&id).copied() else {
            return String::new();
        };
        let house = record
            .organisation
            .map(|o| self.org_name(o))
            .unwrap_or_else(|| "no house".to_owned());
        let age = match record.death {
            None => format!("age {}", record.age_years(self.date)),
            Some(death) => format!("died {death}"),
        };
        let trait_names: Vec<&str> = traits
            .0
            .iter()
            .filter_map(|k| self.content.traits.get(k).map(|d| d.name.as_str()))
            .collect();
        let mut summary = format!(
            "{} — {house}, {age}\nCmd {} · Dip {} · Int {} · Ste {}",
            record.name,
            skills.0.command,
            skills.0.diplomacy,
            skills.0.intrigue,
            skills.0.stewardship,
        );
        if !trait_names.is_empty() {
            summary.push_str(&format!("\n{}", trait_names.join(", ")));
        }
        summary
    }

    /// The hover summary for a link to an organisation.
    pub fn org_hover(&self, id: OrgId) -> String {
        let Some((record, resources)) = self.orgs.get(&id).copied() else {
            return String::new();
        };
        let name = self.org_name(id);
        let standing = match (record.kind, record.tier) {
            (OrgKind::SanctoraImperim, _) => "Imperial government".to_owned(),
            (_, Some(HouseTier::Great)) => "great house".to_owned(),
            (_, Some(HouseTier::Vassal)) => match record.liege {
                Some(liege) => format!("vassal of {}", self.org_name(liege)),
                None => "vassal house".to_owned(),
            },
            (_, Some(HouseTier::Independent)) => "independent house".to_owned(),
            _ => String::new(),
        };
        let head = record
            .head
            .and_then(|h| self.chars.get(&h))
            .map(|(r, ..)| r.name.as_str())
            .unwrap_or("none");
        let held = self.titles_held.get(&id).copied().unwrap_or(0);
        let mut summary = format!("{name} — {standing}\nHead: {head} · {held} titles held");
        if let Some(r) = resources {
            summary.push_str(&format!(
                "\nW {} · M {} · S {} · I {}/{}",
                r.wealth, r.manpower, r.supplies, r.influence, r.legitimacy
            ));
        }
        summary
    }
}
