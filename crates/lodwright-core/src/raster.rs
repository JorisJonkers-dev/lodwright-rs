// SPDX-License-Identifier: GPL-3.0-only
//! Block-aligned raster: the grid that LOD output is cut into (first slice).
//!
//! A raster over a worldspace is addressed in cell coordinates (signed, the origin is the worldspace origin).
//! Output is written per block of `block_cells` x `block_cells` cells. The block size is a parameter: the real
//! sizes per LOD level are an open format question, not a fact this module assumes.

use crate::Error;
use crate::land::SIDE;
use crate::worldspace::Worldspace;

/// Vertex intervals per cell edge: neighbouring cells share their edge vertices.
pub const CELL_INTERVALS: usize = SIDE - 1;
/// Most samples a [`HeightRaster`] may hold (512 MiB of `f32`); larger worlds are refused, not allocated.
pub const MAX_SAMPLES: u128 = 1 << 27;

/// The cell-to-block mapping for one LOD level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockGrid {
    block_cells: u32,
}

impl BlockGrid {
    /// A grid whose blocks span `block_cells` cells on each axis.
    ///
    /// # Errors
    /// [`Error::ZeroBlockSize`] when `block_cells` is zero.
    pub fn new(block_cells: u32) -> Result<Self, Error> {
        if block_cells == 0 {
            return Err(Error::ZeroBlockSize);
        }
        Ok(Self { block_cells })
    }

    /// Cells per block edge.
    #[must_use]
    pub fn block_cells(self) -> u32 {
        self.block_cells
    }

    /// Index of the block that contains `cell`. Rounds towards negative infinity, so cell -1 is in block -1.
    #[must_use]
    pub fn block_of(self, cell: i32) -> i32 {
        cell.div_euclid(self.step())
    }

    /// First cell of `block`. Saturates at the ends of the `i32` range.
    #[must_use]
    pub fn block_origin(self, block: i32) -> i32 {
        block.saturating_mul(self.step())
    }

    fn step(self) -> i32 {
        i32::try_from(self.block_cells).unwrap_or(i32::MAX)
    }
}

/// The heights of a worldspace on one grid of vertices, covering whole blocks.
///
/// One sample per vertex, `32 * cells + 1` samples per axis because neighbouring cells share their edge row and
/// column (so cell `c` owns local vertices `32 * c ..= 32 * c + 32`). Sample (0, 0) is the south-west corner of the
/// first cell of the first block; `y` grows with the cell grid's `y`. Vertices of cells without LAND hold no data.
/// Where two cells carry different values for a shared vertex, the cell with the larger (y, x) wins.
#[derive(Debug, Clone, PartialEq)]
pub struct HeightRaster {
    grid: BlockGrid,
    origin: (i32, i32),
    cells: (u32, u32),
    width: usize,
    height: usize,
    samples: Vec<f32>,
}

impl HeightRaster {
    /// Rasterise `world` onto whole blocks of `grid`. `None` when the world has no cells.
    ///
    /// # Errors
    /// [`Error::RasterTooLarge`] when the covered area needs more than [`MAX_SAMPLES`] samples.
    pub fn build(world: &Worldspace, grid: BlockGrid) -> Result<Option<Self>, Error> {
        let Some(bounds) = world.bounds() else {
            return Ok(None);
        };
        let first_x = grid.block_of(bounds.min_x);
        let first_y = grid.block_of(bounds.min_y);
        let block_cells = i64::from(grid.block_cells());
        let cells_w =
            (i64::from(grid.block_of(bounds.max_x)) - i64::from(first_x) + 1) * block_cells;
        let cells_h =
            (i64::from(grid.block_of(bounds.max_y)) - i64::from(first_y) + 1) * block_cells;
        let intervals = i128::try_from(CELL_INTERVALS).unwrap_or(32);
        let width = i128::from(cells_w) * intervals + 1;
        let height = i128::from(cells_h) * intervals + 1;
        let total = u128::try_from(width * height).unwrap_or(u128::MAX);
        if total > MAX_SAMPLES {
            return Err(Error::RasterTooLarge { samples: total });
        }
        let (width, height) = (
            usize::try_from(width).unwrap_or(0),
            usize::try_from(height).unwrap_or(0),
        );
        let origin = (grid.block_origin(first_x), grid.block_origin(first_y));
        let mut raster = Self {
            grid,
            origin,
            cells: (
                u32::try_from(cells_w).unwrap_or(0),
                u32::try_from(cells_h).unwrap_or(0),
            ),
            width,
            height,
            samples: vec![f32::NAN; width * height],
        };
        // BTreeMap order is (x, y); write in (y, x) order so the larger (y, x) wins a shared vertex.
        let mut ordered: Vec<_> = world.cells().collect();
        ordered.sort_by_key(|((x, y), _)| (*y, *x));
        for ((x, y), cell) in ordered {
            let base_x =
                usize::try_from(i64::from(x) - i64::from(origin.0)).unwrap_or(0) * CELL_INTERVALS;
            let base_y =
                usize::try_from(i64::from(y) - i64::from(origin.1)).unwrap_or(0) * CELL_INTERVALS;
            for vy in 0..SIDE {
                let row = (base_y + vy) * width + base_x;
                for vx in 0..SIDE {
                    raster.samples[row + vx] = cell.get(vx, vy);
                }
            }
        }
        Ok(Some(raster))
    }

    /// The block grid this raster is aligned to.
    #[must_use]
    pub fn grid(&self) -> BlockGrid {
        self.grid
    }

    /// Cell coordinates of the first covered cell.
    #[must_use]
    pub fn origin_cell(&self) -> (i32, i32) {
        self.origin
    }

    /// Covered cells (wide, high); both are multiples of the block size.
    #[must_use]
    pub fn cells(&self) -> (u32, u32) {
        self.cells
    }

    /// Samples per row.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Rows of samples.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The sample at (`x`, `y`), `None` outside the raster or where no cell has data.
    #[must_use]
    pub fn sample(&self, x: usize, y: usize) -> Option<f32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let value = self.samples[y * self.width + x];
        (!value.is_nan()).then_some(value)
    }

    /// Smallest and largest height present, `None` when no sample has data.
    #[must_use]
    pub fn height_range(&self) -> Option<(f32, f32)> {
        let mut range: Option<(f32, f32)> = None;
        for value in self.samples.iter().filter(|v| !v.is_nan()) {
            range = Some(range.map_or((*value, *value), |(lo, hi)| {
                (lo.min(*value), hi.max(*value))
            }));
        }
        range
    }

    /// The samples of block (`block_x`, `block_y`) in the grid's block numbering: `32 * block_cells + 1` samples
    /// per axis, row-major, `NaN` where there is no data. `None` when the block lies outside the raster.
    #[must_use]
    pub fn block_samples(&self, block_x: i32, block_y: i32) -> Option<Vec<f32>> {
        let span = usize::try_from(self.grid.block_cells()).ok()? * CELL_INTERVALS + 1;
        let first_x = i64::from(self.grid.block_of(self.origin.0));
        let first_y = i64::from(self.grid.block_of(self.origin.1));
        let local_x = usize::try_from(i64::from(block_x) - first_x).ok()?;
        let local_y = usize::try_from(i64::from(block_y) - first_y).ok()?;
        let blocks_w = usize::try_from(self.cells.0 / self.grid.block_cells()).ok()?;
        let blocks_h = usize::try_from(self.cells.1 / self.grid.block_cells()).ok()?;
        if local_x >= blocks_w || local_y >= blocks_h {
            return None;
        }
        let (start_x, start_y) = (local_x * (span - 1), local_y * (span - 1));
        let mut out = Vec::with_capacity(span * span);
        for y in 0..span {
            let row = (start_y + y) * self.width + start_x;
            out.extend_from_slice(&self.samples[row..row + span]);
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockGrid, CELL_INTERVALS, HeightRaster, MAX_SAMPLES};
    use crate::Error;
    use crate::land::CellHeights;
    use crate::worldspace::Worldspace;
    use lodwright_testkit::rng::Rng;

    fn world(cells: &[(i32, i32, f32)]) -> Worldspace {
        Worldspace::from_cells(
            1,
            "W",
            cells
                .iter()
                .map(|(x, y, h)| ((*x, *y), CellHeights::flat(*h))),
        )
    }

    #[test]
    fn an_empty_world_has_no_raster() {
        let grid = BlockGrid::new(4).unwrap();
        assert_eq!(HeightRaster::build(&world(&[]), grid), Ok(None));
    }

    #[test]
    fn one_cell_is_padded_to_a_whole_block_and_keeps_its_height() {
        let grid = BlockGrid::new(4).unwrap();
        let raster = HeightRaster::build(&world(&[(1, 2, 80.0)]), grid)
            .unwrap()
            .unwrap();
        assert_eq!(raster.cells(), (4, 4));
        assert_eq!(raster.origin_cell(), (0, 0));
        assert_eq!(raster.width(), 4 * CELL_INTERVALS + 1);
        assert_eq!(raster.height(), 4 * CELL_INTERVALS + 1);
        assert_eq!(
            raster.sample(CELL_INTERVALS, 2 * CELL_INTERVALS),
            Some(80.0)
        );
        assert_eq!(raster.sample(0, 0), None);
        assert_eq!(raster.sample(raster.width(), 0), None);
        assert_eq!(raster.height_range(), Some((80.0, 80.0)));
    }

    #[test]
    fn negative_cells_start_in_the_block_below_the_origin() {
        let grid = BlockGrid::new(8).unwrap();
        let raster = HeightRaster::build(&world(&[(-1, -9, 8.0), (3, 2, 8.0)]), grid)
            .unwrap()
            .unwrap();
        assert_eq!(raster.origin_cell(), (-8, -16));
        assert_eq!(raster.cells(), (16, 24));
    }

    #[test]
    fn the_shared_edge_reads_one_value_and_the_larger_cell_wins() {
        let grid = BlockGrid::new(2).unwrap();
        let raster = HeightRaster::build(&world(&[(0, 0, 8.0), (1, 0, 16.0)]), grid)
            .unwrap()
            .unwrap();
        assert_eq!(raster.sample(CELL_INTERVALS - 1, 0), Some(8.0));
        assert_eq!(raster.sample(CELL_INTERVALS, 0), Some(16.0));
        assert_eq!(raster.sample(CELL_INTERVALS + 1, 0), Some(16.0));
    }

    #[test]
    fn width_and_height_are_whole_blocks_for_random_worlds() {
        let mut rng = Rng::new(11);
        for _ in 0..100 {
            let size = [1_u32, 2, 4, 8][usize::try_from(rng.below(4)).unwrap()];
            let grid = BlockGrid::new(size).unwrap();
            let cells: Vec<(i32, i32, f32)> = (0..=rng.below(5))
                .map(|_| {
                    (
                        i32::try_from(rng.below(40)).unwrap() - 20,
                        i32::try_from(rng.below(40)).unwrap() - 20,
                        8.0,
                    )
                })
                .collect();
            let raster = HeightRaster::build(&world(&cells), grid).unwrap().unwrap();
            let (w, h) = raster.cells();
            assert_eq!(w % size, 0);
            assert_eq!(h % size, 0);
            assert_eq!(raster.origin_cell().0 % i32::try_from(size).unwrap(), 0);
            for (x, y, _) in &cells {
                let vx = usize::try_from((x - raster.origin_cell().0) * 32).unwrap() + 3;
                let vy = usize::try_from((y - raster.origin_cell().1) * 32).unwrap() + 3;
                assert_eq!(raster.sample(vx, vy), Some(8.0));
            }
        }
    }

    #[test]
    fn a_block_crop_returns_exactly_that_blocks_cells() {
        let grid = BlockGrid::new(2).unwrap();
        let raster = HeightRaster::build(
            &world(&[(0, 0, 8.0), (1, 1, 16.0), (2, 0, 24.0), (3, 1, 32.0)]),
            grid,
        )
        .unwrap()
        .unwrap();
        let first = raster.block_samples(0, 0).unwrap();
        let second = raster.block_samples(1, 0).unwrap();
        let span = 2 * CELL_INTERVALS + 1;
        assert_eq!(first.len(), span * span);
        assert_eq!(first[0], 8.0);
        assert_eq!(first[span * span - 1 - CELL_INTERVALS], 16.0);
        assert_eq!(second[0], 24.0);
        assert!(second[span - 1].is_nan());
        assert_eq!(raster.block_samples(0, 1), None);
        assert_eq!(raster.block_samples(-1, 0), None);
        assert_eq!(raster.block_samples(2, 0), None);
    }

    #[test]
    fn a_world_too_big_for_a_raster_is_refused_not_allocated() {
        let grid = BlockGrid::new(32).unwrap();
        let sparse = world(&[(-100_000, 0, 8.0), (100_000, 0, 8.0)]);
        assert!(matches!(
            HeightRaster::build(&sparse, grid),
            Err(Error::RasterTooLarge { samples }) if samples > MAX_SAMPLES
        ));
        let extreme = world(&[(i32::MIN, i32::MIN, 8.0), (i32::MAX, i32::MAX, 8.0)]);
        assert!(matches!(
            HeightRaster::build(&extreme, grid),
            Err(Error::RasterTooLarge { .. })
        ));
    }

    #[test]
    fn the_height_range_spans_every_cell() {
        let grid = BlockGrid::new(2).unwrap();
        let raster = HeightRaster::build(&world(&[(0, 0, -40.0), (1, 0, 120.0)]), grid)
            .unwrap()
            .unwrap();
        assert_eq!(raster.height_range(), Some((-40.0, 120.0)));
    }

    #[test]
    fn the_grid_is_kept() {
        let grid = BlockGrid::new(4).unwrap();
        let raster = HeightRaster::build(&world(&[(0, 0, 8.0)]), grid)
            .unwrap()
            .unwrap();
        assert_eq!(raster.grid(), grid);
    }

    #[test]
    fn zero_sized_blocks_are_rejected() {
        assert_eq!(BlockGrid::new(0), Err(Error::ZeroBlockSize));
    }

    #[test]
    fn cells_map_to_blocks_across_the_origin() {
        let grid = BlockGrid::new(8).unwrap();
        assert_eq!(grid.block_of(0), 0);
        assert_eq!(grid.block_of(7), 0);
        assert_eq!(grid.block_of(8), 1);
        assert_eq!(grid.block_of(-1), -1);
        assert_eq!(grid.block_of(-8), -1);
        assert_eq!(grid.block_of(-9), -2);
    }

    #[test]
    fn every_cell_lies_inside_its_block() {
        for size in [1_u32, 4, 8, 16, 32] {
            let grid = BlockGrid::new(size).unwrap();
            for cell in -70..70 {
                let origin = grid.block_origin(grid.block_of(cell));
                assert!(
                    origin <= cell && cell < origin + i32::try_from(size).unwrap(),
                    "{size} {cell}"
                );
            }
        }
    }

    #[test]
    fn block_size_is_reported() {
        assert_eq!(BlockGrid::new(32).unwrap().block_cells(), 32);
    }

    #[test]
    fn block_origin_saturates_instead_of_overflowing() {
        let grid = BlockGrid::new(32).unwrap();
        assert_eq!(grid.block_origin(i32::MAX), i32::MAX);
    }
}
