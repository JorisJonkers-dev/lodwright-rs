// SPDX-License-Identifier: GPL-3.0-only
//! A guard on the speed of the hot steps, on synthetic data.
//!
//! The terrain block step and the atlas encode are the steps that dominate a real run (`docs/performance.md`). Each is
//! timed here in CPU seconds and divided by the CPU seconds of a fixed calibration loop run on the same machine, so
//! a slow runner does not fail the check. The ratio may be at most `ALLOWED` times the one recorded when the guard was
//! written: a slowdown of that size is a regression to look at, a smaller one is noise on a shared machine.
//! The guard is ignored in a plain `cargo test` (a debug or instrumented build has other ratios); run it with
//! `cargo test --release -p lodwright-api --test step_bench -- --ignored` (`task bench`). Set `LODWRIGHT_BENCH=print` to print the ratios instead of checking them (to record a new baseline after a speed-up).

#![allow(
    clippy::cast_precision_loss,
    reason = "small counts and milliseconds in a timing guard"
)]
use lodwright_api::{CancelToken, GenerateOutcome, GenerateRequest, NoProgress, generate};
use lodwright_contract::Request;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};

/// How many times slower than the recorded ratio a step may be before the check fails.
const ALLOWED: f64 = 4.0;
/// Recorded ratios (step CPU time over calibration CPU time) on the machine of the author.
const TERRAIN_BLOCKS_RATIO: f64 = 12.0;
const ATLAS_ENCODE_RATIO: f64 = 5.0;

fn cpu_ms() -> f64 {
    lodwright_contract::process_cpu_ms().map_or(f64::NAN, |ms| ms as f64)
}

/// A fixed amount of single-threaded integer work.
fn calibration() -> f64 {
    let (wall, start) = (std::time::Instant::now(), cpu_ms());
    let mut x = 0x9E37_79B9_7F4A_7C15_u64;
    for _ in 0..400_000_000_u64 {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
    }
    std::hint::black_box(x);
    let used = cpu_ms() - start;
    if used.is_nan() {
        wall.elapsed().as_secs_f64() * 1000.0
    } else {
        used
    }
}

fn check(name: &str, ratio: f64, recorded: f64) {
    if std::env::var("LODWRIGHT_BENCH").is_ok_and(|v| v == "print") {
        eprintln!("{name}: ratio {ratio:.2}");
        return;
    }
    assert!(
        ratio <= recorded * ALLOWED,
        "{name} takes {ratio:.2} times the calibration loop, the recorded ratio is {recorded:.2} (allowed {ALLOWED}x): \
         a slowdown to look at; LODWRIGHT_BENCH=print prints the ratios"
    );
}

#[test]
#[ignore = "timing guard: cargo test --release -p lodwright-api --test step_bench -- --ignored"]
fn the_terrain_block_step_stays_within_a_few_times_its_recorded_cost() {
    let dir = std::env::temp_dir().join(format!("lodwright-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    // 8 by 8 cells of rolling land: 4 level-4 blocks, each in several levels
    let mut world = SyntheticWorld::new(0xB00, "Bench");
    for cx in 0..8 {
        for cy in 0..8 {
            let phase = (cx * 8 + cy) as f32;
            world = world.cell(
                cx,
                cy,
                Heights::from_fn(|x, y| {
                    200.0 * ((x as f32 * 0.4 + phase).sin() + (y as f32 * 0.3).cos())
                }),
            );
        }
    }
    std::fs::write(data.join("Bench.esm"), world_plugin(&[], &world, false)).unwrap();
    let mut request = Request::new(
        data.to_str().unwrap(),
        &["Bench.esm"],
        dir.join("out").to_str().unwrap(),
    );
    request.objects.enabled = false;
    request.trees.enabled = false;
    request.terrain.land_textures = false;
    request.limits.threads = Some(1);
    let reference = calibration();
    let outcome = generate(
        &GenerateRequest::new(request),
        &mut NoProgress,
        &CancelToken::new(),
    )
    .unwrap();
    let GenerateOutcome::Done { result } = outcome else {
        panic!("cancelled");
    };
    let blocks = result
        .steps
        .iter()
        .find(|s| s.step == "terrain.blocks")
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let used = blocks.cpu_ms.map_or(blocks.wall_ms as f64, |ms| ms as f64);
    check("terrain.blocks", used / reference, TERRAIN_BLOCKS_RATIO);
}

#[test]
#[ignore = "timing guard: cargo test --release -p lodwright-api --test step_bench -- --ignored"]
fn the_atlas_encode_stays_within_a_few_times_its_recorded_cost() {
    // a 2048 by 2048 atlas page: the work the object step does once per page
    let pixels: Vec<u8> = (0..2048_u32 * 2048)
        .flat_map(|i| {
            let v = (i.wrapping_mul(2_654_435_761) >> 24) as u8;
            [v, v.wrapping_add(40), v.wrapping_mul(3), 255]
        })
        .collect();
    let image = Image::new(2048, 2048, pixels).unwrap();
    let reference = calibration();
    let start = cpu_ms();
    let wall = std::time::Instant::now();
    let bytes = encode(
        &image,
        &EncodeOptions {
            format: Format::Bc3,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )
    .unwrap();
    std::hint::black_box(bytes);
    let used = cpu_ms() - start;
    let used = if used.is_nan() {
        wall.elapsed().as_secs_f64() * 1000.0
    } else {
        used
    };
    check("atlas encode", used / reference, ATLAS_ENCODE_RATIO);
}
