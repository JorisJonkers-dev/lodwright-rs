// SPDX-License-Identifier: MPL-2.0
//! Finding and reading the plugins of a load order in a data folder.

use std::path::{Path, PathBuf};

use lodwright_core::plugin;
use lodwright_core::worldspace::Worldspace;

/// Plugin names from a load-order text file: one per line, blank lines and `#` comments skipped, a leading `*`
/// (the active marker of a plugins list) removed.
pub fn parse_load_order(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.trim_start_matches('*').trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// The file of plugin `name` in `data`, matched without regard to case (a Windows game folder on Linux often
/// differs in case from the names in a load order).
pub fn find_plugin(data: &Path, name: &str) -> Result<PathBuf, String> {
    if name.contains(['/', '\\']) || name == ".." || name == "." {
        return Err(format!("plugin name {name:?} must be a file name"));
    }
    let entries =
        std::fs::read_dir(data).map_err(|e| format!("cannot read the data folder: {e}"))?;
    let mut found = None;
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
        {
            found = Some(entry.path());
            // an exact-case match wins over a case-insensitive one
            if entry.file_name().to_string_lossy() == name {
                break;
            }
        }
    }
    found.ok_or_else(|| format!("plugin {name} not found in the data folder"))
}

/// Read every plugin of the load order (in order) and assemble the worldspaces.
pub fn load_worlds(data: &Path, names: &[String]) -> Result<Vec<Worldspace>, String> {
    if names.is_empty() {
        return Err("no plugins given: use --plugin NAME (repeatable) or --load-order FILE".into());
    }
    let mut plugins = Vec::with_capacity(names.len());
    for name in names {
        let path = find_plugin(data, name)?;
        let bytes = std::fs::read(&path).map_err(|e| format!("cannot read plugin {name}: {e}"))?;
        let parsed = plugin::read(&bytes).map_err(|e| format!("plugin {name}: {e}"))?;
        plugins.push((name.clone(), parsed));
    }
    Worldspace::from_load_order(&plugins).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{find_plugin, load_worlds, parse_load_order};
    use lodwright_testkit::land::Heights;
    use lodwright_testkit::world::{SyntheticWorld, world_plugin};

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lodwright-load-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_load_order_file_skips_comments_blanks_and_active_markers() {
        let text = "# plugins\n\n*FalloutNV.esm\n  *DeadMoney.esm  \nMyMod.esp\n*\n";
        assert_eq!(
            parse_load_order(text),
            ["FalloutNV.esm", "DeadMoney.esm", "MyMod.esp"]
        );
    }

    #[test]
    fn plugins_are_found_without_regard_to_case() {
        let dir = temp("case");
        std::fs::write(dir.join("FALLOUTNV.ESM"), b"x").unwrap();
        assert_eq!(
            find_plugin(&dir, "FalloutNV.esm").unwrap(),
            dir.join("FALLOUTNV.ESM")
        );
    }

    #[test]
    fn an_exact_case_match_wins() {
        let dir = temp("exact");
        std::fs::write(dir.join("a.esm"), b"x").unwrap();
        std::fs::write(dir.join("A.ESM"), b"x").unwrap();
        assert_eq!(find_plugin(&dir, "a.esm").unwrap(), dir.join("a.esm"));
    }

    #[test]
    fn a_missing_plugin_is_named_and_the_path_is_not_echoed() {
        let dir = temp("missing");
        let err = find_plugin(&dir, "Nope.esm").unwrap_err();
        assert_eq!(err, "plugin Nope.esm not found in the data folder");
    }

    #[test]
    fn names_with_separators_are_refused() {
        let dir = temp("sep");
        for bad in ["../x.esm", "a/b.esm", "a\\b.esm", "..", "."] {
            assert!(find_plugin(&dir, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_unreadable_data_folder_is_an_error() {
        let err = find_plugin(std::path::Path::new("/definitely/not/here"), "a.esm").unwrap_err();
        assert!(err.starts_with("cannot read the data folder"));
    }

    #[test]
    fn worlds_load_through_a_master_and_an_override() {
        let dir = temp("load");
        let base = SyntheticWorld::new(0xA00, "Mojave").cell(0, 0, Heights::flat(8.0));
        let patch = SyntheticWorld::new(0xA00, "Mojave").cell(1, 0, Heights::flat(16.0));
        std::fs::write(dir.join("Base.esm"), world_plugin(&[], &base, false)).unwrap();
        std::fs::write(
            dir.join("patch.esp"),
            world_plugin(&["Base.esm"], &patch, true),
        )
        .unwrap();
        let worlds = load_worlds(&dir, &["Base.esm".into(), "Patch.esp".into()]).unwrap();
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].cell_count(), 2);
    }

    #[test]
    fn errors_name_the_plugin_that_failed() {
        let dir = temp("bad");
        std::fs::write(dir.join("Bad.esp"), b"not a plugin at all, just text").unwrap();
        let err = load_worlds(&dir, &["Bad.esp".into()]).unwrap_err();
        assert!(err.starts_with("plugin Bad.esp: "), "{err}");
        let err = load_worlds(&dir, &[]).unwrap_err();
        assert!(err.contains("no plugins given"));
        let world = SyntheticWorld::new(1, "W").cell(0, 0, Heights::flat(8.0));
        std::fs::write(
            dir.join("Orphan.esp"),
            world_plugin(&["Gone.esm"], &world, false),
        )
        .unwrap();
        let err = load_worlds(&dir, &["Orphan.esp".into()]).unwrap_err();
        assert_eq!(err, "master Gone.esm is not in the load order");
    }
}
