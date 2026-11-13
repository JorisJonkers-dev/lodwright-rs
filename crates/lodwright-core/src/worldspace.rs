// SPDX-License-Identifier: MPL-2.0
//! Worldspaces assembled across a load order: `FormID`s resolved through master lists, later plugins overriding
//! the cells of earlier ones.
//!
//! Derived from: the `FormID` and load-order rules on the UESP pages (`provenance/sources.toml`:
//! `plugin-formid-masters`).

use std::collections::BTreeMap;

use crate::Error;
use crate::land::{CellHeights, SIDE};
use crate::plugin::Plugin;

/// One worldspace with the final heights of every exterior cell that has LAND.
#[derive(Debug, Clone, PartialEq)]
pub struct Worldspace {
    form_id: u32,
    name: String,
    cells: BTreeMap<(i32, i32), CellHeights>,
}

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

    /// All cells in (x, y) order.
    pub fn cells(&self) -> impl Iterator<Item = ((i32, i32), &CellHeights)> {
        self.cells.iter().map(|(k, v)| (*k, v))
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
        }
    }

    /// Assemble the worldspaces of a load order. Each item is a plugin's file name and its contents, in load
    /// order. Cells of later plugins replace those of earlier ones; the last editor id seen names the world.
    /// Worldspaces come back sorted by absolute `FormID`.
    ///
    /// # Errors
    /// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
    pub fn from_load_order(plugins: &[(String, Plugin)]) -> Result<Vec<Self>, Error> {
        let mut worlds: BTreeMap<u32, Self> = BTreeMap::new();
        for (index, (_, plugin)) in plugins.iter().enumerate() {
            let table = master_table(plugins, plugin)?;
            let own = u32::try_from(index).map_err(|_| Error::Malformed("too many plugins"))?;
            for source in &plugin.worlds {
                let id = absolute(source.form_id, &table, own);
                let world = worlds.entry(id).or_insert_with(|| Self {
                    form_id: id,
                    name: format!("{id:08X}"),
                    cells: BTreeMap::new(),
                });
                if let Some(editor_id) = &source.editor_id {
                    world.name.clone_from(editor_id);
                }
                for cell in &source.cells {
                    world.cells.insert((cell.x, cell.y), cell.heights.clone());
                }
            }
        }
        Ok(worlds.into_values().collect())
    }
}

/// Load-order index of each master of `plugin`, by position in its master list.
fn master_table(plugins: &[(String, Plugin)], plugin: &Plugin) -> Result<Vec<u32>, Error> {
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
fn absolute(form_id: u32, table: &[u32], own: u32) -> u32 {
    let index = usize::try_from(form_id >> 24).unwrap_or(usize::MAX);
    let load_index = table.get(index).copied().unwrap_or(own);
    (load_index << 24) | (form_id & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::{Bounds, Worldspace};
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
}
