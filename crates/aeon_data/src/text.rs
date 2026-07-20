//! The display-text string table.
//!
//! Every string the player reads lives in one CSV, not in Rust or Rhai
//! source. Rows carry an ID, the context a translator needs to render it
//! well, and the English text.
//!
//! English text that no human has yet approved is wrapped in square
//! brackets in the data itself. Almost all of it starts out that way: the
//! prose was drafted mechanically, and the brackets make that legible on
//! screen rather than hiding it in a manifest. Approving a line is the act
//! of deleting its brackets, and the share still bracketed is a progress
//! figure the loader reports.

use std::collections::BTreeMap;

use crate::key::TextKey;
use crate::report::ContentReport;

/// One row of the string table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRow {
    /// How the string is used in the game, for whoever translates it.
    pub context: String,
    /// The English text, with `{named}` placeholders and, while the line
    /// is unapproved, surrounding square brackets.
    pub english: String,
}

impl TextRow {
    /// Whether this line is still unapproved placeholder prose.
    pub fn is_placeholder(&self) -> bool {
        self.english.starts_with('[') && self.english.ends_with(']')
    }
}

/// Every display string in the game, keyed by [`TextKey`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable {
    rows: BTreeMap<TextKey, TextRow>,
    /// Answer any key with an empty string. Test support only.
    echo: bool,
}

impl StringTable {
    /// Parses a string-table CSV.
    ///
    /// Returns `None` alongside the report when the file is unusable;
    /// every problem found is reported, not just the first, so an author
    /// fixes a batch per run.
    pub fn parse(source: &str, path: &str) -> (Option<Self>, ContentReport) {
        let mut report = ContentReport::default();
        let records = match parse_csv(source) {
            Ok(records) => records,
            Err(error) => {
                report.error(path, None, error);
                return (None, report);
            }
        };

        let mut records = records.into_iter();
        match records.next() {
            Some(header)
                if header.iter().map(String::as_str).collect::<Vec<_>>()
                    == ["id", "context", "english"] => {}
            Some(header) => {
                report.error(
                    path,
                    None,
                    format!(
                        "header must be `id,context,english`, found `{}`",
                        header.join(",")
                    ),
                );
                return (None, report);
            }
            None => {
                report.error(path, None, "the string table is empty");
                return (None, report);
            }
        }

        let mut rows = BTreeMap::new();
        for (index, record) in records.enumerate() {
            // Header consumed above, and humans count from one.
            let line = index + 2;
            if record.len() != 3 {
                report.error(
                    path,
                    None,
                    format!("line {line}: expected 3 fields, found {}", record.len()),
                );
                continue;
            }
            let [id, context, english]: [String; 3] =
                record.try_into().expect("length checked above");
            let key = match TextKey::new(&id) {
                Ok(key) => key,
                Err(error) => {
                    report.error(path, None, format!("line {line}: {error}"));
                    continue;
                }
            };
            if rows.contains_key(&key) {
                report.error(path, Some(&id), format!("line {line}: duplicate ID"));
                continue;
            }
            rows.insert(key, TextRow { context, english });
        }

        if report.has_errors() {
            (None, report)
        } else {
            let total = rows.len();
            let bracketed = rows.values().filter(|row| row.is_placeholder()).count();
            report.info(
                path,
                format!("{bracketed} of {total} strings still bracketed"),
            );
            (Some(Self { rows, echo: false }), report)
        }
    }

    /// Builds a table from `(id, english)` pairs.
    ///
    /// For tests that assert on what the player is actually shown, and so
    /// need real prose behind their fixture's IDs. Panics on a malformed
    /// ID, which in a test is the right moment to find out.
    pub fn from_rows(rows: &[(&str, &str)]) -> Self {
        Self {
            rows: rows
                .iter()
                .map(|(id, english)| {
                    let key = TextKey::new(id).unwrap_or_else(|e| panic!("{e}"));
                    let row = TextRow {
                        context: String::new(),
                        english: (*english).to_owned(),
                    };
                    (key, row)
                })
                .collect(),
            echo: false,
        }
    }

    /// Adds `(id, english)` rows, replacing any already present.
    ///
    /// For a test fixture that brings its own prose: start from the shipped
    /// table, so the simulation's own rows are still there, and add the
    /// rows the fixture's IDs derive. Panics on a malformed ID.
    pub fn extend(&mut self, rows: &[(&str, &str)]) {
        for (id, english) in rows {
            let key = TextKey::new(id).unwrap_or_else(|e| panic!("{e}"));
            self.rows.insert(
                key,
                TextRow {
                    context: String::new(),
                    english: (*english).to_owned(),
                },
            );
        }
    }

    /// A table that answers every key with an empty string.
    ///
    /// For tests whose subject is loading mechanics rather than prose: a
    /// fixture defining a province `test-a` should not also have to author
    /// a row for its name. Tests that *are* about the prose — that every
    /// shipped definition has a row — use the shipped table instead.
    pub fn blank() -> Self {
        Self {
            rows: BTreeMap::new(),
            echo: true,
        }
    }

    /// The text for a key, or `None` if the table has no such row.
    pub fn get(&self, key: &str) -> Option<&str> {
        let key = TextKey::new(key).ok()?;
        match self.rows.get(&key) {
            Some(row) => Some(row.english.as_str()),
            None if self.echo => Some(""),
            None => None,
        }
    }

    /// The row for a key, context included.
    pub fn row(&self, key: &str) -> Option<&TextRow> {
        TextKey::new(key).ok().and_then(|key| self.rows.get(&key))
    }

    /// Every key in the table, in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &TextKey> {
        self.rows.keys()
    }

    /// How many rows are still unapproved placeholder prose.
    pub fn placeholder_count(&self) -> usize {
        self.rows
            .values()
            .filter(|row| row.is_placeholder())
            .count()
    }

    /// The number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The text for a key, with no placeholders to fill.
    ///
    /// # Panics
    /// Panics if the key is absent. Validation checks every key the game
    /// can reach before it runs, so this only fires on a broken local
    /// edit — and should fire loudly rather than paint a blank label.
    pub fn text(&self, key: &str) -> &str {
        self.get(key)
            .unwrap_or_else(|| panic!("string table has no row for '{key}'"))
    }

    /// The text for a key with its `{named}` placeholders filled.
    ///
    /// # Panics
    /// Panics if the key is absent, or if the row names a placeholder that
    /// `args` does not supply.
    pub fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        fill(key, self.text(key), args)
    }

    /// The text for a pluralised key, selected on `count`.
    ///
    /// Appends `.one` or `.other` to the key, so a pluralised string is
    /// two whole rows rather than a sentence glued together from
    /// fragments — word order stays the translator's to choose.
    ///
    /// # Panics
    /// Panics if the selected row is absent or a placeholder is unfilled.
    pub fn format_plural(&self, key: &str, count: i64, args: &[(&str, &str)]) -> String {
        let suffix = if count == 1 { "one" } else { "other" };
        self.format(&format!("{key}.{suffix}"), args)
    }
}

/// The `{named}` placeholders a template asks for, in order of appearance.
pub fn placeholders_in(template: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                if !name.is_empty() && !names.iter().any(|seen| seen == name) {
                    names.push(name.to_owned());
                }
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    names
}

fn fill(key: &str, template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // An unclosed brace is literal text, not a placeholder.
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        let value = args
            .iter()
            .find(|(arg, _)| *arg == name)
            .map(|(_, value)| *value)
            .unwrap_or_else(|| panic!("string '{key}' needs placeholder '{{{name}}}'"));
        out.push_str(value);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Parses an RFC 4180 subset: quoted fields, `""` for a literal quote,
/// commas and newlines allowed inside quotes, CRLF or LF line endings.
fn parse_csv(source: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut field_started = false;
    let mut chars = source.chars().peekable();

    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if !field_started => {
                quoted = true;
                field_started = true;
            }
            '"' => return Err("a quote may only open a field".to_owned()),
            ',' => {
                record.push(core::mem::take(&mut field));
                field_started = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                record.push(core::mem::take(&mut field));
                records.push(core::mem::take(&mut record));
                field_started = false;
            }
            _ => {
                field.push(c);
                field_started = true;
            }
        }
    }

    if quoted {
        return Err("the file ends inside a quoted field".to_owned());
    }
    // A trailing newline leaves nothing behind; anything else is a last
    // record without one.
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "id,context,english\n";

    fn table(body: &str) -> StringTable {
        let (table, report) = StringTable::parse(&format!("{HEADER}{body}"), "strings.csv");
        assert!(!report.has_errors(), "{:?}", report.findings);
        table.expect("table parses")
    }

    #[test]
    fn reads_plain_and_quoted_fields() {
        let table = table(
            "ui.a,Plain context.,[Plain]\n\
             ui.b,\"Context, with a comma.\",\"[Text, with a comma]\"\n\
             ui.c,\"Context with a \"\"quote\"\".\",\"[He said \"\"no\"\"]\"\n",
        );
        assert_eq!(table.get("ui.a"), Some("[Plain]"));
        assert_eq!(table.get("ui.b"), Some("[Text, with a comma]"));
        assert_eq!(table.get("ui.c"), Some(r#"[He said "no"]"#));
        assert_eq!(
            table.row("ui.b").map(|row| row.context.as_str()),
            Some("Context, with a comma.")
        );
    }

    #[test]
    fn reads_newlines_inside_quotes() {
        let table = table("ui.a,\"Context.\",\"[First line\nsecond line]\"\n");
        assert_eq!(table.get("ui.a"), Some("[First line\nsecond line]"));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn reads_crlf_and_a_missing_trailing_newline() {
        let (crlf, _) = StringTable::parse("id,context,english\r\nui.a,C.,[A]\r\n", "s.csv");
        let (bare, _) = StringTable::parse("id,context,english\nui.a,C.,[A]", "s.csv");
        assert_eq!(crlf.expect("crlf parses").get("ui.a"), Some("[A]"));
        assert_eq!(bare.expect("bare parses").get("ui.a"), Some("[A]"));
    }

    #[test]
    fn rejects_a_bad_header() {
        let (table, report) = StringTable::parse("id,english\nui.a,[A]\n", "s.csv");
        assert!(table.is_none());
        assert!(report.has_errors());
    }

    #[test]
    fn rejects_duplicate_and_malformed_ids() {
        let (table, report) = StringTable::parse(
            &format!("{HEADER}ui.a,C.,[A]\nui.a,C.,[Again]\nui.Bad,C.,[B]\n"),
            "s.csv",
        );
        assert!(table.is_none());
        assert_eq!(report.findings.len(), 2, "{:?}", report.findings);
    }

    #[test]
    fn rejects_an_unterminated_quote() {
        let (table, report) = StringTable::parse(&format!("{HEADER}ui.a,C.,\"[A]\n"), "s.csv");
        assert!(table.is_none());
        assert!(report.has_errors());
    }

    #[test]
    fn fills_named_placeholders() {
        let table = table("ui.a,C.,\"[Held by {holder}, {hops} steps]\"\n");
        assert_eq!(
            table.format("ui.a", &[("holder", "Vaskal"), ("hops", "3")]),
            "[Held by Vaskal, 3 steps]"
        );
    }

    #[test]
    fn selects_plural_rows_on_count() {
        let table = table(
            "ui.a.one,C.,[1 step down]\n\
             ui.a.other,C.,[{hops} steps down]\n",
        );
        assert_eq!(table.format_plural("ui.a", 1, &[]), "[1 step down]");
        assert_eq!(
            table.format_plural("ui.a", 4, &[("hops", "4")]),
            "[4 steps down]"
        );
    }

    #[test]
    #[should_panic(expected = "no row for 'ui.missing'")]
    fn panics_on_a_missing_key() {
        table("ui.a,C.,[A]\n").text("ui.missing");
    }

    #[test]
    #[should_panic(expected = "needs placeholder '{holder}'")]
    fn panics_on_an_unfilled_placeholder() {
        table("ui.a,C.,[Held by {holder}]\n").format("ui.a", &[]);
    }

    #[test]
    fn finds_placeholders_in_a_template() {
        assert_eq!(
            placeholders_in("[Held by {holder}, {hops} steps, {holder} again]"),
            vec!["holder".to_owned(), "hops".to_owned()]
        );
        assert!(placeholders_in("[No placeholders]").is_empty());
    }

    #[test]
    fn counts_bracketed_rows() {
        let table = table("ui.a,C.,[Draft]\nui.b,C.,Approved\n");
        assert_eq!(table.placeholder_count(), 1);
        assert_eq!(table.len(), 2);
    }
}
