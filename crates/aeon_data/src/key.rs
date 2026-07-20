//! Authored content identity.

use core::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A validated authored-content ID: lowercase kebab-case, like PASM entity
/// IDs.
///
/// Content keys name definitions (assignments, bodies, provinces, scenarios) and
/// authored scenario entities. They are the durable names in authored files
/// and cross-references; the simulation interns them to compact handles at
/// load.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ContentKey(String);

/// A string that is not valid kebab-case.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("content keys must be lowercase kebab-case (a-z, 0-9, '-'): '{0}'")]
pub struct InvalidContentKey(pub String);

impl ContentKey {
    /// Validates and wraps a key.
    pub fn new(value: &str) -> Result<Self, InvalidContentKey> {
        if is_kebab_case(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(InvalidContentKey(value.to_owned()))
        }
    }

    /// The key text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated display-text ID: dot-separated kebab-case segments, like
/// `ui.inspector.heading` or `province.karvessa.name`.
///
/// Text keys name rows in the string table. UI keys are written at their
/// call sites; content keys are derived from a definition's [`ContentKey`]
/// and the field being resolved, so a definition's display text cannot
/// drift from its ID.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TextKey(String);

/// A string that is not a valid dotted text key.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("text keys must be dot-separated lowercase kebab-case segments: '{0}'")]
pub struct InvalidTextKey(pub String);

impl TextKey {
    /// Validates and wraps a key.
    pub fn new(value: &str) -> Result<Self, InvalidTextKey> {
        if !value.is_empty() && value.split('.').all(is_kebab_case) {
            Ok(Self(value.to_owned()))
        } else {
            Err(InvalidTextKey(value.to_owned()))
        }
    }

    /// The key text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TextKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn is_kebab_case(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let mut previous_was_hyphen = true; // leading hyphen is invalid
    for c in value.chars() {
        match c {
            'a'..='z' | '0'..='9' => previous_was_hyphen = false,
            '-' if !previous_was_hyphen => previous_was_hyphen = true,
            _ => return false,
        }
    }
    !previous_was_hyphen // trailing hyphen is invalid
}

impl fmt::Display for ContentKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for ContentKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ContentKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        ContentKey::new(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_kebab_case() {
        for valid in ["a", "hold-court", "sector-7-tithe", "x9"] {
            assert!(ContentKey::new(valid).is_ok(), "{valid} should be valid");
        }
    }

    #[test]
    fn rejects_everything_else() {
        for invalid in [
            "",
            "-lead",
            "trail-",
            "double--hyphen",
            "Upper",
            "under_score",
            "sp ace",
        ] {
            assert!(
                ContentKey::new(invalid).is_err(),
                "{invalid} should be invalid"
            );
        }
    }

    #[test]
    fn accepts_dotted_text_keys() {
        for valid in [
            "ui",
            "ui.inspector.heading",
            "province.karvessa.name",
            "ui.map-mode.holder.hint.vassal.one",
            "assignment.hold-court.success.log-text",
        ] {
            assert!(TextKey::new(valid).is_ok(), "{valid} should be valid");
        }
    }

    #[test]
    fn rejects_malformed_text_keys() {
        for invalid in [
            "",
            ".leading",
            "trailing.",
            "double..dot",
            "ui.Inspector",
            "ui.under_score",
            "ui.sp ace",
            "ui.-lead",
        ] {
            assert!(
                TextKey::new(invalid).is_err(),
                "{invalid} should be invalid"
            );
        }
    }
}
