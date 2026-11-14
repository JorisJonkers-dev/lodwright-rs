// SPDX-License-Identifier: MPL-2.0
//! A failed command: a stable machine-readable code and a reason for people.

use std::fmt;

/// Why a command failed. `code` is what callers match on; `reason` is prose and never contains a path taken
/// from the data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// Stable identifier in `snake_case`.
    pub code: &'static str,
    /// Prose for people.
    pub reason: String,
}

impl Failure {
    /// A failure with `code` and `reason`.
    pub fn new(code: &'static str, reason: impl Into<String>) -> Self {
        Self {
            code,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}
