// SPDX-License-Identifier: GPL-3.0-only
//! Terrain meshes by greedy insertion: start from the block's border vertices and repeatedly insert the height sample
//! that the current triangulation misses by most, keeping the triangulation Delaunay in plan.
//!
//! Own design from the published idea of greedy insertion into a triangulated irregular network (Garland and
//! Heckbert, "Fast polygonal approximation of terrains and height fields", 1995); it is not another generator's
//! simplifier. The vertices on a block's own edges are chosen along that edge's heights alone, which the neighbouring
//! block shares, so two neighbouring blocks of the same level meet with identical edge vertices.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    reason = "sample indices are below 2048, the triangle arithmetic is exact in i64 and the geometry uses a, b, c, d, p"
)]

use std::collections::{BinaryHeap, HashMap};

use crate::terrain::{TerrainMesh, VERTEX_SPACING};

/// Border vertices per block edge: an edge gets at most this many segments.
const BORDER_SEGMENTS: usize = 16;

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

/// The error given to a sample without data inside a triangle whose corners all have data: such samples are inserted
/// first so the mesh does not cover ground without data (the triangles with a corner without data are dropped).
const HOLE_ERROR: f32 = 1.0e30;

type Point = (i32, i32);

struct Tri {
    v: [u32; 3],
    alive: bool,
    /// The sample with the largest error inside the triangle and that error; error 0 when there is none.
    best: (f32, Point),
}

struct Tin<'a> {
    h: &'a [f32],
    original: &'a [f32],
    span: usize,
    tile: i32,
    pts: Vec<Point>,
    tris: Vec<Tri>,
    /// Directed edge (a, b) of an alive triangle (counter-clockwise) to that triangle.
    edges: HashMap<(u32, u32), usize>,
    heap: BinaryHeap<(u32, std::cmp::Reverse<usize>)>,
    /// Whether `insert` scores the triangles it makes: off while the border vertices go in, whose scores are all made
    /// again afterwards.
    scoring: bool,
}

/// Division of `n` by a positive `d`, rounded up.
fn ceil_div(n: i64, d: i64) -> i64 {
    -(-n).div_euclid(d)
}

fn cross(a: Point, b: Point, c: Point) -> i64 {
    i64::from(b.0 - a.0) * i64::from(c.1 - a.1) - i64::from(b.1 - a.1) * i64::from(c.0 - a.0)
}

/// Whether `d` lies strictly inside the circle through the counter-clockwise triangle `a`, `b`, `c`.
fn in_circle(a: Point, b: Point, c: Point, d: Point) -> bool {
    let m = |p: Point| {
        let (x, y) = (i128::from(p.0 - d.0), i128::from(p.1 - d.1));
        (x, y, x * x + y * y)
    };
    let ((ax, ay, az), (bx, by, bz), (cx, cy, cz)) = (m(a), m(b), m(c));
    ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx) > 0
}

impl Tin<'_> {
    fn at(&self, p: Point) -> usize {
        p.1 as usize * self.span + p.0 as usize
    }

    fn add_tri(&mut self, v: [u32; 3]) -> usize {
        let id = self.tris.len();
        for k in 0..3 {
            self.edges.insert((v[k], v[(k + 1) % 3]), id);
        }
        self.tris.push(Tri {
            v,
            alive: true,
            best: (0.0, (0, 0)),
        });
        id
    }

    fn remove_tri(&mut self, id: usize) {
        let v = self.tris[id].v;
        for k in 0..3 {
            self.edges.remove(&(v[k], v[(k + 1) % 3]));
        }
        self.tris[id].alive = false;
    }

    /// Find the sample of triangle `id` that its plane misses by most and queue the triangle.
    fn score(&mut self, id: usize) {
        let [a, b, c] = self.tris[id].v.map(|i| self.pts[i as usize]);
        let (za, zb, zc) = (self.h[self.at(a)], self.h[self.at(b)], self.h[self.at(c)]);
        let area = cross(a, b, c);
        let corners_valid = [a, b, c]
            .iter()
            .all(|p| !self.original[self.at(*p)].is_nan());
        let (x0, x1) = (a.0.min(b.0).min(c.0), a.0.max(b.0).max(c.0));
        let (y0, y1) = (a.1.min(b.1).min(c.1), a.1.max(b.1).max(c.1));
        let mut best = (0.0_f32, (0, 0));
        // a triangle with a corner without data refines towards the ground that has data: the sample with data nearest
        // to its centre goes in, so triangles with three corners with data appear around it
        let centre = (
            f64::from(a.0 + b.0 + c.0) / 3.0,
            f64::from(a.1 + b.1 + c.1) / 3.0,
        );
        let mut nearest = (f64::INFINITY, (0, 0));
        // the doubled-area weights are linear in the sample position: weight = k + dx * x + dy * y, with the edge
        // opposite each corner; each row's range of samples inside the triangle follows from them exactly
        let edge = |q: Point, r: Point| {
            (
                i64::from(q.0) * i64::from(r.1) - i64::from(q.1) * i64::from(r.0),
                i64::from(q.1 - r.1),
                i64::from(r.0 - q.0),
            )
        };
        let (ea, eb, ec) = (edge(b, c), edge(c, a), edge(a, b));
        let tile = i64::from(self.tile);
        for y in y0..=y1 {
            // the samples on the block's own border never go in, so they are left out of the range
            if y == 0 || y == self.tile {
                continue;
            }
            let (mut lo, mut hi) = (i64::from(x0).max(1), i64::from(x1).min(tile - 1));
            for (k, dx, dy) in [ea, eb, ec] {
                let rest = k + dy * i64::from(y);
                match dx.cmp(&0) {
                    std::cmp::Ordering::Greater => lo = lo.max(ceil_div(-rest, dx)),
                    std::cmp::Ordering::Less => hi = hi.min(rest.div_euclid(-dx)),
                    std::cmp::Ordering::Equal => {
                        if rest < 0 {
                            lo = hi + 1;
                        }
                    }
                }
            }
            if lo > hi {
                continue;
            }
            let row = &self.original[y as usize * self.span..(y as usize + 1) * self.span];
            let yy = i64::from(y);
            let (mut wa, mut wb, mut wc) = (
                ea.0 + ea.1 * lo + ea.2 * yy,
                eb.0 + eb.1 * lo + eb.2 * yy,
                ec.0 + ec.1 * lo + ec.2 * yy,
            );
            for x in lo..=hi {
                let (cwa, cwb, cwc) = (wa, wb, wc);
                wa += ea.1;
                wb += eb.1;
                wc += ec.1;
                let p = (x as i32, y);
                if p == a || p == b || p == c {
                    continue;
                }
                let truth = row[x as usize];
                if !corners_valid && !truth.is_nan() {
                    let d = (f64::from(p.0) - centre.0).powi(2) + (f64::from(y) - centre.1).powi(2);
                    if d < nearest.0 {
                        nearest = (d, p);
                    }
                    continue;
                }
                let error = if truth.is_nan() {
                    if corners_valid { HOLE_ERROR } else { 0.0 }
                } else {
                    let plane = (cwa as f32 * za + cwb as f32 * zb + cwc as f32 * zc) / area as f32;
                    (truth - plane).abs()
                };
                if error > best.0 {
                    best = (error, p);
                }
            }
        }
        if !corners_valid && nearest.0.is_finite() {
            best = (HOLE_ERROR * 0.5, nearest.1);
        }
        self.tris[id].best = best;
        if best.0 > 0.0 {
            self.heap.push((best.0.to_bits(), std::cmp::Reverse(id)));
        }
    }

    /// Insert `p`, which lies inside or on an edge of the triangle `id`, then restore the Delaunay property.
    fn insert(&mut self, id: usize, p: Point) {
        let pi = u32::try_from(self.pts.len()).unwrap_or(u32::MAX);
        self.pts.push(p);
        let v = self.tris[id].v;
        let corners = v.map(|i| self.pts[i as usize]);
        // an edge of the triangle that p lies on (zero cross product)
        let on_edge = (0..3).find(|k| cross(corners[*k], corners[(*k + 1) % 3], p) == 0);
        let mut stack: Vec<(u32, u32)> = Vec::new();
        let mut made: Vec<usize> = Vec::new();
        if let Some(k) = on_edge {
            let (a, b, c) = (v[k], v[(k + 1) % 3], v[(k + 2) % 3]);
            let neighbour = self.edges.get(&(b, a)).copied();
            self.remove_tri(id);
            made.push(self.add_tri([a, pi, c]));
            made.push(self.add_tri([pi, b, c]));
            stack.push((b, c));
            stack.push((c, a));
            if let Some(n) = neighbour {
                let d = self.tris[n].v.iter().copied().find(|i| *i != a && *i != b);
                if let Some(d) = d {
                    self.remove_tri(n);
                    made.push(self.add_tri([b, pi, d]));
                    made.push(self.add_tri([pi, a, d]));
                    stack.push((a, d));
                    stack.push((d, b));
                }
            }
        } else {
            self.remove_tri(id);
            for k in 0..3 {
                made.push(self.add_tri([v[k], v[(k + 1) % 3], pi]));
                stack.push((v[k], v[(k + 1) % 3]));
            }
        }
        // legalise: edge (a, b) with the new point on its left; the triangle across is (b, a, d)
        while let Some((a, b)) = stack.pop() {
            let Some(&t) = self.edges.get(&(a, b)) else {
                continue;
            };
            let Some(&n) = self.edges.get(&(b, a)) else {
                continue;
            };
            let third =
                |tri: &Tri, x: u32, y: u32| tri.v.iter().copied().find(|i| *i != x && *i != y);
            let (Some(apex), Some(d)) = (third(&self.tris[t], a, b), third(&self.tris[n], a, b))
            else {
                continue;
            };
            let (pa, pb, pc, pd) = (
                self.pts[a as usize],
                self.pts[b as usize],
                self.pts[apex as usize],
                self.pts[d as usize],
            );
            if in_circle(pa, pb, pc, pd) && cross(pc, pa, pd) > 0 && cross(pc, pd, pb) > 0 {
                self.remove_tri(t);
                self.remove_tri(n);
                made.push(self.add_tri([a, d, apex]));
                made.push(self.add_tri([d, b, apex]));
                stack.push((a, d));
                stack.push((d, b));
            }
        }
        for id in made {
            if self.scoring && self.tris[id].alive {
                self.score(id);
            }
        }
    }
}

/// The interior positions (at most `count`) along one block edge whose vertices bring the polyline closest to the
/// edge's samples: greedily the sample with the largest deviation from the line between the vertices around it.
/// `profile` holds the samples with `NaN` for no data (those are never chosen), `fallback` the same filled, used only
/// for the height of an end point without data.
fn edge_vertices(profile: &[f32], fallback: &[f32], count: usize) -> Vec<usize> {
    let last = profile.len() - 1;
    let mut chosen = vec![0, last];
    let height = |k: usize| {
        if profile[k].is_nan() {
            fallback[k]
        } else {
            profile[k]
        }
    };
    for _ in 0..count {
        let mut best = (0.0_f32, 0);
        for pair in chosen.windows(2) {
            let (lo, hi) = (pair[0], pair[1]);
            for (offset, sample) in profile[lo + 1..hi].iter().enumerate() {
                if sample.is_nan() {
                    continue;
                }
                let k = lo + 1 + offset;
                let t = (k - lo) as f32 / (hi - lo) as f32;
                let line = height(lo) + (height(hi) - height(lo)) * t;
                let error = (sample - line).abs();
                if error > best.0 {
                    best = (error, k);
                }
            }
        }
        if best.0 <= 0.0 {
            break;
        }
        chosen.push(best.1);
        chosen.sort_unstable();
    }
    chosen.retain(|k| *k != 0 && *k != last);
    chosen
}

/// Mesh a block of `span * span` samples by greedy insertion (see the module text): vertices are inserted until the
/// largest error is within `tolerance` or `max_vertices` is reached. `None` when the block has no data, no triangle
/// with data, or its size is not a power of two plus one.
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
    let tile = i32::try_from(span - 1).ok()?;
    let limit = max_vertices.min(usize::from(u16::MAX));
    let mut tin = Tin {
        h: &h,
        original: samples,
        span,
        tile,
        pts: vec![(0, 0), (tile, 0), (tile, tile), (0, tile)],
        tris: Vec::new(),
        edges: HashMap::new(),
        heap: BinaryHeap::new(),
        scoring: false,
    };
    tin.add_tri([0, 1, 2]);
    tin.add_tri([0, 2, 3]);
    // the border vertices: on each edge the same number as a fixed stride would give, but where the edge's own heights
    // need them; the choice reads only that edge's samples, which the neighbouring block shares
    let segments = BORDER_SEGMENTS.min(span - 1);
    let mut border: Vec<Point> = Vec::new();
    for edge in 0..4 {
        let at = |k: usize| match edge {
            0 => (k, 0),
            1 => (span - 1, k),
            2 => (k, span - 1),
            _ => (0, k),
        };
        let profile: Vec<f32> = (0..span)
            .map(|k| samples[at(k).1 * span + at(k).0])
            .collect();
        let fallback: Vec<f32> = (0..span).map(|k| h[at(k).1 * span + at(k).0]).collect();
        for k in edge_vertices(&profile, &fallback, segments - 1) {
            let (x, y) = at(k);
            border.push((x as i32, y as i32));
        }
    }
    for p in border {
        let id = (0..tin.tris.len()).find(|i| {
            tin.tris[*i].alive && {
                let [a, b, c] = tin.tris[*i].v.map(|k| tin.pts[k as usize]);
                cross(a, b, p) >= 0 && cross(b, c, p) >= 0 && cross(c, a, p) >= 0
            }
        })?;
        tin.insert(id, p);
    }
    let alive: Vec<usize> = (0..tin.tris.len()).filter(|i| tin.tris[*i].alive).collect();
    tin.heap.clear();
    tin.scoring = true;
    for id in alive {
        tin.score(id);
    }
    while tin.pts.len() < limit {
        let Some((bits, std::cmp::Reverse(id))) = tin.heap.pop() else {
            break;
        };
        if !tin.tris[id].alive || tin.tris[id].best.0.to_bits() != bits {
            continue;
        }
        if f32::from_bits(bits) <= tolerance {
            break;
        }
        let p = tin.tris[id].best.1;
        tin.insert(id, p);
    }
    // the mesh: triangles whose three corners have data, counter-clockwise
    let mut slot = vec![u32::MAX; tin.pts.len()];
    let mut mesh = TerrainMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for t in tin.tris.iter().filter(|t| t.alive) {
        let corners = t.v.map(|i| tin.pts[i as usize]);
        if corners.iter().any(|p| samples[tin.at(*p)].is_nan()) {
            continue;
        }
        for i in t.v {
            if slot[i as usize] == u32::MAX {
                let p = tin.pts[i as usize];
                slot[i as usize] = u32::try_from(mesh.vertices.len()).ok()?;
                mesh.vertices.push([
                    p.0 as f32 * VERTEX_SPACING,
                    p.1 as f32 * VERTEX_SPACING,
                    h[tin.at(p)],
                ]);
            }
            mesh.indices.push(u16::try_from(slot[i as usize]).ok()?);
        }
    }
    if mesh.indices.is_empty() {
        return None;
    }
    let max_error = measured_error(&mesh, samples, span);
    Some(Meshed {
        mesh,
        max_error,
        finest_step: 1,
    })
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
                m.max_error <= tolerance * 1.25,
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
    fn the_triangles_are_counter_clockwise_and_cover_the_block_exactly() {
        let mut rng = Rng::new(21);
        let h: Vec<f32> = (0..65 * 65).map(|_| rng.below(400) as f32).collect();
        let m = mesh_block(&h, 65, 10.0, 700).unwrap().mesh;
        let mut area = 0.0_f64;
        for t in m.indices.as_chunks::<3>().0 {
            let p = t.map(|i| m.vertices[usize::from(i)]);
            let cross = f64::from(p[1][0] - p[0][0]) * f64::from(p[2][1] - p[0][1])
                - f64::from(p[1][1] - p[0][1]) * f64::from(p[2][0] - p[0][0]);
            assert!(cross > 0.0, "{p:?}");
            area += cross / 2.0;
        }
        let side = 64.0 * 128.0;
        assert!((area - side * side).abs() < 1.0, "{area}");
    }

    #[test]
    fn isolated_spikes_are_found_before_the_budget_runs_out() {
        let mut h = vec![50.0_f32; 129 * 129];
        for (x, y) in [(20, 30), (77, 101), (110, 12)] {
            h[y * 129 + x] = 900.0;
        }
        let m = mesh_block(&h, 129, 0.0, 120).unwrap();
        assert!(m.max_error < 1.0, "{}", m.max_error);
    }

    #[test]
    fn a_spike_on_the_block_edge_gets_an_edge_vertex_and_the_neighbour_agrees() {
        let mut left = vec![0.0_f32; 65 * 65];
        let mut right = vec![0.0_f32; 65 * 65];
        for y in 0..65 {
            let v = if y == 37 { 600.0 } else { 0.0 };
            left[y * 65 + 64] = v;
            right[y * 65] = v;
        }
        let on_edge = |h: &[f32], x: f32| -> Vec<u32> {
            let m = mesh_block(h, 65, 5.0, 400).unwrap().mesh;
            let mut v: Vec<u32> = m
                .vertices
                .iter()
                .filter(|v| (v[0] - x).abs() < 0.5)
                .map(|v| (v[1] / 128.0) as u32)
                .collect();
            v.sort_unstable();
            v
        };
        let (a, b) = (on_edge(&left, 64.0 * 128.0), on_edge(&right, 0.0));
        assert!(a.contains(&37), "{a:?}");
        assert_eq!(a, b);
    }

    #[test]
    fn a_size_that_is_not_a_power_of_two_plus_one_is_refused() {
        assert!(mesh_block(&vec![0.0; 64 * 64], 64, 1.0, 100).is_none());
        assert!(mesh_block(&[0.0; 5], 2, 1.0, 100).is_none());
    }

    /// A FNV-1a digest of the mesh of a rough field with a hole, pinned from the version that scored the triangles of
    /// the border vertices too (that work was thrown away, so the mesh must not have changed).
    #[test]
    fn the_mesh_of_a_rough_field_with_a_hole_is_pinned() {
        let span = 65;
        let mut rng = Rng::new(7);
        let mut samples: Vec<f32> = (0..span * span)
            .map(|i| {
                let (x, y) = ((i % span) as f32, (i / span) as f32);
                (x * 0.3).sin() * 40.0 + (y * 0.2).cos() * 25.0 + rng.below(600) as f32 / 100.0
            })
            .collect();
        for y in 20..30 {
            for x in 30..44 {
                samples[y * span + x] = f32::NAN;
            }
        }
        let meshed = mesh_block(&samples, span, 2.0, 400).unwrap();
        let mut digest = 0xcbf2_9ce4_8422_2325_u64;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                digest = (digest ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
            }
        };
        for v in &meshed.mesh.vertices {
            for c in v {
                eat(&c.to_bits().to_le_bytes());
            }
        }
        for i in &meshed.mesh.indices {
            eat(&i.to_le_bytes());
        }
        assert_eq!(
            (
                digest,
                meshed.mesh.vertices.len(),
                meshed.mesh.indices.len()
            ),
            (0xe4c2_fab1_5b69_02e5, 392, 2025)
        );
    }
}
