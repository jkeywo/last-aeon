//! Filling authored definitions with their display text.
//!
//! Authored content carries no prose. A definition declares its ID and its
//! mechanics; every string the player reads is a row in the string table,
//! keyed by that ID and the field it fills — `province.karvessa.name`,
//! `event.grain-riot.log-text`. The builders leave those fields empty and
//! this pass fills them in.
//!
//! Deriving the key rather than authoring it is what stops the two drifting
//! apart: a definition cannot point at the wrong row, and a renamed
//! definition takes its prose with it or fails loudly. Every required key
//! that finds no row is an error naming the row to write, so one pass over
//! a content set reports the whole list of missing prose rather than the
//! first line of it.
//!
//! Optional prose — a house's surname, an event's log line, the body of a
//! result's popup — is decided by whether the row exists. The script used
//! to say whether it had one; now the table does, and there is no second
//! place for the two to disagree.

use std::collections::BTreeSet;

use crate::key::ContentKey;
use crate::model::{ContentSet, OutcomeKind};
use crate::report::ContentReport;
use crate::text::StringTable;

use super::builders::BuilderState;

/// Fills every display-text field in `builder` from `strings`.
///
/// Reports an error for each required key with no row.
pub(super) fn fill_display_text(builder: &mut BuilderState, strings: &StringTable, path: &str) {
    let mut fill = Filler {
        strings,
        report: &mut builder.report,
        path,
    };

    for (key, def) in &mut builder.assignments {
        def.title = fill.req("assignment", key, "title");
        def.summary = fill.req("assignment", key, "summary");
        for (kind, result) in &mut def.results {
            let stem = format!("assignment.{key}.{}", result_stem(*kind));
            result.popup_text = fill.opt(&format!("{stem}.popup-text"));
            result.log_text = fill.opt(&format!("{stem}.log-text"));
            for choice in &mut result.choices {
                choice.label = fill.at(&format!("{stem}.choice.{}", choice.id));
            }
        }
    }
    for (key, def) in &mut builder.bodies {
        def.name = fill.req("body", key, "name");
    }
    for (key, def) in &mut builder.goods {
        def.name = fill.req("good", key, "name");
    }
    for (key, def) in &mut builder.provinces {
        def.name = fill.req("province", key, "name");
    }
    for (key, def) in &mut builder.traits {
        def.name = fill.req("trait", key, "name");
        def.summary = fill.req("trait", key, "summary");
    }
    for (key, def) in &mut builder.characters {
        def.name = fill.req("character", key, "name");
    }
    for (key, def) in &mut builder.organisations {
        def.name = fill.req("organisation", key, "name");
        def.surname = fill.opt(&format!("organisation.{key}.surname"));
    }
    for (key, def) in &mut builder.titles {
        def.name = fill.req("title", key, "name");
    }
    for (key, def) in &mut builder.offices {
        def.name = fill.req("office", key, "name");
    }
    for (key, def) in &mut builder.ships {
        def.name = fill.req("ship", key, "name");
    }
    for (key, def) in &mut builder.armies {
        def.name = fill.req("army", key, "name");
    }
    for (key, def) in &mut builder.events {
        def.title = fill.req("event", key, "title");
        def.text = fill.req("event", key, "text");
        def.log_text = fill.opt(&format!("event.{key}.log-text"));
        for choice in &mut def.choices {
            choice.label = fill.at(&format!("event.{key}.choice.{}", choice.id));
        }
    }
    for (key, def) in &mut builder.plans {
        def.title = fill.req("plan", key, "title");
        def.summary = fill.req("plan", key, "summary");
    }
    for (key, def) in &mut builder.goals {
        def.title = fill.req("goal", key, "title");
        def.summary = fill.req("goal", key, "summary");
    }
    if let Some(scenario) = &mut builder.scenario {
        let key = scenario.key.clone();
        scenario.name = fill.req("scenario", &key, "name");
    }
}

/// Every string-table row a loaded content set draws on.
///
/// The mirror of [`fill_display_text`], over the finished set rather than
/// the builder. Together they answer both halves of the question: the fill
/// says which rows are missing, and this says which rows nothing asks for.
/// The two derivations agreeing is not assumed — a row this misses shows
/// up as an orphan, and a row it invents shows up as missing.
pub fn text_keys(set: &ContentSet) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut add = |key: String| {
        keys.insert(key);
    };

    for (key, def) in &set.assignments {
        add(format!("assignment.{key}.title"));
        add(format!("assignment.{key}.summary"));
        for (kind, result) in &def.results {
            let stem = format!("assignment.{key}.{}", result_stem(*kind));
            if result.popup_text.is_some() {
                add(format!("{stem}.popup-text"));
            }
            if result.log_text.is_some() {
                add(format!("{stem}.log-text"));
            }
            for choice in &result.choices {
                add(format!("{stem}.choice.{}", choice.id));
            }
        }
    }
    for key in set.bodies.keys() {
        add(format!("body.{key}.name"));
    }
    for key in set.goods.keys() {
        add(format!("good.{key}.name"));
    }
    for key in set.provinces.keys() {
        add(format!("province.{key}.name"));
    }
    for key in set.traits.keys() {
        add(format!("trait.{key}.name"));
        add(format!("trait.{key}.summary"));
    }
    for key in set.characters.keys() {
        add(format!("character.{key}.name"));
    }
    for (key, def) in &set.organisations {
        add(format!("organisation.{key}.name"));
        if def.surname.is_some() {
            add(format!("organisation.{key}.surname"));
        }
    }
    for key in set.titles.keys() {
        add(format!("title.{key}.name"));
    }
    for key in set.offices.keys() {
        add(format!("office.{key}.name"));
    }
    for key in set.ships.keys() {
        add(format!("ship.{key}.name"));
    }
    for key in set.armies.keys() {
        add(format!("army.{key}.name"));
    }
    for (key, def) in &set.events {
        add(format!("event.{key}.title"));
        add(format!("event.{key}.text"));
        if def.log_text.is_some() {
            add(format!("event.{key}.log-text"));
        }
        for choice in &def.choices {
            add(format!("event.{key}.choice.{}", choice.id));
        }
    }
    for key in set.plans.keys() {
        add(format!("plan.{key}.title"));
        add(format!("plan.{key}.summary"));
    }
    for key in set.goals.keys() {
        add(format!("goal.{key}.title"));
        add(format!("goal.{key}.summary"));
    }
    if let Some(scenario) = &set.scenario {
        add(format!("scenario.{}.name", scenario.key));
    }
    keys
}

struct Filler<'a> {
    strings: &'a StringTable,
    report: &'a mut ContentReport,
    path: &'a str,
}

impl Filler<'_> {
    /// Prose a definition cannot do without.
    fn req(&mut self, kind: &str, key: &ContentKey, field: &str) -> String {
        self.at(&format!("{kind}.{key}.{field}"))
    }

    /// Prose a definition may or may not have. The row decides.
    ///
    /// Asks for the row rather than the text, so the blank table used by
    /// fixture tests leaves optional prose absent instead of empty.
    fn opt(&mut self, key: &str) -> Option<String> {
        self.strings.row(key).map(|row| row.english.clone())
    }

    fn at(&mut self, key: &str) -> String {
        match self.strings.get(key) {
            Some(text) => text.to_owned(),
            None => {
                self.report.error(
                    self.path,
                    Some(key),
                    "authored content needs this row in the string table",
                );
                String::new()
            }
        }
    }
}

/// The key segment naming a assignment result, which has no ID of its own.
fn result_stem(kind: OutcomeKind) -> &'static str {
    match kind {
        OutcomeKind::CriticalSuccess => "critical-success",
        OutcomeKind::Success => "success",
        OutcomeKind::Failure => "failure",
        OutcomeKind::Disaster => "disaster",
    }
}
