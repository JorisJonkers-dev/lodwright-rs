// SPDX-License-Identifier: MPL-2.0
//! Terrain LOD planning: which blocks exist at each level, how coarse each block's mesh can be, and what files
//! and texture sizes the block needs.
//!
//! Meshes are regular grids decimated by a uniform step per block: the coarsest step whose largest vertical
//! error against the height raster stays inside the level's tolerance and whose vertex count stays under the
//! cap (16-bit indices). Triangles exist only over quads in which every sample has data. This is our own measure of
//! quality, not a copy of another tool's: the `quality` number is converted to a tolerance in world units
//! ([`UNITS_PER_QUALITY`]), so quality numbers of other tools do not map onto it.
//!
//! No file is written here: the block NIF writer lives in `lodwright-nif` and the generator in the CLI.
//!
//! Derived from: `provenance/sources.toml` rows `cell-size-units` and the layout rows used by [`crate::layout`].

use crate::Error;
use crate::layout::{self, BlockId};
use crate::raster::{BlockGrid, CELL_INTERVALS, HeightRaster};
use crate::worldspace::Worldspace;

/// The LOD levels of terrain, in cells per block edge.
pub const LEVELS: [u32; 4] = [4, 8, 16, 32];
/// Most vertices in one mesh: indices are 16-bit.
pub const MAX_VERTICES: usize = 32767;
/// World units of allowed vertical error per point of quality.
pub const UNITS_PER_QUALITY: f32 = 2.0;
// source: cell-size-units
/// World units between two neighbouring LAND vertices.
pub const VERTEX_SPACING: f32 = 128.0;

/// Settings of one level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelSettings {
    /// Cells per block edge.
    pub level: u32,
    /// Quality: allowed vertical error is `quality * UNITS_PER_QUALITY` world units.
    pub quality: f32,
    /// Vertex cap per mesh (at most [`MAX_VERTICES`]).
    pub max_vertices: usize,
    /// Edge length in pixels of the diffuse texture of a block (a power of two).
    pub diffuse_size: u32,
    /// Edge length in pixels of the normal map of a block (a power of two).
    pub normal_size: u32,
}

impl LevelSettings {
    /// The allowed vertical error in world units.
    #[must_use]
    pub fn tolerance(&self) -> f32 {
        self.quality * UNITS_PER_QUALITY
    }
}

/// Settings of all levels to plan.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainSettings {
    /// One entry per level.
    pub levels: Vec<LevelSettings>,
}

impl Default for TerrainSettings {
    /// Levels 4, 8, 16 and 32 with qualities 10, 15, 20 and 25, the vertex cap, 512 and 256 pixel textures.
    fn default() -> Self {
        let level = |level, quality, size| LevelSettings {
            level,
            quality,
            max_vertices: MAX_VERTICES,
            diffuse_size: size,
            normal_size: size,
        };
        Self {
            levels: vec![
                level(4, 10.0, 512),
                level(8, 15.0, 256),
                level(16, 20.0, 256),
                level(32, 25.0, 256),
            ],
        }
    }
}

/// A triangle mesh in block-local world units: x and y from the block's south-west corner, z the height.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainMesh {
    /// Vertex positions.
    pub vertices: Vec<[f32; 3]>,
    /// Triangle corner indices, three per triangle, counter-clockwise seen from above.
    pub indices: Vec<u16>,
}

impl TerrainMesh {
    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// What the planner decided for one block.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockPlan {
    /// The block.
    pub id: BlockId,
    /// Raster samples between neighbouring mesh vertices (1 keeps every LAND vertex).
    pub step: usize,
    /// Vertices of the mesh.
    pub vertices: usize,
    /// Triangles of the mesh.
    pub triangles: usize,
    /// Largest vertical difference between the mesh and the raster, in world units.
    pub max_error: f32,
    /// Lowest and highest vertex height.
    pub height_range: (f32, f32),
    /// Where the mesh goes.
    pub mesh_path: String,
    /// Where the diffuse texture goes.
    pub diffuse_path: String,
    /// Where the normal map goes.
    pub normal_path: String,
    /// Diffuse texture size in pixels.
    pub diffuse_size: u32,
    /// Normal map size in pixels.
    pub normal_size: u32,
    /// Mip levels of both textures (down to 1 x 1).
    pub mip_levels: u32,
}

/// The plan of one world.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainPlan {
    /// World name (editor id).
    pub world: String,
    /// Blocks, by level then x then y.
    pub blocks: Vec<BlockPlan>,
    /// Where the settings file goes.
    pub settings_path: String,
}

/// Mip levels of a square texture of `size` pixels, down to 1 x 1 (0 for size 0).
#[must_use]
pub fn mip_levels(size: u32) -> u32 {
    if size == 0 { 0 } else { size.ilog2() + 1 }
}

fn to_f32(n: usize) -> f32 {
    f32::from(u16::try_from(n).unwrap_or(u16::MAX))
}

/// Height of the decimated surface at fine sample (`x`, `y`) given the coarse corner heights of its quad:
/// the quad is split along its south-west to north-east diagonal, as in [`decimate`].
fn surface(corners: [f32; 4], u: f32, v: f32) -> f32 {
    let [sw, se, ne, nw] = corners;
    if u >= v {
        sw + u * (se - sw) + v * (ne - se)
    } else {
        sw + v * (nw - sw) + u * (ne - nw)
    }
}

/// Decimate a square block of `span * span` samples (`NaN` is no data) with a uniform `step` (which divides
/// `span - 1`). Returns the mesh, the largest vertical error over the samples that lie in a quad in which
/// every sample has data (a coarse quad never bridges a hole), and the number of samples with data that no quad covers (a coarse step can lose the edge
/// of a partly covered block).
fn decimate(samples: &[f32], span: usize, step: usize) -> (TerrainMesh, f32, usize) {
    let quads = (span - 1) / step;
    let corner = |qx: usize, qy: usize| samples[qy * step * span + qx * step];
    let mut remap = vec![u32::MAX; (quads + 1) * (quads + 1)];
    let mut mesh = TerrainMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    let mut worst = 0.0_f32;
    let mut covered = vec![false; span * span];
    let mut index_of = |qx: usize, qy: usize, mesh: &mut TerrainMesh| -> u16 {
        let slot = &mut remap[qy * (quads + 1) + qx];
        if *slot == u32::MAX {
            *slot = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
            mesh.vertices.push([
                to_f32(qx * step) * VERTEX_SPACING,
                to_f32(qy * step) * VERTEX_SPACING,
                corner(qx, qy),
            ]);
        }
        u16::try_from(*slot).unwrap_or(u16::MAX)
    };
    for qy in 0..quads {
        for qx in 0..quads {
            let corners = [
                corner(qx, qy),
                corner(qx + 1, qy),
                corner(qx + 1, qy + 1),
                corner(qx, qy + 1),
            ];
            let full = (0..=step).all(|dy| {
                (0..=step).all(|dx| !samples[(qy * step + dy) * span + qx * step + dx].is_nan())
            });
            if !full {
                continue;
            }
            let sw = index_of(qx, qy, &mut mesh);
            let se = index_of(qx + 1, qy, &mut mesh);
            let ne = index_of(qx + 1, qy + 1, &mut mesh);
            let nw = index_of(qx, qy + 1, &mut mesh);
            mesh.indices.extend_from_slice(&[sw, se, ne, sw, ne, nw]);
            for dy in 0..=step {
                for dx in 0..=step {
                    let at = (qy * step + dy) * span + qx * step + dx;
                    covered[at] = true;
                    let u = to_f32(dx) / to_f32(step);
                    let v = to_f32(dy) / to_f32(step);
                    worst = worst.max((samples[at] - surface(corners, u, v)).abs());
                }
            }
        }
    }
    let uncovered = samples
        .iter()
        .zip(&covered)
        .filter(|(h, c)| !h.is_nan() && !**c)
        .count();
    (mesh, worst, uncovered)
}

/// Pick the step for a block and build its mesh.
///
/// The step is the largest power of two (dividing the block's intervals) whose error is within `tolerance`,
/// whose vertex count is within `max_vertices` and which still covers every sample that has data; if even step 1
/// breaks the tolerance, the finest step that meets the vertex cap is used (the error is then reported, not
/// hidden). Returns `None` when no quad has complete data.
fn best_mesh(
    samples: &[f32],
    span: usize,
    tolerance: f32,
    max_vertices: usize,
) -> Option<(usize, TerrainMesh, f32)> {
    let intervals = span - 1;
    let mut steps = Vec::new();
    let mut step = 1;
    while step <= intervals {
        if intervals.is_multiple_of(step) {
            steps.push(step);
        }
        step *= 2;
    }
    let mut fallback = None;
    let mut chosen = None;
    for step in steps {
        let (mesh, error, uncovered) = decimate(samples, span, step);
        if mesh.indices.is_empty() {
            if step == 1 {
                return None;
            }
            continue;
        }
        if mesh.vertices.len() > max_vertices || uncovered > 0 {
            continue;
        }
        if fallback.is_none() {
            fallback = Some((step, mesh.clone(), error));
        }
        if error <= tolerance {
            chosen = Some((step, mesh, error));
        }
    }
    chosen.or(fallback)
}

/// Build the mesh of one block of `raster` (whose grid defines the level), with the tolerance and vertex cap of
/// `settings`. `None` when the block is outside the raster or has no complete quad.
#[must_use]
pub fn build_block_mesh(
    raster: &HeightRaster,
    settings: &LevelSettings,
    block_x: i32,
    block_y: i32,
) -> Option<(usize, TerrainMesh, f32)> {
    let samples = raster.block_samples(block_x, block_y)?;
    let span = usize::try_from(settings.level).ok()? * CELL_INTERVALS + 1;
    best_mesh(
        &samples,
        span,
        settings.tolerance(),
        settings.max_vertices.min(MAX_VERTICES),
    )
}

/// Plan every level of `settings` for `world`.
///
/// # Errors
/// [`Error::BadWorldName`] for a name that cannot be a folder, [`Error::ZeroLevel`] for a level of zero, and
/// [`Error::RasterTooLarge`] when a level's raster would be too big.
pub fn plan_world(world: &Worldspace, settings: &TerrainSettings) -> Result<TerrainPlan, Error> {
    layout::check_world_name(world.name())?;
    let mut blocks = Vec::new();
    for level in &settings.levels {
        let grid = BlockGrid::new(level.level).map_err(|_| Error::ZeroLevel)?;
        let Some(raster) = HeightRaster::build(world, grid)? else {
            continue;
        };
        let (cells_w, cells_h) = raster.cells();
        let (origin_x, origin_y) = raster.origin_cell();
        let step = i32::try_from(level.level).unwrap_or(i32::MAX);
        for by in 0..cells_h / level.level {
            for bx in 0..cells_w / level.level {
                let cell_x = origin_x + i32::try_from(bx).unwrap_or(0) * step;
                let cell_y = origin_y + i32::try_from(by).unwrap_or(0) * step;
                let (block_x, block_y) = (grid.block_of(cell_x), grid.block_of(cell_y));
                let Some((decimation, mesh, error)) =
                    build_block_mesh(&raster, level, block_x, block_y)
                else {
                    continue;
                };
                let id = BlockId {
                    level: level.level,
                    x: cell_x,
                    y: cell_y,
                };
                let heights = mesh.vertices.iter().map(|v| v[2]);
                let low = heights.clone().fold(f32::INFINITY, f32::min);
                let high = heights.fold(f32::NEG_INFINITY, f32::max);
                blocks.push(BlockPlan {
                    id,
                    step: decimation,
                    vertices: mesh.vertices.len(),
                    triangles: mesh.triangle_count(),
                    max_error: error,
                    height_range: (low, high),
                    mesh_path: layout::mesh_path(world.name(), id)?,
                    diffuse_path: layout::diffuse_path(world.name(), id)?,
                    normal_path: layout::normal_path(world.name(), id)?,
                    diffuse_size: level.diffuse_size,
                    normal_size: level.normal_size,
                    mip_levels: mip_levels(level.diffuse_size.max(level.normal_size)),
                });
            }
        }
    }
    blocks.sort_by_key(|b| b.id);
    Ok(TerrainPlan {
        world: world.name().to_owned(),
        blocks,
        settings_path: layout::settings_path(world.name())?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        LEVELS, LevelSettings, MAX_VERTICES, TerrainSettings, VERTEX_SPACING, best_mesh,
        build_block_mesh, decimate, mip_levels, plan_world,
    };
    use crate::Error;
    use crate::land::CellHeights;
    use crate::raster::{BlockGrid, HeightRaster};
    use crate::worldspace::Worldspace;
    use lodwright_testkit::rng::Rng;

    fn flat_world(name: &str, cells: &[(i32, i32)]) -> Worldspace {
        Worldspace::from_cells(1, name, cells.iter().map(|c| (*c, CellHeights::flat(80.0))))
    }

    fn level(level: u32, quality: f32) -> LevelSettings {
        LevelSettings {
            level,
            quality,
            max_vertices: MAX_VERTICES,
            diffuse_size: 256,
            normal_size: 256,
        }
    }

    #[test]
    fn mip_chains_run_down_to_one_pixel() {
        assert_eq!(mip_levels(512), 10);
        assert_eq!(mip_levels(256), 9);
        assert_eq!(mip_levels(1), 1);
        assert_eq!(mip_levels(0), 0);
    }

    #[test]
    fn the_defaults_cover_the_four_levels_in_order() {
        let levels: Vec<u32> = TerrainSettings::default()
            .levels
            .iter()
            .map(|l| l.level)
            .collect();
        assert_eq!(levels, LEVELS);
        assert!((level(4, 10.0).tolerance() - 20.0).abs() < f32::EPSILON);
    }

    #[test]
    fn flat_ground_collapses_to_one_quad() {
        let samples = vec![80.0; 129 * 129];
        let (step, mesh, error) = best_mesh(&samples, 129, 1.0, MAX_VERTICES).unwrap();
        assert_eq!(step, 128);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.triangle_count(), 2);
        assert!(error.abs() < f32::EPSILON);
    }

    #[test]
    fn a_plane_has_no_error_at_any_step() {
        let span = 65;
        let samples: Vec<f32> = (0..span * span)
            .map(|i| {
                3.0 * f32::from(u8::try_from(i % span).unwrap())
                    + 5.0 * f32::from(u8::try_from(i / span).unwrap())
            })
            .collect();
        for step in [1, 2, 4, 8, 16, 32, 64] {
            let (_, error, uncovered) = decimate(&samples, span, step);
            assert_eq!(uncovered, 0);
            assert!(error < 1e-3, "step {step}: {error}");
        }
    }

    #[test]
    fn the_error_never_exceeds_the_tolerance_unless_the_cap_forces_it() {
        let mut rng = Rng::new(5);
        for _ in 0..30 {
            let span = 65;
            let samples: Vec<f32> = (0..span * span)
                .map(|_| f32::from(u8::try_from(rng.below(40)).unwrap()))
                .collect();
            let tolerance = f32::from(u8::try_from(rng.below(30)).unwrap());
            let (step, _, error) = best_mesh(&samples, span, tolerance, MAX_VERTICES).unwrap();
            assert!(
                error <= tolerance || step == 1,
                "{error} > {tolerance} at {step}"
            );
        }
    }

    #[test]
    fn a_lower_tolerance_never_yields_fewer_vertices() {
        let span = 65;
        let samples: Vec<f32> = (0..span * span)
            .map(|i| {
                let x = f32::from(u8::try_from(i % span).unwrap());
                let y = f32::from(u8::try_from(i / span).unwrap());
                20.0 * (x / 6.0).sin() + 15.0 * (y / 9.0).cos()
            })
            .collect();
        let mut last = 0;
        for tolerance in [100.0_f32, 30.0, 10.0, 3.0, 1.0, 0.1, 0.0] {
            let (_, mesh, _) = best_mesh(&samples, span, tolerance, MAX_VERTICES).unwrap();
            assert!(mesh.vertices.len() >= last, "{tolerance}");
            last = mesh.vertices.len();
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn every_vertex_height_equals_a_source_sample_and_indices_stay_in_range() {
        let span = 65;
        let samples: Vec<f32> = (0..span * span)
            .map(|i| f32::from(u8::try_from((i * 7) % 61).unwrap()))
            .collect();
        let (mesh, _, _) = decimate(&samples, span, 4);
        for v in &mesh.vertices {
            let x = (v[0] / VERTEX_SPACING).round();
            let y = (v[1] / VERTEX_SPACING).round();
            let at = usize::try_from(y as u32).unwrap() * span + usize::try_from(x as u32).unwrap();
            assert!((samples[at] - v[2]).abs() < f32::EPSILON);
        }
        assert!(
            mesh.indices
                .iter()
                .all(|i| usize::from(*i) < mesh.vertices.len())
        );
        assert_eq!(mesh.indices.len() % 3, 0);
    }

    #[test]
    fn the_vertex_cap_forces_a_coarser_step() {
        let span = 129;
        let samples: Vec<f32> = (0..span * span)
            .map(|i| f32::from(u8::try_from(i % 31).unwrap()))
            .collect();
        let (step, mesh, _) = best_mesh(&samples, span, 0.0, 1000).unwrap();
        assert!(mesh.vertices.len() <= 1000);
        assert!(step >= 8, "{step}");
    }

    #[test]
    fn missing_data_leaves_no_triangles_there() {
        let span = 33;
        let mut samples = vec![8.0; span * span];
        for x in 0..span {
            samples[16 * span + x] = f32::NAN;
        }
        let (mesh, _, uncovered) = decimate(&samples, span, 16);
        // a 2 x 2 quad grid where the middle row is missing: every quad touches the hole
        assert_eq!(mesh.indices.len(), 0);
        assert!(uncovered > 0);
        // the valid rows 0..=15 only tile at step 1, so that is the step chosen despite the loose tolerance
        assert_eq!(
            best_mesh(&samples, span, 1000.0, MAX_VERTICES).unwrap().0,
            1
        );
        let none = vec![f32::NAN; span * span];
        assert!(best_mesh(&none, span, 1.0, MAX_VERTICES).is_none());
    }

    #[test]
    fn a_partly_covered_block_keeps_every_sample_with_data() {
        let span = 129;
        let mut samples = vec![f32::NAN; span * span];
        for y in 0..33 {
            for x in 0..33 {
                samples[y * span + x] = 8.0;
            }
        }
        let (step, mesh, error) = best_mesh(&samples, span, 100.0, MAX_VERTICES).unwrap();
        // flat, so error is zero, but steps beyond one cell would drop the covered corner
        assert!(step <= 32, "{step}");
        assert!(error.abs() < f32::EPSILON);
        assert_eq!(mesh.vertices.len(), 4);
    }

    #[test]
    fn a_block_outside_the_raster_has_no_mesh() {
        let world = flat_world("W", &[(0, 0)]);
        let raster = HeightRaster::build(&world, BlockGrid::new(4).unwrap())
            .unwrap()
            .unwrap();
        assert!(build_block_mesh(&raster, &level(4, 10.0), 0, 0).is_some());
        assert!(build_block_mesh(&raster, &level(4, 10.0), 5, 5).is_none());
    }

    #[test]
    fn a_plan_lists_each_block_with_data_with_paths_and_texture_sizes() {
        let world = flat_world("Mojave", &[(0, 0), (1, 1), (5, 0)]);
        let plan = plan_world(&world, &TerrainSettings::default()).unwrap();
        assert_eq!(plan.settings_path, "lodsettings/mojave.dlodsettings");
        // level 4: blocks (0,0) and (4,0); levels 8, 16, 32: one block each
        let per_level: Vec<usize> = LEVELS
            .iter()
            .map(|l| plan.blocks.iter().filter(|b| b.id.level == *l).count())
            .collect();
        assert_eq!(per_level, [2, 1, 1, 1]);
        let first = &plan.blocks[0];
        assert_eq!((first.id.level, first.id.x, first.id.y), (4, 0, 0));
        assert_eq!(
            first.mesh_path,
            "meshes/landscape/lod/mojave/mojave.level4.x0.y0.nif"
        );
        assert_eq!(
            (first.diffuse_size, first.normal_size, first.mip_levels),
            (512, 512, 10)
        );
        assert!(first.triangles >= 2);
        assert_eq!(first.height_range, (80.0, 80.0));
    }

    #[test]
    fn plan_blocks_are_sorted_and_negative_blocks_use_multiples_of_the_level() {
        let world = flat_world("W", &[(-1, -1), (0, 0)]);
        let plan = plan_world(&world, &TerrainSettings::default()).unwrap();
        let level4: Vec<(i32, i32)> = plan
            .blocks
            .iter()
            .filter(|b| b.id.level == 4)
            .map(|b| (b.id.x, b.id.y))
            .collect();
        assert_eq!(level4, [(-4, -4), (0, 0)]);
        let ids: Vec<_> = plan.blocks.iter().map(|b| b.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn an_empty_world_has_an_empty_plan() {
        let plan = plan_world(&flat_world("W", &[]), &TerrainSettings::default()).unwrap();
        assert_eq!(plan.blocks.len(), 0);
    }

    #[test]
    fn a_bad_name_or_a_zero_level_is_an_error() {
        assert_eq!(
            plan_world(&flat_world("../x", &[(0, 0)]), &TerrainSettings::default()),
            Err(Error::BadWorldName("../x".into()))
        );
        let zero = TerrainSettings {
            levels: vec![level(0, 10.0)],
        };
        assert_eq!(
            plan_world(&flat_world("W", &[(0, 0)]), &zero),
            Err(Error::ZeroLevel)
        );
    }
}
