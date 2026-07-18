//! Embedded authored content.

use std::sync::Arc;

use aeon_data::{ContentSet, ContentSource, load_content};

include!(concat!(env!("OUT_DIR"), "/embedded_content.rs"));

/// Loads the content embedded at build time.
///
/// # Panics
/// Panics if the embedded content fails validation. CI validates the same
/// files on every push, so this only fires on a broken local edit — and
/// should fire loudly.
pub fn load_embedded() -> Arc<ContentSet> {
    let sources: Vec<ContentSource> = EMBEDDED_CONTENT
        .iter()
        .map(|(path, source)| ContentSource {
            path: (*path).to_owned(),
            source: (*source).to_owned(),
        })
        .collect();
    let (set, report) = load_content(&sources);
    match set {
        Some(set) => Arc::new(set),
        None => {
            for finding in &report.findings {
                eprintln!("{finding}");
            }
            panic!("embedded content failed validation");
        }
    }
}
