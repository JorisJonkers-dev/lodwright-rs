// SPDX-License-Identifier: MPL-2.0
//! The loose-over-archive resolver: given a listing of what is on disk, which source supplies a path.
//!
//! The resolver is a pure function of its inputs (no file system access), so its rules are testable with plain
//! strings. Rules, from `fnv-archive-loading` (guide text, unverified): loose files override archive contents; an
//! archive loads with the plugin that shares its name; the archives of one plugin load alphanumerically; with an
//! empty `.override` file next to it an archive can replace files of earlier archives.
//!
//! Two points the source leaves open, both decided here and listed in `docs/design.md`:
//!
//! * "Shares its name": an archive belongs to a plugin when its file stem equals the plugin's stem or continues it
//!   with ` - ` (for example `Plugin - Textures.bsa`). Archives that belong to no listed plugin load first.
//! * Conflicts between archives: [`ConflictRule::LaterWins`] (the default) lets a later archive replace an earlier
//!   one; [`ConflictRule::OverrideMarker`] reads the guide literally: a later archive replaces only when it has
//!   the `.override` marker, otherwise the earlier file stays.

use std::collections::HashMap;

use crate::bsa::Archive;
use crate::path::normalise;

/// An archive as the resolver sees it: a name, the paths inside, and whether `<name>.override` exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveListing {
    /// File name, for example `Fallout - Textures.bsa`.
    pub name: String,
    /// Paths inside the archive, in any case and either separator.
    pub paths: Vec<String>,
    /// Whether an empty `<name>.override` file sits next to the archive.
    pub has_override_marker: bool,
}

impl ArchiveListing {
    /// The listing of an opened archive.
    #[must_use]
    pub fn from_archive(name: &str, archive: &Archive, has_override_marker: bool) -> Self {
        Self {
            name: name.to_owned(),
            paths: archive
                .entries()
                .iter()
                .map(|e| e.path().to_owned())
                .collect(),
            has_override_marker,
        }
    }
}

/// What is on disk: loose files (paths relative to the data folder) and archives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// Loose file paths relative to the data folder.
    pub loose: Vec<String>,
    /// Archives, in any order.
    pub archives: Vec<ArchiveListing>,
}

/// How archives that hold the same path are ordered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConflictRule {
    /// A later archive replaces an earlier one.
    #[default]
    LaterWins,
    /// A later archive replaces an earlier one only when it has the `.override` marker.
    OverrideMarker,
}

/// Where a path resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// A loose file, as listed (original spelling).
    Loose(String),
    /// A file inside the archive at this index of [`Listing::archives`].
    Archive {
        /// Index into the listing's archives.
        archive: usize,
        /// The path inside the archive, as listed.
        path: String,
    },
}

/// Resolves paths against a [`Listing`].
#[derive(Debug, Clone, Default)]
pub struct Resolver {
    winners: HashMap<String, Location>,
    load_order: Vec<usize>,
}

fn stem(file_name: &str) -> &str {
    file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
}

fn belongs_to(archive: &str, plugin: &str) -> bool {
    let archive = stem(archive).to_ascii_lowercase();
    let plugin = stem(plugin).to_ascii_lowercase();
    archive == plugin || archive.starts_with(&format!("{plugin} - "))
}

impl Resolver {
    /// Build a resolver. `plugins` are the plugin file names in load order.
    #[must_use]
    pub fn new(listing: &Listing, plugins: &[String], rule: ConflictRule) -> Self {
        // source: fnv-archive-loading
        let mut order: Vec<usize> = Vec::new();
        let mut placed = vec![false; listing.archives.len()];
        let by_name = |a: &usize, b: &usize| {
            let key = |i: &usize| listing.archives[*i].name.to_ascii_lowercase();
            key(a).cmp(&key(b))
        };
        let mut unowned: Vec<usize> = (0..listing.archives.len())
            .filter(|i| {
                !plugins
                    .iter()
                    .any(|p| belongs_to(&listing.archives[*i].name, p))
            })
            .collect();
        unowned.sort_by(by_name);
        for i in unowned {
            placed[i] = true;
            order.push(i);
        }
        for plugin in plugins {
            let mut owned: Vec<usize> = (0..listing.archives.len())
                .filter(|i| !placed[*i] && belongs_to(&listing.archives[*i].name, plugin))
                .collect();
            owned.sort_by(by_name);
            for i in owned {
                placed[i] = true;
                order.push(i);
            }
        }

        let mut winners: HashMap<String, Location> = HashMap::new();
        for &index in &order {
            let archive = &listing.archives[index];
            let replaces = rule == ConflictRule::LaterWins || archive.has_override_marker;
            for path in &archive.paths {
                let key = normalise(path);
                if replaces || !winners.contains_key(&key) {
                    winners.insert(
                        key,
                        Location::Archive {
                            archive: index,
                            path: path.clone(),
                        },
                    );
                }
            }
        }
        // source: fnv-archive-loading
        let mut loose: Vec<&String> = listing.loose.iter().collect();
        loose.sort();
        for path in loose.into_iter().rev() {
            // reverse sorted insertion: the first spelling in sorted order is the one kept
            winners.insert(normalise(path), Location::Loose(path.clone()));
        }
        Self {
            winners,
            load_order: order,
        }
    }

    /// The source that supplies `path` (case-insensitive, either separator), if any.
    #[must_use]
    pub fn resolve(&self, path: &str) -> Option<&Location> {
        self.winners.get(&normalise(path))
    }

    /// Indexes of the listing's archives in the order they load (earliest first).
    #[must_use]
    pub fn archive_load_order(&self) -> &[usize] {
        &self.load_order
    }
}

#[cfg(test)]
mod tests {
    use super::{ArchiveListing, ConflictRule, Listing, Location, Resolver};

    fn archive(name: &str, paths: &[&str], marker: bool) -> ArchiveListing {
        ArchiveListing {
            name: name.to_owned(),
            paths: paths.iter().map(|p| (*p).to_owned()).collect(),
            has_override_marker: marker,
        }
    }

    fn plugins(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn archive_index(resolver: &Resolver, path: &str) -> usize {
        match resolver.resolve(path) {
            Some(Location::Archive { archive, .. }) => *archive,
            other => panic!("expected an archive, got {other:?}"),
        }
    }

    #[test]
    fn a_loose_file_beats_an_archive() {
        let listing = Listing {
            loose: vec!["textures/a.dds".into()],
            archives: vec![archive("A.bsa", &["textures\\a.dds"], false)],
        };
        let resolver = Resolver::new(&listing, &plugins(&["A.esm"]), ConflictRule::LaterWins);
        assert_eq!(
            resolver.resolve("Textures\\A.DDS"),
            Some(&Location::Loose("textures/a.dds".into()))
        );
    }

    #[test]
    fn a_later_plugins_archive_beats_an_earlier_one() {
        let listing = Listing {
            loose: vec![],
            archives: vec![
                archive("B.bsa", &["x\\f.nif"], false),
                archive("A.bsa", &["x\\f.nif", "x\\only_a.nif"], false),
            ],
        };
        let resolver = Resolver::new(
            &listing,
            &plugins(&["A.esm", "B.esp"]),
            ConflictRule::LaterWins,
        );
        assert_eq!(archive_index(&resolver, "x\\f.nif"), 0);
        assert_eq!(archive_index(&resolver, "x\\only_a.nif"), 1);
        assert_eq!(resolver.archive_load_order(), [1, 0]);
    }

    #[test]
    fn archives_of_one_plugin_load_alphanumerically() {
        let listing = Listing {
            loose: vec![],
            archives: vec![
                archive("A - Textures.bsa", &["t\\f.dds"], false),
                archive("A - Meshes.bsa", &["t\\f.dds"], false),
                archive("a.bsa", &["t\\f.dds"], false),
            ],
        };
        let resolver = Resolver::new(&listing, &plugins(&["A.esm"]), ConflictRule::LaterWins);
        // "a - meshes" < "a - textures" < "a"? compare whole names: "a - meshes.bsa", "a - textures.bsa", "a.bsa"
        assert_eq!(resolver.archive_load_order(), [1, 0, 2]);
        assert_eq!(archive_index(&resolver, "t\\f.dds"), 2);
    }

    #[test]
    fn an_archive_of_no_listed_plugin_loads_before_every_plugin_archive() {
        let listing = Listing {
            loose: vec![],
            archives: vec![
                archive("A.bsa", &["p\\f"], false),
                archive("Vanilla.bsa", &["p\\f"], false),
            ],
        };
        let resolver = Resolver::new(&listing, &plugins(&["A.esm"]), ConflictRule::LaterWins);
        assert_eq!(resolver.archive_load_order(), [1, 0]);
        assert_eq!(archive_index(&resolver, "p\\f"), 0);
    }

    #[test]
    fn without_the_marker_the_literal_rule_keeps_the_earlier_archive() {
        let listing = Listing {
            loose: vec![],
            archives: vec![
                archive("A.bsa", &["p\\f"], false),
                archive("B.bsa", &["p\\f", "p\\g"], false),
            ],
        };
        let resolver = Resolver::new(
            &listing,
            &plugins(&["A.esm", "B.esm"]),
            ConflictRule::OverrideMarker,
        );
        assert_eq!(archive_index(&resolver, "p\\f"), 0);
        assert_eq!(archive_index(&resolver, "p\\g"), 1);
    }

    #[test]
    fn with_the_marker_the_literal_rule_lets_a_later_archive_replace() {
        let listing = Listing {
            loose: vec![],
            archives: vec![
                archive("A.bsa", &["p\\f"], false),
                archive("B.bsa", &["p\\f"], true),
            ],
        };
        let resolver = Resolver::new(
            &listing,
            &plugins(&["A.esm", "B.esm"]),
            ConflictRule::OverrideMarker,
        );
        assert_eq!(archive_index(&resolver, "p\\f"), 1);
    }

    #[test]
    fn lookup_ignores_case_and_separators_and_misses_return_none() {
        let listing = Listing {
            loose: vec![],
            archives: vec![archive("A.bsa", &["Meshes\\Rock.NIF"], false)],
        };
        let resolver = Resolver::new(&listing, &plugins(&["A.esm"]), ConflictRule::default());
        assert!(resolver.resolve("meshes/rock.nif").is_some());
        assert!(resolver.resolve("meshes/other.nif").is_none());
    }

    #[test]
    fn loose_spellings_that_collide_resolve_deterministically() {
        let listing = Listing {
            loose: vec!["B/x.dds".into(), "a/X.dds".into(), "A/x.dds".into()],
            archives: vec![],
        };
        let resolver = Resolver::new(&listing, &[], ConflictRule::default());
        assert_eq!(
            resolver.resolve("a/x.dds"),
            Some(&Location::Loose("A/x.dds".into()))
        );
    }

    #[test]
    fn an_empty_listing_resolves_nothing() {
        let resolver = Resolver::new(&Listing::default(), &[], ConflictRule::default());
        assert!(resolver.resolve("a").is_none());
        assert_eq!(resolver.archive_load_order(), []);
    }

    #[test]
    fn listings_come_from_opened_archives() {
        use lodwright_testkit::bsa::BsaBuilder;
        let bytes = BsaBuilder::new().file("A\\B.dds", &[1]).build();
        let archive = crate::Archive::open(&mut std::io::Cursor::new(&bytes)).unwrap();
        let listing = ArchiveListing::from_archive("X.bsa", &archive, true);
        assert_eq!(listing.paths, ["a\\b.dds"]);
        assert!(listing.has_override_marker);
    }
}
