// SPDX-License-Identifier: GPL-3.0-only
//! Which diffuse tiles are left to the game's own far-land tiles.
//!
//! A tile is left out when the base game's archives ship a tile of that name, the shipped settings file starts the
//! block grid where ours starts, and none of the cells under the tile has land that a plugin outside the game's own
//! masters changed (`lodwright_core::shipped`). Mods' own archives and loose files never count as shipped.

use std::collections::{BTreeSet, HashMap};

use lodwright_core::layout::{self, BlockId};
use lodwright_core::lod::LodSettings;
use lodwright_core::plugin::Plugin;
use lodwright_core::shipped::{ShippedBase, is_game_archive, is_game_master};
use lodwright_core::texture::TextureResolver;
use lodwright_core::worldspace::Worldspace;
use lodwright_gen::{FolderAssets, ShippedPatch, Winner};

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

impl Keep<'_> {
    /// The mean luminance of the game's diffuse tile of the block of `level` cells at `cell`, when the game ships one.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // a mean over pixels
    pub(crate) fn mean(&self, level: u32, cell: (i32, i32)) -> Option<f32> {
        let path = layout::diffuse_path(
            &self.world,
            BlockId {
                level,
                x: cell.0,
                y: cell.1,
            },
        )
        .ok()?;
        let bytes = self.assets.shipped_read(&path, &*self.skip)?;
        let image = lodwright_dds::decode(&bytes)
            .ok()?
            .levels
            .into_iter()
            .next()?;
        let px = image.rgba().as_chunks::<4>().0;
        let sum: f64 = px
            .iter()
            .map(|p| 0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2]))
            .sum();
        Some((sum / px.len().max(1) as f64) as f32)
    }
}

impl Keep<'_> {
    /// The game's tile for the block of `level` cells at `cell` and the cells under it a mod changed, when the game
    /// ships a tile of that name and only some of its cells changed.
    pub(crate) fn patch(&self, level: u32, cell: (i32, i32)) -> Option<ShippedPatch> {
        let n = i32::try_from(level).ok().filter(|n| *n > 0)?;
        let changed: BTreeSet<(i32, i32)> = (cell.0..cell.0 + n)
            .flat_map(|x| (cell.1..cell.1 + n).map(move |y| (x, y)))
            .filter(|c| self.changed.contains(c))
            .collect();
        if changed.is_empty() || i32::try_from(changed.len()).is_ok_and(|len| len == n * n) {
            return None;
        }
        let path = layout::diffuse_path(
            &self.world,
            BlockId {
                level,
                x: cell.0,
                y: cell.1,
            },
        )
        .ok()?;
        let bytes = self.assets.shipped_read(&path, &*self.skip)?;
        let image = lodwright_dds::decode(&bytes)
            .ok()?
            .levels
            .into_iter()
            .next()?;
        Some(ShippedPatch { image, changed })
    }
}

/// Decide for `world`. `None` (with a warning) when the shipped settings file does not start the grid where ours
/// starts or is missing: nothing is left out then, as a tile of the same name would cover other cells.
pub(crate) fn plan<'a>(
    world: &Worldspace,
    base: &ShippedBase,
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
    // the packed output of an earlier run may be in the data folder too, and its stub is not among the plugins
    let own = format!("{}.bsa", crate::pack::ARCHIVE_BASE).to_ascii_lowercase();
    let skip = Box::new(move |archive: &str| {
        let archive = archive.to_ascii_lowercase();
        archive == own || others.iter().any(|stem| archive.starts_with(stem.as_str()))
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
    let mut changed = base.changed_cells(world, resolver)?;
    changed.extend(retextured_cells(world, resolver, assets));
    Ok(Some(Keep {
        world: world.name().to_owned(),
        changed,
        assets,
        skip,
    }))
}

/// The cells that use a land texture whose file does not come from a game archive: a mod replaced it with a loose
/// file or from an archive of its own, with no record to show for it, and the near land draws the new picture.
fn retextured_cells(
    world: &Worldspace,
    resolver: &TextureResolver,
    assets: &FolderAssets,
) -> BTreeSet<(i32, i32)> {
    let mut cache: HashMap<u32, bool> = HashMap::new();
    let mut replaced = |id: u32| {
        *cache.entry(id).or_insert_with(|| {
            let Some(path) = resolver.diffuse_path(id) else {
                return false;
            };
            let key = format!("textures/{}", path.replace('\\', "/"));
            match assets.winner(&key) {
                Winner::Missing => false,
                Winner::Loose => true,
                Winner::Archive(name) => !is_game_archive(&name),
            }
        })
    };
    let mut cells = BTreeSet::new();
    for (x, y) in world.land_cells() {
        let Some(layers) = world.layers(x, y) else {
            continue;
        };
        let ids = layers
            .base
            .iter()
            .flatten()
            .copied()
            .chain(layers.alpha.iter().map(|l| l.texture));
        if ids.into_iter().any(&mut replaced) {
            cells.insert((x, y));
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::plan;
    use lodwright_assets::PackEntry;
    use lodwright_core::land::CellHeights;
    use lodwright_core::layers::CellLayers;
    use lodwright_core::lod::LodSettings;
    use lodwright_core::plugin::{LandCell, Plugin, World};
    use lodwright_core::shipped::{ShippedBase, is_game_master};
    use lodwright_core::texture::TextureResolver;
    use lodwright_core::worldspace::{Inheritance, WorldIndex};
    use lodwright_gen::FolderAssets;

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
            land_textures: Vec::new(),
            texture_sets: Vec::new(),
            objects: Vec::new(),
            lights: Vec::new(),
        }
    }

    fn archive(path: &std::path::Path, entries: &[(&str, &[u8])]) {
        let entries: Vec<PackEntry> = entries
            .iter()
            .map(|(p, d)| PackEntry {
                path: (*p).to_owned(),
                data: d.to_vec(),
                compress: false,
            })
            .collect();
        std::fs::write(path, lodwright_assets::pack::write(&entries).unwrap()).unwrap();
    }

    /// A data folder with the game's archive (tiles at level 4 for the blocks x0.y0 and x4.y0, and its settings
    /// file starting at `start`) and a mod's archive that also has a tile.
    fn data_folder(tag: &str, start: i16) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lodwright-shipped-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let settings =
            LodSettings::for_bounds((start, start), (start + 7, start + 7), 8).to_bytes();
        let tile = |x: i32| {
            format!("textures\\landscape\\lod\\wasteland\\diffuse\\wasteland.n.level4.x{x}.y0.dds")
        };
        archive(
            &dir.join("Game - Textures.bsa"),
            &[
                (&tile(0), b"tile"),
                (&tile(4), b"tile"),
                ("lodsettings\\wasteland.dlodsettings", &settings),
            ],
        );
        archive(&dir.join("Mod - Main.bsa"), &[(&tile(8), b"tile")]);
        dir
    }

    fn keep_for(dir: &std::path::Path, mod_cells: Vec<LandCell>) -> (Vec<bool>, Vec<String>) {
        let plugins = vec![
            (
                "Fallout3.esm".to_owned(),
                plugin(&[], vec![cell(0, 0, 1.0), cell(4, 0, 1.0), cell(8, 0, 1.0)]),
            ),
            ("Mod.esp".to_owned(), plugin(&["Fallout3.esm"], mod_cells)),
        ];
        let world = WorldIndex::new(&plugins)
            .unwrap()
            .build(&plugins, 0x3C, Inheritance::None)
            .unwrap();
        let resolver = TextureResolver::from_load_order(&plugins).unwrap();
        let names: Vec<String> = plugins.iter().map(|(n, _)| n.clone()).collect();
        let assets = FolderAssets::open(dir, &names).unwrap();
        let mut warnings = Vec::new();
        let base = ShippedBase::new(&plugins, &|n| is_game_master(n)).unwrap();
        let keep = plan(
            &world,
            &base,
            &plugins,
            &resolver,
            &assets,
            (0, 0),
            &mut warnings,
        )
        .unwrap();
        let decisions = [0, 4, 8, 12]
            .map(|x| keep.as_ref().is_some_and(|k| k.keeps(4, (x, 0))))
            .to_vec();
        (decisions, warnings)
    }

    #[test]
    fn a_tile_is_kept_where_the_game_ships_it_and_no_mod_changed_the_land() {
        let dir = data_folder("keep", 0);
        // the game's archive has the tiles of x0 and x4; the mod's archive does not count; x12 has none anywhere
        let (kept, warnings) = keep_for(&dir, Vec::new());
        assert_eq!(kept, vec![true, true, false, false]);
        assert_eq!(warnings, Vec::<String>::new());
        // a mod that changes the land of cell (5, 0) composes the block x4 only
        let (kept, _) = keep_for(&dir, vec![cell(5, 0, 9.0)]);
        assert_eq!(kept, vec![true, false, false, false]);
    }

    /// A packed output of an earlier run is in the data folder while the next run reads it, and its stub is not in
    /// the request's plugins: its tiles are not the game's.
    #[test]
    fn the_archive_of_an_earlier_packed_run_is_not_the_games() {
        let dir = data_folder("own-archive", 0);
        let tile = |x: i32| {
            format!("textures\\landscape\\lod\\wasteland\\diffuse\\wasteland.n.level4.x{x}.y0.dds")
        };
        archive(
            &dir.join("Lodwright LOD.bsa"),
            &[(&tile(8), b"ours"), (&tile(12), b"ours")],
        );
        assert_eq!(keep_for(&dir, Vec::new()).0, vec![true, true, false, false]);
    }

    #[test]
    fn nothing_is_kept_when_the_games_grid_is_not_ours_shifted_by_whole_blocks() {
        // the game's settings start at -3: its blocks cover other cells than ours at the same names
        let dir = data_folder("shift", -3);
        let (kept, warnings) = keep_for(&dir, Vec::new());
        assert_eq!(kept, vec![false; 4]);
        assert_eq!(warnings.len(), 1);
        // a start 32 cells away is the same grid
        let dir = data_folder("whole", 32);
        assert_eq!(keep_for(&dir, Vec::new()).0, vec![true, true, false, false]);
    }

    /// The game's archive holds two land textures; cell 0 uses `a`, cell 4 uses `b`. The mod has no land record.
    /// `extra` adds a loose file or a mod archive that replaces `b`.
    fn retextured(tag: &str, extra: &dyn Fn(&std::path::Path)) -> Vec<bool> {
        use lodwright_core::texture::LandTexture;
        let dir = data_folder(tag, 0);
        let texture = |name: &str| format!("textures\\landscape\\{name}.dds");
        archive(
            &dir.join("Fallout - Textures.bsa"),
            &[
                (&texture("a"), b"a"),
                (&texture("b"), b"b"),
                (
                    "textures\\landscape\\lod\\wasteland\\diffuse\\wasteland.n.level4.x0.y0.dds",
                    b"tile",
                ),
                (
                    "textures\\landscape\\lod\\wasteland\\diffuse\\wasteland.n.level4.x4.y0.dds",
                    b"tile",
                ),
                (
                    "lodsettings\\wasteland.dlodsettings",
                    &LodSettings::for_bounds((0, 0), (7, 7), 8).to_bytes(),
                ),
            ],
        );
        extra(&dir);
        let layered = |x: i32, texture: u32| {
            let mut c = cell(x, 0, 1.0);
            c.layers = Some(CellLayers {
                base: [Some(texture), None, None, None],
                ..CellLayers::default()
            });
            c
        };
        let mut base = plugin(&[], vec![layered(0, 0x0B00), layered(4, 0x0B01)]);
        base.land_textures = ["a", "b"]
            .iter()
            .zip([0x0B00, 0x0B01])
            .map(|(name, id)| LandTexture {
                icon: Some(format!("landscape\\{name}.dds")),
                ..LandTexture::new(id)
            })
            .collect();
        let plugins = vec![
            ("Fallout3.esm".to_owned(), base),
            ("Mod.esp".to_owned(), plugin(&["Fallout3.esm"], Vec::new())),
        ];
        let world = WorldIndex::new(&plugins)
            .unwrap()
            .build(&plugins, 0x3C, Inheritance::None)
            .unwrap();
        let resolver = TextureResolver::from_load_order(&plugins).unwrap();
        let names: Vec<String> = plugins.iter().map(|(n, _)| n.clone()).collect();
        let assets = FolderAssets::open(&dir, &names).unwrap();
        let base = ShippedBase::new(&plugins, &|n| is_game_master(n)).unwrap();
        let keep = plan(
            &world,
            &base,
            &plugins,
            &resolver,
            &assets,
            (0, 0),
            &mut Vec::new(),
        )
        .unwrap()
        .unwrap();
        [0, 4].map(|x| keep.keeps(4, (x, 0))).to_vec()
    }

    #[test]
    fn a_texture_replaced_without_a_record_composes_the_tiles_under_it() {
        // no replacement: both tiles stay with the game
        assert_eq!(retextured("tex-none", &|_| {}), vec![true, true]);
        // a loose file replaces b: the tile under cell 4 is composed, the other stays
        let loose = |dir: &std::path::Path| {
            let folder = dir.join("textures").join("landscape");
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("b.dds"), b"new").unwrap();
        };
        assert_eq!(retextured("tex-loose", &loose), vec![true, false]);
        // an archive of the mod replaces a
        let archived = |dir: &std::path::Path| {
            archive(
                &dir.join("Mod - Main.bsa"),
                &[("textures\\landscape\\a.dds", b"new")],
            );
        };
        assert_eq!(retextured("tex-archive", &archived), vec![false, true]);
    }
}
