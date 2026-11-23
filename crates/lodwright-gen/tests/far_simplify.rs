// SPDX-License-Identifier: GPL-3.0-only
//! The simplifier for far shapes, on synthetic grids built with integer arithmetic only, so the input is the same on
//! every system.

#![allow(
    clippy::cast_precision_loss,
    reason = "grid coordinates and the 8-bit test heights fit an f32 exactly"
)]

use std::collections::BTreeSet;

use lodwright_gen::{FarLimits, FarMesh, FarSimplifyError, simplify_far_mesh};

struct Grid {
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    normals: Vec<[f32; 3]>,
    triangles: Vec<[u32; 3]>,
    size: u32,
    seam: Option<u32>,
}

impl Grid {
    /// A square grid. `height` gives each point's height; `seam` splits one column into two sets of vertices whose
    /// texture coordinates do not meet.
    fn new(size: u32, seam: Option<u32>, height: impl Fn(u32, u32) -> f32) -> Self {
        let mut grid = Self {
            positions: Vec::new(),
            uvs: Vec::new(),
            normals: Vec::new(),
            triangles: Vec::new(),
            size,
            seam,
        };
        for y in 0..=size {
            for x in 0..=size {
                grid.push(x, y, height(x, y), 0.0);
            }
        }
        if let Some(column) = seam {
            for y in 0..=size {
                grid.push(column, y, height(column, y), 0.5);
            }
        }
        for y in 0..size {
            for x in 0..size {
                let right_of_seam = seam == Some(x);
                let a = grid.vertex(x, y, right_of_seam);
                let b = grid.vertex(x + 1, y, false);
                let c = grid.vertex(x + 1, y + 1, false);
                let d = grid.vertex(x, y + 1, right_of_seam);
                grid.triangles.push([a, b, c]);
                grid.triangles.push([a, c, d]);
            }
        }
        grid
    }

    fn push(&mut self, x: u32, y: u32, z: f32, shift: f32) {
        self.positions.push([x as f32, y as f32, z]);
        self.uvs.push([
            x as f32 / self.size as f32 + shift,
            y as f32 / self.size as f32,
        ]);
        self.normals.push([0.0, 0.0, 1.0]);
    }

    fn vertex(&self, x: u32, y: u32, right_of_seam: bool) -> u32 {
        let row = self.size + 1;
        if right_of_seam {
            row * row + y
        } else {
            y * row + x
        }
    }

    fn mesh<'a>(&'a self, locked: &'a [bool]) -> FarMesh<'a> {
        FarMesh {
            positions: &self.positions,
            uvs: &self.uvs,
            normals: &self.normals,
            triangles: &self.triangles,
            locked,
        }
    }

    /// The edges along the seam on one side, as pairs of rows.
    fn seam_edges(&self, triangles: &[[u32; 3]], right: bool) -> BTreeSet<(u32, u32)> {
        let column = self.seam.expect("a grid with a seam");
        let row = self.size + 1;
        let on_side = |v: u32| {
            if right {
                (v >= row * row).then(|| v - row * row)
            } else {
                (v < row * row && v % row == column).then_some(v / row)
            }
        };
        let mut edges = BTreeSet::new();
        for t in triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                if let (Some(a), Some(b)) = (on_side(a), on_side(b)) {
                    edges.insert((a.min(b), a.max(b)));
                }
            }
        }
        edges
    }
}

/// Rough ground from integer steps: no floating-point function that could differ between systems.
fn rough(x: u32, y: u32) -> f32 {
    let mixed = x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663);
    (mixed.wrapping_mul(2_654_435_761) >> 24) as f32 / 256.0
}

fn digest(triangles: &[[u32; 3]]) -> u64 {
    triangles
        .iter()
        .flatten()
        .flat_map(|index| index.to_le_bytes())
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

#[test]
fn a_flat_grid_keeps_the_requested_share_within_the_error() {
    let grid = Grid::new(32, None, |_, _| 0.0);
    let limits = FarLimits {
        keep: 0.1,
        max_error: 0.01,
    };
    let out = simplify_far_mesh(grid.mesh(&[]), limits).unwrap();
    assert_ne!(out.triangles.len(), 0);
    assert!(
        out.triangles.len() <= grid.triangles.len().div_ceil(10),
        "{} triangles left",
        out.triangles.len()
    );
    assert!(out.error <= limits.max_error, "error {}", out.error);
}

#[test]
fn the_error_limit_stops_a_rough_surface_before_the_triangle_target() {
    let grid = Grid::new(32, None, rough);
    let out = simplify_far_mesh(
        grid.mesh(&[]),
        FarLimits {
            keep: 0.01,
            max_error: 0.001,
        },
    )
    .unwrap();
    assert!(
        out.triangles.len() * 100 > grid.triangles.len(),
        "{} triangles left",
        out.triangles.len()
    );
    assert!(out.error <= 0.001, "error {}", out.error);
}

#[test]
fn a_texture_seam_stays_closed_on_both_sides() {
    let grid = Grid::new(32, Some(16), |_, _| 0.0);
    let out = simplify_far_mesh(
        grid.mesh(&[]),
        FarLimits {
            keep: 0.1,
            max_error: 0.05,
        },
    )
    .unwrap();
    assert!(
        out.triangles.len() < grid.triangles.len() / 2,
        "{} triangles left",
        out.triangles.len()
    );
    let left = grid.seam_edges(&out.triangles, false);
    let right = grid.seam_edges(&out.triangles, true);
    assert!(!left.is_empty());
    assert_eq!(
        left, right,
        "both sides of the seam must keep the same edges"
    );
    let mut row = 0;
    for &(a, b) in &left {
        assert_eq!(a, row, "the seam must have no gap");
        row = b;
    }
    assert_eq!(row, grid.size, "the seam must reach the far end");
}

#[test]
fn locked_border_vertices_and_edges_stay() {
    let grid = Grid::new(16, None, rough);
    let row = grid.size + 1;
    let locked: Vec<bool> = (0..row * row)
        .map(|v| v % row == 0 || v % row == grid.size || v / row == 0 || v / row == grid.size)
        .collect();
    let out = simplify_far_mesh(
        grid.mesh(&locked),
        FarLimits {
            keep: 0.1,
            max_error: 0.5,
        },
    )
    .unwrap();
    assert!(out.triangles.len() < grid.triangles.len());
    let edges: BTreeSet<(u32, u32)> = out
        .triangles
        .iter()
        .flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
        .map(|(a, b)| (a.min(b), a.max(b)))
        .collect();
    for i in 0..grid.size {
        for (a, b) in [
            (i, i + 1),
            (grid.size * row + i, grid.size * row + i + 1),
            (i * row, (i + 1) * row),
            (i * row + grid.size, (i + 1) * row + grid.size),
        ] {
            assert!(edges.contains(&(a, b)), "border edge {a}-{b} is gone");
        }
    }
}

/// CI runs this on Linux, Windows and macOS: a different result on one of them fails here.
#[test]
fn the_output_is_the_same_on_every_system() {
    let grid = Grid::new(32, Some(16), rough);
    let limits = FarLimits {
        keep: 0.25,
        max_error: 0.05,
    };
    let first = simplify_far_mesh(grid.mesh(&[]), limits).unwrap();
    let second = simplify_far_mesh(grid.mesh(&[]), limits).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        (first.triangles.len(), digest(&first.triangles)),
        (512, 13_172_544_332_083_621_018)
    );
}

#[test]
fn invalid_input_is_refused() {
    let grid = Grid::new(2, None, |_, _| 0.0);
    let limits = FarLimits {
        keep: 0.5,
        max_error: 0.01,
    };
    let mut mesh = grid.mesh(&[]);
    mesh.uvs = &grid.uvs[1..];
    assert_eq!(
        simplify_far_mesh(mesh, limits),
        Err(FarSimplifyError::VertexListsDiffer)
    );

    let bad = [[0, 1, 99]];
    let mut mesh = grid.mesh(&[]);
    mesh.triangles = &bad;
    assert_eq!(
        simplify_far_mesh(mesh, limits),
        Err(FarSimplifyError::IndexOutOfRange(99))
    );

    let mut positions = grid.positions.clone();
    positions[0][2] = f32::NAN;
    let mut mesh = grid.mesh(&[]);
    mesh.positions = &positions;
    assert_eq!(
        simplify_far_mesh(mesh, limits),
        Err(FarSimplifyError::NotFinite)
    );

    for limits in [
        FarLimits {
            keep: 0.0,
            max_error: 0.01,
        },
        FarLimits {
            keep: 0.5,
            max_error: f32::NAN,
        },
    ] {
        assert_eq!(
            simplify_far_mesh(grid.mesh(&[]), limits),
            Err(FarSimplifyError::BadLimits)
        );
    }
}
