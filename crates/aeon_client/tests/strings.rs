//! Every string key the interface asks for exists, and asks for the right
//! placeholders.
//!
//! Keys are written as plain strings at their call sites, which keeps the
//! panels readable but means a typo is invisible until someone opens that
//! panel. This walks the client's own source, collects every key it names,
//! and checks each against the shipped table — so a missing row fails the
//! build rather than painting a blank label three menus deep.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use aeon_data::placeholders_in;
use aeon_sim::TextDb;

/// One `strings.text(…)` / `.format(…)` / `.format_plural(…)` call found in
/// the source.
#[derive(Debug)]
struct Call {
    file: String,
    line: usize,
    key: String,
    /// Argument names supplied at the call site, when they could be read.
    args: Vec<String>,
    plural: bool,
}

fn source_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).expect("source directory readable") {
        let path = entry.expect("source entry readable").path();
        if path.is_dir() {
            source_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Reads the balanced `(…)` argument list starting at `open`.
fn argument_list(text: &str, open: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    for (offset, byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'"' if !in_string => in_string = true,
            b'"' if in_string && bytes[open + offset - 1] != b'\\' => in_string = false,
            b'(' if !in_string => depth += 1,
            b')' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The `("name", …)` argument names in a `&[…]` list, in order.
///
/// Tolerates a rustfmt-broken pair, where the name sits on its own line
/// below the opening bracket. Names picked up from nested calls are
/// harmless: a call is only ever faulted for a name it does *not* supply.
fn argument_names(list: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = list;
    while let Some(at) = rest.find('(') {
        let after = rest[at + 1..].trim_start();
        rest = &rest[at + 1..];
        let Some(quoted) = after.strip_prefix('"') else {
            continue;
        };
        if let Some(end) = quoted.find('"') {
            names.push(quoted[..end].to_owned());
        }
    }
    names
}

fn calls_in(path: &Path) -> Vec<Call> {
    let text = fs::read_to_string(path).expect("source file readable");
    let file = path
        .to_string_lossy()
        .replace('\\', "/")
        .rsplit("crates/")
        .next()
        .unwrap_or_default()
        .to_owned();

    let mut calls = Vec::new();
    for method in [".text(", ".format(", ".format_plural("] {
        let mut from = 0usize;
        while let Some(at) = text[from..].find(method) {
            let open = from + at + method.len() - 1;
            from = open + 1;
            let Some(list) = argument_list(&text, open) else {
                continue;
            };
            // Only literal keys can be checked; a computed key is checked
            // by the tests that own the enum it comes from.
            let trimmed = list.trim_start().trim_start_matches('&');
            if !trimmed.starts_with('"') {
                continue;
            }
            let Some(end) = trimmed[1..].find('"') else {
                continue;
            };
            let key = trimmed[1..1 + end].to_owned();
            let line = text[..open].matches('\n').count() + 1;
            calls.push(Call {
                file: file.clone(),
                line,
                key,
                args: argument_names(&trimmed[1 + end..]),
                plural: method == ".format_plural(",
            });
        }
    }
    calls
}

fn calls_under(crate_src: &Path) -> Vec<Call> {
    let mut files = Vec::new();
    source_files(crate_src, &mut files);
    files.sort();
    files.iter().flat_map(|path| calls_in(path)).collect()
}

fn client_src() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn sim_src() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../aeon_sim/src")
}

/// The definitions crate names keys too — the enums whose variants the
/// player reads, like `RiskTag` and `ObligationKind`.
fn data_src() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../aeon_data/src")
}

fn client_calls() -> Vec<Call> {
    calls_under(&client_src())
}

#[test]
fn every_key_the_interface_names_is_in_the_table() {
    let strings = TextDb::embedded();
    let mut missing = Vec::new();
    for call in client_calls() {
        let present = if call.plural {
            strings.0.get(&format!("{}.one", call.key)).is_some()
                && strings.0.get(&format!("{}.other", call.key)).is_some()
        } else {
            strings.0.get(&call.key).is_some()
        };
        if !present {
            missing.push(format!("{}:{} '{}'", call.file, call.line, call.key));
        }
    }
    assert!(
        missing.is_empty(),
        "these keys have no row in assets/text/strings.csv:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn every_call_supplies_the_placeholders_its_row_asks_for() {
    let strings = TextDb::embedded();
    let mut wrong = Vec::new();
    for call in client_calls() {
        // A plural call fills the same names in both forms; checking the
        // "other" form covers the one that carries the count.
        let key = if call.plural {
            format!("{}.other", call.key)
        } else {
            call.key.clone()
        };
        let Some(english) = strings.0.get(&key) else {
            continue; // reported by the test above
        };
        let wanted: BTreeSet<String> = placeholders_in(english).into_iter().collect();
        let given: BTreeSet<String> = call.args.iter().cloned().collect();
        if !wanted.is_subset(&given) {
            let short: Vec<&str> = wanted.difference(&given).map(String::as_str).collect();
            wrong.push(format!(
                "{}:{} '{}' does not supply {:?}",
                call.file, call.line, call.key, short
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "these calls are missing placeholders their rows name:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn no_row_in_the_table_goes_unread() {
    // A row nothing asks for is prose that will never reach a player —
    // usually the residue of a deleted definition or a rewritten panel.
    // Three things read the table: the client's own call sites, the
    // simulation's, and the authored content, which derives its keys from
    // definition IDs.
    let strings = TextDb::embedded();
    let mut named = Named::default();
    named.absorb(key_shaped_literals(&client_src()));
    named.absorb(key_shaped_literals(&sim_src()));
    named.absorb(key_shaped_literals(&data_src()));
    named.exact.extend(aeon_data::text_keys(&shipped_content()));
    named.exact.extend(message_keys_in_content());

    let orphans: Vec<&str> = strings
        .0
        .keys()
        .map(|key| key.as_str())
        .filter(|key| !named.reads(key))
        .collect();
    assert!(
        orphans.is_empty(),
        "these rows are in assets/text/strings.csv but nothing reads them:\n  {}",
        orphans.join("\n  ")
    );
}

/// Every literal in the source that is shaped like a `ui.` or `sim.` key.
///
/// Deliberately looser than the call scan: a key is often written into a
/// `match` arm and resolved somewhere else entirely — `LogChannel::
/// label_key`, the map-mode stems, the legend closure — so asking only
/// about literals passed directly to a lookup would call most of the table
/// unread. Anything key-shaped counts as naming it.
/// A key literal is exact; a `format!` template names a whole family.
#[derive(Default)]
struct Named {
    exact: BTreeSet<String>,
    families: BTreeSet<String>,
}

impl Named {
    fn absorb(&mut self, other: Named) {
        self.exact.extend(other.exact);
        self.families.extend(other.families);
    }

    fn reads(&self, key: &str) -> bool {
        // A pluralised row is named by its stem; the .one/.other suffix is
        // added by the lookup, so neither appears in the source.
        let stem = key.rsplit_once('.').map(|(stem, _)| stem);
        self.exact.contains(key)
            || stem.is_some_and(|stem| self.exact.contains(stem))
            || self
                .families
                .iter()
                .any(|prefix| key.starts_with(prefix.as_str()))
    }
}

fn key_shaped_literals(dir: &Path) -> Named {
    let mut found = Named::default();
    let mut files = Vec::new();
    source_files(dir, &mut files);
    for path in files {
        let text = fs::read_to_string(&path).expect("source file readable");
        for piece in text.split('"').skip(1).step_by(2) {
            if !(piece.starts_with("ui.") || piece.starts_with("sim.")) {
                continue;
            }
            // `format!("ui.map-mode.{}.label", stem)` names every key under
            // `ui.map-mode.`; the stem itself is only known at runtime.
            if let Some(open) = piece.find('{') {
                let prefix = &piece[..open];
                if prefix.len() > 4 {
                    found.families.insert(prefix.to_owned());
                }
                continue;
            }
            let well_formed = piece
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-');
            if well_formed && !piece.ends_with('.') {
                found.exact.insert(piece.to_owned());
            }
        }
    }
    found
}

/// The rows named by `message_key` inside authored script functions.
///
/// A log effect is returned from inside a function body, so its key is
/// written in the script rather than derived from a definition ID — which
/// puts it beyond both the Rust scan and [`aeon_data::text_keys`].
fn message_keys_in_content() -> BTreeSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("content directory readable");
    let mut found = BTreeSet::new();
    for source in sources {
        for piece in source.source.split("message_key:").skip(1) {
            if let Some(open) = piece.find('"')
                && let Some(close) = piece[open + 1..].find('"')
            {
                found.insert(piece[open + 1..open + 1 + close].to_owned());
            }
        }
    }
    found
}

/// The shipped content, loaded against the shipped table.
fn shipped_content() -> aeon_data::ContentSet {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/content");
    let sources = aeon_data::fs::read_content_dir(&root).expect("content directory readable");
    let strings = TextDb::embedded();
    let (set, report) = aeon_data::load_content(&sources, &strings.0);
    set.unwrap_or_else(|| panic!("shipped content loads: {:?}", report.findings))
}

#[test]
fn the_table_reports_how_much_prose_is_still_unapproved() {
    // Not an assertion about the number — it only has to be readable. The
    // point is that the count exists and shrinks as lines are approved.
    let strings = TextDb::embedded();
    let total = strings.0.len();
    let bracketed = strings.0.placeholder_count();
    assert!(total > 0, "the shipped table has rows");
    assert!(bracketed <= total);
}
