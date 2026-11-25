// SPDX-License-Identifier: GPL-3.0-only
//! The error of a call: a stable code, a summary for people and an optional technical detail.

use std::fmt;

use serde::Serialize;

/// Why a call failed. `code` is what callers match on; `summary` is prose and never contains a path taken from the
/// data folder or a command line; `detail` carries the technical cause when there is one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Error {
    /// Stable identifier in `snake_case`, for example `plugin_not_found`.
    pub code: &'static str,
    /// One sentence for people.
    pub summary: String,
    /// The technical cause (an operating system or format error), when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Error {
    /// An error with `code` and `summary` and no detail.
    pub fn new(code: &'static str, summary: impl Into<String>) -> Self {
        Self {
            code,
            summary: summary.into(),
            detail: None,
        }
    }

    /// The same error with `detail`.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The error as one block of text to paste into a bug report: the library and its version, the code, the
    /// summary and the detail. The home folder is shown as `~`.
    #[must_use]
    pub fn copy_text(&self) -> String {
        self.copy_text_at(home().as_deref())
    }

    /// [`Error::copy_text`] with the home folder given, so the replacement does not depend on the environment.
    fn copy_text_at(&self, home: Option<&str>) -> String {
        let mut text = format!(
            "lodwright {} (api {})\nerror: {}\n{}",
            env!("CARGO_PKG_VERSION"),
            crate::API_VERSION,
            self.code,
            self.summary
        );
        if let Some(detail) = &self.detail {
            text.push_str("\ndetail: ");
            text.push_str(detail);
        }
        match home {
            Some(home) => tilde(&text, home),
            None => text,
        }
    }
}

/// `text` with `home` written as `~` where it starts a path and ends at a path boundary: `/users/player` in
/// `/users/player/out` is replaced, in `/users/player2/out` it is not.
fn tilde(text: &str, home: &str) -> String {
    let path_char =
        |c: char| c.is_alphanumeric() || matches!(c, '/' | '\\' | '.' | '_' | '-' | '~');
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(home) {
        out.push_str(&rest[..at]);
        rest = &rest[at + home.len()..];
        let starts = !out.chars().next_back().is_some_and(path_char);
        let ends = rest
            .chars()
            .next()
            .is_none_or(|c| c == '/' || c == '\\' || !path_char(c));
        out.push_str(if starts && ends { "~" } else { home });
    }
    out.push_str(rest);
    out
}

/// The home folder, when it is long enough that replacing it cannot hit unrelated text.
fn home() -> Option<String> {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|h| h.trim_end_matches(['/', '\\']).to_owned())
        .find(|h| h.len() > 1)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary)
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::Error;
    use lodwright_testkit::{covers, ids};

    #[test]
    fn the_copy_text_names_the_version_the_code_the_summary_and_the_detail() {
        covers(&[ids::generate::run::TYPED_ERRORS]);
        let text = Error::new("output_failed", "cannot write the manifest")
            .with_detail("disk full")
            .copy_text();
        let want = format!(
            "lodwright {} (api {})\nerror: output_failed\ncannot write the manifest\ndetail: disk full",
            env!("CARGO_PKG_VERSION"),
            crate::API_VERSION
        );
        assert_eq!(text, want);
    }

    #[test]
    fn the_copy_text_shows_the_home_folder_as_a_tilde() {
        covers(&[ids::generate::run::TYPED_ERRORS]);
        let home = "/users/player";
        let text = Error::new("output_failed", "cannot write")
            .with_detail(format!("{home}/out/x.nif"))
            .copy_text_at(Some(home));
        assert!(text.ends_with("detail: ~/out/x.nif"), "{text}");
        assert!(!text.contains(home));
    }

    #[test]
    fn only_a_path_that_starts_at_the_home_folder_is_rewritten() {
        let home = "/users/player";
        let text = Error::new("output_failed", "cannot write /users/player2/x")
            .with_detail(format!("{home}/out and /srv{home}/y and {home}"))
            .copy_text_at(Some(home));
        assert!(text.contains("cannot write /users/player2/x"), "{text}");
        assert!(
            text.ends_with("detail: ~/out and /srv/users/player/y and ~"),
            "{text}"
        );
    }

    #[test]
    fn display_is_the_summary_only() {
        let e = Error::new("no_worlds", "no worlds").with_detail("x");
        assert_eq!(e.to_string(), "no worlds");
    }
}
