// SPDX-License-Identifier: MPL-2.0
//! Terrain LOD generation for one worldspace: block NIFs, normal maps, diffuse maps and the settings file.
//!
//! Every block is independent, so blocks run on a bounded number of threads; the output does not depend on the
//! thread count because each file's bytes depend only on the world and the settings, and the report is sorted.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use lodwright_contract::sha256_digest;
use lodwright_core::layout::{self, BlockId};
use lodwright_core::lod::LodSettings;
use lodwright_core::raster::{BlockGrid, HeightRaster};
use lodwright_core::terrain::{LevelSettings, TerrainSettings, build_block_mesh};
use lodwright_core::worldspace::Worldspace;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
use lodwright_nif::{TerrainBlock, terrain_block};

use crate::diffuse::LandTiles;
use crate::error::GenError;
use crate::images::{diffuse_image, normal_image};

const CELL_UNITS: f32 = 4096.0;

/// How to run.
#[derive(Debug, Clone, Copy)]
pub struct Options<'a> {
    /// Worker threads (at least 1).
    pub threads: usize,
    /// Land textures for the diffuse maps; `None` leaves the ground to vertex colours.
    pub land: Option<&'a LandTiles>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            threads: 1,
            land: None,
        }
    }
}

/// One file written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    /// Path relative to the output folder, forward slashes.
    pub path: String,
    /// Size in bytes.
    pub bytes: u64,
    /// `sha256:` of the contents.
    pub digest: String,
}

/// What a run wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The world.
    pub world: String,
    /// Files, sorted by path.
    pub files: Vec<FileRecord>,
}

pub(crate) fn write_file(out: &Path, rel: &str, bytes: &[u8]) -> Result<FileRecord, GenError> {
    let target: PathBuf = out.join(rel);
    let io = |source| GenError::Io {
        what: rel.to_owned(),
        source,
    };
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let temp = target.with_extension("tmp");
    std::fs::write(&temp, bytes).map_err(io)?;
    std::fs::rename(&temp, &target).map_err(io)?;
    Ok(FileRecord {
        path: rel.to_owned(),
        bytes: bytes.len() as u64,
        digest: sha256_digest(bytes),
    })
}

fn dds(image: &Image) -> Result<Vec<u8>, GenError> {
    // source: game-lod-terrain-textures
    Ok(encode(
        image,
        &EncodeOptions {
            format: Format::Bc1,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )?)
}

struct Job {
    level: LevelSettings,
    cell: (i32, i32),
}

fn run_job(
    world: &Worldspace,
    raster: &HeightRaster,
    job: &Job,
    out: &Path,
    tiles: &LandTiles,
) -> Result<Vec<FileRecord>, GenError> {
    let level = &job.level;
    let grid = raster.grid();
    let (bx, by) = (grid.block_of(job.cell.0), grid.block_of(job.cell.1));
    let Some((_, mesh, _)) = build_block_mesh(raster, level, bx, by) else {
        return Ok(Vec::new());
    };
    let id = BlockId {
        level: level.level,
        x: job.cell.0,
        y: job.cell.1,
    };
    let name = world.name();
    let units = level.level as f32 * CELL_UNITS;
    let [diffuse_ref, normal_ref] = layout::nif_texture_paths(name, id)?;
    let nif = terrain_block(&TerrainBlock {
        origin: (
            job.cell.0 as f32 * CELL_UNITS,
            job.cell.1 as f32 * CELL_UNITS,
        ),
        size: units,
        vertices: &mesh.vertices,
        indices: &mesh.indices,
        textures: [&diffuse_ref, &normal_ref],
    })?
    .write()?;
    let samples = raster.block_samples(bx, by).unwrap_or_default();
    let span = (level.level as usize) * 32 + 1;
    let normals = dds(&normal_image(&samples, span, units, level.normal_size))?;
    let diffuse = dds(&diffuse_image(world, id, level.diffuse_size, tiles))?;
    Ok(vec![
        write_file(out, &layout::mesh_path(name, id)?, &nif)?,
        write_file(out, &layout::diffuse_path(name, id)?, &diffuse)?,
        write_file(out, &layout::normal_path(name, id)?, &normals)?,
    ])
}

fn jobs_of(raster: &HeightRaster, level: &LevelSettings) -> Vec<Job> {
    let (cells_w, cells_h) = raster.cells();
    let (origin_x, origin_y) = raster.origin_cell();
    let step = i32::try_from(level.level).unwrap_or(i32::MAX);
    let mut jobs = Vec::new();
    for by in 0..cells_h / level.level {
        for bx in 0..cells_w / level.level {
            jobs.push(Job {
                level: *level,
                cell: (
                    origin_x + i32::try_from(bx).unwrap_or(0) * step,
                    origin_y + i32::try_from(by).unwrap_or(0) * step,
                ),
            });
        }
    }
    jobs
}

/// Generate the terrain LOD of `world` into `out` (created if missing): block meshes and textures for every level
/// of `settings`, and the settings file. Files appear under their final names only when complete.
///
/// # Errors
/// [`GenError`] for a world name that cannot be a folder, a raster that is too large, or a write that fails.
pub fn generate_terrain(
    world: &Worldspace,
    settings: &TerrainSettings,
    out: &Path,
    options: &Options,
) -> Result<Report, GenError> {
    layout::check_world_name(world.name())?;
    let world = &world.without_outliers();
    let no_tiles = LandTiles::empty();
    let tiles = options.land.unwrap_or(&no_tiles);
    let mut files = Vec::new();
    let mut highest = 0;
    for level in &settings.levels {
        let grid = BlockGrid::new(level.level).map_err(|_| lodwright_core::Error::ZeroLevel)?;
        let Some(raster) = HeightRaster::build(world, grid)? else {
            continue;
        };
        highest = highest.max(level.level);
        let jobs = jobs_of(&raster, level);
        let next = AtomicUsize::new(0);
        let done: Mutex<Vec<Result<Vec<FileRecord>, GenError>>> = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..options.threads.max(1) {
                scope.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(i) else { break };
                        let result = run_job(world, &raster, job, out, tiles);
                        let failed = result.is_err();
                        if let Ok(mut done) = done.lock() {
                            done.push(result);
                        }
                        if failed {
                            next.store(jobs.len(), Ordering::Relaxed);
                        }
                    }
                });
            }
        });
        for result in done.into_inner().map_err(|_| GenError::Worker)? {
            files.extend(result?);
        }
    }
    if let Some(bounds) = world.bounds() {
        let clamp = |v: i32| i16::try_from(v).unwrap_or(if v < 0 { i16::MIN } else { i16::MAX });
        let settings_file = LodSettings::for_bounds(
            (clamp(bounds.min_x), clamp(bounds.min_y)),
            (clamp(bounds.max_x), clamp(bounds.max_y)),
            highest,
        );
        files.push(write_file(
            out,
            &layout::settings_path(world.name())?,
            &settings_file.to_bytes(),
        )?);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Report {
        world: world.name().to_owned(),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::{Options, generate_terrain};
    use lodwright_core::land::CellHeights;
    use lodwright_core::terrain::TerrainSettings;
    use lodwright_core::worldspace::Worldspace;
    use lodwright_nif::NifFile;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lodwright-gen-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn world() -> Worldspace {
        Worldspace::from_cells(
            1,
            "Mojave",
            (0..3).flat_map(|x| {
                (0..2).map(move |y| ((x, y), CellHeights::flat(100.0 + 10.0 * x as f32)))
            }),
        )
    }

    #[test]
    fn a_world_gives_blocks_textures_and_a_settings_file_at_the_planned_paths() {
        let out = temp("paths");
        let report = generate_terrain(
            &world(),
            &TerrainSettings::default(),
            &out,
            &Options::default(),
        )
        .unwrap();
        let paths: Vec<&str> = report.files.iter().map(|f| f.path.as_str()).collect();
        for want in [
            "meshes/landscape/lod/mojave/mojave.level4.x0.y0.nif",
            "meshes/landscape/lod/mojave/mojave.level32.x0.y0.nif",
            "textures/landscape/lod/mojave/diffuse/mojave.n.level8.x0.y0.dds",
            "textures/landscape/lod/mojave/normals/mojave.n.level16.x0.y0.dds",
            "lodsettings/mojave.dlodsettings",
        ] {
            assert!(paths.contains(&want), "{want} missing from {paths:?}");
        }
        for f in &report.files {
            assert_eq!(std::fs::metadata(out.join(&f.path)).unwrap().len(), f.bytes);
        }
        let nif =
            std::fs::read(out.join("meshes/landscape/lod/mojave/mojave.level4.x0.y0.nif")).unwrap();
        assert_eq!(NifFile::read(&nif).unwrap().blocks.len(), 8);
    }

    #[test]
    fn the_thread_count_does_not_change_a_single_byte() {
        let (one, four) = (temp("t1"), temp("t4"));
        let a = generate_terrain(
            &world(),
            &TerrainSettings::default(),
            &one,
            &Options {
                threads: 1,
                land: None,
            },
        )
        .unwrap();
        let b = generate_terrain(
            &world(),
            &TerrainSettings::default(),
            &four,
            &Options {
                threads: 4,
                land: None,
            },
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn the_settings_file_carries_the_world_bounds() {
        let out = temp("settings");
        generate_terrain(
            &world(),
            &TerrainSettings::default(),
            &out,
            &Options::default(),
        )
        .unwrap();
        let bytes = std::fs::read(out.join("lodsettings/mojave.dlodsettings")).unwrap();
        let s = lodwright_core::lod::LodSettings::parse(&bytes).unwrap();
        assert_eq!((s.south_west, s.north_east), ((0, 0), (2, 1)));
        assert_eq!(s.highest_level, 32);
    }

    #[test]
    fn an_empty_world_writes_nothing() {
        let out = temp("empty");
        let empty = Worldspace::from_cells(1, "Empty", std::iter::empty());
        let report = generate_terrain(
            &empty,
            &TerrainSettings::default(),
            &out,
            &Options::default(),
        )
        .unwrap();
        assert_eq!(report.files, []);
    }

    #[test]
    fn a_world_name_that_is_not_a_folder_is_refused() {
        let out = temp("badname");
        let bad = Worldspace::from_cells(1, "../x", std::iter::empty());
        assert!(
            generate_terrain(&bad, &TerrainSettings::default(), &out, &Options::default()).is_err()
        );
    }
}
