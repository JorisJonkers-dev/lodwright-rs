// SPDX-License-Identifier: GPL-3.0-only
//! Where a placed object ended up in the object blocks of a generated output.
//!
//! The expected vertices of an object are the vertices of its far model placed like the generator places them
//! (rotation, scale, the shape offset the scale does not touch). A written block holds an object when its vertices
//! coincide with those, within [`TOLERANCE`] game units, so the check follows the object's own extents and not a
//! fixed radius around its position. The block, shape (so texture set) and cell segment of every match are reported.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use lodwright_core::objects::Instance;
use lodwright_nif::{NifFile, ObjectShape, SEGMENTS, read_object_shapes};

use crate::assets::Assets;
use crate::lodmodel::parse_lod_model;
use crate::objects::{BLOCK_CELLS, cell_of, euler, lod_model_path, turn};

/// Two vertices closer than this (game units) are the same vertex: the blocks store 32-bit floats relative to the
/// block's corner.
pub const TOLERANCE: f32 = 0.5;

/// The share of an object's vertices that must be found in the blocks for the object to count as written. Faces
/// below the ground or the water are dropped by generation, so a few vertices of a half-buried object may be absent.
pub const MATCH_FRACTION: f32 = 0.5;

/// The far model of an object, placed.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedGeometry {
    /// The corners in world units.
    pub vertices: Vec<[f32; 3]>,
    /// The lowest corner of the model's extents.
    pub min: [f32; 3],
    /// The highest corner of the model's extents.
    pub max: [f32; 3],
    /// The cell whose segment holds the object.
    pub cell: (i32, i32),
}

/// The far model of `instance` (`<model>_lod.nif`) placed as generation places it.
///
/// # Errors
/// A message when the far model is missing, unreadable or empty.
pub fn expected_geometry(
    instance: &Instance,
    assets: &dyn Assets,
) -> Result<ExpectedGeometry, String> {
    let path = lod_model_path(&instance.model);
    let bytes = assets
        .read(&path)
        .ok_or_else(|| format!("no far model {path}"))?;
    let parts = parse_lod_model(&bytes).map_err(|e| format!("far model {path} unreadable: {e}"))?;
    let placement = &instance.placement;
    let rot = euler(placement.rot);
    let mut vertices = Vec::new();
    for part in &parts {
        // a vertex that no triangle uses is not written
        let mut used = vec![false; part.vertices.len()];
        for t in &part.triangles {
            for &i in t {
                if let Some(u) = used.get_mut(usize::from(i)) {
                    *u = true;
                }
            }
        }
        for v in part
            .vertices
            .iter()
            .zip(&used)
            .filter_map(|(v, used)| used.then_some(v))
        {
            let o = part.offset;
            let p = turn(
                &rot,
                [
                    v[0] * placement.scale + o[0],
                    v[1] * placement.scale + o[1],
                    v[2] * placement.scale + o[2],
                ],
            );
            vertices.push([
                placement.pos[0] + p[0],
                placement.pos[1] + p[1],
                placement.pos[2] + p[2],
            ]);
        }
    }
    if vertices.is_empty() {
        return Err(format!("far model {path} has no vertices"));
    }
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for v in &vertices {
        for k in 0..3 {
            min[k] = min[k].min(v[k]);
            max[k] = max[k].max(v[k]);
        }
    }
    Ok(ExpectedGeometry {
        vertices,
        min,
        max,
        cell: cell_of(instance),
    })
}

/// Where in one shape of one block an object was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The block file, relative to the output folder.
    pub file: String,
    /// The shape's position among the block's shapes.
    pub shape: usize,
    /// The diffuse texture the shape draws with.
    pub texture: String,
    /// Triangles of the object in this shape.
    pub triangles: usize,
    /// The segments that hold them, ascending.
    pub segments: Vec<usize>,
    /// The segment the object's cell should be in, from the block's position.
    pub expected_segment: usize,
}

/// The outcome of looking for one object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presence {
    /// Vertices the far model has once placed.
    pub expected_vertices: usize,
    /// Of those, the vertices found in the blocks.
    pub matched_vertices: usize,
    /// Where they were found, in file order.
    pub hits: Vec<Hit>,
}

impl Presence {
    /// Whether enough of the object is in the blocks to count as written.
    #[must_use]
    pub fn written(&self) -> bool {
        self.expected_vertices > 0
            && self.matched_vertices as f32 >= MATCH_FRACTION * self.expected_vertices as f32
    }
}

struct BlockFile {
    path: String,
    x: i32,
    y: i32,
}

/// The object blocks of one world in an output folder, read when a query needs them.
pub struct WrittenWorld {
    root: PathBuf,
    files: Vec<BlockFile>,
    cache: HashMap<usize, Vec<ObjectShape>>,
}

fn block_coordinates(name: &str) -> Option<(i32, i32)> {
    // <world>.level4.x<X>.y<Y>.nif and <world>.level4.high.x<X>.y<Y>.nif
    let stem = name.strip_suffix(".nif")?;
    let (head, y) = stem.rsplit_once(".y")?;
    let (_, x) = head.rsplit_once(".x")?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

impl WrittenWorld {
    /// List the object block files of `world` below `output`.
    ///
    /// # Errors
    /// A message when the blocks folder cannot be read.
    pub fn open(output: &Path, world: &str) -> Result<Self, String> {
        let world = world.to_ascii_lowercase();
        let relative = format!("meshes/landscape/lod/{world}/blocks");
        let dir = output.join(&relative);
        let mut files = Vec::new();
        let listing =
            std::fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
        for entry in listing.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if !lower.starts_with(&format!("{world}.level4.")) {
                continue;
            }
            if let Some((x, y)) = block_coordinates(&lower) {
                files.push(BlockFile {
                    path: format!("{relative}/{name}"),
                    x,
                    y,
                });
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self {
            root: output.to_path_buf(),
            files,
            cache: HashMap::new(),
        })
    }

    /// How many block files the world has.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.files.len()
    }

    fn shapes(&mut self, index: usize) -> &[ObjectShape] {
        if !self.cache.contains_key(&index) {
            let read = std::fs::read(self.root.join(&self.files[index].path))
                .ok()
                .and_then(|bytes| NifFile::read(&bytes).ok())
                .and_then(|nif| read_object_shapes(&nif).ok())
                .unwrap_or_default();
            self.cache.insert(index, read);
        }
        &self.cache[&index]
    }

    /// Look for an object whose far model, placed, is `expected`.
    pub fn find(&mut self, expected: &ExpectedGeometry) -> Presence {
        let cell_units = 4096.0_f32;
        let wanted = |f: &BlockFile| {
            let (x0, y0) = (f.x as f32 * cell_units, f.y as f32 * cell_units);
            let size = BLOCK_CELLS as f32 * cell_units;
            expected.max[0] >= x0 - cell_units
                && expected.min[0] <= x0 + size + cell_units
                && expected.max[1] >= y0 - cell_units
                && expected.min[1] <= y0 + size + cell_units
        };
        let near: Vec<usize> = (0..self.files.len())
            .filter(|&k| wanted(&self.files[k]))
            .collect();
        // the expected vertices on a grid of twice the tolerance, so a match is in the cell or a neighbour
        let step = TOLERANCE * 2.0;
        let key = |v: [f32; 3]| -> (i64, i64, i64) {
            (
                (v[0] / step).floor() as i64,
                (v[1] / step).floor() as i64,
                (v[2] / step).floor() as i64,
            )
        };
        let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
        for (k, v) in expected.vertices.iter().enumerate() {
            grid.entry(key(*v)).or_default().push(k);
        }
        let mut found = vec![false; expected.vertices.len()];
        let mut hits = Vec::new();
        for index in near {
            let (path, bx, by) = {
                let f = &self.files[index];
                (f.path.clone(), f.x, f.y)
            };
            let shapes = self.shapes(index).to_vec();
            for (number, shape) in shapes.iter().enumerate() {
                let mut matched_here = vec![false; shape.vertices.len()];
                for (n, v) in shape.vertices.iter().enumerate() {
                    let w = [
                        shape.origin[0] + v[0],
                        shape.origin[1] + v[1],
                        shape.origin[2] + v[2],
                    ];
                    let (kx, ky, kz) = key(w);
                    // every expected vertex at this place counts: a model repeats a corner once per UV seam
                    for dx in -1..=1 {
                        for dy in -1..=1 {
                            for dz in -1..=1 {
                                let Some(list) = grid.get(&(kx + dx, ky + dy, kz + dz)) else {
                                    continue;
                                };
                                for &e in list {
                                    let c = expected.vertices[e];
                                    if (c[0] - w[0]).abs() <= TOLERANCE
                                        && (c[1] - w[1]).abs() <= TOLERANCE
                                        && (c[2] - w[2]).abs() <= TOLERANCE
                                    {
                                        found[e] = true;
                                        matched_here[n] = true;
                                    }
                                }
                            }
                        }
                    }
                }
                if !matched_here.iter().any(|m| *m) {
                    continue;
                }
                // the triangles with all three corners matched, and the segment of each
                let mut segments = HashSet::new();
                let mut triangles = 0;
                let mut segment = 0;
                let mut left = shape.segment_triangles.first().copied().unwrap_or(0);
                for tri in &shape.triangles {
                    while left == 0 && segment + 1 < shape.segment_triangles.len() {
                        segment += 1;
                        left = shape.segment_triangles[segment];
                    }
                    left = left.saturating_sub(1);
                    if tri.iter().all(|&i| matched_here[usize::from(i)]) {
                        triangles += 1;
                        segments.insert(segment);
                    }
                }
                if triangles == 0 {
                    // corners that another object shares, no triangle of this one
                    continue;
                }
                let mut segments: Vec<usize> = segments.into_iter().collect();
                segments.sort_unstable();
                let at = |c: i32, b: i32| {
                    usize::try_from((c - b).clamp(0, BLOCK_CELLS - 1)).unwrap_or(0)
                };
                let expected_segment = (at(expected.cell.0, bx) * BLOCK_CELLS as usize
                    + at(expected.cell.1, by))
                .min(SEGMENTS - 1);
                hits.push(Hit {
                    file: path.clone(),
                    shape: number,
                    texture: shape.texture.clone(),
                    triangles,
                    segments,
                    expected_segment,
                });
            }
        }
        Presence {
            expected_vertices: expected.vertices.len(),
            matched_vertices: found.iter().filter(|f| **f).count(),
            hits,
        }
    }
}
