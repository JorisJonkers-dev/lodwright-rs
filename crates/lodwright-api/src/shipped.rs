// SPDX-License-Identifier: GPL-3.0-only
//! Which diffuse tiles are left to the game's own far-land tiles.
//!
//! A tile is left out when the base game's archives ship a tile of that name, the shipped settings file starts the
//! block grid where ours starts, and none of the cells under the tile has land that a plugin outside the game's own
//! masters changed (`lodwright_core::shipped`). Mods' own archives and loose files never count as shipped.

use std::collections::BTreeSet;

use lodwright_core::layout::{self, BlockId};
use lodwright_core::lod::LodSettings;
use lodwright_core::plugin::Plugin;
use lodwright_core::shipped::{changed_cells, is_game_master};
use lodwright_core::texture::TextureResolver;
use lodwright_core::worldspace::Worldspace;
use lodwright_gen::FolderAssets;

/// The decision for one world.
pub(crate) struct Keep<'a> {
    world: String,
    changed: BTreeSet<(i32, i32)>,
    assets: &'a FolderAssets,
    skip: Box<dyn Fn(&str) -> bool + Sync + 'a>,
}

impl Keep<'_> {
    /// Whether the diffuse tile of the block of `level` cells whose south-west cell is `cell` is left to the game.
    pub(crate) fn keeps(&self, level: u32, cell: (i32, i32)) -> bool {
        let n = i32::try_from(level).unwrap_or(0);
        if n == 0 {
            return false;
        }
        let untouched = (cell.0..cell.0 + n)
            .all(|x| (cell.1..cell.1 + n).all(|y| !self.changed.contains(&(x, y))));
        untouched
            && layout::diffuse_path(
                &self.world,
                BlockId {
                    level,
                    x: cell.0,
                    y: cell.1,
                },
            )
            .is_ok_and(|path| self.assets.shipped(&path, &*self.skip))
    }
}

/// Decide for `world`. `None` (with a warning) when the shipped settings file does not start the grid where ours
/// starts or is missing: nothing is left out then, as a tile of the same name would cover other cells.
pub(crate) fn plan<'a>(
    world: &Worldspace,
    plugins: &[(String, Plugin)],
    resolver: &TextureResolver,
    assets: &'a FolderAssets,
    anchor: (i32, i32),
    warnings: &mut Vec<String>,
) -> Result<Option<Keep<'a>>, lodwright_core::Error> {
    // an archive belongs to a plugin outside the game's masters when its name starts with that plugin's name
    let others: Vec<String> = plugins
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| !is_game_master(name))
        .map(|name| {
            name.rsplit_once('.')
                .map_or(name, |(stem, _)| stem)
                .to_ascii_lowercase()
        })
        .collect();
    let skip = Box::new(move |archive: &str| {
        let archive = archive.to_ascii_lowercase();
        others.iter().any(|stem| archive.starts_with(stem.as_str()))
    });
    let settings = layout::settings_path(world.name())
        .ok()
        .and_then(|path| assets.shipped_read(&path, &*skip))
        .and_then(|bytes| LodSettings::parse(&bytes).ok());
    // a tile is named by the cells it covers, so the game's tile of a name covers the cells ours would when the two
    // grids start a whole number of 32 cell blocks apart (the largest level)
    let same_blocks = settings.is_some_and(|s| {
        (i32::from(s.south_west.0) - anchor.0).rem_euclid(32) == 0
            && (i32::from(s.south_west.1) - anchor.1).rem_euclid(32) == 0
    });
    if !same_blocks {
        if settings.is_some() {
            warnings.push(format!(
                "{}: the game's block grid is not ours shifted by whole blocks, so its tiles are not used",
                world.name()
            ));
        }
        return Ok(None);
    }
    let changed = changed_cells(plugins, world, resolver, &|name| is_game_master(name))?;
    Ok(Some(Keep {
        world: world.name().to_owned(),
        changed,
        assets,
        skip,
    }))
}
