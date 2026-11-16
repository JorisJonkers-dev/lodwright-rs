// SPDX-License-Identifier: MPL-2.0
//! Where the game expects LOD output: folder and file names, relative to the data folder, with forward slashes.
//!
//! Names are lower case throughout, as in the game's own archives, so a case-sensitive file system needs no
//! guessing. The strings written inside a NIF keep the world's editor-id case ([`nif_texture_paths`]).
//!
//! Derived from the rows `game-lod-terrain-layout` and `game-lod-object-layout` in `provenance/sources.toml`.

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

// source: game-lod-terrain-layout
fn stem(world: &str, id: BlockId) -> String {
    format!("{world}.level{}.x{}.y{}", id.level, id.x, id.y).to_ascii_lowercase()
}

// source: game-lod-terrain-layout
/// Path of the terrain mesh of `id`.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn mesh_path(world: &str, id: BlockId) -> Result<String, Error> {
    check_world_name(world)?;
    Ok(format!(
        "meshes/landscape/lod/{}/{}.nif",
        world.to_ascii_lowercase(),
        stem(world, id)
    ))
}

// source: game-lod-terrain-layout
fn texture_name(world: &str, id: BlockId) -> String {
    format!("{world}.n.level{}.x{}.y{}", id.level, id.x, id.y).to_ascii_lowercase()
}

// source: game-lod-terrain-layout
/// Path of the diffuse texture of `id`.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn diffuse_path(world: &str, id: BlockId) -> Result<String, Error> {
    check_world_name(world)?;
    Ok(format!(
        "textures/landscape/lod/{}/diffuse/{}.dds",
        world.to_ascii_lowercase(),
        texture_name(world, id)
    ))
}

// source: game-lod-terrain-layout
/// Path of the normal map of `id`.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn normal_path(world: &str, id: BlockId) -> Result<String, Error> {
    check_world_name(world)?;
    Ok(format!(
        "textures/landscape/lod/{}/normals/{}.dds",
        world.to_ascii_lowercase(),
        texture_name(world, id)
    ))
}

// source: game-lod-terrain-block-nif
/// The two texture paths a terrain block NIF carries, diffuse then normal, as the game's own files write them:
/// backslashes, a `Data` prefix and the world's editor-id case.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn nif_texture_paths(world: &str, id: BlockId) -> Result<[String; 2], Error> {
    check_world_name(world)?;
    let name = format!("{world}.n.Level{}.X{}.Y{}.dds", id.level, id.x, id.y);
    Ok([
        format!("Data\\Textures\\Landscape\\LOD\\{world}\\Diffuse\\{name}"),
        format!("Data\\Textures\\Landscape\\LOD\\{world}\\Normals\\{name}"),
    ])
}

// source: game-lod-object-layout
/// Path of an object LOD block mesh: level 4, at south-west cell (`x`, `y`); `high` selects the high-priority file.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn object_mesh_path(world: &str, x: i32, y: i32, high: bool) -> Result<String, Error> {
    check_world_name(world)?;
    let level = if high { "level4.high" } else { "level4" };
    Ok(format!(
        "meshes/landscape/lod/{}/blocks/{}.{level}.x{x}.y{y}.nif",
        world.to_ascii_lowercase(),
        world.to_ascii_lowercase()
    ))
}

// source: game-lod-object-layout
/// Paths of the object atlas and its normal atlas.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn atlas_paths(world: &str) -> Result<[String; 2], Error> {
    check_world_name(world)?;
    let w = world.to_ascii_lowercase();
    Ok([
        format!("textures/landscape/lod/{w}/blocks/{w}.buildings.dds"),
        format!("textures/landscape/lod/{w}/blocks/{w}.buildings_n.dds"),
    ])
}

// source: game-lod-object-block-nif
/// The two paths an object block NIF carries for the atlas and the normal atlas, as the game's own files write
/// them.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn nif_atlas_paths(world: &str) -> Result<[String; 2], Error> {
    check_world_name(world)?;
    let base = format!("Data\\Textures\\Landscape\\LOD\\{world}\\Blocks\\{world}.Buildings");
    Ok([format!("{base}.dds"), format!("{base}_n.dds")])
}

// source: game-dlodsettings-layout
/// Path of the per-world LOD settings file.
///
/// # Errors
/// [`Error::BadWorldName`] when `world` is not a usable name.
pub fn settings_path(world: &str) -> Result<String, Error> {
    check_world_name(world)?;
    Ok(format!(
        "lodsettings/{}.dlodsettings",
        world.to_ascii_lowercase()
    ))
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
            "meshes/landscape/lod/wastelandnv/wastelandnv.level4.x-8.y12.nif"
        );
    }

    #[test]
    fn texture_paths_sit_in_diffuse_and_normals_folders() {
        assert_eq!(
            diffuse_path("W", ID).unwrap(),
            "textures/landscape/lod/w/diffuse/w.n.level4.x-8.y12.dds"
        );
        assert_eq!(
            normal_path("W", ID).unwrap(),
            "textures/landscape/lod/w/normals/w.n.level4.x-8.y12.dds"
        );
    }

    #[test]
    fn the_paths_inside_a_nif_keep_the_world_case_and_use_backslashes() {
        let [d, n] = super::nif_texture_paths("WastelandNV", ID).unwrap();
        assert_eq!(
            d,
            "Data\\Textures\\Landscape\\LOD\\WastelandNV\\Diffuse\\WastelandNV.n.Level4.X-8.Y12.dds"
        );
        assert!(n.contains("\\Normals\\"));
    }

    #[test]
    fn object_paths_follow_the_games_tree() {
        assert_eq!(
            super::object_mesh_path("W", 24, -12, false).unwrap(),
            "meshes/landscape/lod/w/blocks/w.level4.x24.y-12.nif"
        );
        assert!(
            super::object_mesh_path("W", 4, 4, true)
                .unwrap()
                .contains("level4.high.x4.y4")
        );
        assert_eq!(
            super::atlas_paths("W").unwrap()[1],
            "textures/landscape/lod/w/blocks/w.buildings_n.dds"
        );
        assert_eq!(
            super::nif_atlas_paths("Mojave").unwrap()[0],
            "Data\\Textures\\Landscape\\LOD\\Mojave\\Blocks\\Mojave.Buildings.dds"
        );
    }

    #[test]
    fn the_settings_path_is_one_file_per_world() {
        assert_eq!(settings_path("W").unwrap(), "lodsettings/w.dlodsettings");
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
