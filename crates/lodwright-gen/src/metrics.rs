// SPDX-License-Identifier: GPL-3.0-only
//! Numbers for how far a far-object block is from a reference block, and how much of an atlas is used.
//!
//! All of them are measured on the meshes only, so they work for any two folders:
//!
//! - position error: the distance from each vertex to the nearest point of the other surface, in both directions;
//! - UV error: how far the texture detail differs, as `|log2|` of the ratio of the two blocks' median texels per game
//!   unit. Two atlases are laid out differently, so comparing UV coordinates directly would mean nothing; 1 is half or
//!   double the detail along an edge;
//! - silhouette error: `1 - IoU` of the covered area seen along each of the three axes, averaged;
//! - atlas use: the share of an atlas that some triangle's UVs cover.

use std::collections::HashMap;

/// The cells per side of the picture a silhouette is compared on.
const SILHOUETTE_GRID: usize = 256;
/// The cells per side, at most, of the picture an atlas's use is measured on.
pub(crate) const ATLAS_GRID: u32 = 1024;
/// The cells per side of the search grid over a surface's bounds.
const SEARCH_GRID: f64 = 48.0;

/// One shape of a block in world units, with the size of its atlas when it is known.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct MetricShape {
    pub positions: Vec<[f64; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub triangles: Vec<[u16; 3]>,
    pub atlas: Option<(u32, u32)>,
}

/// The numbers of one block against its reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeError {
    /// The largest distance from a vertex to the other surface, in game units.
    pub position_max: f64,
    /// The mean of those distances over both directions, in game units.
    pub position_mean: f64,
    /// `|log2|` of the ratio of median texel densities; `None` when either side's atlas size is unknown.
    pub uv: Option<f64>,
    /// `1 - IoU` of the silhouettes, averaged over the three axes: 0 for the same outline.
    pub silhouette: f64,
}

type Tri = [[f64; 3]; 3];

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn along(a: [f64; 3], d: [f64; 3], t: f64) -> [f64; 3] {
    [a[0] + d[0] * t, a[1] + d[1] * t, a[2] + d[2] * t]
}

/// The point of triangle `t` nearest to `p`, by the triangle's Voronoi regions.
fn closest_on_triangle(p: [f64; 3], t: &Tri) -> [f64; 3] {
    let [a, b, c] = *t;
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let (d3, d4) = (dot(ab, bp), dot(ac, bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let span = d1 - d3;
        return along(a, ab, if span > 0.0 { d1 / span } else { 0.0 });
    }
    let cp = sub(p, c);
    let (d5, d6) = (dot(ab, cp), dot(ac, cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let span = d2 - d6;
        return along(a, ac, if span > 0.0 { d2 / span } else { 0.0 });
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let span = (d4 - d3) + (d5 - d6);
        let bc = sub(c, b);
        return along(b, bc, if span > 0.0 { (d4 - d3) / span } else { 0.0 });
    }
    let sum = va + vb + vc;
    if sum <= 0.0 {
        return a; // a triangle with no area whose edges did not claim the point
    }
    along(along(a, ab, vb / sum), ac, vc / sum)
}

fn triangles_of(shapes: &[MetricShape]) -> Vec<Tri> {
    shapes
        .iter()
        .flat_map(|s| {
            s.triangles.iter().filter_map(|t| {
                Some([
                    *s.positions.get(usize::from(t[0]))?,
                    *s.positions.get(usize::from(t[1]))?,
                    *s.positions.get(usize::from(t[2]))?,
                ])
            })
        })
        .collect()
}

/// A uniform grid over triangles, for the nearest-point search.
struct Search<'a> {
    triangles: &'a [Tri],
    min: [f64; 3],
    cell: f64,
    size: [i64; 3],
    cells: HashMap<[i64; 3], Vec<u32>>,
}

impl<'a> Search<'a> {
    fn new(triangles: &'a [Tri]) -> Self {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in triangles.iter().flatten() {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let longest = (0..3).map(|i| max[i] - min[i]).fold(0.0, f64::max);
        let cell = (longest / SEARCH_GRID).max(1e-3);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a coordinate over the cell size is at most the grid's size"
        )]
        let size = [0, 1, 2].map(|i| ((max[i] - min[i]) / cell).floor() as i64 + 1);
        let mut search = Self {
            triangles,
            min,
            cell,
            size,
            cells: HashMap::new(),
        };
        for (index, t) in triangles.iter().enumerate() {
            let low = [0, 1, 2].map(|i| t[0][i].min(t[1][i]).min(t[2][i]));
            let high = [0, 1, 2].map(|i| t[0][i].max(t[1][i]).max(t[2][i]));
            let (from, to) = (search.key(low), search.key(high));
            for x in from[0]..=to[0] {
                for y in from[1]..=to[1] {
                    for z in from[2]..=to[2] {
                        let id = u32::try_from(index).unwrap_or(u32::MAX);
                        search.cells.entry([x, y, z]).or_default().push(id);
                    }
                }
            }
        }
        search
    }

    fn key(&self, p: [f64; 3]) -> [i64; 3] {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "clamped to the grid's size right after"
        )]
        [0, 1, 2]
            .map(|i| (((p[i] - self.min[i]) / self.cell).floor() as i64).clamp(0, self.size[i] - 1))
    }

    /// The distance from `p` to the nearest triangle: rings of cells grow until no nearer triangle can be outside.
    fn distance(&self, p: [f64; 3]) -> f64 {
        let k = self.key(p);
        // how far `p` lies outside the grid: a ring's cells are at least this much further away
        let outside = (0..3)
            .map(|i| {
                let high = self.min[i] + self.cell * self.size[i] as f64;
                (self.min[i] - p[i]).max(p[i] - high).max(0.0)
            })
            .fold(0.0, f64::max);
        let last = self.size.iter().copied().max().unwrap_or(1);
        let mut best = f64::INFINITY;
        for ring in 0..=last {
            #[allow(
                clippy::cast_precision_loss,
                reason = "a ring is at most the grid's size"
            )]
            if best <= outside + (ring - 1).max(0) as f64 * self.cell {
                break;
            }
            for x in k[0] - ring..=k[0] + ring {
                for y in k[1] - ring..=k[1] + ring {
                    for z in k[2] - ring..=k[2] + ring {
                        let on_ring = (x - k[0]).abs() == ring
                            || (y - k[1]).abs() == ring
                            || (z - k[2]).abs() == ring;
                        if !on_ring {
                            continue;
                        }
                        for &id in self.cells.get(&[x, y, z]).into_iter().flatten() {
                            let t = &self.triangles[id as usize];
                            let d = sub(p, closest_on_triangle(p, t));
                            best = best.min(dot(d, d).sqrt());
                        }
                    }
                }
            }
        }
        best
    }
}

/// The largest distance and the sum and count of the distances from the vertices of `from` to the surface `to`.
fn one_way(from: &[MetricShape], to: &Search<'_>) -> (f64, f64, usize) {
    let (mut max, mut sum, mut count) = (0.0_f64, 0.0_f64, 0_usize);
    for p in from.iter().flat_map(|s| &s.positions) {
        let d = to.distance(*p);
        max = max.max(d);
        sum += d;
        count += 1;
    }
    (max, sum, count)
}

/// Mark the cells of a `width` by `height` picture whose centres a triangle covers, and the cells of its corners, so
/// that a triangle thinner than a cell still shows. Corners are in cells.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "values are clamped to the picture, which is at most a few thousand cells wide"
)]
fn fill(cells: &mut [bool], width: usize, height: usize, t: [[f64; 2]; 3]) {
    let cell = |v: f64, n: usize| (v.floor().max(0.0) as usize).min(n - 1);
    for p in t {
        cells[cell(p[1], height) * width + cell(p[0], width)] = true;
    }
    let [a, b, c] = t;
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area == 0.0 {
        return;
    }
    let low = |i: usize, n: usize| cell(a[i].min(b[i]).min(c[i]), n);
    let high = |i: usize, n: usize| cell(a[i].max(b[i]).max(c[i]), n);
    for y in low(1, height)..=high(1, height) {
        for x in low(0, width)..=high(0, width) {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let edge = |u: [f64; 2], v: [f64; 2]| {
                ((v[0] - u[0]) * (py - u[1]) - (v[1] - u[1]) * (px - u[0])) * area.signum()
            };
            if edge(a, b) >= 0.0 && edge(b, c) >= 0.0 && edge(c, a) >= 0.0 {
                cells[y * width + x] = true;
            }
        }
    }
}

/// `1 - IoU` of what two triangle sets cover, seen along each axis over their common bounds, averaged.
#[allow(clippy::cast_precision_loss, reason = "cell counts are small")]
fn silhouette_error(a: &[Tri], b: &[Tri]) -> f64 {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in a.iter().chain(b).flatten() {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    let n = SILHOUETTE_GRID;
    let mut total = 0.0;
    for (u, v) in [(0, 1), (0, 2), (1, 2)] {
        let scale = |i: usize| n as f64 / (max[i] - min[i]).max(1e-6);
        let project = |tris: &[Tri]| {
            let mut cells = vec![false; n * n];
            for t in tris {
                let corner = |p: [f64; 3]| [(p[u] - min[u]) * scale(u), (p[v] - min[v]) * scale(v)];
                fill(&mut cells, n, n, [corner(t[0]), corner(t[1]), corner(t[2])]);
            }
            cells
        };
        let (ca, cb) = (project(a), project(b));
        let both = ca.iter().zip(&cb).filter(|(x, y)| **x && **y).count();
        let either = ca.iter().zip(&cb).filter(|(x, y)| **x || **y).count();
        total += if either == 0 {
            0.0
        } else {
            1.0 - both as f64 / either as f64
        };
    }
    total / 3.0
}

/// The median over the block's surface area of texels per game unit along an edge; `None` when an atlas size is
/// missing or no triangle has area.
fn texel_density(shapes: &[MetricShape]) -> Option<f64> {
    let mut rows: Vec<(f64, f64)> = Vec::new();
    for s in shapes {
        let (w, h) = s.atlas?;
        let (w, h) = (f64::from(w), f64::from(h));
        for t in &s.triangles {
            let (Some(pa), Some(pb), Some(pc)) = (
                s.positions.get(usize::from(t[0])),
                s.positions.get(usize::from(t[1])),
                s.positions.get(usize::from(t[2])),
            ) else {
                continue;
            };
            let (Some(ua), Some(ub), Some(uc)) = (
                s.uv.get(usize::from(t[0])),
                s.uv.get(usize::from(t[1])),
                s.uv.get(usize::from(t[2])),
            ) else {
                continue;
            };
            let n = cross(sub(*pb, *pa), sub(*pc, *pa));
            let area = dot(n, n).sqrt() / 2.0;
            let texels = (f64::from(ub[0] - ua[0]) * w * f64::from(uc[1] - ua[1]) * h
                - f64::from(ub[1] - ua[1]) * h * f64::from(uc[0] - ua[0]) * w)
                .abs()
                / 2.0;
            if area > 0.0 && texels > 0.0 {
                rows.push(((texels / area).sqrt(), area));
            }
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = rows.iter().map(|r| r.1).sum::<f64>() / 2.0;
    let mut seen = 0.0;
    for (density, area) in rows {
        seen += area;
        if seen >= half {
            return Some(density);
        }
    }
    None
}

/// The numbers of the block `mine` against `theirs`; `None` when either has no triangle.
#[must_use]
pub(crate) fn shape_error(mine: &[MetricShape], theirs: &[MetricShape]) -> Option<ShapeError> {
    let (ta, tb) = (triangles_of(mine), triangles_of(theirs));
    if ta.is_empty() || tb.is_empty() {
        return None;
    }
    let (max_a, sum_a, count_a) = one_way(mine, &Search::new(&tb));
    let (max_b, sum_b, count_b) = one_way(theirs, &Search::new(&ta));
    #[allow(clippy::cast_precision_loss, reason = "vertex counts are small")]
    let position_mean = (sum_a + sum_b) / (count_a + count_b).max(1) as f64;
    let uv = texel_density(mine)
        .zip(texel_density(theirs))
        .map(|(a, b)| (a / b).log2().abs());
    Some(ShapeError {
        position_max: max_a.max(max_b),
        position_mean,
        uv,
        silhouette: silhouette_error(&ta, &tb),
    })
}

/// The cells of one atlas that triangles' UVs cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AtlasCover {
    width: usize,
    height: usize,
    cells: Vec<bool>,
}

impl AtlasCover {
    /// An empty picture for an atlas of this size, at most [`ATLAS_GRID`] cells a side.
    pub(crate) fn new(atlas: (u32, u32)) -> Self {
        let longest = atlas.0.max(atlas.1).max(1);
        let shrink = longest.div_ceil(ATLAS_GRID).max(1);
        let (width, height) = (
            (atlas.0 / shrink).max(1) as usize,
            (atlas.1 / shrink).max(1) as usize,
        );
        Self {
            width,
            height,
            cells: vec![false; width * height],
        }
    }

    /// Add a shape's triangles; a UV outside 0 to 1 is held at the edge, as the game's blocks draw it.
    #[allow(clippy::cast_precision_loss, reason = "the picture is small")]
    pub(crate) fn add(&mut self, shape: &MetricShape) {
        let (w, h) = (self.width as f64, self.height as f64);
        let at = |i: u16| {
            shape.uv.get(usize::from(i)).map(|p| {
                [
                    f64::from(p[0]).clamp(0.0, 1.0) * w,
                    f64::from(p[1]).clamp(0.0, 1.0) * h,
                ]
            })
        };
        for t in &shape.triangles {
            if let (Some(a), Some(b), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) {
                fill(&mut self.cells, self.width, self.height, [a, b, c]);
            }
        }
    }

    /// The share of the atlas covered, 0 to 1.
    #[allow(clippy::cast_precision_loss, reason = "the picture is small")]
    pub(crate) fn used(&self) -> f64 {
        self.cells.iter().filter(|c| **c).count() as f64 / self.cells.len().max(1) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::{AtlasCover, MetricShape, closest_on_triangle, shape_error};

    /// A flat square of side `size` at height `z`, its UVs over `uv` of the atlas.
    fn square(size: f64, z: f64, uv: f32, atlas: Option<(u32, u32)>) -> MetricShape {
        MetricShape {
            positions: vec![
                [0.0, 0.0, z],
                [size, 0.0, z],
                [size, size, z],
                [0.0, size, z],
            ],
            uv: vec![[0.0, 0.0], [uv, 0.0], [uv, uv], [0.0, uv]],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            atlas,
        }
    }

    #[test]
    fn the_nearest_point_is_found_in_every_region_of_a_triangle() {
        let t = [[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
        for (p, want) in [
            ([-1.0, -1.0, 0.0], [0.0, 0.0, 0.0]),
            ([3.0, -1.0, 0.0], [2.0, 0.0, 0.0]),
            ([-1.0, 3.0, 0.0], [0.0, 2.0, 0.0]),
            ([1.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
            ([-1.0, 1.0, 0.0], [0.0, 1.0, 0.0]),
            ([2.0, 2.0, 0.0], [1.0, 1.0, 0.0]),
            ([0.5, 0.5, 4.0], [0.5, 0.5, 0.0]),
        ] {
            assert_eq!(closest_on_triangle(p, &t), want, "{p:?}");
        }
    }

    #[test]
    fn a_block_against_itself_has_no_error() {
        let s = [square(100.0, 5.0, 0.5, Some((1024, 1024)))];
        let e = shape_error(&s, &s).unwrap();
        assert_eq!(
            (e.position_max, e.position_mean, e.silhouette),
            (0.0, 0.0, 0.0)
        );
        assert_eq!(e.uv, Some(0.0));
    }

    #[test]
    fn a_raised_surface_shows_as_its_height_in_position_error() {
        let a = [square(100.0, 0.0, 0.5, None)];
        let b = [square(100.0, 7.0, 0.5, None)];
        let e = shape_error(&a, &b).unwrap();
        assert!((e.position_max - 7.0).abs() < 1e-9, "{e:?}");
        assert!((e.position_mean - 7.0).abs() < 1e-9, "{e:?}");
        assert_eq!(e.uv, None, "no atlas size, no UV number");
    }

    #[test]
    fn a_point_far_outside_the_other_surface_gets_its_true_distance() {
        let a = [square(10.0, 0.0, 0.5, None)];
        let mut b = square(10.0, 0.0, 0.5, None);
        b.positions.push([1000.0, 0.0, 0.0]);
        b.triangles.push([1, 4, 2]);
        let e = shape_error(&a, &[b]).unwrap();
        assert!((e.position_max - 990.0).abs() < 1e-9, "{e:?}");
    }

    #[test]
    fn half_the_texture_detail_is_one_step_of_uv_error() {
        let a = [square(100.0, 0.0, 0.25, Some((1024, 1024)))];
        let b = [square(100.0, 0.0, 0.5, Some((1024, 1024)))];
        let e = shape_error(&a, &b).unwrap();
        assert!((e.uv.unwrap() - 1.0).abs() < 1e-9, "{e:?}");
    }

    #[test]
    fn the_same_detail_on_a_larger_atlas_has_no_uv_error() {
        let a = [square(100.0, 0.0, 0.25, Some((2048, 2048)))];
        let b = [square(100.0, 0.0, 0.5, Some((1024, 1024)))];
        assert_eq!(shape_error(&a, &b).unwrap().uv, Some(0.0));
    }

    #[test]
    fn a_missing_half_shows_in_the_silhouette() {
        let a = [square(100.0, 0.0, 0.5, None)];
        let mut half = square(100.0, 0.0, 0.5, None);
        half.triangles.truncate(1);
        let e = shape_error(&a, &[half]).unwrap();
        // seen from above half the square is gone; along the two side axes the outlines stay the same
        assert!(e.silhouette > 0.1 && e.silhouette < 0.25, "{e:?}");
    }

    #[test]
    fn an_empty_block_has_no_numbers() {
        let a = [square(10.0, 0.0, 0.5, None)];
        assert_eq!(shape_error(&a, &[MetricShape::default()]), None);
    }

    #[test]
    fn atlas_use_is_the_share_its_triangles_cover() {
        let mut cover = AtlasCover::new((2048, 1024));
        assert_eq!(cover.used(), 0.0);
        cover.add(&square(1.0, 0.0, 0.5, None));
        assert!((cover.used() - 0.25).abs() < 0.01, "{}", cover.used());
        cover.add(&square(1.0, 0.0, 1.0, None));
        assert!((cover.used() - 1.0).abs() < 1e-9, "{}", cover.used());
    }

    #[test]
    fn a_uv_outside_the_atlas_is_held_at_its_edge() {
        let mut cover = AtlasCover::new((64, 64));
        cover.add(&square(1.0, 0.0, 4.0, None));
        assert!((cover.used() - 1.0).abs() < 1e-9);
    }
}
