// SPDX-License-Identifier: MPL-2.0
//! Golden-file helper: expectations are text files next to the tests, never binary.
//!
//! `UPDATE_GOLDEN=1 cargo test` rewrites them; the diff is then reviewed like any other change.

use std::path::Path;

/// Compare `actual` with the golden file at `path` (rewriting it when `UPDATE_GOLDEN` is set).
///
/// # Panics
/// When the file is missing or different (and not being updated).
pub fn assert_golden(path: &Path, actual: &str) {
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("create the golden directory");
        }
        std::fs::write(path, actual).expect("write the golden file");
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "golden file {} unreadable: {e} (UPDATE_GOLDEN=1 creates it)",
            path.display()
        )
    });
    assert_eq!(expected, actual, "golden file {} differs", path.display());
}

#[cfg(test)]
mod tests {
    use super::assert_golden;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("lodwright-golden-{}-{name}", std::process::id()))
    }

    #[test]
    fn a_matching_golden_file_passes() {
        let path = temp("match");
        std::fs::write(&path, "abc\n").unwrap();
        assert_golden(&path, "abc\n");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    #[should_panic(expected = "differs")]
    fn a_different_golden_file_fails() {
        let path = temp("differ");
        std::fs::write(&path, "abc\n").unwrap();
        assert_golden(&path, "abd\n");
    }

    #[test]
    #[should_panic(expected = "unreadable")]
    fn a_missing_golden_file_fails() {
        assert_golden(&temp("missing"), "x");
    }
}
