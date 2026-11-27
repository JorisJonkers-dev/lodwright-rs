// SPDX-License-Identifier: GPL-3.0-only
//! Far objects whose shape is taken from the far blocks the games ship.
//!
//! Some objects are drawn by a game's shipped far blocks although nothing in the plugins selects them and no far model
//! exists for them. The request names their models (`objects.shapes_from_shipped_blocks`); this crate holds no list. At
//! generation time, when the shipped blocks are present, the shape of each such reference is cut out of the block that
//! covers it and drawn again in the generated block with the piece of the shipped atlas it uses.
//!
//! Matching: a block holds pieces (triangles joined to each other), and one object can be several pieces whose box need
//! not hold the object's origin. A piece belongs to a reference when all its vertices lie inside the reference's
//! object bounds (`OBND`, or the full model's box where the record leaves them empty), turned and scaled as the reference is placed. When several boxes hold a piece the smallest
//! wins, and a piece won by an object the run already draws is left alone, so nothing is drawn twice.
//!
//! Nothing of the shipped files is stored or redistributed: the shapes are read from the user's own archives at
//! generation time, exactly as the game's own far diffuse tiles are read when they are kept.

use std::collections::{BTreeMap, HashMap};

use lodwright_core::layout;
use lodwright_core::objects::Instance;
use lodwright_dds::Image;
use lodwright_nif::{Body, NiTriShape, NifFile};

use crate::lodmodel::{Part, lighting_texture_set, parse_lod_model, texture_set};
use crate::objects::{BLOCK_CELLS, Mat, euler};

/// A source of the shipped far blocks and their atlases: the references to cut out and a reader of files by game
/// path (`meshes/...`, `textures/...`) that sees only the files the games ship, not a mod's.
#[derive(Clone, Copy)]
pub struct OwnSource<'a> {
    /// The references of the models the request names for this world
    /// (`lodwright_core::objects::references_of_models`).
    pub references: &'a [Instance],
    /// The object bounds of the base objects, by absolute `FormID` (`lodwright_core::objects::base_bounds`).
    pub bounds: &'a HashMap<u32, [i16; 6]>,
    /// The bytes of a shipped file, or `None` when there is none.
    pub read: &'a (dyn Fn(&str) -> Option<Vec<u8>> + Sync),
}

/// Slack, in the object's own units, around its bounds: the bounds are whole numbers and a far model may stand a little
/// proud of the full model.
const MARGIN: f32 = 32.0;

/// One shape of a shipped block, in world units.
struct OwnShape {
    vertices: Vec<[f32; 3]>,
    normals: Option<Vec<[f32; 3]>>,
    tangents: Option<(Vec<[f32; 3]>, Vec<[f32; 3]>)>,
    uv: Vec<[f32; 2]>,
    triangles: Vec<[u16; 3]>,
    diffuse: Option<String>,
}

/// The pieces of a shipped far block that stand for single objects, as far as the file tells.
struct OwnBlock {
    shapes: Vec<OwnShape>,
}

impl OwnBlock {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let file = NifFile::read_ignoring_trailing_bytes(bytes).ok()?;
        let profile = file.header.profile();
        let mut shapes = Vec::new();
        for block in &file.blocks {
            let name = file
                .header
                .block_types
                .get(usize::from(block.type_index))
                .map_or(&[][..], Vec::as_slice);
            let shape: NiTriShape = match &block.body {
                Body::TriShape(s) => s.clone(),
                Body::Opaque(bytes) if name == b"BSSegmentedTriShape" => {
                    let Ok((Body::TriShape(s), _)) = Body::parse(b"NiTriShape", bytes, profile)
                    else {
                        continue;
                    };
                    s
                }
                _ => continue,
            };
            let Ok(data) = usize::try_from(shape.data) else {
                continue;
            };
            let Some(Body::TriShapeData(d)) = file.blocks.get(data).map(|b| &b.body) else {
                continue;
            };
            let (Some(vertices), Some(triangles)) = (&d.geometry.vertices, &d.triangles) else {
                continue;
            };
            let at = shape.av.translation;
            let k = shape.av.scale;
            let world: Vec<[f32; 3]> = vertices
                .iter()
                .map(|v| [at[0] + k * v[0], at[1] + k * v[1], at[2] + k * v[2]])
                .collect();
            let n = world.len();
            let diffuse = shape_diffuse(&file, &shape);
            shapes.push(OwnShape {
                vertices: world,
                normals: d.geometry.normals.clone().filter(|v| v.len() == n),
                tangents: d
                    .geometry
                    .tangents
                    .clone()
                    .filter(|(t, b)| t.len() == n && b.len() == n),
                uv: d
                    .geometry
                    .uv_sets
                    .first()
                    .cloned()
                    .filter(|u| u.len() == n)
                    .unwrap_or_else(|| vec![[0.0; 2]; n]),
                triangles: triangles.clone(),
                diffuse,
            });
        }
        (!shapes.is_empty()).then_some(Self { shapes })
    }
}

fn shape_diffuse(file: &NifFile, shape: &NiTriShape) -> Option<String> {
    for p in &shape.av.properties {
        let Ok(p) = usize::try_from(*p) else {
            continue;
        };
        let Some(Body::Opaque(bytes)) = file.blocks.get(p).map(|b| &b.body) else {
            continue;
        };
        let set = lighting_texture_set(bytes)?;
        if let Some(Body::Opaque(set_bytes)) = file.blocks.get(set).map(|b| &b.body) {
            return texture_set(set_bytes).0;
        }
    }
    None
}

/// Triangles that share a vertex are one piece: a union-find over vertex numbers.
fn pieces(vertex_count: usize, triangles: &[[u16; 3]]) -> Vec<usize> {
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..vertex_count).collect();
    for t in triangles {
        let a = find(&mut parent, usize::from(t[0]));
        for &v in &t[1..] {
            let b = find(&mut parent, usize::from(v));
            if a != b {
                let a = find(&mut parent, a);
                parent[b] = a;
            }
        }
    }
    triangles
        .iter()
        .map(|t| find(&mut parent, usize::from(t[0])))
        .collect()
}

/// The block of the grid that holds a reference, as object generation assigns it.
fn block_of(i: &Instance, anchor: (i32, i32)) -> (i32, i32) {
    let cell = i.cell.unwrap_or_else(|| {
        let c = |v: f32| (v / 4096.0).floor() as i32;
        (c(i.placement.pos[0]), c(i.placement.pos[1]))
    });
    let (ax, ay) = anchor;
    (
        ax + (cell.0 - ax).div_euclid(BLOCK_CELLS) * BLOCK_CELLS,
        ay + (cell.1 - ay).div_euclid(BLOCK_CELLS) * BLOCK_CELLS,
    )
}

/// The box of a base object in its own units: its `OBND` when that has a size (many building-kit records leave it
/// zero), else the box of the full model the games ship.
struct Boxes<'a> {
    source: &'a OwnSource<'a>,
    models: HashMap<String, Option<([f32; 3], [f32; 3])>>,
}

impl Boxes<'_> {
    fn of(&mut self, i: &Instance) -> Option<([f32; 3], [f32; 3])> {
        if let Some(b) = self.source.bounds.get(&i.base) {
            let lo = [f32::from(b[0]), f32::from(b[1]), f32::from(b[2])];
            let hi = [f32::from(b[3]), f32::from(b[4]), f32::from(b[5])];
            if (0..3).all(|k| hi[k] > lo[k]) {
                return Some((lo, hi));
            }
        }
        let path = model_of(i);
        let read = self.source.read;
        *self
            .models
            .entry(path)
            .or_insert_with_key(|path| model_box(&read(path)?))
    }
}

/// The box of every vertex of a model, with the shapes' offsets.
fn model_box(bytes: &[u8]) -> Option<([f32; 3], [f32; 3])> {
    let parts = parse_lod_model(bytes).ok()?;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for part in &parts {
        for v in &part.vertices {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k] + part.offset[k]);
                hi[k] = hi[k].max(v[k] + part.offset[k]);
            }
        }
    }
    // a flat model (a sign, a wall panel) has a box with no depth: the margin gives it one
    (0..3).all(|k| hi[k] >= lo[k]).then_some((lo, hi))
}

/// A reference's object bounds where it stands.
struct Placed {
    pos: [f32; 3],
    turn: Mat,
    scale: f32,
    lo: [f32; 3],
    hi: [f32; 3],
    volume: f32,
    reference: u32,
}

impl Placed {
    fn of(i: &Instance, boxes: &mut Boxes) -> Option<Self> {
        let (lo, hi) = boxes.of(i)?;
        let scale = i.placement.scale;
        if scale.is_nan() || scale <= 0.0 {
            return None;
        }
        let volume = (0..3).map(|k| (hi[k] - lo[k]).max(1.0) * scale).product();
        Some(Self {
            pos: i.placement.pos,
            turn: euler(i.placement.rot),
            scale,
            lo,
            hi,
            volume,
            reference: i.reference,
        })
    }

    fn holds(&self, w: [f32; 3]) -> bool {
        let d = [w[0] - self.pos[0], w[1] - self.pos[1], w[2] - self.pos[2]];
        (0..3).all(|k| {
            // the rotation is orthonormal: its transpose turns a world offset back
            let local = (self.turn[0][k] * d[0] + self.turn[1][k] * d[1] + self.turn[2][k] * d[2])
                / self.scale;
            local >= self.lo[k] - MARGIN && local <= self.hi[k] + MARGIN
        })
    }
}

/// The triangles of one shape joined to each other.
struct Piece {
    shape: usize,
    triangles: Vec<[u16; 3]>,
    vertices: Vec<usize>,
}

fn pieces_of(block: &OwnBlock) -> Vec<Piece> {
    let mut out = Vec::new();
    for (s, shape) in block.shapes.iter().enumerate() {
        let comp = pieces(shape.vertices.len(), &shape.triangles);
        let mut by: BTreeMap<usize, Vec<[u16; 3]>> = BTreeMap::new();
        for (t, c) in shape.triangles.iter().zip(comp) {
            by.entry(c).or_default().push(*t);
        }
        for triangles in by.into_values() {
            let mut vertices: Vec<usize> = triangles
                .iter()
                .flatten()
                .map(|&v| usize::from(v))
                .collect();
            vertices.sort_unstable();
            vertices.dedup();
            out.push(Piece {
                shape: s,
                triangles,
                vertices,
            });
        }
    }
    out
}

/// One reference's shape, ready to place at the reference's own position with no rotation and unit scale.
pub(crate) struct Taken {
    /// A stand-in for the reference: same reference, cell and priority, no rotation, scale 1, a model path of its own.
    pub instance: Instance,
    /// Its pieces relative to the reference's position, each with the key of its cut-out of the shipped atlas and
    /// that cut-out.
    pub parts: Vec<(Part, String, Image)>,
}

/// What a run found.
#[derive(Default)]
pub(crate) struct Outcome {
    pub taken: Vec<Taken>,
    /// References whose block is not present, by model.
    pub no_block: BTreeMap<String, u64>,
    /// References whose block is present but holds no shape for them, by model.
    pub no_shape: BTreeMap<String, u64>,
}

fn model_of(i: &Instance) -> String {
    let m = i.model.to_ascii_lowercase().replace('\\', "/");
    format!("meshes/{}", m.strip_prefix("meshes/").unwrap_or(&m))
}

/// The warnings of a run: said once per kind, with every model and the count of references.
pub(crate) fn warn(
    report: &mut crate::objects::ObjectReport,
    world: &str,
    source: &OwnSource,
    found: &Outcome,
) {
    if source.references.is_empty() {
        return;
    }
    for (kind, by_model, what) in [
        (
            "own_far_blocks_not_found",
            &found.no_block,
            "the shipped far blocks that should hold them are not in the load order, so these objects are left out of the far view",
        ),
        (
            "own_far_objects_not_found",
            &found.no_shape,
            "the shipped far blocks hold no shape at these references, so they are left out of the far view",
        ),
    ] {
        if by_model.is_empty() {
            continue;
        }
        let count: u64 = by_model.values().sum();
        let message = format!(
            "{count} objects of {} models the request takes from shipped far blocks: {what}",
            by_model.len()
        );
        report.records.push(lodwright_contract::WarningRecord::new(
            kind,
            Some(world),
            None,
            by_model.keys().cloned(),
            Some(count),
            message.clone(),
        ));
        report.warnings.push(message);
    }
}

/// The model path of a stand-in (resolved by the generator's own `_lod` naming).
pub(crate) fn stand_in_model(reference: u32) -> String {
    format!("own-far/{reference:08x}.nif")
}

/// Cut the shape of every listed reference out of the shipped blocks. `drawn` are the instances the run already draws
/// from their own far models: a listed reference among them is not cut out, and a piece that stands in the smaller box
/// of one of them is theirs.
pub(crate) fn take(
    source: &OwnSource,
    world: &str,
    anchor: (i32, i32),
    drawn: &[&Instance],
) -> Outcome {
    let mut out = Outcome::default();
    let already: std::collections::BTreeSet<u32> = drawn.iter().map(|i| i.reference).collect();
    let mut wanted: BTreeMap<(i32, i32), Vec<&Instance>> = BTreeMap::new();
    for i in source
        .references
        .iter()
        .filter(|i| !already.contains(&i.reference))
    {
        wanted.entry(block_of(i, anchor)).or_default().push(i);
    }
    let mut sizes = Boxes {
        source,
        models: HashMap::new(),
    };
    let mut rivals: BTreeMap<(i32, i32), Vec<Placed>> = BTreeMap::new();
    for i in drawn {
        let key = block_of(i, anchor);
        if wanted.contains_key(&key)
            && let Some(p) = Placed::of(i, &mut sizes)
        {
            rivals.entry(key).or_default().push(p);
        }
    }
    let mut atlases: BTreeMap<String, Option<Image>> = BTreeMap::new();
    for ((bx, by), references) in wanted {
        let block = layout::object_mesh_path(world, bx, by, false)
            .ok()
            .and_then(|path| (source.read)(&path))
            .and_then(|bytes| OwnBlock::parse(&bytes));
        let Some(block) = block else {
            for i in references {
                *out.no_block.entry(model_of(i)).or_default() += 1;
            }
            continue;
        };
        let boxes: Vec<(usize, Placed)> = references
            .iter()
            .enumerate()
            .filter_map(|(n, i)| Placed::of(i, &mut sizes).map(|p| (n, p)))
            .collect();
        let others = rivals.get(&(bx, by)).map_or(&[][..], Vec::as_slice);
        let mut won: BTreeMap<usize, Vec<Piece>> = BTreeMap::new();
        for piece in pieces_of(&block) {
            let shape = &block.shapes[piece.shape];
            let inside = |p: &Placed| piece.vertices.iter().all(|&v| p.holds(shape.vertices[v]));
            let best = boxes
                .iter()
                .filter(|(_, p)| inside(p))
                .min_by(|(_, a), (_, b)| {
                    a.volume
                        .total_cmp(&b.volume)
                        .then(a.reference.cmp(&b.reference))
                });
            let Some((n, owner)) = best else { continue };
            if others.iter().any(|r| r.volume < owner.volume && inside(r)) {
                continue;
            }
            won.entry(*n).or_default().push(piece);
        }
        for (n, i) in references.iter().enumerate() {
            let mut parts = Vec::new();
            for piece in won.get(&n).map_or(&[][..], Vec::as_slice) {
                if let Some(part) = cut(&block.shapes[piece.shape], piece, i, source, &mut atlases)
                {
                    parts.push(part);
                }
            }
            if parts.is_empty() {
                *out.no_shape.entry(model_of(i)).or_default() += 1;
                continue;
            }
            let mut instance = (*i).clone();
            instance.model = stand_in_model(i.reference);
            instance.placement.rot = [0.0; 3];
            instance.placement.scale = 1.0;
            instance.part = false;
            out.taken.push(Taken { instance, parts });
        }
    }
    out
}

/// One piece as a model part relative to the reference, with its cut-out of the shipped atlas.
fn cut(
    shape: &OwnShape,
    piece: &Piece,
    i: &Instance,
    source: &OwnSource,
    atlases: &mut BTreeMap<String, Option<Image>>,
) -> Option<(Part, String, Image)> {
    let atlas_path = shape.diffuse.clone()?;
    let atlas = atlases
        .entry(atlas_path.clone())
        .or_insert_with(|| {
            let bytes = (source.read)(&atlas_path_for_read(&atlas_path))?;
            lodwright_dds::decode_image(&bytes).ok()
        })
        .as_ref()?;
    let order = &piece.vertices;
    let number = |v: u16| {
        order
            .binary_search(&usize::from(v))
            .ok()
            .and_then(|n| u16::try_from(n).ok())
    };
    let triangles = piece
        .triangles
        .iter()
        .map(|t| Some([number(t[0])?, number(t[1])?, number(t[2])?]))
        .collect::<Option<Vec<[u16; 3]>>>()?;
    let p = i.placement.pos;
    let vertices: Vec<[f32; 3]> = order
        .iter()
        .map(|&v| {
            let w = shape.vertices[v];
            [w[0] - p[0], w[1] - p[1], w[2] - p[2]]
        })
        .collect();
    let pick = |all: &Vec<[f32; 3]>| order.iter().map(|&v| all[v]).collect::<Vec<_>>();
    let uv_all: Vec<[f32; 2]> = order.iter().map(|&v| shape.uv[v]).collect();
    // the cut-out: the box of the piece's UVs, a pixel wider on every side
    let (w, h) = (atlas.width(), atlas.height());
    let lo_u = uv_all.iter().map(|u| u[0]).fold(f32::MAX, f32::min);
    let hi_u = uv_all.iter().map(|u| u[0]).fold(f32::MIN, f32::max);
    let lo_v = uv_all.iter().map(|u| u[1]).fold(f32::MAX, f32::min);
    let hi_v = uv_all.iter().map(|u| u[1]).fold(f32::MIN, f32::max);
    let x0 = ((lo_u.clamp(0.0, 1.0) * w as f32).floor() as u32).saturating_sub(1);
    let y0 = ((lo_v.clamp(0.0, 1.0) * h as f32).floor() as u32).saturating_sub(1);
    let x1 = (((hi_u.clamp(0.0, 1.0) * w as f32).ceil() as u32) + 1).min(w);
    let y1 = (((hi_v.clamp(0.0, 1.0) * h as f32).ceil() as u32) + 1).min(h);
    let (cw, ch) = (x1.saturating_sub(x0).max(1), y1.saturating_sub(y0).max(1));
    let mut px = Vec::with_capacity((cw * ch * 4) as usize);
    for y in y0..y0 + ch {
        for x in x0..x0 + cw {
            px.extend_from_slice(&atlas.pixel(x.min(w - 1), y.min(h - 1)).unwrap_or([128; 4]));
        }
    }
    let image = Image::new(cw, ch, px).ok()?;
    let uv: Vec<[f32; 2]> = uv_all
        .iter()
        .map(|u| {
            [
                ((u[0].clamp(0.0, 1.0) * w as f32 - x0 as f32) / cw as f32).clamp(0.0, 1.0),
                ((u[1].clamp(0.0, 1.0) * h as f32 - y0 as f32) / ch as f32).clamp(0.0, 1.0),
            ]
        })
        .collect();
    let key = format!(
        "own-far/{}/{x0}.{y0}.{cw}.{ch}",
        atlas_path.to_ascii_lowercase().replace('\\', "/")
    );
    let part = Part {
        vertices,
        offset: [0.0; 3],
        normals: shape.normals.as_ref().map(&pick),
        tangents: shape.tangents.as_ref().map(|(t, b)| (pick(t), pick(b))),
        uv,
        triangles,
        diffuse: Some(key.clone()),
        normal_map: None,
        glow_map: None,
        alpha: false,
        unlit: false,
    };
    Some((part, key, image))
}

/// A texture path as a shape's texture set states it (relative to `textures`, or with `textures` first) as a game path.
fn atlas_path_for_read(path: &str) -> String {
    let p = path.to_ascii_lowercase().replace('\\', "/");
    let p = p.strip_prefix("data/").unwrap_or(&p);
    if p.starts_with("textures/") {
        p.to_owned()
    } else {
        format!("textures/{p}")
    }
}
