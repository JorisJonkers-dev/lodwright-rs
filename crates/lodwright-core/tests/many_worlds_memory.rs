// SPDX-License-Identifier: GPL-3.0-only
//! A load order with 60 synthetic worlds can be walked one world at a time within a stated memory cap.
//!
//! Peak resident memory comes from the kernel's high-water mark (`VmHWM` in `/proc/self/status`, reset through
//! `/proc/self/clear_refs`), so the test only runs where those exist (Linux) and is the only test in this file, so
//! no other test in the process moves the mark.

use lodwright_core::plugin::read;
use lodwright_core::worldspace::{Inheritance, WorldIndex};
use lodwright_testkit::land::Heights;
use lodwright_testkit::plugin::PluginBuilder;
use lodwright_testkit::world::{SyntheticWorld, world_group};

const WORLDS: u32 = 60;
const CELLS_PER_WORLD: i32 = 64;
/// Cap on growth of peak memory while walking all worlds one at a time: far below the heights of all worlds held
/// together (60 x 64 cells x 4356 bytes, about 16 MiB) and a few times one world's (about 0.3 MiB).
const CAP_BYTES: u64 = 4 * 1024 * 1024;

fn high_water_mark_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn reset_high_water_mark() -> bool {
    std::fs::write("/proc/self/clear_refs", "5").is_ok()
}

fn peak_growth_bytes(work: impl FnOnce()) -> Option<u64> {
    if !reset_high_water_mark() {
        return None;
    }
    let before = high_water_mark_kib()?;
    work();
    Some((high_water_mark_kib()?.saturating_sub(before)) * 1024)
}

#[test]
fn walking_sixty_worlds_one_at_a_time_stays_under_the_cap() {
    let mut builder = PluginBuilder::new();
    for index in 0..WORLDS {
        let mut world = SyntheticWorld::new(0x1000 + index * 0x2000, &format!("World{index:02}"));
        for cell in 0..CELLS_PER_WORLD {
            world = world.cell(
                cell % 8,
                cell / 8,
                Heights::flat(f32::from(u16::try_from(cell).unwrap())),
            );
        }
        builder = builder.group(world_group(&world, false));
    }
    let bytes = builder.build();
    let plugins = vec![("Many.esm".to_owned(), read(&bytes).unwrap())];
    drop(bytes);
    let index = WorldIndex::new(&plugins).unwrap();
    let ids: Vec<u32> = index.worlds().map(|(id, _)| id).collect();
    assert_eq!(ids.len(), usize::try_from(WORLDS).unwrap());

    let mut cells = 0;
    let Some(streamed) = peak_growth_bytes(|| {
        for id in &ids {
            let world = index.build(&plugins, *id, Inheritance::None).unwrap();
            cells += world.cell_count();
        }
    }) else {
        eprintln!("no peak-memory counters here; skipping");
        return;
    };
    assert_eq!(
        cells,
        usize::try_from(WORLDS).unwrap() * usize::try_from(CELLS_PER_WORLD).unwrap()
    );
    assert!(
        streamed <= CAP_BYTES,
        "peak memory grew by {streamed} bytes walking one world at a time, cap {CAP_BYTES}"
    );

    // the same walk holding every assembled world at once would have blown the cap: the test measures something
    let mut held = Vec::new();
    let all_at_once = peak_growth_bytes(|| {
        for id in &ids {
            held.push(index.build(&plugins, *id, Inheritance::None).unwrap());
        }
    })
    .unwrap();
    eprintln!("peak growth: {streamed} bytes one world at a time, {all_at_once} bytes all at once");
    assert!(
        all_at_once > CAP_BYTES,
        "holding every world grew peak memory by only {all_at_once} bytes"
    );
}
