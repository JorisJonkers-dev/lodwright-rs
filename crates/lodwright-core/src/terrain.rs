// SPDX-License-Identifier: GPL-3.0-only
//! Terrain LOD planning: which blocks exist at each level, how coarse each block's mesh can be, and what files
//! and texture sizes the block needs.
//!
//! Meshes are adaptive ([`crate::rtin`]): triangles are split where the largest vertical error against the height
//! raster exceeds the level's tolerance, the vertex count stays under the cap (16-bit indices) by raising the
//! tolerance when needed, and triangles exist only over ground with data. This is our own measure of quality, not a copy of another tool's: the `quality` number is converted to a tolerance in world units
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
/// Vertices per block mesh the defaults aim for (the game's own blocks have a few hundred to a thousand).
pub const DEFAULT_BLOCK_VERTICES: usize = 2000;
/// World units of allowed vertical error per point of quality.
pub const UNITS_PER_QUALITY: f32 = 2.0;
// source: cell-size-units
/// World units between two neighbouring LAND vertices.
pub const VERTEX_SPACING: f32 = 128.0;

/// Settings of one level.
#[derive(Clone, Copy, PartialEq)]
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
    /// Carry specular in the alpha channel of the normal map (written as BC3). Off: opaque alpha, BC1.
    pub specular_alpha: bool,
    /// Close the gap to a neighbouring block with a skirt: under every border vertex a second vertex at the height
    /// the game's own blocks use, joined by two triangles. Off: no skirt.
    pub skirts: bool,
}

impl std::fmt::Debug for LevelSettings {
    /// `specular_alpha` is listed only when set, so the text (which block digests hash) of a level that does not
    /// use it is the one earlier versions wrote.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("LevelSettings");
        s.field("level", &self.level)
            .field("quality", &self.quality)
            .field("max_vertices", &self.max_vertices)
            .field("diffuse_size", &self.diffuse_size)
            .field("normal_size", &self.normal_size);
        if self.specular_alpha {
            s.field("specular_alpha", &true);
        }
        if self.skirts {
            s.field("skirts", &true);
        }
        s.finish()
    }
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
    /// Levels 4, 8, 16 and 32 with qualities 10, 15, 20 and 25, 2000 vertices per block and 256 pixel textures (the sizes measured in the game's own files).
    fn default() -> Self {
        let level = |level, quality, size| LevelSettings {
            level,
            quality,
            max_vertices: DEFAULT_BLOCK_VERTICES,
            diffuse_size: size,
            normal_size: size,
            specular_alpha: false,
            skirts: false,
        };
        Self {
            levels: vec![
                level(4, 10.0, 256),
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
    /// The shortest triangle leg used, in raster samples (1 means full resolution somewhere).
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
    let cap = settings.max_vertices.min(MAX_VERTICES);
    let mut meshed = crate::rtin::mesh_block(&samples, span, settings.tolerance(), cap)?;
    if settings.skirts {
        // The skirt vertices count against the cap: mesh the surface with the room that is left.
        let edge = (span - 1) as f32 * VERTEX_SPACING;
        let mut room = cap;
        for _ in 0..4 {
            let skirted = add_skirts(&meshed.mesh, edge);
            let total = skirted.vertices.len();
            if total <= cap {
                meshed.mesh = skirted;
                break;
            }
            room = room.saturating_sub(total - cap);
            let Some(next) = crate::rtin::mesh_block(&samples, span, settings.tolerance(), room)
            else {
                break;
            };
            meshed = next;
        }
    }
    Some((meshed.finest_step, meshed.mesh, meshed.max_error))
}

/// Height of the lower vertex of a skirt, the same for every border vertex of every block.
// source: game-lod-block-borders
pub const SKIRT_HEIGHT: f32 = -14098.0;

/// Add a vertical skirt along the block's border: under every border vertex that ends a border edge of a triangle a
/// second vertex at [`SKIRT_HEIGHT`], and two triangles per border edge facing away from the block. `edge` is the
/// block's edge length; a vertex is on the border when its x or y is 0 or `edge`.
// source: game-lod-block-borders
fn add_skirts(mesh: &TerrainMesh, edge: f32) -> TerrainMesh {
    let side = |a: [f32; 3], b: [f32; 3]| {
        (a[0] == 0.0 && b[0] == 0.0)
            || (a[0] == edge && b[0] == edge)
            || (a[1] == 0.0 && b[1] == 0.0)
            || (a[1] == edge && b[1] == edge)
    };
    let mut out = mesh.clone();
    let mut low: std::collections::BTreeMap<u16, u16> = std::collections::BTreeMap::new();
    let mut lower = |out: &mut TerrainMesh, i: u16| -> Option<u16> {
        if let Some(l) = low.get(&i) {
            return Some(*l);
        }
        let [x, y, _] = out.vertices[usize::from(i)];
        let l = u16::try_from(out.vertices.len()).ok()?;
        out.vertices.push([x, y, SKIRT_HEIGHT]);
        low.insert(i, l);
        Some(l)
    };
    for tri in mesh.indices.chunks_exact(3) {
        for (p, q) in [(0, 1), (1, 2), (2, 0)] {
            let (a, b) = (tri[p], tri[q]);
            if !side(mesh.vertices[usize::from(a)], mesh.vertices[usize::from(b)]) {
                continue;
            }
            let (Some(la), Some(lb)) = (lower(&mut out, a), lower(&mut out, b)) else {
                return mesh.clone();
            };
            out.indices.extend_from_slice(&[a, la, b, b, la, lb]);
        }
    }
    out
}

/// Plan every level of `settings` for `world`.
///
/// # Errors
/// [`Error::BadWorldName`] for a name that cannot be a folder, [`Error::ZeroLevel`] for a level of zero, and
/// [`Error::RasterTooLarge`] when a level's raster would be too big.
pub fn plan_world(world: &Worldspace, settings: &TerrainSettings) -> Result<TerrainPlan, Error> {
    layout::check_world_name(world.name())?;
    let world = &world.without_outliers();
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
        LEVELS, LevelSettings, MAX_VERTICES, TerrainSettings, build_block_mesh, mip_levels,
        plan_world,
    };
    use crate::Error;
    use crate::land::CellHeights;
    use crate::raster::{BlockGrid, HeightRaster};
    use crate::worldspace::Worldspace;

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
            specular_alpha: false,
            skirts: false,
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
            (256, 256, 9)
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

    /// Cells of different flat heights, so that the blocks have borders with something to follow.
    fn stepped_world() -> Worldspace {
        let cells: Vec<_> = (0..8)
            .flat_map(|x| (0..8).map(move |y| (x, y)))
            .map(|(x, y)| {
                (
                    (x, y),
                    CellHeights::flat(60.0 + 37.0 * ((x * 3 + y * 5) % 7) as f32),
                )
            })
            .collect();
        Worldspace::from_cells(1, "W", cells)
    }

    fn mesh_of(level_cells: u32, skirts: bool, cap: usize) -> super::TerrainMesh {
        let world = stepped_world();
        let raster = HeightRaster::build(&world, BlockGrid::new(level_cells).unwrap())
            .unwrap()
            .unwrap();
        let mut l = level(level_cells, 1.0);
        l.skirts = skirts;
        l.max_vertices = cap;
        build_block_mesh(&raster, &l, 0, 0).unwrap().1
    }

    #[test]
    fn skirts_off_leave_the_mesh_as_it_was() {
        for cells in [4, 8, 16, 32] {
            let plain = mesh_of(cells, false, MAX_VERTICES);
            assert!(plain.vertices.iter().all(|v| v[2] > super::SKIRT_HEIGHT));
        }
    }

    #[test]
    fn every_border_vertex_has_a_skirt_below_it_at_every_level_pair() {
        // the gap between a finer and a coarser block is closed when every border vertex of both reaches the
        // constant skirt height, whatever vertices the neighbour has
        for cells in [4, 8, 16, 32] {
            let plain = mesh_of(cells, false, MAX_VERTICES);
            let skirted = mesh_of(cells, true, MAX_VERTICES);
            let edge = cells as f32 * 4096.0;
            let on_border =
                |v: &[f32; 3]| v[0] == 0.0 || v[0] == edge || v[1] == 0.0 || v[1] == edge;
            let border: Vec<_> = plain.vertices.iter().filter(|v| on_border(v)).collect();
            assert!(!border.is_empty());
            for v in &border {
                assert!(
                    skirted
                        .vertices
                        .iter()
                        .any(|s| s[0] == v[0] && s[1] == v[1] && s[2] == super::SKIRT_HEIGHT),
                    "level {cells}: no skirt under border vertex {v:?}"
                );
            }
            let low = skirted
                .vertices
                .iter()
                .filter(|v| v[2] == super::SKIRT_HEIGHT)
                .count();
            assert_eq!(low, border.len());
            // every triangle edge along the border adds two triangles
            let mut edges = 0;
            for t in plain.indices.chunks_exact(3) {
                for (p, q) in [(0, 1), (1, 2), (2, 0)] {
                    let (a, b) = (
                        plain.vertices[usize::from(t[p])],
                        plain.vertices[usize::from(t[q])],
                    );
                    if (a[0] == b[0] && (a[0] == 0.0 || a[0] == edge))
                        || (a[1] == b[1] && (a[1] == 0.0 || a[1] == edge))
                    {
                        edges += 1;
                    }
                }
            }
            assert!(edges > 0);
            assert_eq!(skirted.triangle_count() - plain.triangle_count(), 2 * edges);
        }
    }

    #[test]
    fn skirt_triangles_face_away_from_the_block() {
        let skirted = mesh_of(4, true, MAX_VERTICES);
        let centre = 4.0 * 4096.0 / 2.0;
        for tri in skirted.indices.chunks_exact(3) {
            let v = |i: u16| skirted.vertices[usize::from(i)];
            let (a, b, c) = (v(tri[0]), v(tri[1]), v(tri[2]));
            if [a, b, c].iter().all(|p| p[2] > super::SKIRT_HEIGHT) {
                continue;
            }
            let n = [
                (b[1] - a[1]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[1] - a[1]),
                (b[2] - a[2]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[2] - a[2]),
            ];
            let mid = [
                (a[0] + b[0] + c[0]) / 3.0 - centre,
                (a[1] + b[1] + c[1]) / 3.0 - centre,
            ];
            assert!(n[0] * mid[0] + n[1] * mid[1] > 0.0, "{tri:?} faces inwards");
        }
    }

    #[test]
    fn the_skirt_vertices_count_against_the_cap() {
        let free = mesh_of(4, true, MAX_VERTICES);
        let cap = free.vertices.len() / 2;
        let capped = mesh_of(4, true, cap);
        assert!(
            capped.vertices.len() <= cap,
            "{} > {cap}",
            capped.vertices.len()
        );
        assert!(capped.vertices.iter().any(|v| v[2] == super::SKIRT_HEIGHT));
    }

    #[test]
    fn the_skirt_flag_enters_the_settings_text_only_when_set() {
        let mut l = level(4, 1.0);
        let off = format!("{l:?}");
        assert!(!off.contains("skirts"));
        l.skirts = true;
        assert!(format!("{l:?}").contains("skirts: true"));
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
