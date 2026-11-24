// SPDX-License-Identifier: GPL-3.0-only
//! Object LOD generation: the small LOD models of every visible object, merged per block of 4 by 4 cells, with
//! their textures packed into atlases per world (one, or several pages when they do not fit one at native size).
//!
//! What goes in is from the rows `game-lod-object-layout`, `game-lod-object-block-nif` and
//! `fnv-object-records`; the selection rule reproduces the game's own totals (see `docs/gap-list.md`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::par::par_map;
use lodwright_contract::WarningRecord;
use lodwright_core::layout;
use lodwright_core::objects::Instance;
use lodwright_core::plugin::Placement;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode_with_threads, resize};
use lodwright_nif::{ObjectBlock, SEGMENTS, object_block_shapes};

use crate::assets::{Assets, read_texture, texture_warning};
use crate::atlas::{Atlas, LARGEST_ATLAS, Rect, pack_pages};
use crate::error::GenError;
use crate::far_simplify::{FarLimits, far_parts_from_full_model};
use crate::lodmodel::{Part, parse_lod_model};
use crate::own_far::{self, OwnSource};
use crate::terrain::{FileRecord, write_file};

const CELL_UNITS: f32 = 4096.0;
/// Cells per object block edge.
pub const BLOCK_CELLS: i32 = 4;
const MAX_VERTICES: usize = 65_535;
/// A shape's triangle count is a 16-bit field too, as its vertex count is.
const MAX_TRIANGLES: usize = 65_535;
/// Key of the flat texture used by shapes without one.
const NONE: &str = "<none>";
/// Key prefix of a glow card's texture (far glow on): black, the size of the card's glow texture, which follows the
/// prefix.
const CARD: &str = "<glow card>";

/// What object generation produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjectReport {
    /// Files written, sorted by path.
    pub files: Vec<FileRecord>,
    /// Things that did not go in and why, in a stable order.
    pub warnings: Vec<String>,
    /// Every warning as a record (kind, plugin, subjects, advice), the complete list the `warnings` lines
    /// summarise: a far model without a LOD model is one record per plugin that marks it visible from far.
    pub records: Vec<WarningRecord>,
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

/// The far-shape limits a caller's rule gives one object, or `None` when no rule gives it any.
pub type RuleShapes<'a> = &'a dyn Fn(&Instance) -> Option<FarLimits>;

/// Whether a caller's rule draws one object from its full model even when it has an authored LOD model.
pub type RuleReplaces<'a> = &'a dyn Fn(&Instance) -> bool;

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
    /// The plugin names in load order, to name the plugin of an instance (`Instance::flagged_by`) in a warning;
    /// empty names them by their number.
    pub plugins: &'a [String],
    /// Called with the name of each phase as it starts (`models`, `textures`, `atlas`, `blocks`), for the step
    /// profile of a run.
    pub phase: Option<&'a dyn Fn(&str)>,
    /// Worker threads for the steps that spread over them (the atlas pages are encoded on this many); 0 and 1 mean
    /// the calling thread alone. The bytes written do not depend on it.
    pub threads: usize,
    /// The shipped far blocks to take the shapes of the references only they draw from (`own_far`); `None` takes none.
    pub own: Option<OwnSource<'a>>,
    /// Draw objects that have no LOD model with a simplified copy of their full model; `None` leaves them out.
    pub full_models: Option<FarLimits>,
    /// The limits for a simplified full model that a caller's rule gives one object (the `generate` of an include
    /// rule), whatever `full_models` says; `None` for an object no such rule names. Only the objects it answers for
    /// get such a shape: another object with the same model and no LOD model stays out unless `full_models` is set.
    pub rule_shapes: Option<RuleShapes<'a>>,
    /// Write a glow atlas beside each atlas page whose textures have glow maps (black where a texture has none), name
    /// it in the glow slot of that page's shapes and give them an emissive material, so lit windows and signs glow
    /// from far at night. Off by default: the blocks and atlases are then the same bytes as without it.
    pub glow: bool,
    /// The objects whose rule (`rule_shapes`) draws them from their full model even over an authored LOD model. A
    /// model is replaced only when this answers yes for every drawn object that uses it, and keeps its LOD model when
    /// its full model cannot be simplified. `None` replaces none.
    pub rule_replaces: Option<RuleReplaces<'a>>,
}

pub(crate) type Mat = [[f32; 3]; 3];

// source: game-object-rotation
/// The rotation of a reference: the matrix is Rx(-x) * Ry(-y) * Rz(-z) with the usual right-handed axis rotations,
/// which is what places a LOD model's vertices where the game's own blocks have them.
pub(crate) fn euler(rot: [f32; 3]) -> Mat {
    let (sx, cx) = (-rot[0]).sin_cos();
    let (sy, cy) = (-rot[1]).sin_cos();
    let (sz, cz) = (-rot[2]).sin_cos();
    [
        [cy * cz, -cy * sz, sy],
        [cx * sz + sx * sy * cz, cx * cz - sx * sy * sz, -sx * cy],
        [sx * sz - cx * sy * cz, sx * cz + cx * sy * sz, cx * cy],
    ]
}

pub(crate) fn turn(m: &Mat, v: [f32; 3]) -> [f32; 3] {
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

fn full_model_path(model: &str) -> String {
    let m = norm_key(model);
    let m = m.strip_prefix("meshes/").unwrap_or(&m);
    format!("meshes/{m}")
}

pub(crate) fn lod_model_path(model: &str) -> String {
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
pub(crate) fn cell_of(i: &Instance) -> (i32, i32) {
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

/// One atlas page: its rectangles, and the key of a part's texture.
struct Page<'a> {
    atlas: &'a Atlas,
    key_of: &'a dyn Fn(&Part) -> String,
}

#[allow(clippy::too_many_arguments)]
fn add_instance(
    m: &mut Merged,
    model: &Model,
    placement: &Placement,
    origin: (f32, f32),
    segment: usize,
    page: &Page,
    surfaces: Surfaces,
) -> bool {
    let rot = euler(placement.rot);
    // only the parts whose texture is on this page
    let parts: Vec<&Part> = model
        .parts
        .iter()
        .filter(|p| page.atlas.rects.contains_key(&(page.key_of)(p)))
        .collect();
    // per part: the corners in world units, the faces kept and the vertices those faces use
    let mut kept: Vec<(Vec<[f32; 3]>, Vec<[u16; 3]>, Vec<Option<u16>>)> = Vec::new();
    let (mut under_terrain, mut under_water) = (0, 0);
    for part in &parts {
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
    let added_triangles: usize = kept.iter().map(|(_, faces, _)| faces.len()).sum();
    if m.vertices.len() + added > MAX_VERTICES
        || m.indices.len() / 3 + added_triangles > MAX_TRIANGLES
    {
        return false;
    }
    m.dropped_under_terrain += under_terrain;
    m.dropped_under_water += under_water;
    for (part, (world, faces, remap)) in parts.iter().zip(kept) {
        let base = u16::try_from(m.vertices.len()).unwrap_or(u16::MAX);
        let normals = part
            .normals
            .clone()
            .unwrap_or_else(|| face_normals(&part.vertices, &part.triangles));
        let (tangents, bitangents) = part
            .tangents
            .clone()
            .unwrap_or_else(|| tangent_frame(&part.vertices, &part.uv, &normals, &part.triangles));
        let atlas = page.atlas;
        let rect = atlas.rects.get(&(page.key_of)(part)).copied();
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

pub(crate) fn encode_bc3(image: &Image, threads: usize) -> Result<Vec<u8>, GenError> {
    Ok(encode_with_threads(
        image,
        &EncodeOptions {
            format: Format::Bc3,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
        threads.max(1),
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

/// The records and lines for the objects whose LOD model is missing or cannot be read: one per plugin that marks them
/// visible from far (the objects are counted, the models listed in full in the record), not one per object.
#[allow(
    clippy::too_many_arguments,
    reason = "the state of one generation step"
)]
fn far_model_records(
    report: &mut ObjectReport,
    world: &str,
    plugins: &[String],
    instances: &[&Instance],
    models: &BTreeMap<String, Option<Model>>,
    unreadable: &BTreeSet<String>,
    no_solid_shape: &BTreeSet<String>,
    withheld: &dyn Fn(&Instance) -> bool,
    assets: &dyn Assets,
) {
    // (kind, plugin index) -> full model path -> objects
    let mut gaps: BTreeMap<(&'static str, usize), BTreeMap<String, u64>> = BTreeMap::new();
    let mut full_found: BTreeMap<String, bool> = BTreeMap::new();
    for i in instances {
        let lod = lod_model_path(&i.model);
        // a withheld object is reported as it would be without the shape that was made for other objects only
        let withheld = withheld(i);
        match models.get(&lod) {
            None => continue,
            Some(Some(_)) if !withheld => continue,
            _ => {}
        }
        let full = full_model_path(&i.model);
        let kind = if unreadable.contains(&lod) {
            "far_model_unreadable"
        } else if no_solid_shape.contains(&lod) && !withheld {
            "far_model_no_solid_shape"
        } else if *full_found
            .entry(full.clone())
            .or_insert_with(|| assets.read(&full).is_some())
        {
            "far_model_without_lod_model"
        } else {
            "far_model_not_found"
        };
        *gaps
            .entry((kind, i.flagged_by))
            .or_default()
            .entry(full)
            .or_default() += 1;
    }
    for ((kind, plugin), by_model) in gaps {
        let name = plugins
            .get(plugin)
            .cloned()
            .unwrap_or_else(|| format!("plugin number {plugin}"));
        let objects: u64 = by_model.values().sum();
        let shown: Vec<&str> = by_model.keys().take(5).map(String::as_str).collect();
        let more = by_model.len().saturating_sub(shown.len());
        let list = if more > 0 {
            format!("{} and {more} more", shown.join(", "))
        } else {
            shown.join(", ")
        };
        let what = match kind {
            "far_model_unreadable" => "their LOD models cannot be read",
            "far_model_not_found" => "neither a LOD model nor the full model is in the load order",
            "far_model_no_solid_shape" => {
                "every shape of their full models has an alpha property or is drawn without lighting"
            }
            _ => "no LOD model exists for them in the game or any mod",
        };
        let message = format!(
            "{name} marks {objects} objects of {} models visible from far, but {what}, so the objects are left out of the far view ({list})",
            by_model.len()
        );
        report.records.push(WarningRecord::new(
            kind,
            Some(world),
            Some(&name),
            by_model.keys().cloned(),
            Some(objects),
            message.clone(),
        ));
        report.warnings.push(message);
    }
}

/// Fills in the models without a LOD model with simplified copies of their full models, and returns the LOD paths of
/// those whose full model has no solid shape, and of those made for some of their objects only. `limits_of` gives the
/// limits for one object, or `None` for an object that gets no such shape; a model takes the limits of the first object
/// a rule names (`rule_shapes`), else of the first other object. A static collection whose parts are drawn keeps its
/// parts: a copy of its own model would draw it twice.
#[allow(
    clippy::too_many_arguments,
    reason = "the state of one generation step"
)]
fn shapes_from_full_models(
    report: &mut ObjectReport,
    world: &str,
    instances: &[&Instance],
    models: &mut BTreeMap<String, Option<Model>>,
    unreadable: &BTreeSet<String>,
    full_of: &BTreeMap<String, String>,
    assets: &dyn Assets,
    surfaces: &Surfaces<'_>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let with_parts: BTreeSet<u32> = instances
        .iter()
        .filter(|i| i.part)
        .map(|i| i.reference)
        .collect();
    let candidates: Vec<&&Instance> = instances
        .iter()
        .filter(|i| i.part || !with_parts.contains(&i.reference))
        .filter(|i| {
            let lod = lod_model_path(&i.model);
            models.get(&lod).is_some_and(Option::is_none) && !unreadable.contains(&lod)
        })
        .collect();
    let mut wanted: BTreeMap<String, FarLimits> = BTreeMap::new();
    if let Some(rule_shapes) = surfaces.rule_shapes {
        for i in &candidates {
            if let Some(limits) = rule_shapes(i) {
                wanted.entry(lod_model_path(&i.model)).or_insert(limits);
            }
        }
    }
    if let Some(limits) = surfaces.full_models {
        for i in &candidates {
            wanted.entry(lod_model_path(&i.model)).or_insert(limits);
        }
    }
    // the models made for some of their objects only: with `full_models` set every object may have one
    let mut partial: BTreeSet<String> = BTreeSet::new();
    if surfaces.full_models.is_none()
        && let Some(rule_shapes) = surfaces.rule_shapes
    {
        for i in instances {
            let lod = lod_model_path(&i.model);
            if wanted.contains_key(&lod) && rule_shapes(i).is_none() {
                partial.insert(lod);
            }
        }
    }
    let wanted: Vec<(String, FarLimits)> = wanted.into_iter().collect();
    let glow = surfaces.glow;
    let made = par_map(&wanted, surfaces.threads, |(lod, limits)| {
        let bytes = assets.read(full_of.get(lod)?)?;
        let parts = parse_lod_model(&bytes).ok()?;
        // the shapes drawn without lighting are effects and are left out (with far glow they are glow cards); what
        // is left must have a solid shape or be glow cards alone
        let shapes = || parts.iter().filter(|p| !p.triangles.is_empty());
        let cards = shapes().any(|p| glow && p.unlit && p.glow_map.is_some());
        if !cards && shapes().filter(|p| !p.unlit).all(|p| p.alpha) {
            return Some(None);
        }
        // a model that cannot be simplified stays an ordinary model without a LOD model
        far_parts_from_full_model(parts, *limits, glow).map(Some)
    });
    let (mut simplified, mut no_solid_shape) = (BTreeSet::new(), BTreeSet::new());
    for ((lod, _), result) in wanted.into_iter().zip(made) {
        match result {
            Some(Some(parts)) => {
                simplified.insert(full_of[&lod].clone());
                models.insert(lod, Some(Model { parts }));
            }
            Some(None) => {
                no_solid_shape.insert(lod);
            }
            None => {}
        }
    }
    if !simplified.is_empty() {
        let message = format!(
            "{} models are drawn from far as simplified copies of their full models",
            simplified.len()
        );
        report.records.push(WarningRecord::new(
            "far_model_simplified",
            Some(world),
            None,
            simplified,
            None,
            message.clone(),
        ));
        report.warnings.push(message);
    }
    (no_solid_shape, partial)
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
    generate_objects_with_atlas_limit(
        world,
        instances,
        assets,
        out,
        surfaces,
        LARGEST_ATLAS,
        block_done,
    )
}

/// `image` with each colour weighted by its alpha and the alpha full: the light a texture blended by its alpha gives
/// off.
fn weighted_by_alpha(image: &Image) -> Image {
    let mut px = image.rgba().to_vec();
    for p in px.as_chunks_mut::<4>().0 {
        let a = u16::from(p[3]);
        for c in &mut p[..3] {
            *c = u8::try_from((u16::from(*c) * a + 127) / 255).unwrap_or(u8::MAX);
        }
        p[3] = 255;
    }
    Image::new(image.width(), image.height(), px).unwrap_or_else(|_| image.clone())
}

/// The glow atlas of one atlas page at half its size, as the normal atlas is: each texture's glow map in the texture's
/// rectangle, black where a texture has none or its glow map cannot be read. `None` when no texture on the page has a
/// glow map that can be read, so such a page gets no glow atlas.
fn glow_atlas(
    rects: &[(&String, &Rect)],
    glow_of: &HashMap<String, (String, bool)>,
    assets: &dyn Assets,
    atlas: &Atlas,
    threads: usize,
) -> Option<Image> {
    if !rects.iter().any(|(k, _)| glow_of.contains_key(*k)) {
        return None;
    }
    let (gw, gh) = ((atlas.width / 2).max(4), (atlas.height / 2).max(4));
    let halves = par_map(rects, threads, |(k, r)| {
        let half = Rect {
            x: r.x / 2,
            y: r.y / 2,
            w: (r.w / 2).max(1),
            h: (r.h / 2).max(1),
        };
        let (path, by_alpha) = glow_of.get(*k)?;
        let mut img = load_texture(assets, path)?;
        if *by_alpha {
            img = weighted_by_alpha(&img);
        }
        resize(&img, half.w, half.h, Filter::Box)
            .ok()
            .map(|small| (half, small))
    });
    if halves.iter().all(Option::is_none) {
        return None;
    }
    let mut glow = vec![0_u8; (gw * gh * 4) as usize];
    for px in glow.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
    for (half, small) in halves.iter().flatten() {
        blit(&mut glow, gw, small, half);
    }
    Image::new(gw, gh, glow).ok()
}

/// [`generate_objects_with_progress`] with atlases no larger than `largest` (width, height): the textures that do not
/// fit one such atlas at their native size go on further atlases, each with its own shapes in the blocks.
///
/// # Errors
/// As [`generate_objects`].
pub fn generate_objects_with_atlas_limit(
    world: &str,
    instances: &[Instance],
    assets: &dyn Assets,
    out: &Path,
    surfaces: Surfaces,
    largest: (u32, u32),
    block_done: &mut dyn FnMut(u64, u64),
) -> Result<ObjectReport, GenError> {
    layout::check_world_name(world)?;
    let phase = |name: &str| {
        if let Some(f) = surfaces.phase {
            f(name);
        }
    };
    let mut report = ObjectReport::default();
    phase("models");
    // models
    let mut models: BTreeMap<String, Option<Model>> = BTreeMap::new();
    // the LOD models that exist as a file but cannot be read (the others that are missing have no file)
    let mut unreadable: BTreeSet<String> = BTreeSet::new();
    // the full model each LOD model stands for, for a shape that names no texture
    let mut full_of: BTreeMap<String, String> = BTreeMap::new();
    for i in instances {
        let path = lod_model_path(&i.model);
        full_of
            .entry(path.clone())
            .or_insert_with(|| full_model_path(&i.model));
        models.entry(path.clone()).or_insert_with(|| {
            let bytes = assets.read(&path)?;
            let parsed = parse_lod_model(&bytes).ok();
            if parsed.is_none() {
                unreadable.insert(path.clone());
            }
            parsed.map(|parts| Model { parts })
        });
    }
    // a shape with no diffuse texture takes the texture of the full model it stands for, and says so; when the full
    // model has none either, its objects are drawn in a flat colour and that is said by the model's path (a user must
    // not get pale shapes silently). The models that are missing or cannot be read are said in one line with their
    // count and the first paths (many flagged bases have no LOD model in the game's own data, which is how the game's
    // own blocks are made too)
    for (path, model) in &mut models {
        let Some(m) = model else {
            continue;
        };
        if m.parts.iter().all(|p| p.diffuse.is_some()) {
            continue;
        }
        let full = full_of.get(path.as_str()).and_then(|f| {
            assets
                .read(f)
                .and_then(|b| parse_lod_model(&b).ok())
                .and_then(|parts| parts.into_iter().find_map(|p| p.diffuse))
                .map(|d| (f.clone(), d))
        });
        let message = if let Some((full_path, diffuse)) = full {
            for p in m.parts.iter_mut().filter(|p| p.diffuse.is_none()) {
                p.diffuse = Some(diffuse.clone());
            }
            format!(
                "model {path} has a shape with no diffuse texture, the texture {} of {full_path} is used",
                norm_key(&diffuse)
            )
        } else {
            format!(
                "model {path} has a shape with no diffuse texture and its full model names none, a flat colour is used"
            )
        };
        report.records.push(WarningRecord::new(
            "lod_shape_without_texture",
            Some(world),
            None,
            [path.clone()],
            None,
            message.clone(),
        ));
        report.warnings.push(message);
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
    // the references only the shipped far blocks draw: their shapes are cut out of those blocks
    let mut own_images: BTreeMap<String, Image> = BTreeMap::new();
    let mut stand_ins: Vec<Instance> = Vec::new();
    if let Some(source) = &surfaces.own {
        let drawn: Vec<&Instance> = instances
            .iter()
            .copied()
            .filter(|i| {
                models
                    .get(&lod_model_path(&i.model))
                    .is_some_and(Option::is_some)
            })
            .collect();
        let found = own_far::take(source, world, surfaces.grid_anchor, &drawn);
        own_far::warn(&mut report, world, source, &found);
        for t in found.taken {
            let mut parts = Vec::with_capacity(t.parts.len());
            for (part, key, image) in t.parts {
                own_images.entry(key).or_insert(image);
                parts.push(part);
            }
            models.insert(lod_model_path(&t.instance.model), Some(Model { parts }));
            stand_ins.push(t.instance);
        }
    }
    // a reference the shipped blocks cover is not also a far model that is missing
    let covered: BTreeSet<u32> = stand_ins.iter().map(|i| i.reference).collect();
    let instances: Vec<&Instance> = instances
        .into_iter()
        .filter(|i| {
            !(covered.contains(&i.reference)
                && models
                    .get(&lod_model_path(&i.model))
                    .is_none_or(Option::is_none))
        })
        .collect();
    // the authored LOD models that a rule replaces: set aside, and put back when no full-model shape is made for them
    let mut replaced: BTreeMap<String, Model> = BTreeMap::new();
    if let Some(replaces) = surfaces.rule_replaces {
        let mut every: BTreeMap<String, bool> = BTreeMap::new();
        for i in &instances {
            let lod = lod_model_path(&i.model);
            if models.get(&lod).is_some_and(Option::is_some) {
                *every.entry(lod).or_insert(true) &= replaces(i);
            }
        }
        for (lod, yes) in every {
            if yes && let Some(Some(model)) = models.insert(lod.clone(), None) {
                replaced.insert(lod, model);
            }
        }
    }
    let (no_solid_shape, partial) =
        if surfaces.full_models.is_some() || surfaces.rule_shapes.is_some() {
            shapes_from_full_models(
                &mut report,
                world,
                &instances,
                &mut models,
                &unreadable,
                &full_of,
                assets,
                &surfaces,
            )
        } else {
            (BTreeSet::new(), BTreeSet::new())
        };
    for (lod, model) in replaced {
        if let Some(slot @ None) = models.get_mut(&lod) {
            *slot = Some(model);
        }
    }
    // an object whose model was made from its full model for other objects only (a rule's `generate`) stays out
    let withheld = |i: &Instance| {
        !partial.is_empty()
            && partial.contains(&lod_model_path(&i.model))
            && surfaces.rule_shapes.is_some_and(|f| f(i).is_none())
    };
    far_model_records(
        &mut report,
        world,
        surfaces.plugins,
        &instances,
        &models,
        &unreadable,
        &no_solid_shape,
        &withheld,
        assets,
    );
    let instances: Vec<&Instance> = instances
        .into_iter()
        .filter(|i| !withheld(i))
        .chain(stand_ins.iter())
        .collect();
    if models.values().all(Option::is_none) {
        report.skipped = instances.len();
        return Ok(report);
    }
    // textures: key by the diffuse path of each shape
    phase("textures");
    let key_of = |p: &Part| match (&p.glow_map, p.unlit && surfaces.glow) {
        (Some(g), true) => format!("{CARD}{}", norm_key(g)),
        _ => p
            .diffuse
            .as_deref()
            .map_or_else(|| NONE.to_owned(), norm_key),
    };
    let mut diffuse_keys: BTreeSet<String> = BTreeSet::new();
    let mut normal_of: HashMap<String, String> = HashMap::new();
    // a glow map and whether its colour is weighted by its alpha (a glow card's texture is blended by its alpha)
    let mut glow_of: HashMap<String, (String, bool)> = HashMap::new();
    for model in models.values().flatten() {
        for p in &model.parts {
            let k = key_of(p);
            if let Some(n) = &p.normal_map {
                normal_of.entry(k.clone()).or_insert_with(|| norm_key(n));
            }
            if surfaces.glow
                && let Some(g) = &p.glow_map
            {
                glow_of
                    .entry(k.clone())
                    .or_insert_with(|| (norm_key(g), p.unlit));
            }
            diffuse_keys.insert(k);
        }
    }
    diffuse_keys.insert(NONE.to_owned());
    let mut images: BTreeMap<String, Image> = BTreeMap::new();
    let keys: Vec<&String> = diffuse_keys
        .iter()
        .filter(|k| !own_images.contains_key(*k))
        .collect();
    for (k, image) in &own_images {
        if diffuse_keys.contains(k) {
            images.insert(k.clone(), image.clone());
        }
    }
    let decoded = par_map(&keys, surfaces.threads, |k| match k.strip_prefix(CARD) {
        // a glow card is black by day: only its glow shows
        Some(glow) => {
            read_texture(assets, glow).map(|g| flat(g.width(), g.height(), [0, 0, 0, 255]))
        }
        None => read_texture(assets, k),
    });
    for (k, read) in keys.into_iter().zip(decoded) {
        match read {
            Ok(img) => {
                images.insert(k.clone(), img);
            }
            Err(fault) => {
                if k != NONE {
                    let message = texture_warning("texture", k, &fault, ", a flat colour is used");
                    let kind = match fault {
                        crate::assets::TextureFault::Missing => "texture_not_found",
                        crate::assets::TextureFault::Unreadable(_) => "texture_unreadable",
                    };
                    report.records.push(WarningRecord::new(
                        kind,
                        Some(world),
                        None,
                        [k.clone()],
                        None,
                        message.clone(),
                    ));
                    report.warnings.push(message);
                }
                images.insert(k.clone(), flat(8, 8, [128, 128, 128, 255]));
            }
        }
    }
    let sizes: Vec<(String, u32, u32)> = images
        .iter()
        .map(|(k, i)| (k.clone(), i.width(), i.height()))
        .collect();
    // every texture at its native size: several atlases when they do not fit one, as the oracle's output has
    // (halving every texture to fit one atlas blurred every far object of a large load order)
    phase("atlas");
    let atlases = pack_pages(&sizes, largest);
    // the pages that got a glow atlas
    let mut glowing = vec![false; atlases.len()];
    for (page, atlas) in atlases.iter().enumerate() {
        let rects: Vec<(&String, &Rect)> = atlas.rects.iter().collect();
        // the textures that need resizing, and the normal maps, are made on the worker threads; the pages are
        // filled in order afterwards
        let diffuse_small = par_map(&rects, surfaces.threads, |(k, r)| {
            let img = &images[*k];
            if (img.width(), img.height()) == (r.w, r.h) {
                None
            } else {
                resize(img, r.w, r.h, Filter::Box).ok()
            }
        });
        let mut diffuse = vec![0_u8; (atlas.width * atlas.height * 4) as usize];
        for ((k, r), small) in rects.iter().zip(&diffuse_small) {
            let img = &images[*k];
            if (img.width(), img.height()) == (r.w, r.h) {
                blit(&mut diffuse, atlas.width, img, r);
            } else if let Some(small) = small {
                blit(&mut diffuse, atlas.width, small, r);
            }
        }
        let (nw, nh) = ((atlas.width / 2).max(4), (atlas.height / 2).max(4));
        let mut normal = vec![0_u8; (nw * nh * 4) as usize];
        for px in normal.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&[128, 128, 255, 255]);
        }
        let halves = par_map(&rects, surfaces.threads, |(k, r)| {
            let half = Rect {
                x: r.x / 2,
                y: r.y / 2,
                w: (r.w / 2).max(1),
                h: (r.h / 2).max(1),
            };
            let img = normal_of.get(*k).and_then(|n| load_texture(assets, n))?;
            resize(&img, half.w, half.h, Filter::Box)
                .ok()
                .map(|small| (half, small))
        });
        for (half, small) in halves.iter().flatten() {
            blit(&mut normal, nw, small, half);
        }
        let [diffuse_path, normal_path] = layout::atlas_page_paths(world, page)?;
        let diffuse_image = Image::new(atlas.width, atlas.height, diffuse)?;
        let normal_image = Image::new(nw, nh, normal)?;
        report.files.push(write_file(
            out,
            &diffuse_path,
            &encode_bc3(&diffuse_image, surfaces.threads)?,
        )?);
        report.files.push(write_file(
            out,
            &normal_path,
            &encode_bc3(&normal_image, surfaces.threads)?,
        )?);
        if let Some(glow) = glow_atlas(&rects, &glow_of, assets, atlas, surfaces.threads) {
            report.files.push(write_file(
                out,
                &layout::atlas_page_glow_path(world, page)?,
                &encode_bc3(&glow, surfaces.threads)?,
            )?);
            glowing[page] = true;
        }
    }
    // blocks
    phase("blocks");
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
    let refs: Vec<[String; 2]> = (0..atlases.len())
        .map(|page| layout::nif_atlas_page_paths(world, page))
        .collect::<Result<_, _>>()?;
    let glow_refs: Vec<Option<String>> = glowing
        .iter()
        .enumerate()
        .map(|(page, on)| {
            on.then(|| layout::nif_atlas_page_glow_path(world, page))
                .transpose()
        })
        .collect::<Result<_, _>>()?;
    let pages: Vec<Page> = atlases
        .iter()
        .map(|atlas| Page {
            atlas,
            key_of: &key_of,
        })
        .collect();
    let total = groups.len() as u64;
    for (done, ((bx, by, high), mut members)) in (1_u64..).zip(groups) {
        members.sort_by_key(|i| (segment_of(i, (bx, by)), i.reference, i.base));
        let origin = (bx as f32 * CELL_UNITS, by as f32 * CELL_UNITS);
        // per atlas page, a block is one shape while its objects fit under the 16-bit vertex and triangle limits and
        // several shapes after that, as the game's blocks hold several shapes
        let mut chunks: Vec<Vec<Merged>> =
            (0..pages.len()).map(|_| vec![Merged::default()]).collect();
        let mut dropped = 0;
        for i in members {
            let segment = segment_of(i, (bx, by));
            let Some(model) = models
                .get(&lod_model_path(&i.model))
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let mut placed = true;
            for (page, shapes) in pages.iter().zip(chunks.iter_mut()) {
                let add = |m: &mut Merged| {
                    add_instance(m, model, &i.placement, origin, segment, page, surfaces)
                };
                let mut fits = shapes.last_mut().is_some_and(&add);
                if !fits && shapes.last().is_some_and(|m| !m.vertices.is_empty()) {
                    shapes.push(Merged::default());
                    fits = shapes.last_mut().is_some_and(&add);
                }
                placed &= fits;
            }
            if placed {
                report.instances += 1;
            } else {
                dropped += 1;
            }
        }
        if dropped > 0 {
            let message = format!(
                "block x{bx} y{by}: {dropped} objects left out, each alone has more vertices or triangles than one shape can hold"
            );
            report.records.push(WarningRecord::new(
                "block_objects_left_out",
                Some(world),
                None,
                [format!("x{bx} y{by}")],
                Some(dropped as u64),
                message.clone(),
            ));
            report.warnings.push(message);
        }
        // a high-priority instance is in its block's normal file too: count its faces once, from the normal file
        if !high {
            for m in chunks.iter().flatten() {
                report.dropped_under_terrain += m.dropped_under_terrain;
                report.dropped_under_water += m.dropped_under_water;
            }
        }
        for shapes in &mut chunks {
            shapes.retain(|m| !m.indices.is_empty());
        }
        let extra: usize = chunks.iter().map(|c| c.len().saturating_sub(1)).sum();
        let count: usize = chunks.iter().map(Vec::len).sum();
        if count == 0 {
            block_done(done, total);
            continue;
        }
        if extra > 0 {
            let message = format!(
                "block x{bx} y{by}: {extra} objects did not fit one shape, the block is written as {count} shapes"
            );
            report.records.push(WarningRecord::new(
                "block_split_into_shapes",
                Some(world),
                None,
                [format!("x{bx} y{by}")],
                Some(extra as u64),
                message.clone(),
            ));
            report.warnings.push(message);
        }
        let shapes: Vec<ObjectBlock> = chunks
            .iter()
            .zip(refs.iter().zip(&glow_refs))
            .flat_map(|(c, r)| c.iter().map(move |m| (m, r)))
            .map(|(m, ([diffuse_ref, normal_ref], glow_ref))| ObjectBlock {
                origin,
                vertices: &m.vertices,
                normals: &m.normals,
                tangents: &m.tangents,
                bitangents: &m.bitangents,
                uv: &m.uv,
                indices: &m.indices,
                segment_triangles: m.segment_triangles,
                textures: [diffuse_ref, normal_ref],
                glow: glow_ref.as_deref(),
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
