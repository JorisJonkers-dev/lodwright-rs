// SPDX-License-Identifier: GPL-3.0-only
//! Which cells of a world the plugins outside the game's own masters changed.
//!
//! The game ships far-land diffuse tiles for its own worlds. A tile is still right for a load order as long as nothing
//! outside the game's own masters changed the land under it; where something did, the tile has to be composed again.
//! "Changed" is judged against the same world built from the game's own masters alone: a cell counts as changed when
//! its `LAND` record exists on one side only, or its heights, vertex colours or texture layers differ, or a land texture
//! its layers name resolves to another file.

use std::collections::BTreeSet;

use crate::Error;
use crate::plugin::Plugin;
use crate::texture::TextureResolver;
use crate::worldspace::{Inheritance, WorldIndex, Worldspace};

/// The plugins of the games themselves (Fallout: New Vegas and its add-ons, Fallout 3 and its add-ons, and the two
/// plugins of the Tale of Two Wastelands that carry no land of their own are not among them: see the list below).
const GAME_MASTERS: &[&str] = &[
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "CaravanPack.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "Fallout3.esm",
    "Anchorage.esm",
    "ThePitt.esm",
    "BrokenSteel.esm",
    "PointLookout.esm",
    "Zeta.esm",
];

/// Whether `plugin` (a file name) is one of the games' own masters.
#[must_use]
pub fn is_game_master(plugin: &str) -> bool {
    GAME_MASTERS.iter().any(|m| m.eq_ignore_ascii_case(plugin))
}

/// The cells of `world` (assembled from `plugins`) whose land differs from the same world built from the plugins for
/// which `is_base` is true. Every cell is changed when the base plugins do not have the world at all.
///
/// The other plugins stay in the load order as empty shells, so the form ids of both builds agree.
///
/// # Errors
/// [`Error::MissingMaster`] as the load order assembly gives it.
pub fn changed_cells(
    plugins: &[(String, Plugin)],
    world: &Worldspace,
    resolver: &TextureResolver,
    is_base: &dyn Fn(&str) -> bool,
) -> Result<BTreeSet<(i32, i32)>, Error> {
    let shells: Vec<(String, Plugin)> = plugins
        .iter()
        .map(|(name, plugin)| {
            let mut plugin = plugin.clone();
            if !is_base(name) {
                plugin.worlds.clear();
                plugin.land_textures.clear();
                plugin.texture_sets.clear();
                plugin.objects.clear();
            }
            (name.clone(), plugin)
        })
        .collect();
    let base_resolver = TextureResolver::from_load_order(&shells)?;
    let index = WorldIndex::new(&shells)?;
    let Some(base) = index.build(&shells, world.form_id(), Inheritance::None) else {
        return Ok(world.land_cells().collect());
    };
    let path_of = |r: &TextureResolver, id: u32| r.diffuse_path(id).map(str::to_ascii_lowercase);
    let mut changed = BTreeSet::new();
    let keys: BTreeSet<(i32, i32)> = world.land_cells().chain(base.land_cells()).collect();
    for (x, y) in keys {
        let same_land =
            world.cell(x, y) == base.cell(x, y) && world.layers(x, y) == base.layers(x, y);
        let same_textures = world.layers(x, y).is_none_or(|layers| {
            layers
                .base
                .iter()
                .flatten()
                .copied()
                .chain(layers.alpha.iter().map(|l| l.texture))
                .all(|id| path_of(resolver, id) == path_of(&base_resolver, id))
        });
        if !(same_land && same_textures) {
            changed.insert((x, y));
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::{changed_cells, is_game_master};
    use crate::land::CellHeights;
    use crate::layers::CellLayers;
    use crate::plugin::{LandCell, Plugin, World};
    use crate::texture::{LandTexture, TextureResolver};
    use crate::worldspace::{Inheritance, WorldIndex};

    fn cell(x: i32, y: i32, height: f32) -> LandCell {
        LandCell {
            x,
            y,
            heights: CellHeights::flat(height),
            layers: Some(CellLayers::default()),
            has_water: false,
            water_height: None,
        }
    }

    fn plugin(masters: &[&str], cells: Vec<LandCell>) -> Plugin {
        Plugin {
            masters: masters.iter().map(|m| (*m).to_owned()).collect(),
            worlds: vec![World {
                form_id: 0x3C,
                editor_id: Some("Wasteland".to_owned()),
                record: None,
                land_coords: cells.iter().map(|c| (c.x, c.y)).collect(),
                cell_coords: cells.iter().map(|c| (c.x, c.y)).collect(),
                cells,
                refs: Vec::new(),
                deleted_refs: Vec::new(),
            }],
            land_textures: Vec::<LandTexture>::new(),
            texture_sets: Vec::new(),
            objects: Vec::new(),
        }
    }

    fn run(mod_cells: Vec<LandCell>) -> Vec<(i32, i32)> {
        let plugins = vec![
            (
                "Game.esm".to_owned(),
                plugin(&[], vec![cell(0, 0, 1.0), cell(1, 0, 1.0)]),
            ),
            ("Mod.esp".to_owned(), plugin(&["Game.esm"], mod_cells)),
        ];
        let world = WorldIndex::new(&plugins)
            .unwrap()
            .build(&plugins, 0x3C, Inheritance::None)
            .unwrap();
        let resolver = TextureResolver::from_load_order(&plugins).unwrap();
        changed_cells(&plugins, &world, &resolver, &|n| n == "Game.esm")
            .unwrap()
            .into_iter()
            .collect()
    }

    #[test]
    fn the_games_own_plugins_are_recognised_by_name_in_any_case() {
        assert!(is_game_master("falloutnv.esm") && is_game_master("Fallout3.ESM"));
        assert!(!is_game_master("TaleOfTwoWastelands.esm") && !is_game_master("Mod.esp"));
    }

    #[test]
    fn a_load_order_without_a_land_change_has_no_changed_cells() {
        assert_eq!(run(Vec::new()), Vec::<(i32, i32)>::new());
        // a mod that writes the same land again changes nothing
        assert_eq!(run(vec![cell(0, 0, 1.0)]), Vec::<(i32, i32)>::new());
    }

    #[test]
    fn changed_heights_and_new_land_records_change_their_cells_only() {
        assert_eq!(run(vec![cell(1, 0, 5.0)]), vec![(1, 0)]);
        assert_eq!(run(vec![cell(2, 0, 1.0)]), vec![(2, 0)]);
    }

    #[test]
    fn a_world_the_games_do_not_have_is_changed_everywhere() {
        let plugins = vec![
            (
                "Game.esm".to_owned(),
                Plugin {
                    worlds: Vec::new(),
                    ..plugin(&[], Vec::new())
                },
            ),
            (
                "Mod.esp".to_owned(),
                plugin(&["Game.esm"], vec![cell(0, 0, 1.0), cell(3, 3, 1.0)]),
            ),
        ];
        let world = WorldIndex::new(&plugins)
            .unwrap()
            .build(&plugins, 0x3C, Inheritance::None)
            .unwrap();
        let resolver = TextureResolver::from_load_order(&plugins).unwrap();
        let all = changed_cells(&plugins, &world, &resolver, &|n| n == "Game.esm").unwrap();
        assert_eq!(all.into_iter().collect::<Vec<_>>(), vec![(0, 0), (3, 3)]);
    }
}
