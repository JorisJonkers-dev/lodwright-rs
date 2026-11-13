// SPDX-License-Identifier: MPL-2.0
//! Block-aligned raster: the grid that LOD output is cut into (first slice).
//!
//! A raster over a worldspace is addressed in cell coordinates (signed, the origin is the worldspace origin).
//! Output is written per block of `block_cells` x `block_cells` cells. The block size is a parameter: the real
//! sizes per LOD level are an open format question, not a fact this module assumes.

use crate::Error;

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

#[cfg(test)]
mod tests {
    use super::BlockGrid;
    use crate::Error;

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
