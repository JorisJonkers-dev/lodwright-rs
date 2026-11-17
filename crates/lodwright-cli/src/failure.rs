// SPDX-License-Identifier: GPL-3.0-only
//! A failed command: the library's [`lodwright_api::Error`], worded for the command line.

pub use lodwright_api::Error as Failure;

/// The reason the command line prints for `failure`: the summary, the detail when there is one, and the option that
/// overrides a refusal.
pub fn reason(failure: &Failure) -> String {
    let mut text = failure.summary.clone();
    if let Some(detail) = &failure.detail {
        text.push_str(": ");
        text.push_str(detail);
    }
    match failure.code {
        "output_in_repo" => text.push_str("; pass --allow-in-repo to override"),
        "output_not_empty" => text.push_str(": use an empty folder or pass --overwrite"),
        "no_plugins" => text.push_str(": use --plugin NAME (repeatable) or --load-order FILE"),
        _ => {}
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{Failure, reason};

    #[test]
    fn the_reason_carries_the_detail_and_the_overriding_option() {
        let f = Failure::new("output_failed", "cannot write the manifest").with_detail("disk full");
        assert_eq!(reason(&f), "cannot write the manifest: disk full");
        let f = Failure::new(
            "output_in_repo",
            "refusing to write inside a git working tree",
        );
        assert_eq!(
            reason(&f),
            "refusing to write inside a git working tree; pass --allow-in-repo to override"
        );
    }
}
