// SPDX-License-Identifier: GPL-3.0-only
//! Timing and a checksum of the terrain mesher on synthetic fractal ground: `cargo bench -p lodwright-core --bench tin`.
//! The checksum (a hash of every vertex and index) must not change when the mesher is made faster.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::print_stdout,
    clippy::many_single_char_names,
    reason = "a benchmark on synthetic grids of at most 1025 samples that prints its numbers"
)]

use std::time::Instant;

use lodwright_core::tin::mesh_block;
use lodwright_testkit::rng::Rng;

/// Fractal heights on a `span * span` grid: octaves of bilinear value noise, with a few cliffs.
fn ground(span: usize, seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let mut h = vec![0.0_f32; span * span];
    let mut cell = (span - 1) / 2;
    let mut amplitude = 600.0_f32;
    while cell >= 1 {
        let n = (span - 1) / cell + 1;
        let lattice: Vec<f32> = (0..n * n)
            .map(|_| rng.below(2000) as f32 / 1000.0 - 1.0)
            .collect();
        for y in 0..span {
            for x in 0..span {
                let (gx, gy) = (x / cell, y / cell);
                let (fx, fy) = (
                    (x % cell) as f32 / cell as f32,
                    (y % cell) as f32 / cell as f32,
                );
                let at = |i: usize, j: usize| lattice[j.min(n - 1) * n + i.min(n - 1)];
                let top = at(gx, gy) * (1.0 - fx) + at(gx + 1, gy) * fx;
                let bottom = at(gx, gy + 1) * (1.0 - fx) + at(gx + 1, gy + 1) * fx;
                h[y * span + x] += amplitude * (top * (1.0 - fy) + bottom * fy);
            }
        }
        cell /= 2;
        amplitude *= 0.5;
    }
    for y in 0..span {
        for x in span / 3..span {
            h[y * span + x] += 400.0;
        }
    }
    h
}

fn checksum(mesh: &lodwright_core::terrain::TerrainMesh) -> u64 {
    let mut sum = 0xcbf2_9ce4_8422_2325_u64;
    let mut mix = |v: u64| {
        sum ^= v;
        sum = sum.wrapping_mul(0x0100_0000_01b3);
    };
    for v in &mesh.vertices {
        for c in v {
            mix(u64::from(c.to_bits()));
        }
    }
    for i in &mesh.indices {
        mix(u64::from(*i));
    }
    sum
}

fn main() {
    for (level, span, blocks) in [(4, 129, 40), (8, 257, 10), (16, 513, 4), (32, 1025, 2)] {
        let grounds: Vec<Vec<f32>> = (0..blocks).map(|b| ground(span, 100 + b as u64)).collect();
        let start = Instant::now();
        let mut sum = 0_u64;
        let mut vertices = 0;
        for h in &grounds {
            let m = mesh_block(h, span, 0.0, 2000).expect("a mesh");
            sum ^= checksum(&m.mesh).rotate_left(1);
            vertices += m.mesh.vertices.len();
        }
        let elapsed = start.elapsed();
        println!(
            "level {level}: {blocks} blocks, {vertices} vertices, {:.1} ms per block, checksum {sum:016x}",
            elapsed.as_secs_f64() * 1000.0 / f64::from(blocks)
        );
    }
}
