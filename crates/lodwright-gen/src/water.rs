// SPDX-License-Identifier: GPL-3.0-only
//! Water planes for terrain blocks: flat rectangles at each cell's water height over the ground that lies below it.
//!
//! The game's own blocks carry a water shape (`game-lod-water-shape`) whose outline and slopes come from a rule
//! that is not established; this module is our own design. It covers the same ground at the same heights
//! (`game-lod-water-height`) with as few rectangles as the grid allows: the block is cut into squares (128 units at
//! level 4, twice that per level doubling), a square is wet when the mean of the ground samples on it lies below its
//! cell's water height, and the wet squares of each cell are merged into rectangles. Of the rules tried (any, half,
//! mean or all of the samples below water), the mean agrees best with the game's blocks at every level.

use lodwright_core::worldspace::Worldspace;

const CELL_UNITS: f32 = 4096.0;
const CELL_INTERVALS: usize = 32;
/// At level 4 a square is wet only when its mean ground lies this many units below the water height. Own design,
/// fitted to the game's level 4 blocks of the main world: of the margins tried (0 to 60 units) 25 leaves the most
/// blocks at 0.95 overlap or better with an unchanged mean overlap.
const LEVEL4_WET_MARGIN: f32 = 25.0;

/// Whether a block with water gets the game's empty one-vertex segmented shape between the terrain and the water:
/// every cell of the block has water at the world's default height and that height is not above 0. Measured on the
/// game's blocks (`game-lod-water-placeholder`): all 212 blocks that have it satisfy the rule, 3 blocks that satisfy it
/// do not have it, and where the default is above 0 (two small worlds, 33 blocks) the game writes none. Own fit, not a statement of the game's rule.
fn wants_placeholder(world: &Worldspace, cells: usize, cell: (i32, i32)) -> bool {
    // source: game-lod-water-placeholder
    let Some(default) = world.default_water_height() else {
        return false;
    };
    let Ok(n) = i32::try_from(cells) else {
        return false;
    };
    default <= 0.0
        && (cell.0..cell.0 + n).all(|x| (cell.1..cell.1 + n).all(|y| world.water_is_default(x, y)))
}

/// The water plane of one block, relative to the block origin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WaterMesh {
    pub vertices: Vec<[f32; 3]>,
    pub indices: Vec<u16>,
    /// For a level 4 block, the triangle count of each cell in the order x * 4 + y.
    pub segments: Option<[u32; 16]>,
    /// The block gets the game's empty one-vertex shape between the terrain and the water (see [`wants_placeholder`]).
    pub placeholder: bool,
}

/// Greedy rectangles over a square boolean grid: (x, y, width, height) in squares, row by row.
fn rectangles(wet: &[bool], n: usize) -> Vec<(usize, usize, usize, usize)> {
    let keys: Vec<Option<u32>> = wet.iter().map(|w| w.then_some(0)).collect();
    rectangles_by_key(&keys, n)
}

/// Greedy rectangles over a square grid of keys (`None` is dry): a rectangle holds squares of one key only.
fn rectangles_by_key(keys: &[Option<u32>], n: usize) -> Vec<(usize, usize, usize, usize)> {
    let mut used = vec![false; keys.len()];
    let free = |used: &[bool], x: usize, y: usize, key: Option<u32>| {
        keys[y * n + x].is_some() && keys[y * n + x] == key && !used[y * n + x]
    };
    let mut out = Vec::new();
    for y in 0..n {
        for x in 0..n {
            let key = keys[y * n + x];
            if !free(&used, x, y, key) {
                continue;
            }
            let mut w = 1;
            while x + w < n && free(&used, x + w, y, key) {
                w += 1;
            }
            let mut h = 1;
            while y + h < n && (x..x + w).all(|xx| free(&used, xx, y + h, key)) {
                h += 1;
            }
            for yy in y..y + h {
                for xx in x..x + w {
                    used[yy * n + xx] = true;
                }
            }
            out.push((x, y, w, h));
        }
    }
    out
}

/// The water plane of the block of `level` cells whose south-west cell is `cell`, from the block's height samples
/// (`level * 32 + 1` per side, row-major, `NaN` where there is no ground). `None` when no ground lies below water
/// or the plane would exceed the 16-bit vertex range.
pub(crate) fn water_mesh(
    world: &Worldspace,
    samples: &[f32],
    level: u32,
    cell: (i32, i32),
) -> Option<WaterMesh> {
    let cells = usize::try_from(level).ok()?;
    let span = cells * CELL_INTERVALS + 1;
    if samples.len() != span * span {
        return None;
    }
    // samples per square side and squares per cell side
    let step = (cells / 4).max(1);
    let per_cell = (CELL_INTERVALS / step).max(1);
    let square = CELL_UNITS / per_cell as f32;
    if cells > 4 {
        return coarse_water(world, samples, cells, cell);
    }
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    let mut segments = [0_u32; 16];
    // cells in x-major order, so a level 4 block's triangles come in segment order
    for cx in 0..cells {
        for cy in 0..cells {
            let (wx, wy) = (
                cell.0 + i32::try_from(cx).ok()?,
                cell.1 + i32::try_from(cy).ok()?,
            );
            // source: game-lod-water-height
            let Some(height) = world.water_height(wx, wy) else {
                continue;
            };
            let mut wet = vec![false; per_cell * per_cell];
            for qy in 0..per_cell {
                for qx in 0..per_cell {
                    let (x0, y0) = (
                        cx * CELL_INTERVALS + qx * step,
                        cy * CELL_INTERVALS + qy * step,
                    );
                    let (sum, n) = (y0..=y0 + step)
                        .flat_map(|y| (x0..=x0 + step).map(move |x| samples[y * span + x]))
                        .filter(|h| !h.is_nan())
                        .fold((0.0_f32, 0_u32), |(s, n), h| (s + h, n + 1));
                    wet[qy * per_cell + qx] =
                        n > 0 && sum / (n as f32) < height - LEVEL4_WET_MARGIN;
                }
            }
            let rects = rectangles(&wet, per_cell);
            if cells == 4 {
                segments[cx * 4 + cy] = u32::try_from(rects.len() * 2).ok()?;
            }
            let (ox, oy) = (cx as f32 * CELL_UNITS, cy as f32 * CELL_UNITS);
            for (x, y, w, h) in rects {
                let first = u16::try_from(vertices.len()).ok()?;
                let (x0, y0) = (ox + x as f32 * square, oy + y as f32 * square);
                let (x1, y1) = (x0 + w as f32 * square, y0 + h as f32 * square);
                vertices.extend([
                    [x0, y0, height],
                    [x1, y0, height],
                    [x0, y1, height],
                    [x1, y1, height],
                ]);
                u16::try_from(vertices.len() - 1).ok()?;
                indices.extend([0, 1, 2, 2, 1, 3].map(|i| first + i));
            }
        }
    }
    if vertices.is_empty() {
        return None;
    }
    Some(WaterMesh {
        vertices,
        indices,
        segments: (cells == 4).then_some(segments),
        placeholder: wants_placeholder(world, cells, cell),
    })
}

/// The water plane of a block above level 4 (a plain shape without segments): wet squares of equal water height merge
/// into rectangles across cell borders, which keeps the covered ground and heights of the per-cell plane and writes
/// fewer vertices (the game's blocks above level 4 carry a median of 61, 38 and 30 water vertices, `game-lod-water-outline`).
fn coarse_water(
    world: &Worldspace,
    samples: &[f32],
    cells: usize,
    cell: (i32, i32),
) -> Option<WaterMesh> {
    let span = cells * CELL_INTERVALS + 1;
    let step = (cells / 4).max(1);
    let per_cell = (CELL_INTERVALS / step).max(1);
    let square = CELL_UNITS / per_cell as f32;
    let n = cells * per_cell;
    let mut heights = vec![f32::NAN; n * n];
    let mut keys: Vec<Option<u32>> = vec![None; n * n];
    for cx in 0..cells {
        for cy in 0..cells {
            let (wx, wy) = (
                cell.0 + i32::try_from(cx).ok()?,
                cell.1 + i32::try_from(cy).ok()?,
            );
            // source: game-lod-water-height
            let Some(height) = world.water_height(wx, wy) else {
                continue;
            };
            for qy in 0..per_cell {
                for qx in 0..per_cell {
                    let (x0, y0) = (
                        cx * CELL_INTERVALS + qx * step,
                        cy * CELL_INTERVALS + qy * step,
                    );
                    let (sum, count) = (y0..=y0 + step)
                        .flat_map(|y| (x0..=x0 + step).map(move |x| samples[y * span + x]))
                        .filter(|h| !h.is_nan())
                        .fold((0.0_f32, 0_u32), |(s, c), h| (s + h, c + 1));
                    if count > 0 && sum / (count as f32) < height {
                        let at = (cy * per_cell + qy) * n + cx * per_cell + qx;
                        heights[at] = height;
                        keys[at] = Some(height.to_bits());
                    }
                }
            }
        }
    }
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    for (x, y, w, h) in rectangles_by_key(&keys, n) {
        let height = heights[y * n + x];
        let first = u16::try_from(vertices.len()).ok()?;
        let (x0, y0) = (x as f32 * square, y as f32 * square);
        let (x1, y1) = (x0 + w as f32 * square, y0 + h as f32 * square);
        vertices.extend([
            [x0, y0, height],
            [x1, y0, height],
            [x0, y1, height],
            [x1, y1, height],
        ]);
        u16::try_from(vertices.len() - 1).ok()?;
        indices.extend([0, 1, 2, 2, 1, 3].map(|i| first + i));
    }
    if vertices.is_empty() {
        return None;
    }
    Some(WaterMesh {
        vertices,
        indices,
        segments: None,
        placeholder: wants_placeholder(world, cells, cell),
    })
}

#[cfg(test)]
mod tests {
    use super::{rectangles, water_mesh};
    use lodwright_core::land::CellHeights;
    use lodwright_core::worldspace::Worldspace;

    #[test]
    fn wet_squares_merge_into_rectangles() {
        // 3 by 3: an L shape
        let wet = [true, true, false, true, true, false, true, false, false];
        assert_eq!(rectangles(&wet, 3), vec![(0, 0, 2, 2), (0, 2, 1, 1)]);
        assert_eq!(rectangles(&[false; 4], 2), vec![]);
    }

    fn world(water: Option<f32>) -> Worldspace {
        let mut w = Worldspace::from_cells(1, "Mojave", vec![((0, 0), CellHeights::flat(0.0))]);
        if let Some(h) = water {
            w.set_cell_water(0, 0, true, Some(h));
        }
        w
    }

    fn placeholder_of(default: f32, own: Option<(i32, i32)>) -> Option<bool> {
        let cells = (0..4).flat_map(|x| (0..4).map(move |y| ((x, y), CellHeights::flat(-200.0))));
        let mut w = Worldspace::from_cells(1, "Mojave", cells);
        w.set_default_water_height(Some(default));
        for x in 0..4 {
            for y in 0..4 {
                let height = if own == Some((x, y)) { -50.0 } else { f32::MAX };
                w.set_cell_water(x, y, true, Some(height));
            }
        }
        let span = 4 * 32 + 1;
        water_mesh(&w, &vec![-200.0; span * span], 4, (0, 0)).map(|m| m.placeholder)
    }

    #[test]
    fn a_block_of_default_water_at_or_below_zero_gets_the_placeholder_shape() {
        // every cell on the world's default (an XCLW of the largest float), default height -100
        assert_eq!(placeholder_of(-100.0, None), Some(true));
        assert_eq!(placeholder_of(0.0, None), Some(true));
        // one cell with a height of its own, or a default above 0 (small worlds in the measurement): none
        assert_eq!(placeholder_of(-100.0, Some((2, 1))), Some(false));
        assert_eq!(placeholder_of(5.0, None), Some(false));
        // a block with no water gets no plane, so no placeholder either
        assert_eq!(placeholder_of(-300.0, None), None);
    }

    #[test]
    fn a_cell_below_its_water_gets_one_rectangle_in_its_segment() {
        let span = 4 * 32 + 1;
        let mut samples = vec![f32::NAN; span * span];
        for y in 0..=32 {
            for x in 0..=32 {
                samples[y * span + x] = 0.0;
            }
        }
        let mesh = water_mesh(&world(Some(100.0)), &samples, 4, (0, 0)).unwrap();
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.vertices[3], [4096.0, 4096.0, 100.0]);
        assert_eq!(mesh.indices, vec![0, 1, 2, 2, 1, 3]);
        let mut want = [0; 16];
        want[0] = 2;
        assert_eq!(mesh.segments, Some(want));
        assert!(water_mesh(&world(Some(-10.0)), &samples, 4, (0, 0)).is_none());
        // ground only 10 units under the water: inside the level 4 margin, so dry
        assert!(water_mesh(&world(Some(10.0)), &samples, 4, (0, 0)).is_none());
        assert!(water_mesh(&world(None), &samples, 4, (0, 0)).is_none());
    }

    #[test]
    fn at_coarser_levels_cells_of_one_water_height_merge_and_other_heights_do_not() {
        let span = 8 * 32 + 1;
        let mut samples = vec![f32::NAN; span * span];
        // ground at 0 over the two cells (0, 0) and (1, 0)
        for y in 0..=32 {
            for x in 0..=64 {
                samples[y * span + x] = 0.0;
            }
        }
        let water = |a: f32, b: f32| {
            let mut w = Worldspace::from_cells(
                1,
                "Mojave",
                vec![
                    ((0, 0), CellHeights::flat(0.0)),
                    ((1, 0), CellHeights::flat(0.0)),
                ],
            );
            w.set_cell_water(0, 0, true, Some(a));
            w.set_cell_water(1, 0, true, Some(b));
            w
        };
        let same = water_mesh(&water(10.0, 10.0), &samples, 8, (0, 0)).unwrap();
        assert_eq!(same.vertices.len(), 4, "one rectangle over both cells");
        assert_eq!(same.vertices[3], [8192.0, 4096.0, 10.0]);
        let apart = water_mesh(&water(10.0, 20.0), &samples, 8, (0, 0)).unwrap();
        assert_eq!(apart.vertices.len(), 8);
    }

    #[test]
    fn coarser_levels_use_coarser_squares_and_no_segments() {
        let span = 8 * 32 + 1;
        let mut samples = vec![f32::NAN; span * span];
        samples[0] = -5.0;
        let mesh = water_mesh(&world(Some(0.0)), &samples, 8, (0, 0)).unwrap();
        assert_eq!(mesh.vertices[3], [256.0, 256.0, 0.0]);
        assert!(mesh.segments.is_none());
    }
}
