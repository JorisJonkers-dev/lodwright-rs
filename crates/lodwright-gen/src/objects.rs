// SPDX-License-Identifier: GPL-3.0-only
//! Object LOD generation: the small LOD models of every visible object, merged per block of 4 by 4 cells, with
//! their textures packed into one atlas per world.
//!
//! What goes in is from the rows `game-lod-object-layout`, `game-lod-object-block-nif` and
//! `fnv-object-records`; the selection rule reproduces the game's own totals (see `docs/gap-list.md`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use lodwright_core::layout;
use lodwright_core::objects::Instance;
use lodwright_core::plugin::Placement;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode, resize};
use lodwright_nif::{ObjectBlock, SEGMENTS, object_block_shapes};

use crate::assets::{Assets, read_texture, texture_warning};
use crate::atlas::{Atlas, Rect, pack};
use crate::error::GenError;
use crate::lodmodel::{Part, parse_lod_model};
use crate::terrain::{FileRecord, write_file};

const CELL_UNITS: f32 = 4096.0;
/// Cells per object block edge.
pub const BLOCK_CELLS: i32 = 4;
const MAX_VERTICES: usize = 65_535;
/// Key of the flat texture used by shapes without one.
const NONE: &str = "<none>";

/// What object generation produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectReport {
    /// Files written, sorted by path.
    pub files: Vec<FileRecord>,
    /// Things that did not go in and why, in a stable order.
    pub warnings: Vec<String>,
    /// Instances placed into blocks.
    pub instances: usize,
    /// Instances left out because their model has no LOD model or it could not be read.
    pub skipped: usize,
    /// Faces left out because they lie wholly under the ground ([`Surfaces::ground`]).
    pub dropped_under_terrain: usize,
    /// Faces left out because they lie wholly under a cell's water ([`Surfaces::water`]).
    pub dropped_under_water: usize,
}

/// A height at a world position (x, y), or `None` where there is none.
pub type HeightAt<'a> = &'a dyn Fn(f32, f32) -> Option<f32>;

/// The surfaces under which object faces are left out; `None` keeps every face (the default).
#[derive(Clone, Copy, Default)]
pub struct Surfaces<'a> {
    /// The ground height; a face whose three corners all lie below it is left out.
    pub ground: Option<HeightAt<'a>>,
    /// The water height of cells that have water; a face whose three corners all lie below it is left out.
    pub water: Option<HeightAt<'a>>,
    /// The cell where the block grid of the world starts (`Worldspace::grid_anchor`); object blocks are 4 by 4
    /// cells from there. (0, 0) is the default.
    pub grid_anchor: (i32, i32),
}

type Mat = [[f32; 3]; 3];

// source: game-object-rotation
/// The rotation of a reference: the matrix is Rx(-x) * Ry(-y) * Rz(-z) with the usual right-handed axis rotations,
/// which is what places a LOD model's vertices where the game's own blocks have them.
fn euler(rot: [f32; 3]) -> Mat {
    let (sx, cx) = (-rot[0]).sin_cos();
    let (sy, cy) = (-rot[1]).sin_cos();
    let (sz, cz) = (-rot[2]).sin_cos();
    [
        [cy * cz, -cy * sz, sy],
        [cx * sz + sx * sy * cz, cx * cz - sx * sy * sz, -sx * cy],
        [sx * sz - cx * sy * cz, sx * cz + cx * sy * sz, cx * cy],
    ]
}

fn turn(m: &Mat, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn norm_key(path: &str) -> String {
    let p = path.to_ascii_lowercase().replace('\\', "/");
    p.strip_prefix("data/").map_or(p.clone(), str::to_owned)
}

fn lod_model_path(model: &str) -> String {
    let m = norm_key(model);
    let m = m.strip_prefix("meshes/").unwrap_or(&m);
    let stem = m.strip_suffix(".nif").unwrap_or(m);
    format!("meshes/{stem}_lod.nif")
}

/// Normals from the triangles, for a model that has none.
fn face_normals(vertices: &[[f32; 3]], triangles: &[[u16; 3]]) -> Vec<[f32; 3]> {
    let mut n = vec![[0.0_f32; 3]; vertices.len()];
    for t in triangles {
        let [a, b, c] = t.map(|i| vertices[usize::from(i)]);
        let (u, v) = (
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
        );
        let f = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for i in t {
            for k in 0..3 {
                n[usize::from(*i)][k] += f[k];
            }
        }
    }
    n.into_iter().map(unit_or_up).collect()
}

fn unit_or_up(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 1e-12 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// Tangents and bitangents from the UVs, for a model that has none.
fn tangent_frame(
    vertices: &[[f32; 3]],
    uv: &[[f32; 2]],
    normals: &[[f32; 3]],
    triangles: &[[u16; 3]],
) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let mut t = vec![[0.0_f32; 3]; vertices.len()];
    let mut b = vec![[0.0_f32; 3]; vertices.len()];
    for tri in triangles {
        let ix = tri.map(usize::from);
        let (p0, p1, p2) = (vertices[ix[0]], vertices[ix[1]], vertices[ix[2]]);
        let (w0, w1, w2) = (uv[ix[0]], uv[ix[1]], uv[ix[2]]);
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let (du1, dv1, du2, dv2) = (w1[0] - w0[0], w1[1] - w0[1], w2[0] - w0[0], w2[1] - w0[1]);
        let det = du1 * dv2 - du2 * dv1;
        if det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        for i in ix {
            for k in 0..3 {
                t[i][k] += (e1[k] * dv2 - e2[k] * dv1) * r;
                b[i][k] += (e2[k] * du1 - e1[k] * du2) * r;
            }
        }
    }
    let fix = |v: [f32; 3], n: [f32; 3], fallback: [f32; 3]| {
        let d = v[0] * n[0] + v[1] * n[1] + v[2] * n[2];
        let o = [v[0] - n[0] * d, v[1] - n[1] * d, v[2] - n[2] * d];
        let l = (o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt();
        if l > 1e-9 {
            [o[0] / l, o[1] / l, o[2] / l]
        } else {
            fallback
        }
    };
    let tangents: Vec<[f32; 3]> = t
        .iter()
        .zip(normals)
        .map(|(v, n)| fix(*v, *n, [1.0, 0.0, 0.0]))
        .collect();
    let bitangents = b
        .iter()
        .zip(normals)
        .map(|(v, n)| fix(*v, *n, [0.0, 1.0, 0.0]))
        .collect();
    (tangents, bitangents)
}

struct Model {
    parts: Vec<Part>,
}

/// Merged geometry of one block, in block-local coordinates.
#[derive(Default)]
struct Merged {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    tangents: Vec<[f32; 3]>,
    bitangents: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    indices: Vec<u16>,
    segment_triangles: [u32; SEGMENTS],
    dropped_under_terrain: usize,
    dropped_under_water: usize,
}

// source: game-object-block-cell
/// The cell an instance counts in for its block: the cell that holds a temporary reference, else the cell under
/// its position.
fn cell_of(i: &Instance) -> (i32, i32) {
    i.cell.unwrap_or_else(|| {
        let cell = |v: f32| (v / CELL_UNITS).floor() as i32;
        (cell(i.placement.pos[0]), cell(i.placement.pos[1]))
    })
}

// source: game-object-block-segments
/// The segment of a block that holds an instance's triangles: one per cell of the block's four by four cells,
/// x-major (`cell x * 4 + cell y`), by the cell of the reference, so the game can switch off the LOD of the cells it
/// has loaded.
fn segment_of(i: &Instance, block: (i32, i32)) -> usize {
    let (cx, cy) = cell_of(i);
    let at = |c: i32, b: i32| usize::try_from((c - b).clamp(0, BLOCK_CELLS - 1)).unwrap_or(0);
    at(cx, block.0) * BLOCK_CELLS as usize + at(cy, block.1)
}

// source: game-object-uv-clamp
/// A UV inside the atlas rectangle of its texture. A coordinate outside 0 to 1 is held at the nearest edge, as the
/// game's own blocks do: a texture that is meant to repeat cannot repeat inside an atlas, and the fractional part
/// would turn a vertex that is a hair outside the texture (-0.02) into the far edge (0.98) and smear the whole
/// texture across its triangles.
fn rect_uv(r: &Rect, atlas: &Atlas, uv: [f32; 2]) -> [f32; 2] {
    let (u, v) = (uv[0].clamp(0.0, 1.0), uv[1].clamp(0.0, 1.0));
    [
        (r.x as f32 + u * r.w as f32) / atlas.width as f32,
        (r.y as f32 + v * r.h as f32) / atlas.height as f32,
    ]
}

#[allow(clippy::too_many_arguments)]
fn add_instance(
    m: &mut Merged,
    model: &Model,
    placement: &Placement,
    origin: (f32, f32),
    segment: usize,
    atlas: &Atlas,
    key_of: &dyn Fn(&Part) -> String,
    surfaces: Surfaces,
) -> bool {
    let rot = euler(placement.rot);
    // per part: the corners in world units, the faces kept and the vertices those faces use
    let mut kept: Vec<(Vec<[f32; 3]>, Vec<[u16; 3]>, Vec<Option<u16>>)> = Vec::new();
    let (mut under_terrain, mut under_water) = (0, 0);
    for part in &model.parts {
        let world: Vec<[f32; 3]> = part
            .vertices
            .iter()
            .map(|v| {
                let o = part.offset;
                // source: game-object-shape-offset
                let p = turn(
                    &rot,
                    [
                        v[0] * placement.scale + o[0],
                        v[1] * placement.scale + o[1],
                        v[2] * placement.scale + o[2],
                    ],
                );
                [
                    placement.pos[0] + p[0],
                    placement.pos[1] + p[1],
                    placement.pos[2] + p[2],
                ]
            })
            .collect();
        let below = |surface: Option<HeightAt>, t: &[u16; 3]| {
            surface.is_some_and(|height| {
                t.iter().all(|&i| {
                    let c = world[usize::from(i)];
                    height(c[0], c[1]).is_some_and(|h| c[2] < h)
                })
            })
        };
        let mut faces = Vec::with_capacity(part.triangles.len());
        for t in &part.triangles {
            if below(surfaces.ground, t) {
                under_terrain += 1;
            } else if below(surfaces.water, t) {
                under_water += 1;
            } else {
                faces.push(*t);
            }
        }
        // a vertex no kept face uses is left out; the others are numbered in their original order
        let mut used = vec![false; part.vertices.len()];
        for t in &faces {
            for &i in t {
                used[usize::from(i)] = true;
            }
        }
        let mut next = 0_u16;
        let remap: Vec<Option<u16>> = used
            .iter()
            .map(|&u| {
                u.then(|| {
                    next += 1;
                    next - 1
                })
            })
            .collect();
        kept.push((world, faces, remap));
    }
    let added: usize = kept
        .iter()
        .map(|(_, _, remap)| remap.iter().flatten().count())
        .sum();
    if m.vertices.len() + added > MAX_VERTICES {
        return false;
    }
    m.dropped_under_terrain += under_terrain;
    m.dropped_under_water += under_water;
    for (part, (world, faces, remap)) in model.parts.iter().zip(kept) {
        let base = u16::try_from(m.vertices.len()).unwrap_or(u16::MAX);
        let normals = part
            .normals
            .clone()
            .unwrap_or_else(|| face_normals(&part.vertices, &part.triangles));
        let (tangents, bitangents) = part
            .tangents
            .clone()
            .unwrap_or_else(|| tangent_frame(&part.vertices, &part.uv, &normals, &part.triangles));
        let key = key_of(part);
        let rect = atlas
            .rects
            .get(&key)
            .or_else(|| atlas.rects.get(NONE))
            .copied();
        for i in (0..part.vertices.len()).filter(|&i| remap[i].is_some()) {
            let w = world[i];
            m.vertices.push([w[0] - origin.0, w[1] - origin.1, w[2]]);
            m.normals.push(unit_or_up(turn(&rot, normals[i])));
            m.tangents.push(unit_or_up(turn(&rot, tangents[i])));
            m.bitangents.push(unit_or_up(turn(&rot, bitangents[i])));
            m.uv.push(rect.map_or(part.uv[i], |r| rect_uv(&r, atlas, part.uv[i])));
        }
        for t in &faces {
            m.indices
                .extend(t.map(|i| base + remap[usize::from(i)].unwrap_or(0)));
        }
        m.segment_triangles[segment.min(SEGMENTS - 1)] += u32::try_from(faces.len()).unwrap_or(0);
    }
    true
}

fn flat(width: u32, height: u32, rgba: [u8; 4]) -> Image {
    let px: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
    Image::new(width, height, px).expect("a small flat image")
}

fn load_texture(assets: &dyn Assets, key: &str) -> Option<Image> {
    read_texture(assets, key).ok()
}

fn encode_bc3(image: &Image) -> Result<Vec<u8>, GenError> {
    Ok(encode(
        image,
        &EncodeOptions {
            format: Format::Bc3,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )?)
}

fn blit(canvas: &mut [u8], width: u32, src: &Image, r: &Rect) {
    for y in 0..r.h.min(src.height()) {
        for x in 0..r.w.min(src.width()) {
            if let Some(p) = src.pixel(x, y) {
                let at = (((r.y + y) * width + r.x + x) * 4) as usize;
                canvas[at..at + 4].copy_from_slice(&p);
            }
        }
    }
}

/// Generate the object LOD of one world into `out`.
///
/// `instances` come from [`lodwright_core::objects::visible_instances`]; models and textures are read through
/// `assets`. Blocks are 4 by 4 cells; references flagged high priority go to the `high` file of their block.
///
/// # Errors
/// [`GenError`] for a world name that cannot be a folder, a texture that cannot be encoded or a write that fails.
pub fn generate_objects(
    world: &str,
    instances: &[Instance],
    assets: &dyn Assets,
    out: &Path,
) -> Result<ObjectReport, GenError> {
    generate_objects_with_progress(
        world,
        instances,
        assets,
        out,
        Surfaces::default(),
        &mut |_, _| {},
    )
}

/// [`generate_objects`], calling `block_done(done, total)` after each block: `total` is the number of block files
/// the world can have (a normal and a high-priority file count separately), known before the first block is built,
/// and `done` reaches it. A world without a usable model reports no block.
///
/// # Errors
/// As [`generate_objects`].
pub fn generate_objects_with_progress(
    world: &str,
    instances: &[Instance],
    assets: &dyn Assets,
    out: &Path,
    surfaces: Surfaces,
    block_done: &mut dyn FnMut(u64, u64),
) -> Result<ObjectReport, GenError> {
    layout::check_world_name(world)?;
    let mut report = ObjectReport::default();
    // models
    let mut models: BTreeMap<String, Option<Model>> = BTreeMap::new();
    for i in instances {
        let path = lod_model_path(&i.model);
        models.entry(path.clone()).or_insert_with(|| {
            assets
                .read(&path)
                .and_then(|b| parse_lod_model(&b).ok())
                .map(|parts| Model { parts })
        });
    }
    // a model with a shape that has no diffuse texture is said by its path (its objects are drawn in a flat colour, and a
    // user must not get pale shapes silently); the models that are missing or cannot be read are said in one line with
    // their count and the first paths (many flagged bases have no LOD model in the game's own data, which is how the
    // game's own blocks are made too)
    let mut missing: Vec<&str> = Vec::new();
    for (path, model) in &models {
        match model {
            None => missing.push(path),
            Some(m) if m.parts.iter().any(|p| p.diffuse.is_none()) => report.warnings.push(
                format!("model {path} has a shape with no diffuse texture, a flat colour is used"),
            ),
            Some(_) => {}
        }
    }
    if !missing.is_empty() {
        let shown: Vec<&str> = missing.iter().take(3).copied().collect();
        report.warnings.push(format!(
            "{} LOD models are missing or cannot be read, their objects are left out (for example {})",
            missing.len(),
            shown.join(", ")
        ));
    }
    if models.values().all(Option::is_none) {
        report.skipped = instances.len();
        return Ok(report);
    }
    // a static collection draws its own LOD model; its parts count only when that model is missing
    let own_lod: BTreeSet<u32> = instances
        .iter()
        .filter(|i| {
            !i.part
                && models
                    .get(&lod_model_path(&i.model))
                    .is_some_and(Option::is_some)
        })
        .map(|i| i.reference)
        .collect();
    let instances: Vec<&Instance> = instances
        .iter()
        .filter(|i| !(i.part && own_lod.contains(&i.reference)))
        .collect();
    // textures: key by the diffuse path of each shape
    let key_of = |p: &Part| {
        p.diffuse
            .as_deref()
            .map_or_else(|| NONE.to_owned(), norm_key)
    };
    let mut diffuse_keys: BTreeSet<String> = BTreeSet::new();
    let mut normal_of: HashMap<String, String> = HashMap::new();
    for model in models.values().flatten() {
        for p in &model.parts {
            let k = key_of(p);
            if let Some(n) = &p.normal_map {
                normal_of.entry(k.clone()).or_insert_with(|| norm_key(n));
            }
            diffuse_keys.insert(k);
        }
    }
    diffuse_keys.insert(NONE.to_owned());
    let mut images: BTreeMap<String, Image> = BTreeMap::new();
    for k in &diffuse_keys {
        match read_texture(assets, k) {
            Ok(img) => {
                images.insert(k.clone(), img);
            }
            Err(fault) => {
                if k != NONE {
                    report.warnings.push(texture_warning(
                        "texture",
                        k,
                        &fault,
                        ", a flat colour is used",
                    ));
                }
                images.insert(k.clone(), flat(8, 8, [128, 128, 128, 255]));
            }
        }
    }
    let sizes: Vec<(String, u32, u32)> = images
        .iter()
        .map(|(k, i)| (k.clone(), i.width(), i.height()))
        .collect();
    let atlas = pack(&sizes);
    let mut diffuse = vec![0_u8; (atlas.width * atlas.height * 4) as usize];
    for (k, r) in &atlas.rects {
        let img = &images[k];
        if (img.width(), img.height()) == (r.w, r.h) {
            blit(&mut diffuse, atlas.width, img, r);
        } else if let Ok(small) = resize(img, r.w, r.h, Filter::Box) {
            blit(&mut diffuse, atlas.width, &small, r);
        }
    }
    let (nw, nh) = ((atlas.width / 2).max(4), (atlas.height / 2).max(4));
    let mut normal = vec![0_u8; (nw * nh * 4) as usize];
    for px in normal.as_chunks_mut::<4>().0 {
        px.copy_from_slice(&[128, 128, 255, 255]);
    }
    for (k, r) in &atlas.rects {
        let half = Rect {
            x: r.x / 2,
            y: r.y / 2,
            w: (r.w / 2).max(1),
            h: (r.h / 2).max(1),
        };
        let Some(img) = normal_of.get(k).and_then(|n| load_texture(assets, n)) else {
            continue;
        };
        if let Ok(small) = resize(&img, half.w, half.h, Filter::Box) {
            blit(&mut normal, nw, &small, &half);
        }
    }
    let [diffuse_path, normal_path] = layout::atlas_paths(world)?;
    let diffuse_image = Image::new(atlas.width, atlas.height, diffuse)?;
    let normal_image = Image::new(nw, nh, normal)?;
    report.files.push(write_file(
        out,
        &diffuse_path,
        &encode_bc3(&diffuse_image)?,
    )?);
    report
        .files
        .push(write_file(out, &normal_path, &encode_bc3(&normal_image)?)?);
    // blocks
    let mut groups: BTreeMap<(i32, i32, bool), Vec<&Instance>> = BTreeMap::new();
    for i in instances {
        if models
            .get(&lod_model_path(&i.model))
            .is_none_or(Option::is_none)
        {
            report.skipped += 1;
            continue;
        }
        let (cx, cy) = cell_of(i);
        let (ax, ay) = surfaces.grid_anchor;
        let key = (
            ax + (cx - ax).div_euclid(BLOCK_CELLS) * BLOCK_CELLS,
            ay + (cy - ay).div_euclid(BLOCK_CELLS) * BLOCK_CELLS,
            i.high_priority,
        );
        groups.entry(key).or_default().push(i);
        if i.high_priority {
            groups.entry((key.0, key.1, false)).or_default().push(i);
        }
    }
    let [diffuse_ref, normal_ref] = layout::nif_atlas_paths(world)?;
    let total = groups.len() as u64;
    for (done, ((bx, by, high), mut members)) in (1_u64..).zip(groups) {
        members.sort_by_key(|i| (segment_of(i, (bx, by)), i.reference, i.base));
        let origin = (bx as f32 * CELL_UNITS, by as f32 * CELL_UNITS);
        // a block is one shape while its objects fit under the 16-bit vertex limit and several shapes after that, each
        // with the atlas, as the game's blocks hold several shapes
        let mut chunks: Vec<Merged> = vec![Merged::default()];
        let mut dropped = 0;
        for i in members {
            let segment = segment_of(i, (bx, by));
            let Some(model) = models
                .get(&lod_model_path(&i.model))
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let add = |m: &mut Merged| {
                add_instance(
                    m,
                    model,
                    &i.placement,
                    origin,
                    segment,
                    &atlas,
                    &key_of,
                    surfaces,
                )
            };
            let last = chunks.last_mut().expect("one chunk at least");
            let mut placed = add(last);
            if !placed && !last.vertices.is_empty() {
                chunks.push(Merged::default());
                placed = add(chunks.last_mut().expect("a new chunk"));
            }
            if placed {
                report.instances += 1;
            } else {
                dropped += 1;
            }
        }
        if dropped > 0 {
            report.warnings.push(format!(
                "block x{bx} y{by}: {dropped} objects left out, each alone has more vertices than one shape can hold"
            ));
        }
        // a high-priority instance is in its block's normal file too: count its faces once, from the normal file
        if !high {
            for m in &chunks {
                report.dropped_under_terrain += m.dropped_under_terrain;
                report.dropped_under_water += m.dropped_under_water;
            }
        }
        chunks.retain(|m| !m.indices.is_empty());
        if chunks.is_empty() {
            block_done(done, total);
            continue;
        }
        if chunks.len() > 1 {
            report.warnings.push(format!(
                "block x{bx} y{by}: {} objects did not fit one shape, the block is written as {} shapes",
                chunks[1..].len(),
                chunks.len()
            ));
        }
        let shapes: Vec<ObjectBlock> = chunks
            .iter()
            .map(|m| ObjectBlock {
                origin,
                vertices: &m.vertices,
                normals: &m.normals,
                tangents: &m.tangents,
                bitangents: &m.bitangents,
                uv: &m.uv,
                indices: &m.indices,
                segment_triangles: m.segment_triangles,
                textures: [&diffuse_ref, &normal_ref],
            })
            .collect();
        let file = object_block_shapes(&shapes)?.write()?;
        report.files.push(write_file(
            out,
            &layout::object_mesh_path(world, bx, by, high)?,
            &file,
        )?);
        block_done(done, total);
    }
    report.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(report)
}
