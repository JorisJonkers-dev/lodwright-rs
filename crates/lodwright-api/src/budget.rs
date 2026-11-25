// SPDX-License-Identifier: GPL-3.0-only
//! Holding `limits.memory_mib` while a run goes: the terrain step of each world gets as many workers as the budget
//! leaves room for, never fewer than one.
//!
//! The worker count never changes a byte of the output (terrain blocks are byte-identical for any thread count), so
//! lowering it is always safe. What is left of the budget is the limit minus what the process holds now and minus the
//! world's height rasters; each worker then needs room for the textures of one block. The resident size is read on
//! Linux (`VmRSS`); elsewhere the run uses the preflight estimate (four times the plugin bytes) as a stand-in, so the
//! budget is measured on Linux and best effort on Windows and macOS.

use lodwright_core::terrain::TerrainSettings;
use lodwright_core::worldspace::Worldspace;

const MIB: u64 = 1024 * 1024;
/// Height samples per cell edge of a raster, plus the shared edge.
const SAMPLES_PER_CELL_EDGE: u64 = 33;
/// Bytes per raster sample: the height and what the planner keeps beside it.
const BYTES_PER_SAMPLE: u64 = 8;
/// Buffers per worker texture: the image, its mip chain and the encoded file.
const TEXTURE_BUFFERS: u64 = 3;
/// A worker's room beside its textures: the block mesh and its planner state.
const WORKER_BASE_MIB: u64 = 8;

/// The resident size of this process in MiB (`VmRSS`), where the platform tells.
#[must_use]
pub(crate) fn resident_mib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vm_rss_kib(&status).map(|kib| kib.div_ceil(1024))
}

fn parse_vm_rss_kib(status: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// The worker threads of a run that names none: every core the machine offers.
#[must_use]
pub(crate) fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}

/// The memory a run holds when the request names no budget: three quarters of the machine's physical memory
/// (`MemTotal`), so that a 16 GiB machine keeps 4 GiB for the rest of the system. Read on Linux only; elsewhere
/// there is no default budget and the thread count alone applies.
#[must_use]
pub(crate) fn default_budget_mib() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_mem_total_kib(&info).map(|kib| kib / 1024 * 3 / 4)
}

fn parse_mem_total_kib(info: &str) -> Option<u64> {
    let line = info.lines().find(|l| l.starts_with("MemTotal:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// The height rasters of `world` for the levels of `settings`, in MiB (rounded up).
#[must_use]
pub(crate) fn raster_mib(world: &Worldspace, settings: &TerrainSettings) -> u64 {
    let Some(b) = world.bounds() else {
        return 0;
    };
    let cells_x = u64::try_from(b.max_x - b.min_x + 1).unwrap_or(0);
    let cells_y = u64::try_from(b.max_y - b.min_y + 1).unwrap_or(0);
    if settings.levels.is_empty() {
        return 0;
    }
    // each level builds its raster over the whole world, one level at a time, so one raster is held at once
    let raster = (cells_x * (SAMPLES_PER_CELL_EDGE - 1) + 1)
        * (cells_y * (SAMPLES_PER_CELL_EDGE - 1) + 1)
        * BYTES_PER_SAMPLE;
    raster.div_ceil(MIB)
}

/// What one terrain worker needs for the largest block textures of `settings`, in MiB (rounded up).
#[must_use]
pub(crate) fn worker_mib(settings: &TerrainSettings) -> u64 {
    let texture_bytes = settings
        .levels
        .iter()
        .map(|l| {
            let d = u64::from(l.diffuse_size);
            let n = u64::from(l.normal_size);
            (d * d + n * n) * 4 * TEXTURE_BUFFERS
        })
        .max()
        .unwrap_or(0);
    WORKER_BASE_MIB + texture_bytes.div_ceil(MIB)
}

/// The terrain workers for a world: `threads`, lowered so that `resident + raster + workers * per_worker` stays
/// within `limit`, and never below one. Without a limit, `threads`.
#[must_use]
pub(crate) fn workers(
    limit: Option<u64>,
    resident: u64,
    raster: u64,
    per_worker: u64,
    threads: usize,
) -> usize {
    let Some(limit) = limit else {
        return threads.max(1);
    };
    let room = limit.saturating_sub(resident).saturating_sub(raster);
    let fit = usize::try_from(room / per_worker.max(1)).unwrap_or(usize::MAX);
    fit.clamp(1, threads.max(1))
}

#[cfg(test)]
mod tests {
    use super::{parse_mem_total_kib, parse_vm_rss_kib, worker_mib, workers};
    use lodwright_core::terrain::TerrainSettings;
    use lodwright_testkit::{covers, ids};

    #[test]
    fn the_default_budget_reads_the_total_memory() {
        assert_eq!(
            parse_mem_total_kib("MemTotal:       16384000 kB\nMemFree: 1 kB\n"),
            Some(16_384_000)
        );
        assert_eq!(parse_mem_total_kib("nothing"), None);
    }

    #[test]
    fn without_a_limit_every_thread_works() {
        assert_eq!(workers(None, 10_000, 10_000, 10_000, 4), 4);
    }

    #[test]
    fn the_workers_shrink_to_what_the_budget_leaves_and_never_below_one() {
        covers(&[ids::generate::memory::WORKERS_SCALE_TO_THE_BUDGET]);
        // 100 MiB left after the process and the raster: three workers of 30 MiB fit
        assert_eq!(workers(Some(300), 150, 50, 30, 4), 3);
        assert_eq!(
            workers(Some(300), 150, 50, 10, 4),
            4,
            "never more than asked for"
        );
        assert_eq!(
            workers(Some(100), 150, 50, 30, 4),
            1,
            "an exhausted budget still runs"
        );
        assert_eq!(workers(Some(300), 0, 0, 0, 0), 1);
    }

    #[test]
    fn a_worker_needs_room_for_the_largest_textures() {
        covers(&[ids::generate::memory::WORKERS_SCALE_TO_THE_BUDGET]);
        // 256 px diffuse and normal, four bytes a pixel, three buffers each, plus the base
        let default = worker_mib(&TerrainSettings::default());
        assert_eq!(
            default,
            8 + (2 * 256 * 256 * 4 * 3_u64).div_ceil(1024 * 1024)
        );
    }

    #[test]
    fn the_resident_size_is_read_from_a_status_text() {
        covers(&[ids::generate::memory::PEAK_MEMORY_ON_LINUX]);
        assert_eq!(
            parse_vm_rss_kib("Name:\tx\nVmHWM:\t 9 kB\nVmRSS:\t  2049 kB\n"),
            Some(2049)
        );
        assert_eq!(parse_vm_rss_kib("Name:\tx\n"), None);
    }
}
