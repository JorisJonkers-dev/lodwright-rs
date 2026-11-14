// SPDX-License-Identifier: MPL-2.0
//! Path normalisation shared by the archive index and the resolver.

/// The lookup key of a path: ASCII lower case, backslash separators, no leading or trailing separator.
///
/// Archive names are lower case with backslashes (`bsa-path-form`); accepting a forward slash and any case is a
/// project choice so that a loose-file listing and an archive index compare equal.
#[must_use]
pub fn normalise(path: &str) -> String {
    // source: bsa-path-form
    path.trim_matches(['/', '\\'])
        .chars()
        .map(|c| {
            if c == '/' {
                '\\'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn case_and_separators_are_folded() {
        assert_eq!(
            normalise("Textures/Landscape\\Dirt.DDS"),
            "textures\\landscape\\dirt.dds"
        );
    }

    #[test]
    fn leading_and_trailing_separators_are_dropped() {
        assert_eq!(normalise("/a/b/"), "a\\b");
    }
}
