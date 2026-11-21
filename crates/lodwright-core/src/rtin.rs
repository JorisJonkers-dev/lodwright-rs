// SPDX-License-Identifier: GPL-3.0-only
//! Adaptive terrain meshes on a right-triangle binary tree (RTIN).
//!
//! The block's `(2^k + 1)` square of samples is covered by two right triangles; a triangle is split at the middle
//! of its long edge while the largest height error beneath it exceeds a tolerance. Because every triangle is split
//! at a vertex that the neighbouring triangle also splits at, the mesh has no cracks inside a block. Vertices on
//! the block's own edges are placed at a fixed stride that does not depend on the heights inside, so two
//! neighbouring blocks of the same level meet with identical edge vertices.
//!
//! This is our own design from the published idea of right-triangulated irregular networks (Evans, Duchaineau
//! and others); it is not another generator's simplifier and its tolerance does not map onto another tool's
//! quality number.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::manual_midpoint,
    reason = "sample indices are small, the maths is in f32 and the midpoints cannot overflow"
)]

use crate::terrain::{TerrainMesh, VERTEX_SPACING};

/// Border vertices per block edge are placed every `(side) / BORDER_SEGMENTS` samples.
const BORDER_SEGMENTS: usize = 16;
/// Split threshold as a fraction of the requested tolerance. The tree's error is the true largest deviation of the
/// samples inside each triangle (see `deviation`), so a triangle that is not split is within the tolerance and the
/// whole tolerance is used.
const SPLIT_FRACTION: f32 = 1.0;

/// A finished mesh and the numbers the planner reports.
#[derive(Debug, Clone, PartialEq)]
pub struct Meshed {
    /// The triangles.
    pub mesh: TerrainMesh,
    /// The largest vertical error of the mesh against the samples that have data, in world units.
    pub max_error: f32,
    /// The shortest triangle leg used, in samples (1 means full resolution somewhere).
    pub finest_step: usize,
}

/// Fill samples without data from the nearest sample with data in the same row, then in the same column.
/// `None` when no sample has data.
fn fill_gaps(samples: &[f32], span: usize) -> Option<Vec<f32>> {
    if samples.iter().all(|v| v.is_nan()) {
        return None;
    }
    let mut h = samples.to_vec();
    let mut empty_rows = Vec::new();
    for y in 0..span {
        let row = &mut h[y * span..(y + 1) * span];
        let mut last = f32::NAN;
        for v in row.iter_mut() {
            if v.is_nan() {
                *v = last;
            } else {
                last = *v;
            }
        }
        let mut next = f32::NAN;
        for v in row.iter_mut().rev() {
            if v.is_nan() {
                *v = next;
            } else {
                next = *v;
            }
        }
        if row[0].is_nan() {
            empty_rows.push(y);
        }
    }
    for y in 0..span {
        if !h[y * span].is_nan() {
            continue;
        }
        let nearest = (1..span)
            .flat_map(|d| [y.checked_sub(d), Some(y + d)])
            .flatten()
            .find(|r| *r < span && !h[r * span].is_nan())?;
        let (src, dst) = (nearest * span, y * span);
        for x in 0..span {
            h[dst + x] = h[src + x];
        }
    }
    Some(h)
}

/// The error of every vertex: the largest deviation from linear interpolation over all triangles it is the
/// long-edge middle of, and of the triangles below them. Border vertices that are not on the fixed stride are
/// zeroed (they are never split at) and those on it are forced.
fn errors(h: &[f32], original: &[f32], span: usize) -> Vec<f32> {
    let tile = span - 1;
    let stride = (tile / BORDER_SEGMENTS).max(1);
    let triangles = tile * tile * 2 - 2;
    let parents = triangles - tile * tile;
    let mut err = vec![0.0_f32; span * span];
    // bit 0: some sample under the triangles around this vertex has data; bit 1: some has none
    let mut cover = vec![0_u8; span * span];
    let has = |x: usize, y: usize| 1_u8 << u8::from(original[y * span + x].is_nan());
    let on_border = |x: usize, y: usize| x == 0 || y == 0 || x == tile || y == tile;
    for y in 0..span {
        for x in 0..span {
            if on_border(x, y) && x % stride == 0 && y % stride == 0 {
                err[y * span + x] = f32::INFINITY;
            }
        }
    }
    for i in (0..triangles).rev() {
        let [ax, ay, bx, by, cx, cy] = triangle(i, tile);
        let (mx, my) = ((ax + bx) / 2, (ay + by) / 2);
        let mid = my * span + mx;
        let own = if i < parents {
            let left = ((ay + cy) / 2) * span + (ax + cx) / 2;
            let right = ((by + cy) / 2) * span + (bx + cx) / 2;
            cover[left] | cover[right]
        } else {
            has(ax, ay) | has(bx, by) | has(cx, cy)
        };
        cover[mid] |= own;
        if cover[mid] == 3 {
            err[mid] = f32::INFINITY;
        }
        err[mid] = err[mid].max(deviation(h, span, [ax, ay, bx, by, cx, cy]));
        if i < parents {
            let left = ((ay + cy) / 2) * span + (ax + cx) / 2;
            let right = ((by + cy) / 2) * span + (bx + cx) / 2;
            err[mid] = err[mid].max(err[left]).max(err[right]);
        }
    }
    for y in 0..span {
        for x in 0..span {
            if on_border(x, y) && !(x % stride == 0 && y % stride == 0) {
                err[y * span + x] = 0.0;
            }
        }
    }
    err
}

/// The largest deviation of the samples inside a triangle (edges included) from the plane through its three corners:
/// the true error of the triangle, not the estimate at the middle of its long edge. The right angle is at the third
/// corner and both legs run along an axis.
fn deviation(h: &[f32], span: usize, [ax, ay, bx, by, cx, cy]: [usize; 6]) -> f32 {
    let (ax, ay, bx, by, cx, cy) = (
        ax.cast_signed(),
        ay.cast_signed(),
        bx.cast_signed(),
        by.cast_signed(),
        cx.cast_signed(),
        cy.cast_signed(),
    );
    let (ux, uy) = (ax - cx, ay - cy);
    let (vx, vy) = (bx - cx, by - cy);
    let len2 = ux * ux + uy * uy;
    if len2 <= 1 {
        return 0.0;
    }
    let at = |x: isize, y: isize| h[y.cast_unsigned() * span + x.cast_unsigned()];
    let (za, zb, zc) = (at(ax, ay), at(bx, by), at(cx, cy));
    let (x0, x1) = (ax.min(bx).min(cx), ax.max(bx).max(cx));
    let (y0, y1) = (ay.min(by).min(cy), ay.max(by).max(cy));
    let mut worst = 0.0_f32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (dx, dy) = (x - cx, y - cy);
            let s = dx * ux + dy * uy;
            let t = dx * vx + dy * vy;
            if s < 0 || t < 0 || s + t > len2 {
                continue;
            }
            let plane =
                zc + (za - zc) * s as f32 / len2 as f32 + (zb - zc) * t as f32 / len2 as f32;
            worst = worst.max((at(x, y) - plane).abs());
        }
    }
    worst
}

/// Corners of triangle `i` of the tree: the two ends of the long edge, then the right-angle corner.
fn triangle(i: usize, tile: usize) -> [usize; 6] {
    let mut id = i + 2;
    let (mut ax, mut ay, mut bx, mut by, mut cx, mut cy) = (0, 0, 0, 0, 0, 0);
    if id & 1 == 1 {
        bx = tile;
        by = tile;
        cx = tile;
    } else {
        ax = tile;
        ay = tile;
        cy = tile;
    }
    loop {
        id >>= 1;
        if id <= 1 {
            break;
        }
        let (mx, my) = ((ax + bx) / 2, (ay + by) / 2);
        if id & 1 == 1 {
            bx = ax;
            by = ay;
            ax = cx;
            ay = cy;
        } else {
            ax = bx;
            ay = by;
            bx = cx;
            by = cy;
        }
        cx = mx;
        cy = my;
    }
    // the right-angle corner from the long edge
    let (mx, my) = ((ax + bx) / 2, (ay + by) / 2);
    [ax, ay, bx, by, mx + my - ay, my + ax - mx]
}

struct Extract<'a> {
    err: &'a [f32],
    span: usize,
    tolerance: f32,
    corners: Vec<[usize; 6]>,
    finest: usize,
}

impl Extract<'_> {
    fn visit(&mut self, [ax, ay, bx, by, cx, cy]: [usize; 6]) {
        let (mx, my) = ((ax + bx) / 2, (ay + by) / 2);
        let leg = ax.abs_diff(cx) + ay.abs_diff(cy);
        let e = self.err[my * self.span + mx];
        if leg > 1 && e > self.tolerance {
            self.visit([cx, cy, ax, ay, mx, my]);
            self.visit([bx, by, cx, cy, mx, my]);
        } else {
            self.finest = self.finest.min(leg);
            self.corners.push([ax, ay, bx, by, cx, cy]);
        }
    }
}

/// The largest difference between the mesh surface and the samples that have data, over every sample inside a
/// triangle.
fn measured_error(mesh: &TerrainMesh, original: &[f32], span: usize) -> f32 {
    let mut worst = 0.0_f32;
    for t in mesh.indices.as_chunks::<3>().0 {
        let p = [
            mesh.vertices[usize::from(t[0])],
            mesh.vertices[usize::from(t[1])],
            mesh.vertices[usize::from(t[2])],
        ];
        let low = |k: usize| p.iter().map(|v| v[k]).fold(f32::MAX, f32::min);
        let high = |k: usize| p.iter().map(|v| v[k]).fold(f32::MIN, f32::max);
        let cell = |v: f32| (v / VERTEX_SPACING).round() as usize;
        let det =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
        if det == 0.0 {
            continue;
        }
        for sy in cell(low(1))..=cell(high(1)) {
            for sx in cell(low(0))..=cell(high(0)) {
                let truth = original[sy * span + sx];
                if truth.is_nan() {
                    continue;
                }
                let (px, py) = (sx as f32 * VERTEX_SPACING, sy as f32 * VERTEX_SPACING);
                let w1 = ((px - p[0][0]) * (p[2][1] - p[0][1])
                    - (p[2][0] - p[0][0]) * (py - p[0][1]))
                    / det;
                let w2 = ((p[1][0] - p[0][0]) * (py - p[0][1])
                    - (px - p[0][0]) * (p[1][1] - p[0][1]))
                    / det;
                if w1 < -1e-4 || w2 < -1e-4 || w1 + w2 > 1.0001 {
                    continue;
                }
                let z = p[0][2] + w1 * (p[1][2] - p[0][2]) + w2 * (p[2][2] - p[0][2]);
                worst = worst.max((z - truth).abs());
            }
        }
    }
    worst
}

/// The outcome of one extraction.
enum Built {
    /// More vertices than the limit.
    Over,
    /// A mesh within the limit.
    Mesh(Meshed),
}

fn build(
    h: &[f32],
    original: &[f32],
    err: &[f32],
    span: usize,
    tolerance: f32,
    limit: usize,
) -> Option<Built> {
    let tile = span - 1;
    let mut x = Extract {
        err,
        span,
        tolerance,
        corners: Vec::new(),
        finest: usize::MAX,
    };
    x.visit([0, 0, tile, tile, tile, 0]);
    x.visit([tile, tile, 0, 0, 0, tile]);
    let mut slot = vec![u32::MAX; span * span];
    let mut mesh = TerrainMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for [ax, ay, bx, by, cx, cy] in &x.corners {
        // a triangle is kept only over ground with data: all three corners must have it
        let valid = |px: usize, py: usize| !original[py * span + px].is_nan();
        if !(valid(*ax, *ay) && valid(*bx, *by) && valid(*cx, *cy)) {
            continue;
        }
        // counter-clockwise seen from above
        let cross = (bx.cast_signed() - ax.cast_signed()) * (cy.cast_signed() - ay.cast_signed())
            - (by.cast_signed() - ay.cast_signed()) * (cx.cast_signed() - ax.cast_signed());
        let order = if cross > 0 {
            [(*ax, *ay), (*bx, *by), (*cx, *cy)]
        } else {
            [(*ax, *ay), (*cx, *cy), (*bx, *by)]
        };
        for (vx, vy) in order {
            let s = &mut slot[vy * span + vx];
            if *s == u32::MAX {
                if mesh.vertices.len() >= limit {
                    return Some(Built::Over);
                }
                *s = u32::try_from(mesh.vertices.len()).ok()?;
                mesh.vertices.push([
                    f32::from(u16::try_from(vx).ok()?) * VERTEX_SPACING,
                    f32::from(u16::try_from(vy).ok()?) * VERTEX_SPACING,
                    h[vy * span + vx],
                ]);
            }
            mesh.indices.push(u16::try_from(*s).ok()?);
        }
    }
    if mesh.indices.is_empty() {
        return None;
    }
    let max_error = measured_error(&mesh, original, span);
    Some(Built::Mesh(Meshed {
        mesh,
        max_error,
        finest_step: x.finest,
    }))
}

/// Mesh a block of `span * span` samples (`span` is a power of two plus one; `NaN` is no data) so that the vertical
/// error stays within `tolerance` and the mesh has at most `max_vertices` vertices: when the tolerance would need
/// more, the tolerance is raised to the smallest value that fits (the error reported is then larger). `None` when
/// the block has no data, no triangle with data, or its size is not a power of two plus one.
#[must_use]
pub fn mesh_block(
    samples: &[f32],
    span: usize,
    tolerance: f32,
    max_vertices: usize,
) -> Option<Meshed> {
    if span < 3 || !(span - 1).is_power_of_two() || samples.len() != span * span {
        return None;
    }
    let h = fill_gaps(samples, span)?;
    let err = errors(&h, samples, span);
    let limit = max_vertices.min(usize::from(u16::MAX));
    let start = tolerance.max(0.0) * SPLIT_FRACTION;
    if let Built::Mesh(m) = build(&h, samples, &err, span, start, limit)? {
        return Some(m);
    }
    // raise the threshold until the vertex budget holds: bisect between the request and the largest error
    let top = err
        .iter()
        .copied()
        .filter(|e| e.is_finite())
        .fold(start, f32::max);
    let (mut low, mut high) = (start, top);
    let Built::Mesh(mut best) = build(&h, samples, &err, span, high, limit)? else {
        return None;
    };
    for _ in 0..24 {
        let mid = f32::midpoint(low, high);
        match build(&h, samples, &err, span, mid, limit)? {
            Built::Mesh(m) => {
                high = mid;
                best = m;
            }
            Built::Over => low = mid,
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::mesh_block;
    use lodwright_testkit::rng::Rng;

    fn field(span: usize, f: impl Fn(f32, f32) -> f32) -> Vec<f32> {
        (0..span * span)
            .map(|i| f((i % span) as f32, (i / span) as f32))
            .collect()
    }

    #[test]
    fn flat_ground_needs_little_more_than_its_border() {
        let m = mesh_block(&vec![80.0; 129 * 129], 129, 1.0, 32767).unwrap();
        assert!(m.mesh.vertices.len() < 200, "{}", m.mesh.vertices.len());
        assert!(m.max_error.abs() < f32::EPSILON);
    }

    #[test]
    fn a_plane_needs_few_interior_vertices() {
        let h = field(65, |x, y| 3.0 * x + 5.0 * y);
        let m = mesh_block(&h, 65, 0.01, 32767).unwrap();
        let interior = m
            .mesh
            .vertices
            .iter()
            .filter(|v| v[0] > 0.0 && v[1] > 0.0 && v[0] < 64.0 * 128.0 && v[1] < 64.0 * 128.0)
            .count();
        assert!(interior < 100, "{interior}");
    }

    #[test]
    fn vertices_are_source_samples_and_indices_are_valid() {
        let mut rng = Rng::new(11);
        let h: Vec<f32> = (0..65 * 65).map(|_| rng.below(500) as f32).collect();
        let m = mesh_block(&h, 65, 20.0, 32767).unwrap().mesh;
        for v in &m.vertices {
            let (x, y) = ((v[0] / 128.0) as usize, (v[1] / 128.0) as usize);
            assert!((h[y * 65 + x] - v[2]).abs() < f32::EPSILON);
        }
        assert!(m.indices.iter().all(|i| usize::from(*i) < m.vertices.len()));
        assert_eq!(m.indices.len() % 3, 0);
    }

    #[test]
    fn a_lower_tolerance_never_gives_fewer_vertices() {
        let h = field(129, |x, y| 40.0 * (x / 9.0).sin() + 25.0 * (y / 13.0).cos());
        let mut last = 0;
        for tolerance in [200.0, 50.0, 20.0, 5.0, 1.0, 0.0] {
            let m = mesh_block(&h, 129, tolerance, 32767).unwrap();
            assert!(m.mesh.vertices.len() >= last, "{tolerance}");
            last = m.mesh.vertices.len();
        }
    }

    #[test]
    fn the_reported_error_is_the_true_error_and_stays_near_the_tolerance() {
        let h = field(65, |x, y| 30.0 * (x / 7.0).sin() * (y / 11.0).cos());
        for tolerance in [2.0, 4.0, 8.0] {
            let m = mesh_block(&h, 65, tolerance, 32767).unwrap();
            assert!(
                m.max_error <= tolerance * 1.001,
                "{} > {tolerance}",
                m.max_error
            );
        }
    }

    #[test]
    fn the_vertex_cap_is_honoured_by_raising_the_tolerance() {
        let mut rng = Rng::new(3);
        let h: Vec<f32> = (0..129 * 129).map(|_| rng.below(300) as f32).collect();
        let m = mesh_block(&h, 129, 0.0, 600).unwrap();
        assert!(m.mesh.vertices.len() <= 600);
        assert!(m.max_error > 0.0);
    }

    #[test]
    fn neighbouring_blocks_share_identical_edge_vertices() {
        // two blocks that share their east / west column but differ inside
        let mut rng = Rng::new(9);
        let shared: Vec<f32> = (0..65).map(|_| rng.below(200) as f32).collect();
        let mut left: Vec<f32> = (0..65 * 65).map(|_| rng.below(200) as f32).collect();
        let mut right: Vec<f32> = (0..65 * 65).map(|_| rng.below(200) as f32).collect();
        for y in 0..65 {
            left[y * 65 + 64] = shared[y];
            right[y * 65] = shared[y];
        }
        let edge = |h: &[f32], x: usize| -> Vec<(usize, u32)> {
            let m = mesh_block(h, 65, 15.0, 32767).unwrap().mesh;
            let mut v: Vec<(usize, u32)> = m
                .vertices
                .iter()
                .filter(|v| (v[0] / 128.0) as usize == x)
                .map(|v| ((v[1] / 128.0) as usize, v[2].to_bits()))
                .collect();
            v.sort_unstable();
            v
        };
        assert_eq!(edge(&left, 64), edge(&right, 0));
    }

    #[test]
    fn ground_without_data_gets_no_triangles_and_no_data_at_all_gives_none() {
        let span = 65;
        let mut h = vec![f32::NAN; span * span];
        for y in 0..33 {
            for x in 0..33 {
                h[y * span + x] = 8.0;
            }
        }
        let m = mesh_block(&h, span, 1.0, 32767).unwrap().mesh;
        assert!(
            m.vertices
                .iter()
                .all(|v| v[0] <= 40.0 * 128.0 && v[1] <= 40.0 * 128.0)
        );
        assert!(mesh_block(&vec![f32::NAN; span * span], span, 1.0, 100).is_none());
    }

    #[test]
    fn a_size_that_is_not_a_power_of_two_plus_one_is_refused() {
        assert!(mesh_block(&vec![0.0; 64 * 64], 64, 1.0, 100).is_none());
        assert!(mesh_block(&[0.0; 5], 2, 1.0, 100).is_none());
    }
}
