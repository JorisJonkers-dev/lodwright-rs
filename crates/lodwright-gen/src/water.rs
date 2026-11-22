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

/// The height of the walls' foot: the constant the game's water outlines drop to (`game-lod-water-outline`).
const WALL_FOOT: f32 = -14098.0;

/// The wet squares of the block as `water_mesh` decides them: per square the bits of its water height, row-major over
/// `n * n` squares of `square` units; the same rule as the planes themselves.
fn wet_grid(
    world: &Worldspace,
    samples: &[f32],
    cells: usize,
    cell: (i32, i32),
) -> Option<(usize, Vec<Option<u32>>, f32)> {
    let span = cells * CELL_INTERVALS + 1;
    let step = (cells / 4).max(1);
    let per_cell = (CELL_INTERVALS / step).max(1);
    let n = cells * per_cell;
    let margin = if cells == 4 { LEVEL4_WET_MARGIN } else { 0.0 };
    let mut grid = vec![None; n * n];
    for cx in 0..cells {
        for cy in 0..cells {
            let (wx, wy) = (
                cell.0 + i32::try_from(cx).ok()?,
                cell.1 + i32::try_from(cy).ok()?,
            );
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
                    if count > 0 && sum / (count as f32) < height - margin {
                        grid[(cy * per_cell + qy) * n + cx * per_cell + qx] =
                            Some(height.to_bits());
                    }
                }
            }
        }
    }
    Some((n, grid, CELL_UNITS / per_cell as f32))
}

/// A wall under one run of exposed square sides: its two top corners (counter-clockwise seen from outside, so the
/// outside is on the right of the direction of travel), the water height and the group it belongs to.
struct Wall {
    from: [f32; 2],
    to: [f32; 2],
    height: f32,
    group: usize,
}

/// How far a level 4 plane and its walls reach beyond the block's south and west borders and past its north border:
/// the game's blocks overlap their neighbours there (`game-lod-water-walls`), so no crack opens between planes.
const BORDER_OVERLAP: f32 = 32.0;

/// The walls of the water of one block: under every side of a wet square that lies on the block's own border, from
/// the water down to [`WALL_FOOT`]. The sides inside the block get none. Runs of equal height along a side are one
/// wall; at level 4 a run stops at the cell's border, as the segments are per cell, and the wall stands where the
/// extended plane ends: [`BORDER_OVERLAP`] beyond the south, west and north borders, on the east border itself.
fn walls_of(grid: &[Option<u32>], n: usize, square: f32, per_cell: Option<usize>) -> Vec<Wall> {
    let exposed = |x: usize, y: usize, side: usize| -> bool {
        let Some(_) = grid[y * n + x] else {
            return false;
        };
        match side {
            0 => y == 0,
            1 => x + 1 == n,
            2 => y + 1 == n,
            _ => x == 0,
        }
    };
    let overlap = if per_cell.is_some() {
        BORDER_OVERLAP
    } else {
        0.0
    };
    let group_of = |x: usize, y: usize| per_cell.map_or(0, |p| (x / p) * 4 + y / p);
    let mut out = Vec::new();
    for side in 0..4 {
        // only the border line of each side can carry a wall: line 0 or line n - 1
        let line = match side {
            0 | 3 => 0,
            _ => n - 1,
        };
        // an open run: where it starts, its water height and its group
        let mut run: Option<(usize, u32, usize)> = None;
        for k in 0..=n {
            let (x, y) = if side % 2 == 0 { (k, line) } else { (line, k) };
            let here = (k < n && exposed(x, y, side))
                .then(|| (grid[y * n + x].unwrap_or(0), group_of(x, y)));
            // a run goes on while the water height and the group (the cell, at level 4) stay the same
            let continues = match (run, here) {
                (Some((_, bits, group)), Some((hb, hg))) => bits == hb && group == hg,
                _ => false,
            };
            if continues {
                continue;
            }
            if let Some((start, bits, group)) = run.take() {
                let (a, b) = (start as f32 * square, k as f32 * square);
                let (south, west) = (-overlap, -overlap);
                let (north, east) = (n as f32 * square + overlap, n as f32 * square);
                let (from, to) = match side {
                    0 => ([a, south], [b, south]),
                    1 => ([east, a], [east, b]),
                    2 => ([b, north], [a, north]),
                    _ => ([west, b], [west, a]),
                };
                out.push(Wall {
                    from,
                    to,
                    height: f32::from_bits(bits),
                    group,
                });
            }
            if let Some((bits, group)) = here {
                run = Some((k, bits, group));
            }
        }
    }
    out
}

/// Let a level 4 plane overlap its neighbours as the game's do: the rectangles on the south and west borders reach
/// [`BORDER_OVERLAP`] beyond them and those on the north border beyond that one, so the seam between two planes is
/// covered and not a hairline of rasterisation gaps.
fn overlap_borders(mesh: &mut WaterMesh, level: u32) {
    if level != 4 {
        return;
    }
    let far = 4.0 * CELL_UNITS;
    for v in &mut mesh.vertices {
        if v[0] == 0.0 {
            v[0] = -BORDER_OVERLAP;
        }
        if v[1] == 0.0 {
            v[1] = -BORDER_OVERLAP;
        } else if v[1].to_bits() == far.to_bits() {
            v[1] = far + BORDER_OVERLAP;
        }
    }
}

/// Add the walls on the block's borders and the plane's overlap across them to `mesh` (the option
/// `terrain.water_walls`; `game-lod-water-walls`).
pub(crate) fn add_walls(
    mesh: &mut WaterMesh,
    world: &Worldspace,
    samples: &[f32],
    level: u32,
    cell: (i32, i32),
) -> Option<()> {
    let cells = usize::try_from(level).ok()?;
    let (n, grid, square) = wet_grid(world, samples, cells, cell)?;
    let per_cell = (cells == 4).then_some(n / 4);
    overlap_borders(mesh, level);
    let walls = walls_of(&grid, n, square, per_cell);
    if walls.is_empty() {
        return Some(());
    }
    let groups = mesh.segments.map(|s| s.map(|c| c as usize));
    // the triangles of each segment, then the walls of that segment's cell
    let mut per_group: Vec<Vec<u16>> = vec![Vec::new(); 16];
    if let Some(counts) = groups {
        let mut at = 0;
        for (g, count) in counts.iter().enumerate() {
            per_group[g].extend_from_slice(&mesh.indices[at * 3..(at + count) * 3]);
            at += count;
        }
    } else {
        per_group[0].extend_from_slice(&mesh.indices);
    }
    for wall in &walls {
        let first = u16::try_from(mesh.vertices.len()).ok()?;
        u16::try_from(mesh.vertices.len() + 3).ok()?;
        mesh.vertices.extend([
            [wall.from[0], wall.from[1], wall.height],
            [wall.from[0], wall.from[1], WALL_FOOT],
            [wall.to[0], wall.to[1], WALL_FOOT],
            [wall.to[0], wall.to[1], wall.height],
        ]);
        per_group[wall.group].extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    }
    mesh.indices = per_group.concat();
    if let Some(segments) = &mut mesh.segments {
        for (g, indices) in per_group.iter().enumerate() {
            segments[g] = u32::try_from(indices.len() / 3).ok()?;
        }
    }
    Some(())
}

/// How the water plane of a block above level 4 is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plane {
    /// Four vertices per rectangle.
    Rectangles,
    /// A corner that two rectangles have in common is written once.
    Shared,
    /// The outline of each region is traced, reduced to its corners and triangulated.
    Polygons,
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
    plane: Plane,
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
        return coarse_water(world, samples, cells, cell, plane);
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
    plane: Plane,
) -> Option<WaterMesh> {
    let share = plane != Plane::Rectangles;
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
    if plane == Plane::Polygons {
        // a region that cannot be triangulated keeps the rectangles of this block
        if let Some((vertices, indices)) = crate::outline::plane(&keys, &heights, n, square) {
            return Some(WaterMesh {
                vertices,
                indices,
                segments: None,
                placeholder: wants_placeholder(world, cells, cell),
            });
        }
    }
    let (mut vertices, mut indices) = (Vec::new(), Vec::new());
    // with `share`, a corner that two rectangles have in common is written once
    let mut known: std::collections::HashMap<[u32; 3], u16> = std::collections::HashMap::new();
    for (x, y, w, h) in rectangles_by_key(&keys, n) {
        let height = heights[y * n + x];
        let (x0, y0) = (x as f32 * square, y as f32 * square);
        let (x1, y1) = (x0 + w as f32 * square, y0 + h as f32 * square);
        let mut corner = |p: [f32; 3]| -> Option<u16> {
            let key = p.map(f32::to_bits);
            if share && let Some(i) = known.get(&key) {
                return Some(*i);
            }
            let i = u16::try_from(vertices.len()).ok()?;
            vertices.push(p);
            known.insert(key, i);
            Some(i)
        };
        let sw = corner([x0, y0, height])?;
        let se = corner([x1, y0, height])?;
        let nw = corner([x0, y1, height])?;
        let ne = corner([x1, y1, height])?;
        indices.extend([sw, se, nw, nw, se, ne]);
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
    use super::{Plane, add_walls, rectangles, water_mesh};
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
        water_mesh(&w, &vec![-200.0; span * span], 4, (0, 0), Plane::Rectangles)
            .map(|m| m.placeholder)
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
        let mesh = water_mesh(&world(Some(100.0)), &samples, 4, (0, 0), Plane::Rectangles).unwrap();
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.vertices[3], [4096.0, 4096.0, 100.0]);
        assert_eq!(mesh.indices, vec![0, 1, 2, 2, 1, 3]);
        let mut want = [0; 16];
        want[0] = 2;
        assert_eq!(mesh.segments, Some(want));
        assert!(water_mesh(&world(Some(-10.0)), &samples, 4, (0, 0), Plane::Rectangles).is_none());
        // ground only 10 units under the water: inside the level 4 margin, so dry
        assert!(water_mesh(&world(Some(10.0)), &samples, 4, (0, 0), Plane::Rectangles).is_none());
        assert!(water_mesh(&world(None), &samples, 4, (0, 0), Plane::Rectangles).is_none());
    }

    /// Ground at 0 over the cells `x0..x1` by `0..1` of a block of `level` cells, water at 100 over those cells.
    fn strip(level: u32, x0: i32, x1: i32) -> (Worldspace, Vec<f32>) {
        let span = level as usize * 32 + 1;
        let mut samples = vec![f32::NAN; span * span];
        let mut cells = Vec::new();
        for cx in x0..x1 {
            cells.push(((cx, 0), CellHeights::flat(0.0)));
            for y in 0..=32 {
                for x in 0..=32 {
                    samples[y * span + cx as usize * 32 + x] = 0.0;
                }
            }
        }
        let mut w = Worldspace::from_cells(1, "Mojave", cells);
        for cx in x0..x1 {
            w.set_cell_water(cx, 0, true, Some(100.0));
        }
        (w, samples)
    }

    #[test]
    fn walls_stand_on_the_block_border_and_not_inside_it() {
        // level 8, water over the cells 2 and 3 of the south row: only the south side is on the block's border; its
        // wall is one run of two triangles, the sides inside the block (west, east, north of the water) get none
        let (w, samples) = strip(8, 2, 4);
        let mut mesh = water_mesh(&w, &samples, 8, (0, 0), Plane::Rectangles).unwrap();
        let (vertices, triangles) = (mesh.vertices.len(), mesh.indices.len() / 3);
        assert_eq!((vertices, triangles), (4, 2));
        add_walls(&mut mesh, &w, &samples, 8, (0, 0)).unwrap();
        assert_eq!(mesh.vertices.len(), vertices + 4);
        assert_eq!(mesh.indices.len() / 3, triangles + 2);
        let feet = mesh
            .vertices
            .iter()
            .filter(|v| v[2] == super::WALL_FOOT)
            .count();
        assert_eq!(feet, 2);
        // above level 4 the wall stands on the border itself, and the plane does not overlap
        assert!(mesh.vertices[vertices..].iter().all(|v| v[1] == 0.0));
        assert!(mesh.vertices[..vertices].iter().all(|v| v[1] >= 0.0));
        // every wall is vertical: its corners share x and y in pairs
        for t in mesh.indices[6..].as_chunks::<3>().0 {
            let p = t.map(|i| mesh.vertices[usize::from(i)]);
            let flat = |a: [f32; 3], b: [f32; 3]| {
                a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits()
            };
            assert!(flat(p[0], p[1]) || flat(p[1], p[2]) || flat(p[0], p[2]));
        }
        // the plane of an unwalled block is as before
        let plain = water_mesh(&w, &samples, 8, (0, 0), Plane::Rectangles).unwrap();
        assert_eq!(plain.vertices.len(), 4);
    }

    #[test]
    fn level_4_planes_overlap_their_neighbours_and_the_walls_stand_in_their_cells_segment() {
        // water over cells 1 and 2 of the south row of a level 4 block: the south side of each cell is on the border;
        // the plane reaches 32 units past it and each wall is a run of its own cell's segment
        let (w, samples) = strip(4, 1, 3);
        let mut mesh = water_mesh(&w, &samples, 4, (0, 0), Plane::Rectangles).unwrap();
        let before = mesh.segments.unwrap();
        assert_eq!((before[4], before[8]), (2, 2));
        assert!(mesh.vertices.iter().any(|v| v[1] == 0.0));
        add_walls(&mut mesh, &w, &samples, 4, (0, 0)).unwrap();
        let after = mesh.segments.unwrap();
        // one wall of two triangles per cell on top of the plane
        assert_eq!((after[4], after[8]), (2 + 2, 2 + 2));
        assert_eq!(after.iter().sum::<u32>() as usize, mesh.indices.len() / 3);
        assert!(mesh.vertices.iter().all(|v| v[1] != 0.0));
        assert!(
            mesh.vertices
                .iter()
                .filter(|v| v[2] != super::WALL_FOOT)
                .any(|v| v[1].to_bits() == (-32.0_f32).to_bits())
        );
        assert!(
            mesh.indices
                .iter()
                .all(|i| usize::from(*i) < mesh.vertices.len())
        );
    }

    #[test]
    fn a_whole_block_of_water_has_a_wall_on_each_of_its_four_borders() {
        let full = |level: i32| {
            let mut w = Worldspace::from_cells(
                1,
                "Mojave",
                (0..level)
                    .flat_map(|x| (0..level).map(move |y| ((x, y), CellHeights::flat(0.0))))
                    .collect::<Vec<_>>(),
            );
            for x in 0..level {
                for y in 0..level {
                    w.set_cell_water(x, y, true, Some(100.0));
                }
            }
            w
        };
        // level 8: one wall per side, two triangles each, on the border itself
        let w = full(8);
        let span = 8 * 32 + 1;
        let samples = vec![0.0; span * span];
        let mut mesh = water_mesh(&w, &samples, 8, (0, 0), Plane::Rectangles).unwrap();
        let (v, i) = (mesh.vertices.len(), mesh.indices.len());
        add_walls(&mut mesh, &w, &samples, 8, (0, 0)).unwrap();
        assert_eq!((mesh.vertices.len(), mesh.indices.len()), (v + 16, i + 24));
        // level 4: a wall per cell on the border, and the plane overlaps on the south, west and north
        let w = full(4);
        let span = 4 * 32 + 1;
        let samples = vec![0.0; span * span];
        let mut mesh = water_mesh(&w, &samples, 4, (0, 0), Plane::Rectangles).unwrap();
        let (v, i) = (mesh.vertices.len(), mesh.indices.len());
        add_walls(&mut mesh, &w, &samples, 4, (0, 0)).unwrap();
        // 12 border cells, one wall each, a cell in a corner has two
        assert_eq!(
            (mesh.vertices.len(), mesh.indices.len()),
            (v + 16 * 4, i + 16 * 6)
        );
        let plane: Vec<_> = mesh.vertices[..v].iter().collect();
        let (xmin, xmax) = plane
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
        let (ymin, ymax) = plane
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[1]), b.max(p[1])));
        assert_eq!((xmin, xmax, ymin, ymax), (-32.0, 16384.0, -32.0, 16416.0));
    }

    #[test]
    fn shared_corners_are_written_once_and_the_planes_stay_the_same() {
        // water over cells (0,0), (1,0) and (0,1) at one height, an L: two rectangles that meet along an edge
        let span = 8 * 32 + 1;
        let mut samples = vec![f32::NAN; span * span];
        for y in 0..=64 {
            for x in 0..=64 {
                if y <= 32 || x <= 32 {
                    samples[y * span + x] = 0.0;
                }
            }
        }
        let mut w = Worldspace::from_cells(
            1,
            "Mojave",
            vec![
                ((0, 0), CellHeights::flat(0.0)),
                ((1, 0), CellHeights::flat(0.0)),
                ((0, 1), CellHeights::flat(0.0)),
            ],
        );
        for c in [(0, 0), (1, 0), (0, 1)] {
            w.set_cell_water(c.0, c.1, true, Some(10.0));
        }
        let plain = water_mesh(&w, &samples, 8, (0, 0), Plane::Rectangles).unwrap();
        let shared = water_mesh(&w, &samples, 8, (0, 0), Plane::Shared).unwrap();
        assert_eq!(plain.indices.len(), shared.indices.len());
        assert!(
            shared.vertices.len() < plain.vertices.len(),
            "{} {}",
            shared.vertices.len(),
            plain.vertices.len()
        );
        // the same triangles, corner by corner
        let corners = |m: &super::WaterMesh| -> Vec<[[u32; 3]; 3]> {
            m.indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| m.vertices[usize::from(i)].map(f32::to_bits)))
                .collect()
        };
        assert_eq!(corners(&plain), corners(&shared));
        // every vertex of the shared mesh is used
        let used: std::collections::BTreeSet<u16> = shared.indices.iter().copied().collect();
        assert_eq!(used.len(), shared.vertices.len());
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
        let same = water_mesh(&water(10.0, 10.0), &samples, 8, (0, 0), Plane::Rectangles).unwrap();
        assert_eq!(same.vertices.len(), 4, "one rectangle over both cells");
        assert_eq!(same.vertices[3], [8192.0, 4096.0, 10.0]);
        let apart = water_mesh(&water(10.0, 20.0), &samples, 8, (0, 0), Plane::Rectangles).unwrap();
        assert_eq!(apart.vertices.len(), 8);
    }

    #[test]
    fn coarser_levels_use_coarser_squares_and_no_segments() {
        let span = 8 * 32 + 1;
        let mut samples = vec![f32::NAN; span * span];
        samples[0] = -5.0;
        let mesh = water_mesh(&world(Some(0.0)), &samples, 8, (0, 0), Plane::Rectangles).unwrap();
        assert_eq!(mesh.vertices[3], [256.0, 256.0, 0.0]);
        assert!(mesh.segments.is_none());
    }

    /// A staircase of wet cells (3, 2 and 1 cells in three rows) in a level 8 block.
    fn staircase() -> (Worldspace, Vec<f32>) {
        let span = 8 * 32 + 1;
        let mut samples = vec![f32::NAN; span * span];
        let wet = [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (0, 2)];
        let mut w = Worldspace::from_cells(
            1,
            "Mojave",
            wet.iter().map(|c| (*c, CellHeights::flat(0.0))),
        );
        for (cx, cy) in wet {
            w.set_cell_water(cx, cy, true, Some(100.0));
            for y in 0..=32 {
                for x in 0..=32 {
                    samples[(cy as usize * 32 + y) * span + cx as usize * 32 + x] = 0.0;
                }
            }
        }
        (w, samples)
    }

    /// Twice the area of the triangles of a plane, in square units.
    fn area(mesh: &super::WaterMesh) -> f32 {
        mesh.indices
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize]);
                (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
            })
            .sum()
    }

    #[test]
    fn polygons_cover_the_same_ground_with_only_the_corners_of_the_outline() {
        let (w, samples) = staircase();
        let rects = water_mesh(&w, &samples, 8, (0, 0), Plane::Shared).unwrap();
        let polygons = water_mesh(&w, &samples, 8, (0, 0), Plane::Polygons).unwrap();
        // 3 + 2 + 1 cells of 4096 by 4096 units
        assert_eq!(area(&polygons), 2.0 * 6.0 * 4096.0 * 4096.0);
        assert_eq!(area(&polygons), area(&rects));
        // the corners of the staircase: 8; the rectangles share their corners and still need 10
        assert_eq!((rects.vertices.len(), polygons.vertices.len()), (10, 8));
        assert!(
            polygons
                .vertices
                .iter()
                .all(|v| (v[2] - 100.0).abs() < f32::EPSILON)
        );
        assert!(polygons.segments.is_none());
        // level 4 keeps its rectangle per cell segment
        let level4 = water_mesh(&w, &vec![0.0; 129 * 129], 4, (0, 0), Plane::Polygons);
        assert_eq!(
            level4.is_some(),
            water_mesh(&w, &vec![0.0; 129 * 129], 4, (0, 0), Plane::Rectangles).is_some()
        );
    }
}
