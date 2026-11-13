// SPDX-License-Identifier: MPL-2.0
//! Where the game expects LOD output: folder and file names, relative to the data folder, with forward slashes.
//!
//! Names are lower-case folders and the world's editor id verbatim; the game reads them through a
//! case-insensitive file system, and the paths here are what the generator creates on a case-sensitive one.
//!
//! Derived from: `provenance/sources.toml` rows `lod-block-naming`, `lod-terrain-paths`, `lod-settings-path`
//! (all unverified until the owner compares them with their own output tree).

use crate::Error;

/// A terrain LOD block: its level (cells per block edge) and the cell coordinates of its south-west cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockId {
    /// Cells per block edge: 4, 8, 16 or 32 in practice.
    pub level: u32,
    /// Cell x of the south-west cell, a multiple of `level`.
    pub x: i32,
    /// Cell y of the south-west cell, a multiple of `level`.
    pub y: i32,
}

/// Check that `name` is usable as one path component: not empty, letters, digits, `_`, `-`, `.` and space only,
/// no leading dot.
///
/// # Errors
/// [`Error::BadWorldName`] otherwise.
pub fn check_world_name(name: &str) -> Result<(), Error> {
    let ok = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '));
    if ok {
        Ok(())
    } else {
        Err(Error::BadWorldName(name.to_owned()))
    }
}

// source: lod-block-naming
fn stem(world: &str, id: BlockId) -> String {
    format!("{world}.{}.{}.{}", id.level, id.x, id.y)
}

// source: lod-terrain-paths
/// Path of the terrain mesh of `id`.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn mesh_path(world: &str, id: BlockId) -> Result<String, Error> {
    check_world_name(world)?;
    Ok(format!(
        "meshes/landscape/lod/{world}/blocks/{}.btr",
        stem(world, id)
    ))
}

// source: lod-terrain-paths
/// Path of the diffuse texture of `id`.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn diffuse_path(world: &str, id: BlockId) -> Result<String, Error> {
    check_world_name(world)?;
    Ok(format!(
        "textures/landscape/lod/{world}/blocks/{}.dds",
        stem(world, id)
    ))
}

// source: lod-terrain-paths
/// Path of the normal map of `id`.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn normal_path(world: &str, id: BlockId) -> Result<String, Error> {
    check_world_name(world)?;
    Ok(format!(
        "textures/landscape/lod/{world}/blocks/{}_n.dds",
        stem(world, id)
    ))
}

// source: lod-settings-path
/// Path of the per-world LOD settings file.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn settings_path(world: &str) -> Result<String, Error> {
    check_world_name(world)?;
    Ok(format!("lodsettings/{world}.lod"))
}

#[cfg(test)]
mod tests {
    use super::{BlockId, check_world_name, diffuse_path, mesh_path, normal_path, settings_path};
    use crate::Error;

    const ID: BlockId = BlockId {
        level: 4,
        x: -8,
        y: 12,
    };

    #[test]
    fn a_mesh_path_carries_world_level_and_coordinates() {
        assert_eq!(
            mesh_path("WastelandNV", ID).unwrap(),
            "meshes/landscape/lod/WastelandNV/blocks/WastelandNV.4.-8.12.btr"
        );
    }

    #[test]
    fn texture_paths_end_in_dds_and_the_normal_map_has_the_n_suffix() {
        assert_eq!(
            diffuse_path("W", ID).unwrap(),
            "textures/landscape/lod/W/blocks/W.4.-8.12.dds"
        );
        assert_eq!(
            normal_path("W", ID).unwrap(),
            "textures/landscape/lod/W/blocks/W.4.-8.12_n.dds"
        );
    }

    #[test]
    fn the_settings_path_is_one_file_per_world() {
        assert_eq!(settings_path("W").unwrap(), "lodsettings/W.lod");
    }

    #[test]
    fn names_that_could_escape_the_folder_are_refused() {
        for bad in ["", "..", ".hidden", "a/b", "a\\b", "a\0b", "../x", "ä"] {
            assert_eq!(
                check_world_name(bad),
                Err(Error::BadWorldName(bad.to_owned())),
                "{bad:?}"
            );
            assert!(mesh_path(bad, ID).is_err());
            assert!(diffuse_path(bad, ID).is_err());
            assert!(normal_path(bad, ID).is_err());
            assert!(settings_path(bad).is_err());
        }
    }

    #[test]
    fn ordinary_editor_ids_are_accepted() {
        for good in [
            "WastelandNV",
            "Lucky38World",
            "zion_01",
            "Dead-Money",
            "A B",
        ] {
            assert_eq!(check_world_name(good), Ok(()));
        }
    }
}
