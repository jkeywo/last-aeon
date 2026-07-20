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

use crate::key::ContentKey;
use crate::model::JobResultKind;
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

    for (key, def) in &mut builder.jobs {
        def.title = fill.req("job", key, "title");
        def.summary = fill.req("job", key, "summary");
        for (kind, result) in &mut def.results {
            let stem = format!("job.{key}.{}", result_stem(*kind));
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
    if let Some(scenario) = &mut builder.scenario {
        let key = scenario.key.clone();
        scenario.name = fill.req("scenario", &key, "name");
    }
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

/// The key segment naming a job result, which has no ID of its own.
fn result_stem(kind: JobResultKind) -> &'static str {
    match kind {
        JobResultKind::CriticalSuccess => "critical-success",
        JobResultKind::Success => "success",
        JobResultKind::Failure => "failure",
        JobResultKind::Disaster => "disaster",
    }
}
