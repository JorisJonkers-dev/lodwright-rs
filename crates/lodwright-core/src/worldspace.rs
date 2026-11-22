// SPDX-License-Identifier: GPL-3.0-only
//! Worldspaces assembled across a load order: `FormID`s resolved through master lists, later plugins overriding
//! the cells of earlier ones.
//!
//! A [`WorldIndex`] lists the worlds of a load order and their `WRLD` fields cheaply; [`WorldIndex::build`] then
//! assembles one world at a time, so a load order with dozens of worlds never holds more than one world's
//! assembled heights beside the plugins themselves.
//!
//! Derived from: the `FormID` and load-order rules on the UESP pages and xEdit's definitions
//! (`provenance/sources.toml`: `plugin-formid-masters`, `plugin-record-override`, `wrld-parent`, `wrld-land-data`,
//! `wrld-bounds`, `cell-water`, `wrld-child-inherits`).

use std::collections::{BTreeMap, BTreeSet};

use crate::Error;
use crate::land::{CellHeights, SIDE};
use crate::layers::CellLayers;
use crate::plugin::{DeclaredBounds, Plugin, WorldRecord};

/// The extent the game's settings file and level set follow (`game-lod-settings-extent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LodExtent {
    /// First cell of the grid.
    pub south_west: (i32, i32),
    /// Last cell, the maximum corner cell of the world.
    pub north_east: (i32, i32),
    /// Cells across: the larger extent rounded up to a power of two.
    pub width: u32,
    /// Highest terrain level: a quarter of the width, from 4 to 32.
    pub highest_level: u32,
}

/// World units per cell edge.
const CELL_UNITS: f32 = 4096.0;

/// Cells a side of the window the game's LOD covers.
const WINDOW_CELLS: i32 = 128;
/// A world's land wider than this (cells) gets the window; the main world's 129 cells do not.
const WINDOW_FIT: i64 = 160;

/// One worldspace with the final heights of every exterior cell that has LAND.
#[derive(Debug, Clone, PartialEq)]
pub struct Worldspace {
    form_id: u32,
    name: String,
    cells: BTreeMap<(i32, i32), CellHeights>,
    layers: BTreeMap<(i32, i32), CellLayers>,
    water: BTreeMap<(i32, i32), CellWater>,
    record: ResolvedRecord,
    /// The smallest x and the smallest y over the grids of all exterior `CELL` records, with or without LAND.
    cell_min: Option<(i32, i32)>,
    /// The cells that have a `LAND` record, with or without heights (a superset of the keys of `cells` for a world
    /// read from plugins).
    land_records: BTreeSet<(i32, i32)>,
}

/// The water of a cell as its CELL record states it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CellWater {
    has_water: bool,
    height: Option<f32>,
}

/// The `WRLD` fields of the winning record, with the parent made absolute.
#[derive(Debug, Clone, Default, PartialEq)]
struct ResolvedRecord {
    parent: Option<u32>,
    parent_flags: Option<u16>,
    default_land_height: Option<f32>,
    default_water_height: Option<f32>,
    bounds: Option<DeclaredBounds>,
    unnamed: bool,
}

/// What a child worldspace takes from its parent. The public sources do not say that LAND cells are inherited
/// (`wrld-child-inherits`), so no variant inherits cells.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Inheritance {
    /// A child uses only its own data (the default).
    #[default]
    None,
    /// A child whose `PNAM` has "use land data" takes its parent's default land and water height (followed up the
    /// chain of parents).
    FlaggedDefaults,
}

/// The `PNAM` flag bit 0, "use land data" (`wrld-parent`).
const USE_LAND_DATA: u16 = 0x1;
/// Longest chain of parents followed; a longer chain or a cycle stops there.
const MAX_PARENT_DEPTH: usize = 16;

/// Inclusive cell bounds of a worldspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    /// Smallest cell x.
    pub min_x: i32,
    /// Smallest cell y.
    pub min_y: i32,
    /// Largest cell x.
    pub max_x: i32,
    /// Largest cell y.
    pub max_y: i32,
}

impl Worldspace {
    /// Absolute `FormID` (the high byte is the load-order index of the defining plugin).
    #[must_use]
    pub fn form_id(&self) -> u32 {
        self.form_id
    }

    /// Editor id, or the `FormID` in hex when no plugin in the load order names it.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Number of cells with LAND.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Heights of cell (`x`, `y`), if it has LAND.
    #[must_use]
    pub fn cell(&self, x: i32, y: i32) -> Option<&CellHeights> {
        self.cells.get(&(x, y))
    }

    /// Absolute `FormID` of the parent worldspace (`WNAM`), when the winning `WRLD` record names one.
    #[must_use]
    pub fn parent(&self) -> Option<u32> {
        self.record.parent
    }

    /// The `PNAM` flags of the winning `WRLD` record.
    #[must_use]
    pub fn parent_flags(&self) -> Option<u16> {
        self.record.parent_flags
    }

    /// Default land height (`DNAM`), after [`Inheritance`] was applied.
    #[must_use]
    pub fn default_land_height(&self) -> Option<f32> {
        self.record.default_land_height
    }

    /// Default water height (`DNAM`), after [`Inheritance`] was applied.
    #[must_use]
    pub fn default_water_height(&self) -> Option<f32> {
        self.record.default_water_height
    }

    /// The corners `NAM0` and `NAM9` as stored (unit not established).
    #[must_use]
    pub fn declared_bounds(&self) -> Option<DeclaredBounds> {
        self.record.bounds
    }

    /// The extent of the world for the settings file and the set of levels, from the declared corners: the grid start
    /// ([`Worldspace::grid_anchor`]) to the cell of the maximum corner `NAM9`, the width the larger extent rounded up
    /// to a power of two, the highest level a quarter of it (4 to 32). A world whose declared extent exceeds 128 cells
    /// takes the end of its land instead of `NAM9` (the main world). `None` without corners.
    // source: game-lod-settings-extent
    #[must_use]
    pub fn lod_extent(&self) -> Option<LodExtent> {
        let b = self.record.bounds?;
        let sw = self.grid_anchor();
        #[allow(clippy::cast_possible_truncation)] // bounded by the clamp
        let cell = |v: f32| (v / CELL_UNITS).floor().clamp(-1_000_000.0, 1_000_000.0) as i32;
        let declared = (cell(b.max[0]), cell(b.max[1]));
        let data = self.bounds().map(|d| (d.max_x, d.max_y));
        let too_wide = declared.0 - sw.0 > 128 || declared.1 - sw.1 > 128;
        let ne = match data {
            Some(d) if too_wide => d,
            _ => declared,
        };
        let across = (ne.0 - sw.0).max(ne.1 - sw.1).max(1);
        let width = u32::try_from(across).ok()?.next_power_of_two().max(4);
        Some(LodExtent {
            south_west: sw,
            north_east: ne,
            width,
            highest_level: (width / 4).clamp(4, 32),
        })
    }

    /// The fewest cells with LAND a world needs before the game's own tool is known to write terrain LOD for it, as
    /// far as the measurement goes (`game-lod-terrain-worlds`): the smallest world with terrain LOD has 17.
    pub const MIN_LAND_CELLS_FOR_TERRAIN_LOD: usize = 17;

    /// Whether the game's tool is likely to have written terrain LOD for this world. The plugin data holds no flag
    /// that says which worlds the game has terrain LOD for; this is the project's own rule, the best single predictor
    /// found (29 of the 30 measured worlds with LAND): at least [`Self::MIN_LAND_CELLS_FOR_TERRAIN_LOD`] cells with
    /// LAND and a `FULL` name on the `WRLD` record. It decides whether the block set is completed over the declared extent
    /// ([`Worldspace::filled_over_extent`]); no world is left out of a run because of it.
    // source: game-lod-terrain-worlds
    #[must_use]
    pub fn has_terrain_lod(&self) -> bool {
        let land = self.land_records.len().max(self.cells.len());
        land >= Self::MIN_LAND_CELLS_FOR_TERRAIN_LOD && !self.record.unnamed
    }

    /// A copy whose cells without LAND, between the grid start and the end of the settings extent
    /// ([`Worldspace::lod_extent`], clipped to its width), are flat at the world's default land height, as the game's
    /// blocks over such cells are (`game-lod-block-extent`). Unchanged without a default land height or an extent, and
    /// for a world that fails [`Worldspace::has_terrain_lod`] (the game's tool probably wrote nothing for it, so
    /// inventing blocks over its empty extent would only add files).
    #[must_use]
    pub fn filled_over_extent(&self) -> Self {
        let (Some(extent), Some(height)) = (self.lod_extent(), self.record.default_land_height)
        else {
            return self.clone();
        };
        if !self.has_terrain_lod() {
            return self.clone();
        }
        if !height.is_finite() {
            return self.clone();
        }
        let across = i64::from(extent.width) - 1;
        let end = |start: i32, last: i32| {
            i32::try_from((i64::from(start) + across).min(i64::from(last))).unwrap_or(last)
        };
        let (sw, ne) = (extent.south_west, extent.north_east);
        let (last_x, last_y) = (end(sw.0, ne.0), end(sw.1, ne.1));
        let mut out = self.clone();
        for y in sw.1..=last_y {
            for x in sw.0..=last_x {
                out.cells
                    .entry((x, y))
                    .or_insert_with(|| CellHeights::flat(height));
            }
        }
        out
    }

    /// The same world with the corners `NAM0` and `NAM9` replaced (for worlds built from cells alone).
    #[must_use]
    pub fn with_declared_bounds(mut self, bounds: DeclaredBounds) -> Self {
        self.record.bounds = Some(bounds);
        self
    }

    /// The cell where the block grid of every LOD level starts: the smallest grid coordinate of the world's `CELL`
    /// records (cells without LAND count), as the game's own tree places its blocks; without any `CELL` record the
    /// cell of the minimum corner `NAM0` (world units, 4096 to a cell, rounded down). (0, 0) when neither is known
    /// or the corner is not finite or lies beyond 1 000 000 cells.
    // source: game-lod-block-grid
    #[must_use]
    pub fn grid_anchor(&self) -> (i32, i32) {
        let start = self.cell_min.or_else(|| {
            let b = self.record.bounds?;
            #[allow(clippy::cast_possible_truncation)] // bounded by the test before the cast
            let cell = |v: f32| {
                let c = (v / CELL_UNITS).floor();
                (c.is_finite() && c.abs() <= 1_000_000.0).then_some(c as i32)
            };
            Some((cell(b.min[0])?, cell(b.min[1])?))
        });
        let Some((mut x, mut y)) = start else {
            return (0, 0);
        };
        let (wx, wy) = self.window();
        if let Some(w) = wx {
            x = w;
        }
        if let Some(w) = wy {
            y = w;
        }
        // The game's tree of WastelandNV starts at (-64, -64) where its smallest CELL is (-65, -65): a start
        // exactly one cell below the first cell with LAND, where that cell lies on the 32 grid, gives way to it.
        if let Some(data) = self.bounds() {
            if data.min_x == x + 1 && data.min_x.rem_euclid(32) == 0 {
                x = data.min_x;
            }
            if data.min_y == y + 1 && data.min_y.rem_euclid(32) == 0 {
                y = data.min_y;
            }
        }
        (x, y)
    }

    /// The cells that have a `LAND` record, with or without heights.
    pub fn land_cells(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.land_records
            .iter()
            .chain(self.cells.keys().filter(|k| !self.land_records.contains(k)))
            .copied()
    }

    /// The water height of cell (`x`, `y`) for the mesh stage: its `XCLW`, else the world's default water
    /// height when the cell has the has-water flag, else `None` (no water). `None` too for a cell without LAND.
    #[must_use]
    pub fn water_height(&self, x: i32, y: i32) -> Option<f32> {
        // source: cell-water
        let water = self.water.get(&(x, y))?;
        // source: game-lod-water-height
        // an XCLW of the largest float stands for "no height of its own": the game's blocks use the default there
        water.height.filter(|h| *h < f32::MAX).or_else(|| {
            water
                .has_water
                .then_some(self.record.default_water_height)
                .flatten()
        })
    }

    /// Whether cell (`x`, `y`) has water at the world's default height: the has-water flag with no `XCLW` of its own
    /// (an `XCLW` of the largest float counts as none).
    #[must_use]
    pub fn water_is_default(&self, x: i32, y: i32) -> bool {
        // source: game-lod-water-height
        self.water
            .get(&(x, y))
            .is_some_and(|w| w.has_water && w.height.is_none_or(|h| h >= f32::MAX))
    }

    /// The ground height at world position (`x`, `y`) in world units, interpolated between the four height samples
    /// around it; `None` outside the cells that have LAND.
    #[must_use]
    pub fn ground_height(&self, x: f32, y: f32) -> Option<f32> {
        // source: cell-size-units
        const CELL_UNITS: f32 = 4096.0;
        // source: land-vhgt-layout
        const INTERVALS: f32 = 32.0;
        let (cell_x, cell_y) = ((x / CELL_UNITS).floor(), (y / CELL_UNITS).floor());
        #[allow(clippy::cast_possible_truncation)] // a cell number of a world fits an i32
        let cell = self.cell(cell_x as i32, cell_y as i32)?;
        let east = ((x - cell_x * CELL_UNITS) / CELL_UNITS * INTERVALS).clamp(0.0, INTERVALS);
        let north = ((y - cell_y * CELL_UNITS) / CELL_UNITS * INTERVALS).clamp(0.0, INTERVALS);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=32
        let (col, row) = (
            (east.floor() as usize).min(SIDE - 2),
            (north.floor() as usize).min(SIDE - 2),
        );
        #[allow(clippy::cast_precision_loss)] // below 33
        let (along, across) = (east - col as f32, north - row as f32);
        let south = cell.get(col, row) * (1.0 - along) + cell.get(col + 1, row) * along;
        let northern = cell.get(col, row + 1) * (1.0 - along) + cell.get(col + 1, row + 1) * along;
        Some(south * (1.0 - across) + northern * across)
    }

    /// Normals, colours and texture layers of cell (`x`, `y`), with absolute texture `FormID`s. Present only when
    /// the plugins were read with [`crate::plugin::ReadOptions::land_layers`] and the winning LAND record has them.
    #[must_use]
    pub fn layers(&self, x: i32, y: i32) -> Option<&CellLayers> {
        self.layers.get(&(x, y))
    }

    /// All cells in (x, y) order.
    pub fn cells(&self) -> impl Iterator<Item = ((i32, i32), &CellHeights)> {
        self.cells.iter().map(|(k, v)| (*k, v))
    }

    /// A copy without the outlying cells at the edges: an outer row or column that holds fewer than 2 percent of
    /// the cells its line could hold is dropped, repeatedly (a couple of stray cells beside a large world would
    /// otherwise add a row of nearly empty LOD blocks). Small worlds keep their edges.
    #[must_use]
    pub fn without_outliers(&self) -> Self {
        let mut out = self.clone();
        while let Some(b) = out.bounds() {
            let width = i64::from(b.max_x) - i64::from(b.min_x) + 1;
            let height = i64::from(b.max_y) - i64::from(b.min_y) + 1;
            let count = |on: &dyn Fn(i32, i32) -> bool| {
                i64::try_from(out.cells.keys().filter(|(x, y)| on(*x, *y)).count()).unwrap_or(0)
            };
            let sparse = |n: i64, line: i64| n * 50 < line;
            let drop = if sparse(count(&|_, y| y == b.max_y), width) {
                Some((false, b.max_y))
            } else if sparse(count(&|_, y| y == b.min_y), width) {
                Some((false, b.min_y))
            } else if sparse(count(&|x, _| x == b.max_x), height) {
                Some((true, b.max_x))
            } else if sparse(count(&|x, _| x == b.min_x), height) {
                Some((true, b.min_x))
            } else {
                None
            };
            let Some((by_x, line)) = drop else { break };
            let keep = |(x, y): &(i32, i32)| if by_x { *x != line } else { *y != line };
            out.cells.retain(|k, _| keep(k));
            out.layers.retain(|k, _| keep(k));
            out.water.retain(|k, _| keep(k));
        }
        out.clip_to_window()
    }

    /// The game's LOD covers at most 128 cells a side: for a world whose land is wider than that (the Capital Wasteland
    /// of a Tale of Two Wastelands load order has land from cell -99 to 100; the game's own files for it cover -64 to
    /// 63) the window is 128 cells, started on the 32 grid nearest to the cell that centres it on the land. Per axis; `None`
    /// for an axis that fits.
    // source: game-lod-window
    fn window(&self) -> (Option<i32>, Option<i32>) {
        let Some(b) = self.bounds() else {
            return (None, None);
        };
        let start = |lo: i32, hi: i32| {
            (i64::from(hi) - i64::from(lo) + 1 > WINDOW_FIT).then(|| {
                let centre = (i64::from(lo) + i64::from(hi)).div_euclid(2);
                // the nearest multiple of 32 that centres the window
                let at = ((centre - i64::from(WINDOW_CELLS) / 2 + 16).div_euclid(32)) * 32;
                i32::try_from(at).unwrap_or(lo)
            })
        };
        (start(b.min_x, b.max_x), start(b.min_y, b.max_y))
    }

    /// The same world without the cells outside the window of [`Worldspace::window`].
    fn clip_to_window(mut self) -> Self {
        let (wx, wy) = self.window();
        if wx.is_none() && wy.is_none() {
            return self;
        }
        let keep = |(x, y): &(i32, i32)| {
            wx.is_none_or(|s| (s..s + WINDOW_CELLS).contains(x))
                && wy.is_none_or(|s| (s..s + WINDOW_CELLS).contains(y))
        };
        self.cells.retain(|k, _| keep(k));
        self.layers.retain(|k, _| keep(k));
        self.water.retain(|k, _| keep(k));
        // the clipped world starts its grid at the window
        let (cx, cy) = self.cell_min.unwrap_or((0, 0));
        self.cell_min = Some((wx.unwrap_or(cx), wy.unwrap_or(cy)));
        self
    }

    /// Inclusive bounds, `None` for a world without cells.
    #[must_use]
    pub fn bounds(&self) -> Option<Bounds> {
        let mut keys = self.cells.keys();
        let first = keys.next()?;
        let mut b = Bounds {
            min_x: first.0,
            min_y: first.1,
            max_x: first.0,
            max_y: first.1,
        };
        for (x, y) in keys {
            b.min_x = b.min_x.min(*x);
            b.max_x = b.max_x.max(*x);
            b.min_y = b.min_y.min(*y);
            b.max_y = b.max_y.max(*y);
        }
        Some(b)
    }

    /// The largest height difference between the two copies of a vertex that two neighbouring cells share, over
    /// the whole world (0 when every seam matches or there are no neighbours).
    #[must_use]
    pub fn max_seam_error(&self) -> f32 {
        let mut worst = 0.0_f32;
        for (&(x, y), cell) in &self.cells {
            if let Some(east) = self.cells.get(&(x.saturating_add(1), y)) {
                for row in 0..SIDE {
                    worst = worst.max((cell.get(SIDE - 1, row) - east.get(0, row)).abs());
                }
            }
            if let Some(north) = self.cells.get(&(x, y.saturating_add(1))) {
                for col in 0..SIDE {
                    worst = worst.max((cell.get(col, SIDE - 1) - north.get(col, 0)).abs());
                }
            }
        }
        worst
    }

    /// Set the water of cell (`x`, `y`) as a CELL record would: the has-water flag and an optional `XCLW` (for
    /// tests and tools).
    pub fn set_cell_water(&mut self, x: i32, y: i32, has_water: bool, height: Option<f32>) {
        self.water.insert((x, y), CellWater { has_water, height });
    }

    /// Set the world's default water height as its `DNAM` would (for tests and tools).
    pub fn set_default_water_height(&mut self, height: Option<f32>) {
        self.record.default_water_height = height;
    }

    /// Set the texture layers of cell (`x`, `y`) as a LAND record would (for tests and tools).
    pub fn set_cell_layers(&mut self, x: i32, y: i32, layers: CellLayers) {
        self.layers.insert((x, y), layers);
    }

    /// Build a world directly from cells (for tests and tools).
    #[must_use]
    pub fn from_cells(
        form_id: u32,
        name: &str,
        cells: impl IntoIterator<Item = ((i32, i32), CellHeights)>,
    ) -> Self {
        Self {
            form_id,
            name: name.to_owned(),
            cells: cells.into_iter().collect(),
            layers: BTreeMap::new(),
            water: BTreeMap::new(),
            record: ResolvedRecord::default(),
            cell_min: None,
            land_records: BTreeSet::new(),
        }
    }

    /// Assemble every worldspace of a load order with no inheritance. Each item is a plugin's file name and its
    /// contents, in load order. Cells of later plugins replace those of earlier ones; the last editor id seen
    /// names the world. Worldspaces come back sorted by absolute `FormID`. Prefer [`WorldIndex`] when the load
    /// order has many worlds and they can be handled one at a time.
    ///
    /// # Errors
    /// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
    pub fn from_load_order(plugins: &[(String, Plugin)]) -> Result<Vec<Self>, Error> {
        let index = WorldIndex::new(plugins)?;
        let ids: Vec<u32> = index.worlds().map(|(id, _)| id).collect();
        Ok(ids
            .into_iter()
            .filter_map(|id| index.build(plugins, id, Inheritance::None))
            .collect())
    }
}

/// The worlds of a load order, listed without assembling any cells.
#[derive(Debug, Clone)]
pub struct WorldIndex {
    worlds: BTreeMap<u32, IndexEntry>,
    tables: Vec<Vec<u32>>,
}

#[derive(Debug, Clone)]
struct IndexEntry {
    name: String,
    record: ResolvedRecord,
}

impl WorldIndex {
    /// List the worlds of `plugins` (file name and contents, in load order): absolute `FormID`, name (the last
    /// editor id seen) and the winning `WRLD` record (the last plugin that carries one replaces the earlier
    /// records as a whole).
    ///
    /// # Errors
    /// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
    pub fn new(plugins: &[(String, Plugin)]) -> Result<Self, Error> {
        let mut worlds: BTreeMap<u32, IndexEntry> = BTreeMap::new();
        let mut tables = Vec::with_capacity(plugins.len());
        for (index, (_, plugin)) in plugins.iter().enumerate() {
            let table = master_table(plugins, plugin)?;
            let own = u32::try_from(index).map_err(|_| Error::Malformed("too many plugins"))?;
            for source in &plugin.worlds {
                let id = absolute(source.form_id, &table, own);
                let entry = worlds.entry(id).or_insert_with(|| IndexEntry {
                    name: format!("{id:08X}"),
                    record: ResolvedRecord::default(),
                });
                if let Some(editor_id) = &source.editor_id {
                    entry.name.clone_from(editor_id);
                }
                if let Some(record) = &source.record {
                    // source: plugin-record-override
                    entry.record = resolve_record(record, &table, own);
                }
            }
            tables.push(table);
        }
        Ok(Self { worlds, tables })
    }

    /// The worlds as (absolute `FormID`, name), sorted by `FormID`.
    pub fn worlds(&self) -> impl Iterator<Item = (u32, &str)> {
        self.worlds.iter().map(|(id, e)| (*id, e.name.as_str()))
    }

    /// Assemble world `id` from `plugins`, which must be the plugins the index was made from. `None` when the
    /// index has no such world.
    #[must_use]
    pub fn build(
        &self,
        plugins: &[(String, Plugin)],
        id: u32,
        inheritance: Inheritance,
    ) -> Option<Worldspace> {
        let entry = self.worlds.get(&id)?;
        let mut world = Worldspace {
            form_id: id,
            name: entry.name.clone(),
            cells: BTreeMap::new(),
            layers: BTreeMap::new(),
            water: BTreeMap::new(),
            record: entry.record.clone(),
            cell_min: None,
            land_records: BTreeSet::new(),
        };
        if inheritance == Inheritance::FlaggedDefaults {
            self.inherit_defaults(&mut world);
        }
        for (index, (_, plugin)) in plugins.iter().enumerate() {
            let own = u32::try_from(index).ok()?;
            let table = self.tables.get(index)?;
            for source in plugin
                .worlds
                .iter()
                .filter(|w| absolute(w.form_id, table, own) == id)
            {
                for &(x, y) in &source.cell_coords {
                    let (mx, my) = world.cell_min.unwrap_or((x, y));
                    world.cell_min = Some((mx.min(x), my.min(y)));
                }
                world
                    .land_records
                    .extend(source.land_coords.iter().copied());
                for cell in &source.cells {
                    let key = (cell.x, cell.y);
                    world.land_records.insert(key);
                    world.cells.insert(key, cell.heights.clone());
                    // a later LAND record replaces the earlier one as a whole, layers included
                    // source: plugin-record-override
                    world.layers.remove(&key);
                    if let Some(layers) = &cell.layers {
                        let mut layers = layers.clone();
                        layers.map_textures(|id| absolute(id, table, own));
                        world.layers.insert(key, layers);
                    }
                    world.water.insert(
                        key,
                        CellWater {
                            has_water: cell.has_water,
                            height: cell.water_height,
                        },
                    );
                }
            }
        }
        Some(world)
    }

    /// Apply [`Inheritance::FlaggedDefaults`]: follow "use land data" up the parents.
    fn inherit_defaults(&self, world: &mut Worldspace) {
        // source: wrld-child-inherits
        let mut record = &world.record;
        let mut seen = vec![world.form_id];
        for _ in 0..MAX_PARENT_DEPTH {
            let flagged = record.parent_flags.is_some_and(|f| f & USE_LAND_DATA != 0);
            let Some(parent) = record.parent.filter(|_| flagged) else {
                return;
            };
            if seen.contains(&parent) {
                return;
            }
            let Some(entry) = self.worlds.get(&parent) else {
                return;
            };
            seen.push(parent);
            record = &entry.record;
            let inherited_flags = record.parent_flags.is_some_and(|f| f & USE_LAND_DATA != 0)
                && record.parent.is_some();
            if !inherited_flags {
                if let (Some(land), water) =
                    (record.default_land_height, record.default_water_height)
                {
                    world.record.default_land_height = Some(land);
                    world.record.default_water_height = water;
                }
                return;
            }
        }
    }
}

fn resolve_record(record: &WorldRecord, table: &[u32], own: u32) -> ResolvedRecord {
    ResolvedRecord {
        parent: record.parent.map(|p| absolute(p, table, own)),
        parent_flags: record.parent_flags,
        default_land_height: record.default_land_height,
        default_water_height: record.default_water_height,
        bounds: record.bounds,
        unnamed: record.unnamed,
    }
}

/// Load-order index of each master of `plugin`, by position in its master list.
pub(crate) fn master_table(
    plugins: &[(String, Plugin)],
    plugin: &Plugin,
) -> Result<Vec<u32>, Error> {
    plugin
        .masters
        .iter()
        .map(|master| {
            plugins
                .iter()
                .position(|(name, _)| name.eq_ignore_ascii_case(master))
                .and_then(|i| u32::try_from(i).ok())
                .ok_or_else(|| Error::MissingMaster(master.clone()))
        })
        .collect()
}

// source: plugin-formid-masters
pub(crate) fn absolute(form_id: u32, table: &[u32], own: u32) -> u32 {
    let index = usize::try_from(form_id >> 24).unwrap_or(usize::MAX);
    let load_index = table.get(index).copied().unwrap_or(own);
    (load_index << 24) | (form_id & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_ground_height_is_read_inside_a_cell_and_absent_outside_the_land() {
        let world = Worldspace::from_cells(
            0x100,
            "W",
            vec![((1, 0), crate::land::CellHeights::flat(96.0))],
        );
        assert_eq!(world.ground_height(4096.0 + 300.0, 64.0), Some(96.0));
        assert_eq!(world.ground_height(8191.0, 4095.0), Some(96.0));
        assert_eq!(world.ground_height(-1.0, 64.0), None);
        assert_eq!(world.ground_height(8192.0 + 1.0, 64.0), None);
    }

    #[test]
    fn a_stray_cell_beside_a_large_world_is_trimmed_and_small_worlds_are_kept() {
        let big = Worldspace::from_cells(
            1,
            "Big",
            (0..100)
                .flat_map(|x| (0..100).map(move |y| ((x, y), CellHeights::flat(1.0))))
                .chain([
                    ((5, 100), CellHeights::flat(1.0)),
                    ((-1, 50), CellHeights::flat(1.0)),
                ]),
        );
        let trimmed = big.without_outliers();
        assert_eq!(trimmed.cell_count(), 10_000);
        let b = trimmed.bounds().unwrap();
        assert_eq!((b.min_x, b.max_x, b.min_y, b.max_y), (0, 99, 0, 99));
        let small = Worldspace::from_cells(
            1,
            "Small",
            [
                ((0, 0), CellHeights::flat(1.0)),
                ((7, 0), CellHeights::flat(1.0)),
            ],
        );
        assert_eq!(small.without_outliers().cell_count(), 2);
    }

    use super::{Bounds, Inheritance, WorldIndex, Worldspace};
    use crate::Error;
    use crate::land::CellHeights;
    use crate::plugin::read;
    use lodwright_testkit::land::Heights;
    use lodwright_testkit::world::{SyntheticWorld, world_plugin};

    fn load(items: &[(&str, Vec<u8>)]) -> Vec<(String, crate::plugin::Plugin)> {
        items
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), read(bytes).unwrap()))
            .collect()
    }

    #[test]
    fn a_single_plugin_gives_its_world_named_by_the_editor_id() {
        let world = SyntheticWorld::new(0x0000_0A00, "Mojave").cell(0, 0, Heights::flat(8.0));
        let plugins = load(&[("A.esm", world_plugin(&[], &world, false))]);
        let worlds = Worldspace::from_load_order(&plugins).unwrap();
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].name(), "Mojave");
        assert_eq!(worlds[0].form_id(), 0x0000_0A00);
        assert_eq!(worlds[0].cell_count(), 1);
        assert_eq!(worlds[0].cell(0, 0).unwrap().get(1, 1), 8.0);
        assert!(worlds[0].cell(1, 0).is_none());
    }

    #[test]
    fn the_grid_starts_at_the_smallest_cell_record_even_without_land() {
        // NAM0 says (-5, -5); the smallest CELL record is (-4, -4) and has no LAND; the land starts at (-2, -2)
        let world = SyntheticWorld::new(0x10, "W")
            .bounds([-5.0 * 4096.0, -5.0 * 4096.0], [3.0 * 4096.0, 3.0 * 4096.0])
            .cell_without_land(-4, -4)
            .cell_without_land(1, -3)
            .cell(-2, -2, Heights::flat(8.0))
            .cell(2, 2, Heights::flat(8.0));
        let plugins = load(&[("A.esm", world_plugin(&[], &world, false))]);
        let worlds = Worldspace::from_load_order(&plugins).unwrap();
        assert_eq!(worlds[0].grid_anchor(), (-4, -4));
        // a world made without CELL records falls back on the declared corner
        let flat = Worldspace::from_cells(1, "W", [((-2, -2), CellHeights::flat(0.0))]);
        assert_eq!(flat.grid_anchor(), (0, 0));
    }

    #[test]
    fn a_later_plugin_overrides_cells_through_its_master_index() {
        // plugin 1 refers to world 0xA00 of plugin 0 through master index 0
        let base = SyntheticWorld::new(0x0000_0A00, "Mojave")
            .cell(0, 0, Heights::flat(8.0))
            .cell(1, 0, Heights::flat(16.0));
        let patch = SyntheticWorld::new(0x0000_0A00, "Mojave").cell(1, 0, Heights::flat(80.0));
        let plugins = load(&[
            ("Base.esm", world_plugin(&[], &base, false)),
            ("Patch.esp", world_plugin(&["base.esm"], &patch, false)),
        ]);
        let worlds = Worldspace::from_load_order(&plugins).unwrap();
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].cell(0, 0).unwrap().get(0, 0), 8.0);
        assert_eq!(worlds[0].cell(1, 0).unwrap().get(0, 0), 80.0);
    }

    #[test]
    fn a_world_defined_by_a_late_plugin_gets_that_plugins_index() {
        let a = SyntheticWorld::new(0x0000_0A00, "A").cell(0, 0, Heights::flat(8.0));
        let b = SyntheticWorld::new(0x0100_0B00, "B").cell(0, 0, Heights::flat(8.0));
        let plugins = load(&[
            ("A.esm", world_plugin(&[], &a, false)),
            ("B.esp", world_plugin(&["A.esm"], &b, false)),
        ]);
        let ids: Vec<u32> = Worldspace::from_load_order(&plugins)
            .unwrap()
            .iter()
            .map(Worldspace::form_id)
            .collect();
        assert_eq!(ids, [0x0000_0A00, 0x0100_0B00]);
    }

    #[test]
    fn a_missing_master_is_reported_by_name() {
        let world = SyntheticWorld::new(0x10, "W").cell(0, 0, Heights::flat(8.0));
        let plugins = load(&[("B.esp", world_plugin(&["Nope.esm"], &world, false))]);
        assert_eq!(
            Worldspace::from_load_order(&plugins),
            Err(Error::MissingMaster("Nope.esm".into()))
        );
    }

    #[test]
    fn a_world_without_an_editor_id_is_named_by_its_form_id() {
        let world = Worldspace::from_cells(0x0100_0042, "01000042", []);
        assert_eq!(world.name(), "01000042");
        assert_eq!(world.bounds(), None);
    }

    #[test]
    fn the_grid_anchor_is_the_minimum_corner_cell_unless_the_land_starts_on_the_32_grid() {
        let flat = |cells: &[(i32, i32)], min: [f32; 2]| {
            Worldspace::from_cells(1, "W", cells.iter().map(|c| (*c, CellHeights::flat(1.0))))
                .with_declared_bounds(crate::plugin::DeclaredBounds {
                    min,
                    max: [min[0] + 8192.0, min[1] + 8192.0],
                })
        };
        // no corners: the origin
        let none = Worldspace::from_cells(1, "W", [((3, 4), CellHeights::flat(1.0))]);
        assert_eq!(none.grid_anchor(), (0, 0));
        // corner cell (-6, -5) below the land: the corner
        let cells = [(-4, -4), (2, 2)];
        assert_eq!(
            flat(&cells, [-6.0 * 4096.0, -5.0 * 4096.0]).grid_anchor(),
            (-6, -5)
        );
        // a corner one cell below land that starts on the 32 grid (WastelandNV): the land
        let cells = [(-64, -64), (63, 63)];
        assert_eq!(
            flat(&cells, [-65.0 * 4096.0, -65.0 * 4096.0]).grid_anchor(),
            (-64, -64)
        );
        // not finite: the origin
        assert_eq!(flat(&cells, [f32::NAN, 0.0]).grid_anchor(), (0, 0));
    }

    #[test]
    fn terrain_lod_needs_seventeen_cells_with_land_and_a_display_name() {
        let cells = |n: i32| (0..n).map(|x| ((x, 0), CellHeights::flat(1.0)));
        assert!(!Worldspace::from_cells(1, "W", cells(16)).has_terrain_lod());
        assert!(Worldspace::from_cells(1, "W", cells(17)).has_terrain_lod());
        assert!(!Worldspace::from_cells(1, "W", []).has_terrain_lod());
        // read from a plugin: the record without FULL is a test map
        let world = |name: &str| {
            let mut w = SyntheticWorld::new(0x10, name);
            for x in 0..17 {
                w = w.cell(x, 0, Heights::flat(8.0));
            }
            w
        };
        let load = |w: SyntheticWorld| {
            let plugins = vec![(
                "A.esm".to_owned(),
                read(&world_plugin(&[], &w, false)).unwrap(),
            )];
            Worldspace::from_load_order(&plugins).unwrap().remove(0)
        };
        assert!(!load(world("Test")).has_terrain_lod());
        assert!(load(world("Shown").named()).has_terrain_lod());
    }

    #[test]
    fn cells_without_land_inside_the_extent_are_flat_at_the_default_land_height() {
        let u = 4096.0;
        // 17 cells with land in the bottom row, the last one beyond the width of the extent
        let land = (-1..16).map(|x| ((x, -1), CellHeights::flat(5.0)));
        let mut world = Worldspace::from_cells(1, "W", land).with_declared_bounds(
            crate::plugin::DeclaredBounds {
                min: [-u, -u],
                max: [15.0 * u, 2.0 * u],
            },
        );
        world.record.default_land_height = Some(1000.0);
        // corners (-1, -1) to (15, 2): width 16, so cells -1 to 14 across and -1 to 2 up: 64 cells, plus (15, -1)
        let filled = world.filled_over_extent();
        assert_eq!(filled.cell_count(), 65);
        assert_eq!(filled.cell(-1, -1).unwrap().get(0, 0), 5.0);
        assert_eq!(filled.cell(3, 2).unwrap().get(0, 0), 1000.0);
        assert!(filled.cell(15, 2).is_none());
        // without a default land height nothing is added
        world.record.default_land_height = None;
        assert_eq!(world.filled_over_extent().cell_count(), 17);
        // a world with fewer than 17 cells with land is left as it is
        let few =
            Worldspace::from_cells(1, "W", (-1..15).map(|x| ((x, -1), CellHeights::flat(5.0))))
                .with_declared_bounds(crate::plugin::DeclaredBounds {
                    min: [-u, -u],
                    max: [15.0 * u, 2.0 * u],
                });
        assert_eq!(few.filled_over_extent().cell_count(), 16);
    }

    #[test]
    fn a_world_with_land_beyond_128_cells_gets_the_128_cell_window_of_the_game_s_files() {
        // the Capital Wasteland of a Tale of Two Wastelands load order: land from cell -99 to 97 across, -99 to 100
        // up; the game's own settings for it are (-64, -64) to (63, 63), width 128
        let u = 4096.0;
        let cells =
            (-99..=100).flat_map(|y| (-99..=97).map(move |x| ((x, y), CellHeights::flat(1.0))));
        let world = Worldspace::from_cells(1, "W", cells).with_declared_bounds(
            crate::plugin::DeclaredBounds {
                min: [-99.0 * u, -99.0 * u],
                max: [99.0 * u, 101.0 * u],
            },
        );
        let clipped = world.without_outliers();
        let b = clipped.bounds().unwrap();
        assert_eq!((b.min_x, b.min_y, b.max_x, b.max_y), (-64, -64, 63, 63));
        assert_eq!(clipped.grid_anchor(), (-64, -64));
        let e = clipped.lod_extent().unwrap();
        assert_eq!(
            (e.south_west, e.north_east, e.width, e.highest_level),
            ((-64, -64), (63, 63), 128, 32)
        );
        // the world that was not clipped names the same start
        assert_eq!(world.grid_anchor(), (-64, -64));
    }

    #[test]
    fn the_extent_follows_the_declared_corners_as_the_game_s_settings_do() {
        let world = |min: [f32; 2], max: [f32; 2], cells: &[(i32, i32)]| {
            Worldspace::from_cells(1, "W", cells.iter().map(|c| (*c, CellHeights::flat(1.0))))
                .with_declared_bounds(crate::plugin::DeclaredBounds { min, max })
        };
        let u = 4096.0;
        // BoulderCityWorld: corners (-6, -5) and (6, 8): width 16, highest level 4
        let boulder = world(
            [-6.0 * u, -5.0 * u],
            [6.0 * u, 8.0 * u],
            &[(-6, -4), (5, 7)],
        )
        .lod_extent()
        .unwrap();
        assert_eq!(
            (
                boulder.south_west,
                boulder.north_east,
                boulder.width,
                boulder.highest_level
            ),
            ((-6, -5), (6, 8), 16, 4)
        );
        // FreesideWorld: (-10, -8) to (7, 12): width 32, highest level 8
        let freeside = world([-10.0 * u, -8.0 * u], [7.0 * u, 12.0 * u], &[(-5, -6)])
            .lod_extent()
            .unwrap();
        assert_eq!((freeside.width, freeside.highest_level), (32, 8));
        // a 64 by 64 world: width 64, highest level 16
        let big = world([-32.0 * u, -32.0 * u], [32.0 * u, 32.0 * u], &[(-32, -32)])
            .lod_extent()
            .unwrap();
        assert_eq!((big.width, big.highest_level), (64, 16));
        // a declared extent beyond 128 cells (the main world) ends at its land: width 128, highest 32
        let main = world(
            [-65.0 * u, -65.0 * u],
            [65.0 * u, 272.0 * u],
            &[(-64, -64), (63, 63)],
        )
        .lod_extent()
        .unwrap();
        assert_eq!(
            (
                main.south_west,
                main.north_east,
                main.width,
                main.highest_level
            ),
            ((-64, -64), (63, 63), 128, 32)
        );
        assert_eq!(
            Worldspace::from_cells(1, "W", [((0, 0), CellHeights::flat(1.0))]).lod_extent(),
            None
        );
    }

    #[test]
    fn bounds_cover_every_cell_including_negative_ones() {
        let world = Worldspace::from_cells(
            1,
            "W",
            [
                ((-3, 2), CellHeights::flat(0.0)),
                ((4, -5), CellHeights::flat(0.0)),
            ],
        );
        assert_eq!(
            world.bounds(),
            Some(Bounds {
                min_x: -3,
                min_y: -5,
                max_x: 4,
                max_y: 2
            })
        );
        let order: Vec<(i32, i32)> = world.cells().map(|(k, _)| k).collect();
        assert_eq!(order, [(-3, 2), (4, -5)]);
    }

    #[test]
    fn matching_seams_have_zero_error_and_a_step_shows_up() {
        let flat = || CellHeights::flat(8.0);
        let level = Worldspace::from_cells(
            1,
            "W",
            [((0, 0), flat()), ((1, 0), flat()), ((0, 1), flat())],
        );
        assert!(level.max_seam_error().abs() < f32::EPSILON);
        let stepped = Worldspace::from_cells(
            1,
            "W",
            [((0, 0), flat()), ((1, 0), CellHeights::flat(24.0))],
        );
        assert!((stepped.max_seam_error() - 16.0).abs() < f32::EPSILON);
        let alone = Worldspace::from_cells(
            1,
            "W",
            [((0, 0), flat()), ((5, 5), CellHeights::flat(99.0))],
        );
        assert!(alone.max_seam_error().abs() < f32::EPSILON);
    }

    #[test]
    fn layers_come_back_with_absolute_texture_ids_and_the_last_plugin_wins_whole() {
        use crate::plugin::{ReadOptions, read_with};
        use lodwright_testkit::land::{encode_atxt, encode_btxt, encode_vtxt};
        let options = ReadOptions {
            land_layers: true,
            ..ReadOptions::default()
        };
        let base = SyntheticWorld::new(0x0000_0A00, "Mojave").cell_with_subrecords(
            0,
            0,
            Heights::flat(8.0),
            vec![
                (*b"BTXT", encode_btxt(0x0000_0001, 0, 0)),
                (*b"ATXT", encode_atxt(0x0000_0002, 0, 1)),
                (*b"VTXT", encode_vtxt(&[(3, 1.0)])),
            ],
        );
        let patch = SyntheticWorld::new(0x0000_0A00, "Mojave").cell_with_subrecords(
            0,
            0,
            Heights::flat(9.0),
            // texture 0x01000005 is the patch's own (index 1), 0x00000006 is the master's
            vec![(*b"BTXT", encode_btxt(0x0100_0005, 2, 0))],
        );
        let plugins: Vec<(String, crate::plugin::Plugin)> = vec![
            (
                "Base.esm".into(),
                read_with(&world_plugin(&[], &base, false), options).unwrap(),
            ),
            (
                "Patch.esp".into(),
                read_with(&world_plugin(&["Base.esm"], &patch, false), options).unwrap(),
            ),
        ];
        let only_base = Worldspace::from_load_order(&plugins[..1]).unwrap();
        let layers = only_base[0].layers(0, 0).unwrap();
        assert_eq!(layers.base[0], Some(1));
        assert_eq!(layers.alpha[0].texture, 2);
        let both = Worldspace::from_load_order(&plugins).unwrap();
        let layers = both[0].layers(0, 0).unwrap();
        assert_eq!(layers.base, [None, None, Some(0x0100_0005), None]);
        assert!(
            layers.alpha.is_empty(),
            "the patch cell replaces the base cell whole"
        );
        assert!(both[0].layers(1, 0).is_none());
    }

    #[test]
    fn plain_reads_keep_no_layers() {
        let world = SyntheticWorld::new(0x10, "W").cell(0, 0, Heights::flat(1.0));
        let plugins = load(&[("A.esm", world_plugin(&[], &world, false))]);
        assert!(
            Worldspace::from_load_order(&plugins).unwrap()[0]
                .layers(0, 0)
                .is_none()
        );
    }

    fn chain(items: &[(&str, Vec<u8>)]) -> Vec<(String, crate::plugin::Plugin)> {
        load(items)
    }

    #[test]
    fn world_record_fields_and_cell_water_are_read() {
        let world = SyntheticWorld::new(0x0A00, "Mojave")
            .land_data(-2048.0, -4096.0)
            .bounds([-10.0, -20.0], [30.0, 40.0])
            .cell(0, 0, Heights::flat(8.0))
            .cell(1, 0, Heights::flat(8.0))
            .cell(2, 0, Heights::flat(8.0))
            .cell(3, 0, Heights::flat(8.0))
            .cell_water(0, 0, true, Some(-512.0))
            .cell_water(1, 0, true, None)
            .cell_water(2, 0, false, Some(64.0));
        let plugins = load(&[("A.esm", world_plugin(&[], &world, false))]);
        let world = &Worldspace::from_load_order(&plugins).unwrap()[0];
        assert_eq!(world.parent(), None);
        assert_eq!(world.default_land_height(), Some(-2048.0));
        assert_eq!(world.default_water_height(), Some(-4096.0));
        let bounds = world.declared_bounds().unwrap();
        assert_eq!((bounds.min, bounds.max), ([-10.0, -20.0], [30.0, 40.0]));
        // an XCLW is used as is, a has-water cell without one takes the default, a dry cell has none
        assert_eq!(world.water_height(0, 0), Some(-512.0));
        assert_eq!(world.water_height(1, 0), Some(-4096.0));
        assert_eq!(world.water_height(2, 0), Some(64.0));
        assert_eq!(world.water_height(3, 0), None);
        assert_eq!(world.water_height(9, 9), None);
    }

    #[test]
    fn a_has_water_cell_without_any_default_has_no_height() {
        let world = SyntheticWorld::new(0x0A00, "W")
            .cell(0, 0, Heights::flat(8.0))
            .cell_water(0, 0, true, None);
        let plugins = load(&[("A.esm", world_plugin(&[], &world, false))]);
        assert_eq!(
            Worldspace::from_load_order(&plugins).unwrap()[0].water_height(0, 0),
            None
        );
    }

    fn parent_and_child(child_flags: u16) -> Vec<(String, crate::plugin::Plugin)> {
        let parent = SyntheticWorld::new(0x0B00, "Wasteland")
            .land_data(-1000.0, -200.0)
            .cell(0, 0, Heights::flat(8.0));
        let child = SyntheticWorld::new(0x0B10, "DCworld01")
            .parent(0x0B00, child_flags)
            .cell(5, 5, Heights::flat(16.0));
        load(&[
            ("Base.esm", world_plugin(&[], &parent, false)),
            ("Child.esm", world_plugin(&["Base.esm"], &child, false)),
        ])
    }

    fn named<'a>(worlds: &'a [Worldspace], name: &str) -> &'a Worldspace {
        worlds.iter().find(|w| w.name() == name).unwrap()
    }

    #[test]
    fn a_child_names_its_parent_and_keeps_only_its_own_data_by_default() {
        let worlds = Worldspace::from_load_order(&parent_and_child(0x1)).unwrap();
        let child = named(&worlds, "DCworld01");
        assert_eq!(child.parent(), Some(0x0B00));
        assert_eq!(child.parent_flags(), Some(0x1));
        assert_eq!(child.default_land_height(), None);
        assert_eq!(child.cell_count(), 1);
        assert!(child.cell(0, 0).is_none(), "LAND cells are never inherited");
        assert_eq!(named(&worlds, "Wasteland").parent(), None);
    }

    #[test]
    fn a_flagged_child_takes_the_parents_default_heights_only_when_asked() {
        let plugins = parent_and_child(0x1);
        let index = WorldIndex::new(&plugins).unwrap();
        let (child_id, _) = index.worlds().find(|(_, n)| *n == "DCworld01").unwrap();
        let plain = index.build(&plugins, child_id, Inheritance::None).unwrap();
        assert_eq!(plain.default_land_height(), None);
        let inherited = index
            .build(&plugins, child_id, Inheritance::FlaggedDefaults)
            .unwrap();
        assert_eq!(inherited.default_land_height(), Some(-1000.0));
        assert_eq!(inherited.default_water_height(), Some(-200.0));
        assert!(inherited.cell(0, 0).is_none());
    }

    #[test]
    fn an_unflagged_child_inherits_nothing_even_when_asked() {
        let plugins = parent_and_child(0x8);
        let index = WorldIndex::new(&plugins).unwrap();
        let (child_id, _) = index.worlds().find(|(_, n)| *n == "DCworld01").unwrap();
        let child = index
            .build(&plugins, child_id, Inheritance::FlaggedDefaults)
            .unwrap();
        assert_eq!(child.default_land_height(), None);
    }

    #[test]
    fn inheritance_follows_a_chain_and_survives_a_cycle() {
        let root = SyntheticWorld::new(0x0A, "A").land_data(1.0, 2.0);
        let middle = SyntheticWorld::new(0x0B, "B").parent(0x0A, 0x1);
        let leaf = SyntheticWorld::new(0x0C, "C").parent(0x0B, 0x1);
        let mut bytes = lodwright_testkit::plugin::PluginBuilder::new();
        for w in [&root, &middle, &leaf] {
            bytes = bytes.group(lodwright_testkit::world::world_group(w, false));
        }
        let plugins = load(&[("Chain.esm", bytes.build())]);
        let index = WorldIndex::new(&plugins).unwrap();
        let built = index
            .build(&plugins, 0x0C, Inheritance::FlaggedDefaults)
            .unwrap();
        assert_eq!(built.default_land_height(), Some(1.0));

        let first = SyntheticWorld::new(0x01, "X").parent(0x02, 0x1);
        let second = SyntheticWorld::new(0x02, "Y").parent(0x01, 0x1);
        let mut bytes = lodwright_testkit::plugin::PluginBuilder::new();
        for w in [&first, &second] {
            bytes = bytes.group(lodwright_testkit::world::world_group(w, false));
        }
        let plugins = load(&[("Loop.esm", bytes.build())]);
        let index = WorldIndex::new(&plugins).unwrap();
        let built = index
            .build(&plugins, 0x01, Inheritance::FlaggedDefaults)
            .unwrap();
        assert_eq!(built.default_land_height(), None);
    }

    #[test]
    fn a_world_missing_from_the_index_builds_to_none() {
        let plugins = parent_and_child(0);
        let index = WorldIndex::new(&plugins).unwrap();
        assert!(index.build(&plugins, 0xDEAD, Inheritance::None).is_none());
    }

    /// TTW-shaped load order: both games' masters, the merged plugin that masters both, many child worlds, and a
    /// plugin that overrides the `WRLD` record of the Capital Wasteland.
    fn ttw_load_order() -> Vec<(String, crate::plugin::Plugin)> {
        use lodwright_testkit::plugin::PluginBuilder;
        use lodwright_testkit::world::world_group;
        let mojave = SyntheticWorld::new(0x0A00, "WastelandNV")
            .land_data(-2048.0, 0.0)
            .cell(0, 0, Heights::flat(8.0));
        let capital = SyntheticWorld::new(0x0B00, "Wasteland")
            .land_data(-2048.0, -100.0)
            .cell(0, 0, Heights::flat(16.0))
            .cell(1, 0, Heights::flat(16.0));
        // TTW's own plugin: masters FalloutNV.esm (index 0) and Fallout3.esm (index 1); its children name the
        // Capital Wasteland through master index 1
        let mut ttw = PluginBuilder::new()
            .master("FalloutNV.esm")
            .master("Fallout3.esm");
        for n in 1..=18_u32 {
            let child = SyntheticWorld::new(0x0200_0000 + n * 0x100, &format!("DCworld{n:02}"))
                .parent(0x0100_0B00, 0x1)
                .cell(i32::try_from(n).unwrap(), 0, Heights::flat(32.0));
            ttw = ttw.group(world_group(&child, false));
        }
        // the adjustments plugin overrides Wasteland (master index 2 = TTW, but the world itself is from
        // Fallout3.esm, index 1) with a changed record and one more cell
        let adjusted = SyntheticWorld::new(0x0100_0B00, "Wasteland")
            .land_data(-3000.0, -50.0)
            .cell(2, 0, Heights::flat(24.0));
        let mut adjustments = PluginBuilder::new()
            .master("FalloutNV.esm")
            .master("Fallout3.esm")
            .master("TaleOfTwoWastelands.esm");
        adjustments = adjustments.group(world_group(&adjusted, false));
        chain(&[
            ("FalloutNV.esm", world_plugin(&[], &mojave, false)),
            ("Fallout3.esm", world_plugin(&[], &capital, false)),
            ("TaleOfTwoWastelands.esm", ttw.build()),
            ("CapitalAdjustments.esp", adjustments.build()),
        ])
    }

    #[test]
    fn ttw_children_resolve_their_parent_through_the_fallout_3_master() {
        let worlds = Worldspace::from_load_order(&ttw_load_order()).unwrap();
        assert_eq!(worlds.len(), 2 + 18);
        let dc = named(&worlds, "DCworld07");
        // master index 1 is Fallout3.esm, load index 1
        assert_eq!(dc.parent(), Some(0x0100_0B00 & 0x0100_FFFF));
        assert_eq!(dc.form_id() >> 24, 2);
        assert_eq!(named(&worlds, "WastelandNV").form_id(), 0x0A00);
    }

    #[test]
    fn a_plugin_overriding_wrld_replaces_the_record_and_adds_cells() {
        let worlds = Worldspace::from_load_order(&ttw_load_order()).unwrap();
        let capital = named(&worlds, "Wasteland");
        assert_eq!(capital.form_id(), 0x0100_0B00);
        assert_eq!(capital.default_land_height(), Some(-3000.0));
        assert_eq!(capital.default_water_height(), Some(-50.0));
        assert_eq!(capital.cell_count(), 3);
        assert!(capital.cell(2, 0).is_some());
    }

    #[test]
    fn ttw_children_inherit_the_overridden_defaults_when_flagged() {
        let plugins = ttw_load_order();
        let index = WorldIndex::new(&plugins).unwrap();
        let (id, _) = index.worlds().find(|(_, n)| *n == "DCworld03").unwrap();
        let dc = index
            .build(&plugins, id, Inheritance::FlaggedDefaults)
            .unwrap();
        assert_eq!(dc.default_land_height(), Some(-3000.0));
        assert_eq!(index.worlds().count(), 20);
    }
}
