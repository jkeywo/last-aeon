//! Content-loading findings.
//!
//! Loading never aborts on the first problem: every issue in an authored
//! content set is reported at once with its file, so authors fix a batch of
//! findings per run, the same workflow the PASM validator gives the
//! architecture model.

use core::fmt;

/// How serious a finding is.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Severity {
    /// Informational, e.g. captured `print` output.
    Info,
    /// Suspicious but loadable.
    Warning,
    /// The content set is unusable until fixed.
    Error,
}

/// One issue found while loading content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentFinding {
    /// Severity of the finding.
    pub severity: Severity,
    /// Content-relative path of the file involved.
    pub path: String,
    /// The definition key involved, when one is known.
    pub key: Option<String>,
    /// Human-readable description.
    pub message: String,
}

impl fmt::Display for ContentFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(f, "{severity}: {}", self.path)?;
        if let Some(key) = &self.key {
            write!(f, " [{key}]")?;
        }
        write!(f, ": {}", self.message)
    }
}

/// Everything found while loading a content set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentReport {
    /// All findings, in file order then discovery order.
    pub findings: Vec<ContentFinding>,
}

impl ContentReport {
    /// Whether any finding is an error.
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    pub(crate) fn error(&mut self, path: &str, key: Option<&str>, message: impl Into<String>) {
        self.findings.push(ContentFinding {
            severity: Severity::Error,
            path: path.to_owned(),
            key: key.map(str::to_owned),
            message: message.into(),
        });
    }

    pub(crate) fn info(&mut self, path: &str, message: impl Into<String>) {
        self.findings.push(ContentFinding {
            severity: Severity::Info,
            path: path.to_owned(),
            key: None,
            message: message.into(),
        });
    }
}
