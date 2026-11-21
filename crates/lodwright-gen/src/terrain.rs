// SPDX-License-Identifier: GPL-3.0-only
//! Terrain LOD generation for one worldspace: block NIFs, normal maps, diffuse maps and the settings file.
//!
//! Every block is independent, so blocks run on a bounded number of threads; the output does not depend on the
//! thread count because each file's bytes depend only on the world and the settings, and the report is sorted.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use lodwright_contract::Sha256Stream;
use lodwright_core::layout::{self, BlockId};
use lodwright_core::lod::LodSettings;
use lodwright_core::raster::{BlockGrid, HeightRaster};
use lodwright_core::terrain::{
    DropUnderWater, LevelSettings, TerrainSettings, build_block_mesh_dropping,
};
use lodwright_core::worldspace::Worldspace;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
use lodwright_nif::{TerrainBlock, WaterShape, terrain_block};

use crate::diffuse::LandTiles;
use crate::error::GenError;
use crate::images::{diffuse_image, normal_image_with_alpha, specular_alpha};
use crate::water::{Plane, add_walls, water_mesh};

const CELL_UNITS: f32 = 4096.0;

/// How to run.
#[derive(Clone, Copy)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "one switch per option of the request"
)]
pub struct Options<'a> {
    /// Worker threads (at least 1).
    pub threads: usize,
    /// Land textures for the diffuse maps; `None` leaves the ground to vertex colours.
    pub land: Option<&'a LandTiles>,
    /// Called after each block with the world, the level, the blocks finished at this level, the blocks of the
    /// level and whether the block was reused; from worker threads, in no fixed order.
    pub progress: Option<&'a (dyn Fn(&str, u32, u64, u64, bool) + Sync)>,
    /// Write a water plane into each block that has ground below its cells' water height.
    pub water: bool,
    /// Leave out terrain faces that lie at least this many world units below the water plane of their cell (see
    /// `DropUnderWater`); `None` keeps every face.
    pub drop_under_water: Option<f32>,
    /// Write walls under the outline of the water planes (see `water::add_walls`).
    pub water_walls: bool,
    /// Write a corner that two rectangles of a water plane share once, at levels 8 to 32.
    pub water_shared_vertices: bool,
    /// Write the water planes of levels 8 to 32 as the polygons of their outlines (corners only), which takes
    /// precedence over `water_shared_vertices`.
    pub water_polygons: bool,
    /// Asked with a block's key and its inputs digest before the block is built: the files an earlier run wrote
    /// for the same key and inputs, when they are still in the output folder. The block is then not built again.
    pub reuse: Option<&'a (dyn Fn(&str, &str) -> Option<Vec<FileRecord>> + Sync)>,
    /// Asked before each block is built: `true` stops the run at that point. The blocks finished so far are kept and
    /// reported, the settings file is not written, and [`Report::cancelled`] is set.
    pub cancel: Option<&'a (dyn Fn() -> bool + Sync)>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            threads: 1,
            land: None,
            progress: None,
            water: true,
            drop_under_water: None,
            water_walls: false,
            water_shared_vertices: false,
            water_polygons: false,
            reuse: None,
            cancel: None,
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

/// One terrain block of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRecord {
    /// `terrain:<world>:<level>:<x>:<y>`, the world in lower case.
    pub key: String,
    /// `sha256:` over everything the block's files depend on (see [`Options::reuse`]).
    pub inputs: String,
    /// The block's files.
    pub files: Vec<FileRecord>,
    /// The files were kept from an earlier run instead of being built.
    pub reused: bool,
}

/// What a run wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The world.
    pub world: String,
    /// Files, sorted by path.
    pub files: Vec<FileRecord>,
    /// Terrain blocks with files, sorted by key.
    pub blocks: Vec<BlockRecord>,
    /// [`Options::cancel`] stopped the run: `files` and `blocks` hold only the blocks that finished, and the
    /// settings file was not written.
    pub cancelled: bool,
}

pub(crate) use crate::output::write_output_file as write_file;

fn dds(image: &Image, format: Format) -> Result<Vec<u8>, GenError> {
    // source: game-lod-terrain-textures
    // BC3 is only for a normal map that carries specular in alpha, own design (docs/spec/textures.md section 4)
    Ok(encode(
        image,
        &EncodeOptions {
            format,
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

/// The digest of everything a block's files depend on: the generator version, the level settings and switches, the
/// block's height samples, and per cell its water height, its land layers and the pixels of the land textures those
/// layers name.
fn block_inputs(
    world: &Worldspace,
    samples: &[f32],
    job: &Job,
    tiles: &LandTiles,
    options: &Options<'_>,
) -> String {
    let mut h = Sha256Stream::new();
    h.update(env!("CARGO_PKG_VERSION").as_bytes());
    h.update(format!("{:?} water={}", job.level, options.water).as_bytes());
    if let Some(offset) = options.drop_under_water {
        h.update(format!("drop-under-water={offset}").as_bytes());
    }
    if options.water && options.water_walls {
        h.update(b"water-walls");
    }
    if options.water && options.water_shared_vertices {
        h.update(b"water-shared-vertices");
    }
    if options.water && options.water_polygons {
        h.update(b"water-polygons");
    }
    if job.level.specular_alpha {
        // the alpha is scaled over the exponent range of the load order
        h.update(format!("specular-range={:?}", tiles.specular_range()).as_bytes());
    }
    // the grid origin enters the digest only where it is not a multiple of the block size, so blocks of worlds that
    // start on the grid are kept
    let (ax, ay) = world.grid_anchor();
    let size = i32::try_from(job.level.level).unwrap_or(1).max(1);
    let offset = (ax.rem_euclid(size), ay.rem_euclid(size));
    if offset != (0, 0) {
        h.update(format!("grid-offset={offset:?}").as_bytes());
    }
    for s in samples {
        h.update(&s.to_le_bytes());
    }
    let cells = i32::try_from(job.level.level).unwrap_or(0);
    for x in job.cell.0..job.cell.0 + cells {
        for y in job.cell.1..job.cell.1 + cells {
            h.update(
                format!(
                    "{x},{y}:{:?}:{}",
                    world.water_height(x, y),
                    world.water_is_default(x, y)
                )
                .as_bytes(),
            );
            let Some(layers) = world.layers(x, y) else {
                continue;
            };
            h.update(format!("{layers:?}").as_bytes());
            let ids = layers
                .base
                .iter()
                .flatten()
                .copied()
                .chain(layers.alpha.iter().map(|l| l.texture));
            for id in ids {
                if let Some(tile) = tiles.get(id) {
                    h.update(&id.to_le_bytes());
                    for p in tile.pixels() {
                        h.update(p);
                    }
                    // only a scale other than 1 enters the digest, so blocks built before scales existed are kept
                    #[allow(clippy::float_cmp)] // 1 is exact
                    if tile.scale() != 1.0 {
                        h.update(&tile.scale().to_le_bytes());
                    }
                    // the exponents enter the digest only for a level that writes them, so no other block changes
                    if job.level.specular_alpha {
                        h.update(&[
                            u8::from(tile.specular().is_some()),
                            tile.specular().unwrap_or(0),
                        ]);
                    }
                }
            }
        }
    }
    h.finish()
}

fn run_job(
    world: &Worldspace,
    raster: &HeightRaster,
    job: &Job,
    out: &Path,
    tiles: &LandTiles,
    options: &Options,
) -> Result<Option<BlockRecord>, GenError> {
    let level = &job.level;
    let grid = raster.grid();
    let (bx, by) = (grid.block_of_x(job.cell.0), grid.block_of_y(job.cell.1));
    let samples = raster.block_samples(bx, by).unwrap_or_default();
    let name = world.name();
    let key = format!(
        "terrain:{}:{}:{}:{}",
        name.to_ascii_lowercase(),
        level.level,
        job.cell.0,
        job.cell.1
    );
    let inputs = block_inputs(world, &samples, job, tiles, options);
    if let Some(files) = options.reuse.and_then(|reuse| reuse(&key, &inputs)) {
        return Ok(Some(BlockRecord {
            key,
            inputs,
            files,
            reused: true,
        }));
    }
    // water height at a block-local position: the cell the position lies in, counted from the block's first cell
    let water_at = |x: f32, y: f32| {
        let cell = |v: f32| (v / CELL_UNITS).floor() as i32;
        world.water_height(job.cell.0 + cell(x), job.cell.1 + cell(y))
    };
    let drop = options.drop_under_water.map(|offset| DropUnderWater {
        water_at: &water_at,
        offset,
    });
    let Some((_, mesh, _)) = build_block_mesh_dropping(raster, level, bx, by, drop) else {
        return Ok(None);
    };
    let id = BlockId {
        level: level.level,
        x: job.cell.0,
        y: job.cell.1,
    };
    let units = level.level as f32 * CELL_UNITS;
    let [diffuse_ref, normal_ref] = layout::nif_texture_paths(name, id)?;
    let water = if options.water {
        water_mesh(
            world,
            &samples,
            level.level,
            job.cell,
            if options.water_polygons {
                Plane::Polygons
            } else if options.water_shared_vertices {
                Plane::Shared
            } else {
                Plane::Rectangles
            },
        )
        .map(|mut mesh| {
            if options.water_walls {
                // a plane that would need more than 65535 vertices with its walls keeps none
                let plain = mesh.clone();
                if add_walls(&mut mesh, world, &samples, level.level, job.cell).is_none() {
                    mesh = plain;
                }
            }
            mesh
        })
    } else {
        None
    };
    let nif = terrain_block(&TerrainBlock {
        origin: (
            job.cell.0 as f32 * CELL_UNITS,
            job.cell.1 as f32 * CELL_UNITS,
        ),
        size: units,
        vertices: &mesh.vertices,
        indices: &mesh.indices,
        textures: [&diffuse_ref, &normal_ref],
        water: water.as_ref().map(|w| WaterShape {
            vertices: &w.vertices,
            indices: &w.indices,
            segments: w.segments,
            placeholder: w.placeholder,
        }),
    })?
    .write()?;
    let span = (level.level as usize) * 32 + 1;
    let (alpha, normal_format) = if level.specular_alpha {
        (
            Some(specular_alpha(world, id, level.normal_size, tiles)),
            Format::Bc3,
        )
    } else {
        (None, Format::Bc1)
    };
    let normals = dds(
        &normal_image_with_alpha(&samples, span, units, level.normal_size, alpha.as_deref()),
        normal_format,
    )?;
    let diffuse = dds(
        &diffuse_image(world, id, level.diffuse_size, tiles),
        Format::Bc1,
    )?;
    Ok(Some(BlockRecord {
        key,
        inputs,
        files: vec![
            write_file(out, &layout::mesh_path(name, id)?, &nif)?,
            write_file(out, &layout::diffuse_path(name, id)?, &diffuse)?,
            write_file(out, &layout::normal_path(name, id)?, &normals)?,
        ],
        reused: false,
    }))
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
    let world = &world.without_outliers().filled_over_extent();
    let no_tiles = LandTiles::empty();
    let tiles = options.land.unwrap_or(&no_tiles);
    let mut files = Vec::new();
    let mut blocks = Vec::new();
    let extent = world.lod_extent();
    let mut highest = 0;
    let stop = || options.cancel.is_some_and(|cancel| cancel());
    let cancelled = AtomicBool::new(false);
    for level in &settings.levels {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        // the game writes the levels up to a quarter of the world's width and no higher (`game-lod-settings-extent`)
        if extent.is_some_and(|e| level.level > e.highest_level) {
            continue;
        }
        let grid = BlockGrid::anchored(level.level, world.grid_anchor())
            .map_err(|_| lodwright_core::Error::ZeroLevel)?;
        let Some(raster) = HeightRaster::build(world, grid)? else {
            continue;
        };
        highest = highest.max(level.level);
        let jobs = jobs_of(&raster, level);
        let next = AtomicUsize::new(0);
        let finished = AtomicUsize::new(0);
        let done: Mutex<Vec<Result<Option<BlockRecord>, GenError>>> = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..options.threads.max(1) {
                scope.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(i) else { break };
                        if stop() {
                            cancelled.store(true, Ordering::Relaxed);
                            next.store(jobs.len(), Ordering::Relaxed);
                            break;
                        }
                        let result = run_job(world, &raster, job, out, tiles, options);
                        if let Some(report) = options.progress {
                            let n = finished.fetch_add(1, Ordering::Relaxed) + 1;
                            let reused = matches!(&result, Ok(Some(b)) if b.reused);
                            report(
                                world.name(),
                                level.level,
                                n as u64,
                                jobs.len() as u64,
                                reused,
                            );
                        }
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
            if let Some(block) = result? {
                files.extend(block.files.iter().cloned());
                blocks.push(block);
            }
        }
    }
    let cancelled = cancelled.into_inner();
    if let Some(bounds) = world.bounds().filter(|_| !cancelled) {
        let clamp = |v: i32| i16::try_from(v).unwrap_or(if v < 0 { i16::MIN } else { i16::MAX });
        let (anchor_x, anchor_y) = world.grid_anchor();
        let settings_file = match extent {
            Some(e) => LodSettings {
                lowest_level: 4,
                highest_level: highest.min(e.highest_level),
                width_cells: e.width,
                south_west: (clamp(e.south_west.0), clamp(e.south_west.1)),
                north_east: (clamp(e.north_east.0), clamp(e.north_east.1)),
                object_level: 4,
            },
            None => LodSettings::for_bounds(
                (clamp(anchor_x), clamp(anchor_y)),
                (clamp(bounds.max_x), clamp(bounds.max_y)),
                highest,
            ),
        };
        files.push(write_file(
            out,
            &layout::settings_path(world.name())?,
            &settings_file.to_bytes(),
        )?);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    blocks.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(Report {
        world: world.name().to_owned(),
        files,
        blocks,
        cancelled,
    })
}

#[cfg(test)]
mod tests {
    use super::{Job, Options, Report, block_inputs, generate_terrain};
    use crate::diffuse::{LandTiles, Tile};
    use lodwright_core::land::CellHeights;
    use lodwright_core::layers::CellLayers;
    use lodwright_core::layout::BlockId;
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
    fn a_cell_with_ground_below_its_water_gets_a_water_plane_unless_turned_off() {
        let mut w = world();
        w.set_cell_water(0, 0, true, Some(150.0));
        for (water, blocks) in [(true, 10), (false, 8)] {
            let out = temp(&format!("water-{water}"));
            generate_terrain(
                &w,
                &TerrainSettings::default(),
                &out,
                &Options {
                    water,
                    ..Options::default()
                },
            )
            .unwrap();
            let nif =
                std::fs::read(out.join("meshes/landscape/lod/mojave/mojave.level4.x0.y0.nif"))
                    .unwrap();
            assert_eq!(NifFile::read(&nif).unwrap().blocks.len(), blocks);
            let v = crate::verify_folder(&out);
            assert!(v.errors.is_empty(), "{:?}", v.errors);
        }
    }

    #[test]
    fn faces_below_the_water_are_dropped_only_with_the_option_and_it_enters_the_digest() {
        let mut coast = world();
        // water over the low cell (0, 0) at 105: its ground at 100 lies below, the cells beside it at 110 above
        coast.set_cell_water(0, 0, true, Some(105.0));
        let run = |drop: Option<f32>, tag: &str| {
            let out = temp(tag);
            let report = generate_terrain(
                &coast,
                &TerrainSettings::default(),
                &out,
                &Options {
                    drop_under_water: drop,
                    water_walls: false,
                    water_shared_vertices: false,
                    water_polygons: false,
                    ..Options::default()
                },
            )
            .unwrap();
            let nif =
                std::fs::read(out.join("meshes/landscape/lod/mojave/mojave.level4.x0.y0.nif"))
                    .unwrap();
            let v = crate::verify_folder(&out);
            assert!(v.errors.is_empty(), "{:?}", v.errors);
            (nif, report)
        };
        let (kept, kept_report) = run(None, "drop-off");
        let (dropped, dropped_report) = run(Some(0.0), "drop-on");
        let (deep, _) = run(Some(50.0), "drop-deep");
        assert_ne!(kept, dropped, "the option changes the block");
        assert_eq!(kept, deep, "faces less than 50 below the water are kept");
        let key = |r: &Report| r.blocks[0].inputs.clone();
        assert_ne!(key(&kept_report), key(&dropped_report));
        let (again, _) = run(None, "drop-off-2");
        assert_eq!(kept, again);
    }

    #[test]
    fn block_inputs_change_only_where_the_world_changes_and_a_reused_block_is_not_built() {
        let run = |w: &Worldspace, name: &str, reuse: bool| {
            let keep = |_: &str, _: &str| Some(Vec::new());
            generate_terrain(
                w,
                &TerrainSettings::default(),
                &temp(name),
                &Options {
                    reuse: reuse.then_some(&keep as &(dyn Fn(&str, &str) -> _ + Sync)),
                    ..Options::default()
                },
            )
            .unwrap()
        };
        let a = run(&world(), "inputs-a", false);
        let mut changed = world();
        changed.set_cell_water(0, 0, true, Some(500.0));
        let b = run(&changed, "inputs-b", false);
        assert_eq!(a.blocks.len(), b.blocks.len());
        // the water of cell (0, 0) is inside every level's block x0.y0, and there is only that block per level
        assert!(
            a.blocks
                .iter()
                .zip(&b.blocks)
                .all(|(x, y)| x.key == y.key && x.inputs != y.inputs)
        );
        assert_eq!(run(&world(), "inputs-c", false).blocks, a.blocks);
        let kept = run(&world(), "inputs-d", true);
        assert!(kept.blocks.iter().all(|b| b.reused && b.files.is_empty()));
        assert!(!temp("inputs-d").join("meshes").exists());
    }

    #[test]
    fn a_cancel_after_the_first_block_keeps_that_block_and_writes_no_settings_file() {
        let out = temp("cancel");
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let cancel = || asked.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 1;
        let report = generate_terrain(
            &world(),
            &TerrainSettings::default(),
            &out,
            &Options {
                cancel: Some(&cancel),
                ..Options::default()
            },
        )
        .unwrap();
        assert!(report.cancelled);
        assert_eq!(report.blocks.len(), 1);
        assert!(
            report
                .files
                .iter()
                .all(|f| !f.path.ends_with(".dlodsettings"))
        );
        assert!(!out.join("lodsettings").exists());
    }

    #[test]
    fn a_run_without_a_cancel_is_not_cancelled() {
        let report = generate_terrain(
            &world(),
            &TerrainSettings::default(),
            &temp("not-cancelled"),
            &Options::default(),
        )
        .unwrap();
        assert!(!report.cancelled);
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
                progress: None,
                water: true,
                drop_under_water: None,
                water_walls: false,
                water_shared_vertices: false,
                water_polygons: false,
                reuse: None,
                cancel: None,
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
                progress: None,
                water: true,
                drop_under_water: None,
                water_walls: false,
                water_shared_vertices: false,
                water_polygons: false,
                reuse: None,
                cancel: None,
            },
        )
        .unwrap();
        assert_eq!(a, b);
    }

    fn bounded(min: [f32; 2]) -> Worldspace {
        world().with_declared_bounds(lodwright_core::plugin::DeclaredBounds {
            min,
            max: [min[0] + 40960.0, min[1] + 40960.0],
        })
    }

    fn level4_names(world: &Worldspace, tag: &str) -> Vec<String> {
        let out = temp(tag);
        let report = generate_terrain(
            world,
            &TerrainSettings::default(),
            &out,
            &Options::default(),
        )
        .unwrap();
        let mut names: Vec<String> = report
            .files
            .iter()
            .filter(|f| f.path.contains(".level4.") && f.path.contains(".nif"))
            .map(|f| f.path.clone())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn blocks_are_named_from_the_minimum_corner_of_the_world() {
        // corner cell (-6, -5): the cells 0..3 x 0..2 fall into level 4 blocks starting at -6 + 4k and -5 + 4k
        let names = level4_names(&bounded([-6.0 * 4096.0, -5.0 * 4096.0]), "anchor");
        let tails: Vec<&str> = names
            .iter()
            .map(|n| n.rsplit('/').next().unwrap())
            .collect();
        assert_eq!(
            tails,
            ["mojave.level4.x-2.y-1.nif", "mojave.level4.x2.y-1.nif"]
        );
    }

    #[test]
    fn a_small_world_gets_only_the_levels_its_width_allows_and_the_settings_say_so() {
        // corners (-6, -5) to (4, 5): width 16, highest level 4
        let out = temp("extent");
        let report = generate_terrain(
            &bounded([-6.0 * 4096.0, -5.0 * 4096.0]),
            &TerrainSettings::default(),
            &out,
            &Options::default(),
        )
        .unwrap();
        assert!(report.files.iter().all(|f| !f.path.contains(".level8.")));
        assert!(report.files.iter().any(|f| f.path.contains(".level4.")));
        let bytes = std::fs::read(out.join("lodsettings/mojave.dlodsettings")).unwrap();
        let s = lodwright_core::lod::LodSettings::parse(&bytes).unwrap();
        assert_eq!(
            (s.highest_level, s.width_cells, s.south_west, s.north_east),
            (4, 16, (-6, -5), (4, 5))
        );
    }

    #[test]
    fn a_folder_on_an_anchored_grid_verifies_without_errors() {
        let out = temp("anchor-verify");
        generate_terrain(
            &bounded([-6.0 * 4096.0, -5.0 * 4096.0]),
            &TerrainSettings::default(),
            &out,
            &Options::default(),
        )
        .unwrap();
        let verified = crate::verify::verify_folder(&out);
        assert!(verified.errors.is_empty(), "{:?}", verified.errors);
        assert!(verified.terrain_blocks > 0);
    }

    #[test]
    fn a_corner_on_the_grid_or_no_corner_gives_the_old_names_and_digests() {
        let plain = level4_names(&world(), "plain");
        assert_eq!(level4_names(&bounded([0.0, 0.0]), "zero"), plain);
        assert_eq!(
            level4_names(&bounded([-8.0 * 4096.0, -4.0 * 4096.0]), "mult"),
            plain
        );
    }

    #[test]
    fn the_grid_start_changes_the_digest_only_off_the_grid() {
        let samples = vec![0.0_f32; 4];
        let job = |cell| Job {
            level: TerrainSettings::default().levels[0],
            cell,
        };
        let tiles = LandTiles::empty();
        let digest =
            |w: &Worldspace| block_inputs(w, &samples, &job((0, 0)), &tiles, &Options::default());
        assert_eq!(digest(&world()), digest(&bounded([-8.0 * 4096.0, 0.0])));
        assert_ne!(digest(&world()), digest(&bounded([-6.0 * 4096.0, 0.0])));
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

    #[test]
    fn a_tiling_scale_other_than_one_changes_the_digest_of_the_blocks_that_use_the_texture() {
        let mut w = world();
        w.set_cell_layers(
            0,
            0,
            CellLayers {
                normals: None,
                colors: None,
                base: [Some(7); 4],
                alpha: Vec::new(),
            },
        );
        let job = Job {
            level: TerrainSettings::default().levels[0],
            cell: (0, 0),
        };
        let tiles = |scales: &[(u32, f32)]| {
            LandTiles::from_tiles(std::collections::HashMap::from([(
                7,
                Tile::flat([90, 80, 70]),
            )]))
            .with_scales(&scales.iter().copied().collect())
        };
        let plain = block_inputs(&w, &[], &job, &tiles(&[]), &Options::default());
        assert_eq!(
            block_inputs(&w, &[], &job, &tiles(&[(7, 1.0)]), &Options::default()),
            plain
        );
        assert_ne!(
            block_inputs(&w, &[], &job, &tiles(&[(7, 2.0)]), &Options::default()),
            plain
        );
        // a scale for a texture the block does not use leaves its digest alone
        assert_eq!(
            block_inputs(&w, &[], &job, &tiles(&[(8, 2.0)]), &Options::default()),
            plain
        );
    }

    fn specular_world() -> Worldspace {
        let mut w = world();
        for cell in (0..4).flat_map(|x| (0..4).map(move |y| (x, y))) {
            w.set_cell_layers(
                cell.0,
                cell.1,
                CellLayers {
                    normals: None,
                    colors: None,
                    base: [Some(7); 4],
                    alpha: Vec::new(),
                },
            );
        }
        w
    }

    fn specular_tiles(exponent: Option<u8>, range: Option<(u8, u8)>) -> LandTiles {
        LandTiles::from_tiles(std::collections::HashMap::from([(
            7,
            Tile::flat([90, 80, 70]).with_specular(exponent),
        )]))
        .with_specular_range(range)
    }

    fn specular_level(on: bool) -> TerrainSettings {
        let mut settings = TerrainSettings::default();
        settings.levels.retain(|l| l.level == 4);
        settings.levels[0].specular_alpha = on;
        settings
    }

    fn run_specular(
        settings: &TerrainSettings,
        tiles: &LandTiles,
        name: &str,
    ) -> (std::path::PathBuf, Report) {
        let out = temp(name);
        let report = generate_terrain(
            &specular_world(),
            settings,
            &out,
            &Options {
                land: Some(tiles),
                ..Options::default()
            },
        )
        .unwrap();
        (out, report)
    }

    fn normal_files(out: &std::path::Path, report: &Report) -> Vec<Vec<u8>> {
        report
            .files
            .iter()
            .filter(|f| f.path.contains("/normals/"))
            .map(|f| std::fs::read(out.join(&f.path)).unwrap())
            .collect()
    }

    #[test]
    fn specular_off_writes_bc1_normals_and_the_same_bytes_whatever_the_exponents() {
        let tiles_a = specular_tiles(Some(10), Some((10, 200)));
        let tiles_b = specular_tiles(None, None);
        let (out_a, a) = run_specular(&specular_level(false), &tiles_a, "spec-off-a");
        let (_, b) = run_specular(&specular_level(false), &tiles_b, "spec-off-b");
        assert_eq!(a.files, b.files, "off: exponents change nothing");
        for n in normal_files(&out_a, &a) {
            assert_eq!(&n[84..88], b"DXT1");
        }
    }

    #[test]
    fn specular_on_writes_bc3_normals_and_leaves_the_diffuse_alone() {
        let tiles = specular_tiles(Some(100), Some((10, 200)));
        let (out_off, off) = run_specular(&specular_level(false), &tiles, "spec-cmp-off");
        let (out_on, on) = run_specular(&specular_level(true), &tiles, "spec-cmp-on");
        for n in normal_files(&out_on, &on) {
            assert_eq!(&n[84..88], b"DXT5");
        }
        let diffuse = |r: &Report| -> Vec<String> {
            r.files
                .iter()
                .filter(|f| f.path.contains("/diffuse/") || f.path.contains(".nif"))
                .map(|f| f.digest.clone())
                .collect()
        };
        assert_eq!(diffuse(&off), diffuse(&on));
        assert_ne!(normal_files(&out_off, &off), normal_files(&out_on, &on));
    }

    #[test]
    fn a_missing_exponent_everywhere_gives_opaque_alpha_and_never_a_panic() {
        let tiles = specular_tiles(None, None);
        let w = specular_world();
        let id = BlockId {
            level: 4,
            x: 0,
            y: 0,
        };
        assert!(
            crate::images::specular_alpha(&w, id, 4, &tiles)
                .iter()
                .all(|a| *a == 255)
        );
        let (out, report) = run_specular(&specular_level(true), &tiles, "spec-missing");
        assert_ne!(normal_files(&out, &report).len(), 0);
    }

    #[test]
    fn a_flat_source_gives_a_uniform_alpha_scaled_over_the_range() {
        let w = specular_world();
        let id = BlockId {
            level: 4,
            x: 0,
            y: 0,
        };
        let uniform = |tiles: &LandTiles| {
            let a = crate::images::specular_alpha(&w, id, 8, tiles);
            assert!(a.windows(2).all(|p| p[0] == p[1]));
            a[0]
        };
        assert_eq!(uniform(&specular_tiles(Some(200), Some((10, 200)))), 255);
        assert_eq!(uniform(&specular_tiles(Some(10), Some((10, 200)))), 0);
        assert_eq!(uniform(&specular_tiles(Some(105), Some((10, 200)))), 128);
        // one exponent in the whole load order: no range to scale over
        assert_eq!(uniform(&specular_tiles(Some(50), Some((50, 50)))), 255);
    }

    #[test]
    fn the_exponents_change_the_digest_only_of_levels_that_write_them() {
        let w = specular_world();
        let level = |on| Job {
            level: specular_level(on).levels[0],
            cell: (0, 0),
        };
        let digest =
            |on, tiles: &LandTiles| block_inputs(&w, &[], &level(on), tiles, &Options::default());
        let a = specular_tiles(Some(10), Some((10, 200)));
        let b = specular_tiles(Some(20), Some((10, 200)));
        assert_eq!(digest(false, &a), digest(false, &b));
        assert_ne!(digest(true, &a), digest(true, &b));
        assert_ne!(digest(true, &a), digest(false, &a));
    }
}
