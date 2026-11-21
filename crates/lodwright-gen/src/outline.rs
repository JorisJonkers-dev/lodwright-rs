// SPDX-License-Identifier: GPL-3.0-only
//! The wet squares of a block as polygons: the outline of each region of equal water height is traced, reduced to its
//! corners and triangulated, so the plane covers exactly the squares it is given with only the vertices its outline
//! needs (a rectangle cover writes the corners of every rectangle, including the ones that lie on a straight edge).
//!
//! Own design, on integer grid points, so every decision is exact. When a region cannot be triangulated (the
//! construction below is not proven for every shape) the function returns `None` and the caller keeps its
//! rectangles; a result is only returned when the triangles add up to the area of the squares.

use std::collections::HashMap;

type P = (i64, i64);

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

fn cross(a: P, b: P) -> i64 {
    a.0 * b.1 - a.1 * b.0
}

fn orient(a: P, b: P, c: P) -> i64 {
    cross(sub(b, a), sub(c, a))
}

fn on_segment(p: P, a: P, b: P) -> bool {
    orient(a, b, p) == 0
        && p.0 >= a.0.min(b.0)
        && p.0 <= a.0.max(b.0)
        && p.1 >= a.1.min(b.1)
        && p.1 <= a.1.max(b.1)
}

/// Whether the closed segments `a b` and `c d` have a point in common.
fn segments_meet(a: P, b: P, c: P, d: P) -> bool {
    let (o1, o2) = (orient(a, b, c), orient(a, b, d));
    let (o3, o4) = (orient(c, d, a), orient(c, d, b));
    if o1.signum() * o2.signum() < 0 && o3.signum() * o4.signum() < 0 {
        return true;
    }
    on_segment(c, a, b) || on_segment(d, a, b) || on_segment(a, c, d) || on_segment(b, c, d)
}

/// Even-odd test of the point `p` against closed rings, all coordinates doubled by the caller.
fn inside(p: P, rings: &[&[P]]) -> bool {
    let mut odd = false;
    for ring in rings {
        for (i, &a) in ring.iter().enumerate() {
            let b = ring[(i + 1) % ring.len()];
            let (a, b) = ((a.0 * 2, a.1 * 2), (b.0 * 2, b.1 * 2));
            if (a.1 > p.1) != (b.1 > p.1) {
                let den = b.1 - a.1;
                let lhs = (p.0 - a.0) * den;
                let rhs = (p.1 - a.1) * (b.0 - a.0);
                if (den > 0 && lhs < rhs) || (den < 0 && lhs > rhs) {
                    odd = !odd;
                }
            }
        }
    }
    odd
}

fn area2(ring: &[P]) -> i64 {
    (0..ring.len())
        .map(|i| cross(ring[i], ring[(i + 1) % ring.len()]))
        .sum()
}

/// The loops that bound the squares `cells` (x, y) of one region, counter-clockwise around the region (the inside is
/// on the left), reduced to their corners. At a point where the boundary touches itself the loop turns right, which
/// keeps squares that meet at a corner apart.
fn loops_of(cells: &[P]) -> Vec<Vec<P>> {
    let member: std::collections::HashSet<P> = cells.iter().copied().collect();
    let mut edges: Vec<(P, P)> = Vec::new();
    for &(x, y) in cells {
        let sides = [
            ((0, -1), (x, y), (x + 1, y)),
            ((1, 0), (x + 1, y), (x + 1, y + 1)),
            ((0, 1), (x + 1, y + 1), (x, y + 1)),
            ((-1, 0), (x, y + 1), (x, y)),
        ];
        for (d, from, to) in sides {
            if !member.contains(&(x + d.0, y + d.1)) {
                edges.push((from, to));
            }
        }
    }
    edges.sort_unstable();
    let mut out_of: HashMap<P, Vec<usize>> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        out_of.entry(e.0).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        let mut ring = vec![edges[first].0];
        let mut at = first;
        used[first] = true;
        loop {
            let end = edges[at].1;
            if end == ring[0] {
                break;
            }
            ring.push(end);
            let d_in = sub(edges[at].1, edges[at].0);
            let next = out_of[&end]
                .iter()
                .copied()
                .filter(|&j| !used[j])
                .min_by_key(|&j| cross(d_in, sub(edges[j].1, edges[j].0)));
            let Some(next) = next else { return Vec::new() };
            used[next] = true;
            at = next;
        }
        loops.push(corners(&ring));
    }
    loops
}

/// `ring` without the points that lie on a straight run.
fn corners(ring: &[P]) -> Vec<P> {
    let n = ring.len();
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
            orient(a, b, c) != 0
        })
        .map(|i| ring[i])
        .collect()
}

/// Join `hole` (clockwise) to the polygon `poly` (counter-clockwise) by a bridge between two of their points, so one
/// ring without holes remains. `rest` are the holes still to join; the bridge must not touch them.
fn bridge(poly: &[P], hole: &[P], rest: &[Vec<P>]) -> Option<Vec<P>> {
    let mut walls: Vec<(P, P)> = Vec::new();
    let mut rings: Vec<&[P]> = vec![poly, hole];
    rings.extend(rest.iter().map(Vec::as_slice));
    for ring in &rings {
        for (i, &a) in ring.iter().enumerate() {
            walls.push((a, ring[(i + 1) % ring.len()]));
        }
    }
    let mut best: Option<(i64, usize, usize)> = None;
    for (hi, &h) in hole.iter().enumerate() {
        for (pi, &p) in poly.iter().enumerate() {
            let d = sub(h, p);
            let dist = d.0 * d.0 + d.1 * d.1;
            if dist == 0 {
                // the hole touches the outline at this point: the ring passes through it without a bridge
                let mut joined = Vec::with_capacity(poly.len() + hole.len());
                joined.extend_from_slice(&poly[..=pi]);
                joined.extend_from_slice(&hole[hi + 1..]);
                joined.extend_from_slice(&hole[..=hi]);
                joined.extend_from_slice(&poly[pi + 1..]);
                return Some(joined);
            }
            if best.is_some_and(|b| b.0 <= dist) {
                continue;
            }
            let clear = walls
                .iter()
                .all(|&(a, b)| a == h || b == h || a == p || b == p || !segments_meet(h, p, a, b));
            let mid = (h.0 + p.0, h.1 + p.1);
            let touching = walls
                .iter()
                .any(|&(a, b)| on_segment(mid, (a.0 * 2, a.1 * 2), (b.0 * 2, b.1 * 2)));
            if clear && !touching && inside(mid, &rings) {
                best = Some((dist, hi, pi));
            }
        }
    }
    let (_, hi, pi) = best?;
    let mut joined = Vec::with_capacity(poly.len() + hole.len() + 2);
    joined.extend_from_slice(&poly[..=pi]);
    joined.extend_from_slice(&hole[hi..]);
    joined.extend_from_slice(&hole[..=hi]);
    joined.push(poly[pi]);
    joined.extend_from_slice(&poly[pi + 1..]);
    Some(joined)
}

/// Triangles (as points, counter-clockwise) of a ring that may touch itself along bridges.
fn ears(mut ring: Vec<P>) -> Option<Vec<[P; 3]>> {
    let mut out = Vec::new();
    while ring.len() > 3 {
        let n = ring.len();
        let mut clipped = false;
        for i in 0..n {
            let (pi, ni) = ((i + n - 1) % n, (i + 1) % n);
            let (a, b, c) = (ring[pi], ring[i], ring[ni]);
            let turn = orient(a, b, c);
            if turn == 0 {
                // a straight point goes unless the ring passes through the same place elsewhere (a bridge): then
                // the other visit has to go first
                if ring.iter().filter(|&&q| q == b).count() == 1 {
                    ring.remove(i);
                    clipped = true;
                    break;
                }
                continue;
            }
            if turn < 0 {
                continue;
            }
            // a point of the ring stands in the way when it lies in the triangle (the edges included); one at a
            // corner's own place (the other side of a bridge) only when its edges run into the corner's wedge
            let tri = [a, b, c];
            let free = ring.iter().enumerate().all(|(j, &q)| {
                if j == pi || j == i || j == ni {
                    return true;
                }
                if let Some(k) = tri.iter().position(|&t| t == q) {
                    let (x, y, z) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
                    let (before, after) = (ring[(j + n - 1) % n], ring[(j + 1) % n]);
                    let into = |r: P| orient(x, y, r) > 0 && orient(x, z, r) < 0;
                    return !(into(before) || into(after));
                }
                !(orient(a, b, q) >= 0 && orient(b, c, q) >= 0 && orient(c, a, q) >= 0)
            });
            if free {
                out.push([a, b, c]);
                ring.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            return None;
        }
    }
    if ring.len() == 3 && orient(ring[0], ring[1], ring[2]) > 0 {
        out.push([ring[0], ring[1], ring[2]]);
    }
    Some(out)
}

/// The triangles of one region of squares, as points on the grid.
fn region_triangles(cells: &[P]) -> Option<Vec<[P; 3]>> {
    let loops = loops_of(cells);
    if loops.is_empty() {
        return None;
    }
    let (mut outers, mut holes): (Vec<Vec<P>>, Vec<Vec<P>>) =
        loops.into_iter().partition(|l| area2(l) > 0);
    outers.sort_by_key(|l| area2(l));
    // a hole belongs to the smallest outline that contains it
    let mut by_outline: Vec<Vec<Vec<P>>> = vec![Vec::new(); outers.len()];
    for hole in holes.drain(..) {
        let (a, b) = (hole[0], hole[1 % hole.len()]);
        let mid = (a.0 + b.0, a.1 + b.1);
        let at = outers.iter().position(|o| inside(mid, &[o.as_slice()]))?;
        by_outline[at].push(hole);
    }
    let mut triangles = Vec::new();
    for (outer, mut mine) in outers.into_iter().zip(by_outline) {
        let mut ring = outer;
        // holes joined from the right to the left
        mine.sort_by_key(|h| std::cmp::Reverse(h.iter().map(|p| p.0).min()));
        while !mine.is_empty() {
            let hole = mine.remove(0);
            ring = bridge(&ring, &hole, &mine)?;
        }
        triangles.extend(ears(ring)?);
    }
    let covered: i64 = triangles.iter().map(|t| orient(t[0], t[1], t[2])).sum();
    (covered == 2 * i64::try_from(cells.len()).ok()?).then_some(triangles)
}

/// The plane of the squares `keys` (`None` is dry; a region is a set of squares that share an edge and a key) of an
/// `n` by `n` grid of `square` units: vertices (at the height named by the key, as float bits) and counter-clockwise
/// triangles. `None` when a region cannot be triangulated or the plane would need more than 65535 vertices.
pub(crate) fn plane(
    keys: &[Option<u32>],
    heights: &[f32],
    n: usize,
    square: f32,
) -> Option<(Vec<[f32; 3]>, Vec<u16>)> {
    let mut seen = vec![false; keys.len()];
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut known: HashMap<(i64, i64, u32), u16> = HashMap::new();
    let mut indices: Vec<u16> = Vec::new();
    for start in 0..keys.len() {
        let Some(key) = keys[start] else { continue };
        if seen[start] {
            continue;
        }
        let mut cells: Vec<P> = Vec::new();
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(at) = stack.pop() {
            let (x, y) = (at % n, at / n);
            cells.push((i64::try_from(x).ok()?, i64::try_from(y).ok()?));
            let near = [
                (x > 0).then(|| at - 1),
                (x + 1 < n).then(|| at + 1),
                (y > 0).then(|| at - n),
                (y + 1 < n).then(|| at + n),
            ];
            for next in near.into_iter().flatten() {
                if !seen[next] && keys[next] == Some(key) {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        cells.sort_unstable();
        let height = heights[start];
        for tri in region_triangles(&cells)? {
            for p in tri {
                let id = if let Some(id) = known.get(&(p.0, p.1, key)) {
                    *id
                } else {
                    let id = u16::try_from(vertices.len()).ok()?;
                    #[allow(clippy::cast_precision_loss)] // grid coordinates are small
                    vertices.push([p.0 as f32 * square, p.1 as f32 * square, height]);
                    known.insert((p.0, p.1, key), id);
                    id
                };
                indices.push(id);
            }
        }
    }
    (!vertices.is_empty()).then_some((vertices, indices))
}

#[cfg(test)]
mod tests {
    use super::plane;

    fn grid(rows: &[&str]) -> (Vec<Option<u32>>, Vec<f32>, usize) {
        let n = rows.len();
        // rows are written north first
        let mut keys = vec![None; n * n];
        for (r, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate() {
                let key = match c {
                    '#' => Some(1.0_f32.to_bits()),
                    '+' => Some(2.0_f32.to_bits()),
                    _ => None,
                };
                keys[(n - 1 - r) * n + x] = key;
            }
        }
        let heights = keys
            .iter()
            .map(|k| k.map_or(f32::NAN, f32::from_bits))
            .collect();
        (keys, heights, n)
    }

    /// Twice the area of the triangles in squares of one unit, and whether every one is counter-clockwise.
    fn area_and_winding(vertices: &[[f32; 3]], indices: &[u16]) -> (f32, bool) {
        let mut area = 0.0;
        let mut ccw = true;
        for t in indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| vertices[i as usize]);
            let cr = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            ccw &= cr > 0.0;
            area += cr;
        }
        (area, ccw)
    }

    fn check(rows: &[&str], squares: f32, vertex_count: usize) {
        let (keys, heights, n) = grid(rows);
        let (v, i) = plane(&keys, &heights, n, 1.0).expect("a plane");
        assert_eq!(area_and_winding(&v, &i), (2.0 * squares, true));
        assert_eq!(v.len(), vertex_count, "{rows:?}");
    }

    #[test]
    fn a_rectangle_is_four_vertices_and_an_l_is_six() {
        check(&["###", "###", "..."], 6.0, 4);
        check(&["#..", "#..", "###"], 5.0, 6);
    }

    #[test]
    fn a_ring_is_joined_through_a_bridge_and_keeps_every_vertex_once() {
        // outer 4, hole 4
        check(&["####", "#..#", "#..#", "####"], 12.0, 8);
    }

    #[test]
    fn squares_that_meet_at_a_corner_are_two_regions_that_share_the_corner() {
        check(&["#.", ".#"], 2.0, 7);
        // the corner point is one vertex: 4 + 4 - 1
    }

    #[test]
    fn regions_of_other_heights_stay_apart_and_islands_in_a_hole_are_their_own_region() {
        check(&["#####", "#+++#", "#+#+#", "#+++#", "#####"], 25.0, 20);
    }

    #[test]
    fn every_pattern_of_a_four_by_four_grid_is_covered_exactly() {
        // all 65536 masks: a plane, when returned, adds up to the squares and winds counter-clockwise
        for mask in 0_u32..1 << 16 {
            let keys: Vec<Option<u32>> = (0..16)
                .map(|i| (mask >> i & 1 == 1).then_some(1.0_f32.to_bits()))
                .collect();
            let heights = vec![1.0; 16];
            let Some((v, i)) = plane(&keys, &heights, 4, 1.0) else {
                // only the empty grid has no plane
                assert_eq!(mask, 0);
                continue;
            };
            let (area, ccw) = area_and_winding(&v, &i);
            assert_eq!(area, 2.0 * mask.count_ones() as f32, "{mask:016b}");
            assert!(ccw, "{mask:016b}");
        }
    }

    #[test]
    fn random_larger_grids_with_two_heights_are_covered_exactly() {
        // xorshift: deterministic, so a failure names its seed
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut refused = [0_u32; 3];
        for round in 0..3000_u32 {
            let n = [8_usize, 16, 32][round as usize % 3];
            let density = 30 + next() % 50;
            let keys: Vec<Option<u32>> = (0..n * n)
                .map(|_| (next() % 100 < density).then(|| (1.0 + (next() % 2) as f32).to_bits()))
                .collect();
            let heights: Vec<f32> = keys
                .iter()
                .map(|k| k.map_or(f32::NAN, f32::from_bits))
                .collect();
            let squares = keys.iter().flatten().count() as f32;
            if squares == 0.0 {
                continue;
            }
            let Some((v, i)) = plane(&keys, &heights, n, 1.0) else {
                refused[round as usize % 3] += 1;
                continue;
            };
            assert_eq!(
                area_and_winding(&v, &i),
                (2.0 * squares, true),
                "round {round}"
            );
        }
        // noise this dense is far from a coast; a few of 3000 grids are refused and keep their rectangles
        assert!(refused.iter().sum::<u32>() <= 30, "{refused:?}");
    }
}
