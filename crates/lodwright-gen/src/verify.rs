// SPDX-License-Identifier: GPL-3.0-only
//! Structural validation of a generated LOD folder against the game's formats.
//!
//! Nothing here needs the game: it re-reads every file with the generic readers and checks the properties the
//! engine relies on (block structure, 16-bit index range, bounds, UVs, texture formats, names and paths). With a
//! reference folder (for example the game's own tree extracted somewhere) it also compares counts and structure.

#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the names are lower-cased before the comparison"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lodwright_core::lod::LodSettings;
use lodwright_dds::{Format, read_header};
use lodwright_nif::{Body, NifFile};

use crate::metrics::{AtlasCover, MetricShape, ShapeError, shape_error};

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file, relative to the folder.
    pub path: String,
    /// What is wrong.
    pub problem: String,
}

/// The result of a verification.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Verification {
    /// Terrain block meshes checked.
    pub terrain_blocks: usize,
    /// Object block meshes checked.
    pub object_blocks: usize,
    /// Tree block files checked.
    pub tree_blocks: usize,
    /// Textures checked.
    pub textures: usize,
    /// Settings files checked.
    pub settings: usize,
    /// Everything that is wrong, sorted.
    pub errors: Vec<Finding>,
    /// Differences from the reference folder, when one was given, sorted.
    pub differences: Vec<Finding>,
}

pub(crate) fn files_below(root: &Path) -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for e in entries {
            let rel = format!("{prefix}{}", e.file_name().to_string_lossy());
            if e.path().is_dir() {
                walk(&e.path(), &format!("{rel}/"), out);
            } else {
                out.push((rel, e.path()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out
}

/// Block coordinates from a file name such as `w.level32.x-64.y0.nif` or `w.n.level4.x4.y8.dds`.
fn parse_block(name: &str) -> Option<(u32, i32, i32, bool)> {
    let stem = name.rsplit_once('.')?.0;
    let high = stem.contains(".high.");
    let after = stem.split(".level").nth(1)?;
    let (level, rest) = after.split_once('.')?;
    let rest = rest.strip_prefix("high.").unwrap_or(rest);
    let (x, y) = rest.strip_prefix('x')?.split_once(".y")?;
    Some((level.parse().ok()?, x.parse().ok()?, y.parse().ok()?, high))
}

/// The translation of a scene-graph block (a node or a shape), read from a decoded body or from the opaque bytes
/// (name, extra data count and links, controller, flags, then three floats).
fn translation(body: &Body) -> Option<[f32; 3]> {
    match body {
        Body::Node(n) => Some(n.av.translation),
        Body::TriShape(s) | Body::TriStrips(s) => Some(s.av.translation),
        Body::Opaque(b) => {
            let count = usize::try_from(u32::from_le_bytes(b.get(4..8)?.try_into().ok()?)).ok()?;
            let at = 16_usize.checked_add(count.checked_mul(4)?)?;
            let f = |k: usize| -> Option<f32> {
                Some(f32::from_le_bytes(
                    b.get(at + 4 * k..at + 4 * k + 4)?.try_into().ok()?,
                ))
            };
            Some([f(0)?, f(1)?, f(2)?])
        }
        _ => None,
    }
}

/// The nodes and shapes of a block file with their type names and translations, in file order. The data blocks,
/// properties and the multibound are not scene-graph objects.
fn scene_translations(nif: &NifFile) -> Vec<(String, [f32; 3])> {
    nif.blocks
        .iter()
        .filter_map(|b| {
            let name =
                String::from_utf8_lossy(nif.header.block_types.get(usize::from(b.type_index))?)
                    .into_owned();
            let scene = name.ends_with("Node") || name.ends_with("TriShape");
            scene.then(|| translation(&b.body).map(|t| (name, t)))?
        })
        .collect()
}

struct Ctx {
    out: Verification,
}

impl Ctx {
    fn err(&mut self, path: &str, problem: impl Into<String>) {
        self.out.errors.push(Finding {
            path: path.to_owned(),
            problem: problem.into(),
        });
    }

    /// Where the block's origin sits is part of the format: a terrain block carries it on its root node and every other
    /// node has none, an object block carries it on its shape and its root has none (measured on every block of the
    /// game's own tree; the engine draws an object block only with the origin on the shape, #294).
    fn check_origin(&mut self, path: &str, nif: &NifFile, x: i32, y: i32, object: bool) {
        let origin = [x as f32 * 4096.0, y as f32 * 4096.0, 0.0];
        let nodes = scene_translations(nif);
        if nodes.is_empty() {
            self.err(path, "no node or shape");
            return;
        }
        for (i, (name, t)) in nodes.iter().enumerate() {
            let carrier = if object {
                name.ends_with("TriShape")
            } else {
                i == 0
            };
            // terrain water shapes carry their own offsets, so only the root of a terrain block is held to the format
            if !object && i != 0 {
                continue;
            }
            let want = if carrier { origin } else { [0.0; 3] };
            if t.iter().zip(want).any(|(a, b)| (a - b).abs() > 0.001) {
                self.err(
                    path,
                    format!(
                        "{name} has translation {t:?}, the format has {want:?} (the origin sits on the {})",
                        if object { "shape" } else { "root node" }
                    ),
                );
            }
        }
    }

    fn check_geometry(&mut self, path: &str, nif: &NifFile, size: Option<f32>, object: bool) {
        // a terrain block with a water plane has two more (`game-lod-water-shape`), and two more again with the empty
        // shape of `game-lod-water-placeholder`
        // an object block is the root, four blocks per shape, the multibound and its box (`game-lod-object-block-nif`;
        // the game's own blocks have one to four shapes)
        // and five more for a far lights shape (`objects.far_lights`: the shape, its two properties, data and marker)
        let expected: &[usize] = &[8, 10, 12];
        let lights = nif
            .blocks
            .iter()
            .filter(|b| matches!(&b.body, Body::TriShapeData(d) if is_far_lights_data(nif, d)))
            .count();
        let ok = if object {
            nif.blocks
                .len()
                .checked_sub(5 * lights)
                .is_some_and(|n| n >= 7 && (n - 3).is_multiple_of(4))
        } else {
            expected.contains(&nif.blocks.len())
        };
        if !ok {
            self.err(
                path,
                format!(
                    "{} blocks, expected {}",
                    nif.blocks.len(),
                    if object {
                        "3 plus 4 per shape, plus 5 for a far lights shape"
                    } else {
                        "8, 10 or 12"
                    }
                ),
            );
        }
        if nif.header.user_version != 11 || nif.header.bethesda.as_ref().map(|b| b.0) != Some(34) {
            self.err(path, "user versions are not 11 and 34");
        }
        let datas: Vec<_> = nif
            .blocks
            .iter()
            .filter_map(|b| match &b.body {
                Body::TriShapeData(d) => Some(d),
                _ => None,
            })
            .collect();
        if datas.is_empty() {
            self.err(path, "no triangle data block");
        }
        for data in datas {
            self.check_data(path, data, size, object, is_far_lights_data(nif, data));
        }
    }

    fn check_data(
        &mut self,
        path: &str,
        data: &lodwright_nif::NiTriShapeData,
        size: Option<f32>,
        object: bool,
        lights: bool,
    ) {
        let g = &data.geometry;
        let verts = g.vertices.as_deref().unwrap_or(&[]);
        if verts.is_empty() || verts.len() > 32767 && !object {
            self.err(path, format!("{} vertices", verts.len()));
        }
        let tris = data.triangles.as_deref().unwrap_or(&[]);
        if tris.is_empty() || tris.len() != usize::from(data.num_triangles) {
            self.err(path, "triangle count disagrees with the triangle list");
        }
        if tris
            .iter()
            .flatten()
            .any(|i| usize::from(*i) >= verts.len())
        {
            self.err(path, "a triangle index is outside the vertex list");
        }
        if verts.iter().flatten().any(|c| !c.is_finite()) {
            self.err(path, "a vertex coordinate is not finite");
        }
        if let Some(size) = size {
            // the game's water planes and their walls reach 32 units past the block's borders (`game-lod-water-walls`)
            let slack = 32.5;
            let outside = verts
                .iter()
                .filter(|v| {
                    v[0] < -slack || v[1] < -slack || v[0] > size + slack || v[1] > size + slack
                })
                .count();
            if outside > 0 {
                self.err(path, format!("{outside} vertices lie outside the block"));
            }
        }
        for set in &g.uv_sets {
            if set
                .iter()
                .any(|u| !(-0.001..=1.001).contains(&u[0]) || !(-0.001..=1.001).contains(&u[1]))
            {
                self.err(path, "a UV lies outside 0..1");
            }
        }
        // the far light points are unlit: no normals or tangents
        if object && !lights {
            let bad = g
                .normals
                .iter()
                .flatten()
                .filter(|n| ((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() > 0.01)
                .count();
            if bad > 0 {
                self.err(path, format!("{bad} normals are not unit length"));
            }
            if g.normals.is_none() || g.tangents.is_none() {
                self.err(path, "an object block needs normals and tangents");
            }
        }
        // the bounding sphere must contain every vertex
        let reach = verts
            .iter()
            .map(|v| {
                ((v[0] - g.center[0]).powi(2)
                    + (v[1] - g.center[1]).powi(2)
                    + (v[2] - g.center[2]).powi(2))
                .sqrt()
            })
            .fold(0.0_f32, f32::max);
        if reach > g.radius * 1.001 + 1.0 {
            self.err(path, "the bounding sphere does not contain every vertex");
        }
    }

    // source: xedit-tree-lod-files, game-tree-lod-dtl, game-tree-lod-lst
    fn check_trees(
        &mut self,
        names: &BTreeMap<String, &PathBuf>,
        lists: &[(String, Vec<u8>)],
        trees: &[(String, Vec<u8>)],
    ) {
        let mut types: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (rel, bytes) in lists {
            let world = rel.split('/').nth(3).unwrap_or("").to_ascii_lowercase();
            let Ok(list) = lodwright_core::trees::parse_types(bytes) else {
                self.err(rel, "not a readable tree type list");
                continue;
            };
            let mut seen = Vec::new();
            for t in &list {
                let [u0, v0, u1, v1] = t.uv;
                let in_range = [u0, v0, u1, v1].iter().all(|v| (0.0..=1.0).contains(v));
                if !in_range || u0 >= u1 || v0 >= v1 {
                    self.err(
                        rel,
                        format!("type {} has an atlas rectangle outside the atlas", t.index),
                    );
                }
                if !(t.width.is_finite() && t.height.is_finite())
                    || t.width <= 0.0
                    || t.height <= 0.0
                {
                    self.err(
                        rel,
                        format!("type {} has no usable billboard size", t.index),
                    );
                }
                if seen.contains(&t.index) {
                    self.err(rel, format!("type {} is listed twice", t.index));
                }
                seen.push(t.index);
            }
            let atlas = if world.starts_with("dlc4") {
                "treeswamplod"
            } else {
                "treedeadlod"
            };
            if !names.contains_key(&format!("textures/landscape/trees/{atlas}.dds")) {
                self.err(
                    rel,
                    format!("missing atlas textures/landscape/trees/{atlas}.dds"),
                );
            }
            types.insert(world, seen);
        }
        for (rel, bytes) in trees {
            let world = rel.split('/').nth(3).unwrap_or("").to_ascii_lowercase();
            let name = rel.rsplit('/').next().unwrap_or("");
            let Some((level, x, y, _)) = parse_block(name) else {
                self.err(rel, "the name is not a block name");
                continue;
            };
            if level != 8 {
                self.err(rel, "tree blocks are level 8");
            }
            let Ok(block) = lodwright_core::trees::parse_block(bytes) else {
                self.err(rel, "not a readable tree block");
                continue;
            };
            let Some(known) = types.get(&world) else {
                self.err(rel, "no treetypes.lst beside the block");
                continue;
            };
            let size = level as f32 * 4096.0;
            let (west, south) = (x as f32 * 4096.0, y as f32 * 4096.0);
            for (index, trees) in &block.groups {
                if !known.contains(index) {
                    self.err(rel, format!("type {index} is not in the list"));
                }
                for t in trees {
                    let inside = (west - 1024.0..west + size + 1024.0).contains(&t.pos[0])
                        && (south - 1024.0..south + size + 1024.0).contains(&t.pos[1]);
                    if !inside || !t.pos.iter().all(|v| v.is_finite()) {
                        self.err(rel, "a tree stands outside its block");
                        break;
                    }
                    let turn_ok = (0.0..=std::f32::consts::TAU + 1e-4).contains(&t.rotation);
                    if !turn_ok || t.scale.is_nan() || t.scale <= 0.0 {
                        self.err(rel, "a tree has a rotation outside 0 to 2 pi or no scale");
                        break;
                    }
                }
            }
        }
    }

    fn check_texture(&mut self, path: &str, bytes: &[u8], want: Option<(u32, u32, Format)>) {
        self.out.textures += 1;
        match read_header(bytes) {
            Err(e) => self.err(path, format!("not a readable DDS: {e}")),
            Ok(h) => {
                if let Some((w, hh, f)) = want
                    && (h.width, h.height, h.format) != (w, hh, f)
                {
                    self.err(
                        path,
                        format!(
                            "{}x{} {:?}, expected {w}x{hh} {f:?}",
                            h.width, h.height, h.format
                        ),
                    );
                }
                let full = h.width.max(h.height).max(1).ilog2() + 1;
                if h.mip_count != full {
                    self.err(
                        path,
                        format!("{} mips, a full chain has {full}", h.mip_count),
                    );
                }
            }
        }
    }
}

/// Verify the LOD folder `root`.
///
/// # Errors
/// Never for problems in the files: they are findings. An unreadable folder gives an empty verification.
#[must_use]
pub fn verify_folder(root: &Path) -> Verification {
    let mut ctx = Ctx {
        out: Verification::default(),
    };
    let files = files_below(root);
    let names: BTreeMap<String, &PathBuf> = files.iter().map(|(n, p)| (n.clone(), p)).collect();
    // the cell each world's block grid starts at is the south-west cell of its settings file (`game-lod-block-grid`)
    let anchors: BTreeMap<String, (i32, i32)> = files
        .iter()
        .filter_map(|(rel, path)| {
            let lower = rel.to_ascii_lowercase();
            let world = lower
                .strip_prefix("lodsettings/")?
                .strip_suffix(".dlodsettings")?
                .to_owned();
            let parsed = LodSettings::parse(&std::fs::read(path).ok()?).ok()?;
            Some((
                world,
                (
                    i32::from(parsed.south_west.0),
                    i32::from(parsed.south_west.1),
                ),
            ))
        })
        .collect();
    let mut terrain: Vec<(String, u32, i32, i32)> = Vec::new();
    let mut trees: Vec<(String, Vec<u8>)> = Vec::new();
    let mut lists: Vec<(String, Vec<u8>)> = Vec::new();
    for (rel, path) in &files {
        let lower = rel.to_ascii_lowercase();
        let Ok(bytes) = std::fs::read(path) else {
            ctx.err(rel, "cannot be read");
            continue;
        };
        if lower.starts_with("meshes/landscape/lod/") && lower.ends_with(".nif") {
            let Ok(nif) = NifFile::read(&bytes) else {
                ctx.err(rel, "not a readable NIF");
                continue;
            };
            if nif.write().ok().as_deref() != Some(&bytes[..]) {
                ctx.err(rel, "does not read back to the same bytes");
            }
            let object = lower.contains("/blocks/");
            let name = lower.rsplit('/').next().unwrap_or("");
            match parse_block(name) {
                None => ctx.err(rel, "the name is not a block name"),
                Some((level, x, y, _)) => {
                    let size = level as f32 * 4096.0;
                    let world = lower
                        .strip_prefix("meshes/landscape/lod/")
                        .and_then(|r| r.split('/').next())
                        .unwrap_or("");
                    // a world without a settings file has no start to check against (a world with no terrain
                    // still gets object blocks on its own grid, #245)
                    let (ax, ay) = anchors.get(world).copied().unwrap_or((x, y));
                    let step = i32::try_from(level).unwrap_or(1);
                    if (x - ax).rem_euclid(step) != 0 || (y - ay).rem_euclid(step) != 0 {
                        ctx.err(
                            rel,
                            "the coordinates are not on the world's grid (multiples of the level from its start)",
                        );
                    }
                    ctx.check_geometry(rel, &nif, (!object).then_some(size), object);
                    ctx.check_origin(rel, &nif, x, y, object);
                    if object {
                        ctx.out.object_blocks += 1;
                    } else {
                        ctx.out.terrain_blocks += 1;
                        terrain.push((rel.clone(), level, x, y));
                    }
                }
            }
        } else if lower.starts_with("textures/landscape/lod/") && lower.ends_with(".dds") {
            let want = if lower.contains("/diffuse/") || lower.contains("/normals/") {
                Some((256, 256, Format::Bc1))
            } else {
                None
            };
            ctx.check_texture(rel, &bytes, want);
        } else if lower.starts_with("meshes/landscape/lod/") && lower.contains("/trees/") {
            if lower.ends_with(".dtl") {
                ctx.out.tree_blocks += 1;
                trees.push((rel.clone(), bytes));
            } else if lower.ends_with("/treetypes.lst") {
                lists.push((rel.clone(), bytes));
            }
        } else if lower.starts_with("textures/landscape/trees/") && lower.ends_with(".dds") {
            ctx.check_texture(rel, &bytes, None);
        } else if lower.starts_with("lodsettings/") && lower.ends_with(".dlodsettings") {
            ctx.out.settings += 1;
            match LodSettings::parse(&bytes) {
                Err(e) => ctx.err(rel, e.to_string()),
                Ok(s) => {
                    // the width is the corner span plus one (a world without declared corners), or the span rounded up
                    // to a power of two (`game-lod-settings-extent`)
                    let dx = i32::from(s.north_east.0) - i32::from(s.south_west.0);
                    let dy = i32::from(s.north_east.1) - i32::from(s.south_west.1);
                    let span = u32::try_from(dx.max(dy)).unwrap_or(0);
                    let plain = u32::try_from(dx + 1).unwrap_or(0);
                    if s.width_cells != plain
                        && !(s.width_cells.is_power_of_two() && s.width_cells >= span)
                    {
                        ctx.err(rel, "the width disagrees with the corners");
                    }
                    // the game's own files cover at most 128 cells a side (a world with more land gets a window of
                    // 128, measured on the Capital Wasteland); a grid wider than that is a wrong extent
                    if s.width_cells > 128 {
                        ctx.err(
                            rel,
                            "the width is more than 128 cells, the most the game's own files cover",
                        );
                    }
                    if s.lowest_level != 4 || ![4, 8, 16, 32].contains(&s.highest_level) {
                        ctx.err(rel, "the level range is not 4 to 4, 8, 16 or 32");
                    }
                }
            }
        }
    }
    // every terrain block has both textures; a diffuse tile that the manifest does not name was left to the game's
    // own (`terrain.compose_shipped_tiles` off), a tile the manifest names must be there
    let manifest = std::fs::read_to_string(root.join("manifest.json")).ok();
    for (rel, level, x, y) in &terrain {
        let world = rel.split('/').nth(3).unwrap_or("");
        for kind in ["diffuse", "normals"] {
            let tex = format!(
                "textures/landscape/lod/{world}/{kind}/{world}.n.level{level}.x{x}.y{y}.dds"
            );
            let left_to_the_game = kind == "diffuse"
                && manifest
                    .as_ref()
                    .is_some_and(|m| !m.to_ascii_lowercase().contains(&tex));
            if !names.contains_key(&tex) && !left_to_the_game {
                ctx.err(rel, format!("missing texture {tex}"));
            }
        }
    }
    ctx.check_trees(&names, &lists, &trees);
    ctx.out
        .errors
        .sort_by(|a, b| (&a.path, &a.problem).cmp(&(&b.path, &b.problem)));
    ctx.out
}

/// Compare the structure of `folder` with `reference`: file names per kind and level, and per matching file the
/// block types and counts. Differences are listed; they are not errors by themselves.
#[must_use]
pub fn compare_folders(folder: &Path, reference: &Path) -> Vec<Finding> {
    let mine: BTreeMap<String, PathBuf> = files_below(folder)
        .into_iter()
        .map(|(n, p)| (n.to_ascii_lowercase(), p))
        .collect();
    let theirs: BTreeMap<String, PathBuf> = files_below(reference)
        .into_iter()
        .map(|(n, p)| (n.to_ascii_lowercase(), p))
        .collect();
    let mut out = Vec::new();
    for name in theirs.keys().filter(|n| !mine.contains_key(*n)) {
        out.push(Finding {
            path: name.clone(),
            problem: "missing from the folder".into(),
        });
    }
    for name in mine.keys().filter(|n| !theirs.contains_key(*n)) {
        out.push(Finding {
            path: name.clone(),
            problem: "not in the reference".into(),
        });
    }
    for (name, path) in &mine {
        let Some(other) = theirs.get(name) else {
            continue;
        };
        if name.ends_with(".dtl") {
            let (Ok(a), Ok(b)) = (std::fs::read(path), std::fs::read(other)) else {
                continue;
            };
            if let (Ok(a), Ok(b)) = (
                lodwright_core::trees::parse_block(&a),
                lodwright_core::trees::parse_block(&b),
            ) && a.len() != b.len()
            {
                out.push(Finding {
                    path: name.clone(),
                    problem: format!("{} trees, the reference has {}", a.len(), b.len()),
                });
            }
            continue;
        }
        if !name.ends_with(".nif") {
            continue;
        }
        let (Ok(a), Ok(b)) = (std::fs::read(path), std::fs::read(other)) else {
            continue;
        };
        let (Ok(a), Ok(b)) = (NifFile::read(&a), NifFile::read(&b)) else {
            continue;
        };
        if a.header.block_types != b.header.block_types {
            out.push(Finding {
                path: name.clone(),
                problem: "block types differ".into(),
            });
        }
        let (mut ta, mut tb) = (scene_translations(&a), scene_translations(&b));
        if !name.contains("/blocks/") {
            // a terrain block's water shapes are our own design (docs/spec/terrain-lod.md, section 6): its root only
            ta.truncate(1);
            tb.truncate(1);
        }
        if ta != tb {
            out.push(Finding {
                path: name.clone(),
                problem: format!("node translations differ: {ta:?}, the reference has {tb:?}"),
            });
        }
        if !name.contains("/blocks/")
            && let Some(problem) = water_difference(water_area(&a), water_area(&b))
        {
            out.push(Finding {
                path: name.clone(),
                problem,
            });
        }
    }
    out
}

/// A triangle seen from above: three corners, x and y.
type Flat = [[f64; 2]; 3];

/// A triangle with its heights.
type Tri3 = [[f64; 3]; 3];

/// The triangles of a terrain block's water plane with their heights: those of every data block after the terrain's.
fn water_triangles3(nif: &NifFile) -> Vec<Tri3> {
    nif.blocks
        .iter()
        .filter_map(|b| match &b.body {
            Body::TriShapeData(d) => Some(d),
            _ => None,
        })
        .skip(1)
        .flat_map(|d| {
            let v = d.geometry.vertices.as_deref().unwrap_or(&[]);
            d.triangles
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .filter_map(move |t| {
                    let [a, b, c] = t.map(|i| v.get(usize::from(i)).copied());
                    let point = |p: [f32; 3]| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
                    Some([point(a?), point(b?), point(c?)])
                })
        })
        .collect()
}

/// The triangles of a terrain block's water plane, seen from above: those of every data block after the terrain's.
fn water_triangles(nif: &NifFile) -> Vec<Flat> {
    water_triangles3(nif)
        .iter()
        .map(|t| t.map(|p| [p[0], p[1]]))
        .collect()
}

/// The triangles of a terrain block's ground (the first data block), corners with their heights.
fn ground_triangles(nif: &NifFile) -> Vec<[[f64; 3]; 3]> {
    let Some(d) = nif.blocks.iter().find_map(|b| match &b.body {
        Body::TriShapeData(d) => Some(d),
        _ => None,
    }) else {
        return Vec::new();
    };
    let v = d.geometry.vertices.as_deref().unwrap_or(&[]);
    let point = |p: [f32; 3]| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])];
    d.triangles
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter_map(|t| {
            let [a, b, c] = t.map(|i| v.get(usize::from(i)).copied());
            Some([point(a?), point(b?), point(c?)])
        })
        .collect()
}

/// The height of the ground at `(x, y)`: the plane of the first triangle that covers the point.
fn height_at(triangles: &[[[f64; 3]; 3]], x: f64, y: f64) -> Option<f64> {
    const EDGE: f64 = 1e-9;
    triangles.iter().find_map(|[a, b, c]| {
        let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if det.abs() < f64::EPSILON {
            return None;
        }
        let l1 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / det;
        let l2 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / det;
        let l3 = 1.0 - l1 - l2;
        (l1 >= -EDGE && l2 >= -EDGE && l3 >= -EDGE).then(|| l1 * a[2] + l2 * b[2] + l3 * c[2])
    })
}

/// Samples per edge of the grid over which terrain heights are compared.
const HEIGHT_GRID: usize = 33;

/// The largest and the mean absolute height difference of two terrain surfaces, sampled on a
/// [`HEIGHT_GRID`] by [`HEIGHT_GRID`] grid over the bounds of the first, at the points both surfaces cover. `None`
/// when they share no sampled point.
fn height_difference(mine: &[[[f64; 3]; 3]], theirs: &[[[f64; 3]; 3]]) -> Option<(f64, f64)> {
    let corners = || mine.iter().flatten();
    let bound = |axis: usize, pick: fn(f64, f64) -> f64, start: f64| {
        corners().map(|p| p[axis]).fold(start, pick)
    };
    let (x0, x1) = (
        bound(0, f64::min, f64::INFINITY),
        bound(0, f64::max, f64::NEG_INFINITY),
    );
    let (y0, y1) = (
        bound(1, f64::min, f64::INFINITY),
        bound(1, f64::max, f64::NEG_INFINITY),
    );
    if !(x1 > x0 && y1 > y0) {
        return None;
    }
    let (mut max, mut sum, mut n) = (0.0_f64, 0.0_f64, 0_u32);
    for i in 0..HEIGHT_GRID {
        for j in 0..HEIGHT_GRID {
            #[allow(clippy::cast_precision_loss, reason = "grid indices are small")]
            let at =
                |lo: f64, hi: f64, k: usize| lo + (hi - lo) * k as f64 / (HEIGHT_GRID - 1) as f64;
            let (x, y) = (at(x0, x1, i), at(y0, y1, j));
            if let (Some(a), Some(b)) = (height_at(mine, x, y), height_at(theirs, x, y)) {
                let d = (a - b).abs();
                max = max.max(d);
                sum += d;
                n += 1;
            }
        }
    }
    (n > 0).then(|| (max, sum / f64::from(n)))
}

/// The ground area a terrain block's water plane covers: the summed area of its triangles, seen from above.
fn water_area(nif: &NifFile) -> f64 {
    water_triangles(nif)
        .iter()
        .map(|[a, b, c]| {
            ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
        })
        .sum()
}

/// Grid cells per side of the water `IoU` sampling (the definition of `WaterReport` in the contract).
const IOU_GRID: usize = 400;

/// Mark the grid cells whose centre lies in one of `triangles` (edges included).
fn rasterise(triangles: &[Flat], min: [f64; 2], cell: [f64; 2], grid: &mut [bool]) {
    let n = IOU_GRID;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "grid indices are clamped to 0..400"
    )]
    let index = |value: f64, axis: usize| -> usize {
        (((value - min[axis]) / cell[axis]).floor().max(0.0) as usize).min(n - 1)
    };
    for t in triangles {
        let side = |p: [f64; 2], a: [f64; 2], b: [f64; 2]| {
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
        };
        let xs = t.map(|p| p[0]);
        let ys = t.map(|p| p[1]);
        let lo = |v: [f64; 3]| v.into_iter().fold(f64::INFINITY, f64::min);
        let hi = |v: [f64; 3]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
        for row in index(lo(ys), 1)..=index(hi(ys), 1) {
            for col in index(lo(xs), 0)..=index(hi(xs), 0) {
                #[allow(clippy::cast_precision_loss, reason = "at most 400")]
                let p = [
                    min[0] + (col as f64 + 0.5) * cell[0],
                    min[1] + (row as f64 + 0.5) * cell[1],
                ];
                let d = [
                    side(p, t[0], t[1]),
                    side(p, t[1], t[2]),
                    side(p, t[2], t[0]),
                ];
                let inside = d.iter().all(|v| *v >= 0.0) || d.iter().all(|v| *v <= 0.0);
                let degenerate = d.iter().all(|v| *v == 0.0);
                if inside && !degenerate {
                    grid[row * n + col] = true;
                }
            }
        }
    }
}

/// The `IoU` of the ground two water planes cover, seen from above: the share of the centres of a 400 by 400 grid over
/// the bounds of both planes that lie in both, out of those in either. 0 when only one side has water or the planes
/// cover no ground; 1 when neither has water.
#[must_use]
fn water_iou(mine: &[Flat], theirs: &[Flat]) -> f64 {
    match (mine.is_empty(), theirs.is_empty()) {
        (true, true) => return 1.0,
        (true, false) | (false, true) => return 0.0,
        (false, false) => {}
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for p in mine.iter().chain(theirs).flatten() {
        for axis in 0..2 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "400")]
    let cell =
        [0, 1].map(|axis| ((max[axis] - min[axis]) / IOU_GRID as f64).max(f64::MIN_POSITIVE));
    let mut a = vec![false; IOU_GRID * IOU_GRID];
    let mut b = vec![false; IOU_GRID * IOU_GRID];
    rasterise(mine, min, cell, &mut a);
    rasterise(theirs, min, cell, &mut b);
    let both = a.iter().zip(&b).filter(|(x, y)| **x && **y).count();
    let either = a.iter().zip(&b).filter(|(x, y)| **x || **y).count();
    if either == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "at most 160000")]
    let iou = both as f64 / either as f64;
    iou
}

/// How the water of one block is shaped beyond the ground it covers: how far down it reaches, how much of it is
/// sloped, and whether the two surfaces agree in height where both cover the ground.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterProfile {
    /// The highest minus the lowest vertex of the block's water, in world units (0 for a flat plane), ours.
    pub depth_mine: f64,
    /// The same for the reference.
    pub depth_theirs: f64,
    /// The area of the steep triangles (seen from above they cover less than a thousandth of their own area: the
    /// walls and slopes at the edge of the water), in square world units, ours.
    pub wall_mine: f64,
    /// The same for the reference.
    pub wall_theirs: f64,
    /// Of the grid cells both cover, the share whose top surface heights differ by at most 1 unit; `None` when the two
    /// cover no cell in common.
    pub height_agreement: Option<f64>,
    /// The water vertices the block carries in the file (every water shape but the empty one), ours.
    pub vertices_mine: f64,
    /// The same for the reference.
    pub vertices_theirs: f64,
}

fn wall_area(tris: &[Tri3]) -> f64 {
    tris.iter()
        .map(|t| {
            let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
            let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let full = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() / 2.0;
            let plan = n[2].abs() / 2.0;
            if full > 0.0 && plan < full * 1.0e-3 {
                full
            } else {
                0.0
            }
        })
        .fold(0.0, |total, area| total + area)
}

/// The vertices the water shapes of a block carry in the file (every data block after the terrain's, not counting the
/// empty one-vertex shape): the number the game's blocks are counted by.
fn water_vertex_count(nif: &NifFile) -> f64 {
    let n: usize = nif
        .blocks
        .iter()
        .filter_map(|b| match &b.body {
            Body::TriShapeData(d) => Some(d),
            _ => None,
        })
        .skip(1)
        .map(|d| d.geometry.vertices.as_deref().map_or(0, <[_]>::len))
        .filter(|n| *n > 1)
        .sum();
    #[allow(clippy::cast_precision_loss, reason = "a few thousand")]
    let n = n as f64;
    n
}

fn depth_of(tris: &[Tri3]) -> f64 {
    let heights = tris.iter().flatten().map(|p| p[2]);
    let (low, high) = heights.fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), z| {
        (l.min(z), h.max(z))
    });
    if low.is_finite() { high - low } else { 0.0 }
}

/// Mark the grid cells whose centre lies in a triangle with the top surface height of each (the highest triangle).
fn rasterise_heights(triangles: &[Tri3], min: [f64; 2], cell: [f64; 2], top: &mut [f64]) {
    let n = IOU_GRID;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "grid indices are clamped to 0..400"
    )]
    let index = |value: f64, axis: usize| -> usize {
        (((value - min[axis]) / cell[axis]).floor().max(0.0) as usize).min(n - 1)
    };
    for t in triangles {
        let area =
            (t[1][0] - t[0][0]) * (t[2][1] - t[0][1]) - (t[1][1] - t[0][1]) * (t[2][0] - t[0][0]);
        if area == 0.0 {
            continue;
        }
        let zs = t.map(|p| p[2]);
        let xs = t.map(|p| p[0]);
        let ys = t.map(|p| p[1]);
        let lo = |v: [f64; 3]| v.into_iter().fold(f64::INFINITY, f64::min);
        let hi = |v: [f64; 3]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
        for row in index(lo(ys), 1)..=index(hi(ys), 1) {
            for col in index(lo(xs), 0)..=index(hi(xs), 0) {
                #[allow(clippy::cast_precision_loss, reason = "at most 400")]
                let p = [
                    min[0] + (col as f64 + 0.5) * cell[0],
                    min[1] + (row as f64 + 0.5) * cell[1],
                ];
                let w0 = (t[1][0] - p[0]) * (t[2][1] - p[1]) - (t[1][1] - p[1]) * (t[2][0] - p[0]);
                let w1 = (t[2][0] - p[0]) * (t[0][1] - p[1]) - (t[2][1] - p[1]) * (t[0][0] - p[0]);
                let w2 = (t[0][0] - p[0]) * (t[1][1] - p[1]) - (t[0][1] - p[1]) * (t[1][0] - p[0]);
                let inside = [w0, w1, w2].iter().all(|w| *w * area >= 0.0);
                if !inside {
                    continue;
                }
                let z = (w0 * zs[0] + w1 * zs[1] + w2 * zs[2]) / area;
                let at = row * n + col;
                if top[at].is_nan() || z > top[at] {
                    top[at] = z;
                }
            }
        }
    }
}

/// The [`WaterProfile`] of one block's two water planes.
#[must_use]
fn water_profile(mine: &[Tri3], theirs: &[Tri3]) -> WaterProfile {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for p in mine.iter().chain(theirs).flatten() {
        for axis in 0..2 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    let mut out = WaterProfile {
        depth_mine: depth_of(mine),
        depth_theirs: depth_of(theirs),
        wall_mine: wall_area(mine),
        wall_theirs: wall_area(theirs),
        height_agreement: None,
        vertices_mine: 0.0,
        vertices_theirs: 0.0,
    };
    if !min[0].is_finite() {
        return out;
    }
    #[allow(clippy::cast_precision_loss, reason = "400")]
    let cell =
        [0, 1].map(|axis| ((max[axis] - min[axis]) / IOU_GRID as f64).max(f64::MIN_POSITIVE));
    let size = IOU_GRID * IOU_GRID;
    let (mut top_a, mut top_b) = (vec![f64::NAN; size], vec![f64::NAN; size]);
    rasterise_heights(mine, min, cell, &mut top_a);
    rasterise_heights(theirs, min, cell, &mut top_b);
    let both: Vec<(f64, f64)> = top_a
        .iter()
        .zip(&top_b)
        .filter(|(a, b)| !a.is_nan() && !b.is_nan())
        .map(|(a, b)| (*a, *b))
        .collect();
    if !both.is_empty() {
        #[allow(clippy::cast_precision_loss, reason = "at most 160000")]
        let agree =
            both.iter().filter(|(a, b)| (a - b).abs() <= 1.0).count() as f64 / both.len() as f64;
        out.height_agreement = Some(agree);
    }
    out
}

/// How far a vertex may lie from the reference's nearest one and still count as matched.
pub const OBJECT_TOLERANCE: f32 = 0.05;

/// The vertices of `mine` with no vertex of `theirs` within [`OBJECT_TOLERANCE`], in any order.
fn unmatched_vertices(mine: &[[f32; 3]], theirs: &[[f32; 3]]) -> usize {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "block-local coordinates divided by 0.05 fit in i64"
    )]
    let key = |v: [f32; 3]| v.map(|c| (f64::from(c) / f64::from(OBJECT_TOLERANCE)).floor() as i64);
    let mut cells: std::collections::HashMap<[i64; 3], Vec<[f32; 3]>> =
        std::collections::HashMap::new();
    for v in theirs {
        cells.entry(key(*v)).or_default().push(*v);
    }
    let near = |a: [f32; 3], b: [f32; 3]| {
        let d: f64 = (0..3)
            .map(|i| (f64::from(a[i]) - f64::from(b[i])).powi(2))
            .sum();
        d.sqrt() <= f64::from(OBJECT_TOLERANCE)
    };
    mine.iter()
        .filter(|v| {
            let k = key(**v);
            let found = (-1..=1).any(|dx| {
                (-1..=1).any(|dy| {
                    (-1..=1).any(|dz| {
                        cells
                            .get(&[k[0] + dx, k[1] + dy, k[2] + dz])
                            .is_some_and(|list| list.iter().any(|w| near(**v, *w)))
                    })
                })
            });
            !found
        })
        .count()
}

/// The water of one terrain block compared with the reference's.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterOverlap {
    /// The block mesh, relative to the folder, lower case.
    pub path: String,
    /// The `IoU` of the covered ground, unrounded.
    pub iou: f64,
    /// The shape of the water beyond the ground it covers.
    pub profile: WaterProfile,
}

/// One object block compared with the reference's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectAgreement {
    /// The block mesh, relative to the folder, lower case.
    pub path: String,
    /// Vertices of the block (all shapes).
    pub vertices: usize,
    /// Vertices of the reference's block.
    pub reference_vertices: usize,
    /// Triangles of the block.
    pub triangles: usize,
    /// Triangles of the reference's block.
    pub reference_triangles: usize,
    /// Vertices with no reference vertex within [`OBJECT_TOLERANCE`].
    pub unmatched_vertices: usize,
}

impl ObjectAgreement {
    /// Equal counts and every vertex matched.
    #[must_use]
    pub fn exact(&self) -> bool {
        self.vertices == self.reference_vertices
            && self.triangles == self.reference_triangles
            && self.unmatched_vertices == 0
    }
}

/// The terrain heights of one block compared with the reference's.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainHeightDifference {
    /// The block mesh, relative to the folder, lower case.
    pub path: String,
    /// The largest absolute height difference over the sampled points, in game units, unrounded.
    pub max: f64,
    /// The mean absolute height difference over the sampled points, in game units, unrounded.
    pub mean: f64,
}

/// Per-block measurements of a folder against a reference.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockComparison {
    /// Terrain blocks present on both sides whose surfaces share ground, sorted by path.
    pub terrain: Vec<TerrainHeightDifference>,
    /// Terrain blocks present on both sides with water on either, sorted by path.
    pub water: Vec<WaterOverlap>,
    /// Object blocks present on both sides, sorted by path.
    pub objects: Vec<ObjectAgreement>,
    /// The error of each object block present on both sides against the reference's, sorted by path.
    pub far_errors: Vec<FarBlockError>,
    /// How much of each object atlas of `folder` its blocks use, sorted by path.
    pub atlases: Vec<AtlasUse>,
}

/// The error of one object block against the reference's.
#[derive(Debug, Clone, PartialEq)]
pub struct FarBlockError {
    /// The block mesh, relative to the folder, lower case.
    pub path: String,
    /// The numbers.
    pub error: ShapeError,
}

/// How much of one object atlas the blocks of a folder use.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasUse {
    /// The atlas, relative to the folder, lower case.
    pub path: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The share of the atlas some triangle covers, 0 to 1.
    pub used: f64,
}

/// A texture path as a block's texture set writes it, as a key of [`atlas_sizes`].
fn texture_key(path: &str) -> String {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    // the game's blocks write the path from the game folder, `Data\\Textures\\...`
    let lower = lower.strip_prefix("data/").unwrap_or(&lower);
    if lower.starts_with("textures/") {
        lower.to_owned()
    } else {
        format!("textures/{lower}")
    }
}

/// The size of every object atlas below `root`, by lower-case path; a file that cannot be read is left out.
fn atlas_sizes(root: &Path) -> BTreeMap<String, (u32, u32)> {
    files_below(root)
        .into_iter()
        .map(|(n, p)| (n.replace('\\', "/").to_ascii_lowercase(), p))
        .filter(|(n, _)| {
            n.starts_with("textures/landscape/lod/")
                && n.contains("/blocks/")
                && n.ends_with(".dds")
        })
        .filter_map(|(n, p)| {
            let header = read_header(&std::fs::read(p).ok()?).ok()?;
            Some((n, (header.width, header.height)))
        })
        .collect()
}

/// The shapes of an object block in world units, each with its atlas size when `sizes` knows it. Empty for a block
/// that is not laid out like the object blocks lodwright writes.
fn metric_shapes(nif: &NifFile, sizes: &BTreeMap<String, (u32, u32)>) -> Vec<MetricShape> {
    lodwright_nif::read_object_shapes(nif)
        .unwrap_or_default()
        .into_iter()
        .map(|s| MetricShape {
            positions: s
                .vertices
                .iter()
                .map(|v| [0, 1, 2].map(|i| f64::from(v[i]) + f64::from(s.origin[i])))
                .collect(),
            atlas: sizes.get(&texture_key(&s.texture)).copied(),
            uv: s.uv,
            triangles: s.triangles,
        })
        .collect()
}

/// How much of each atlas the object blocks in `files` use.
fn atlas_use(
    files: &BTreeMap<String, PathBuf>,
    sizes: &BTreeMap<String, (u32, u32)>,
) -> Vec<AtlasUse> {
    let mut covers: BTreeMap<String, AtlasCover> = BTreeMap::new();
    for (_, path) in files.iter().filter(|(n, _)| n.contains("/blocks/")) {
        let Some(nif) = std::fs::read(path)
            .ok()
            .and_then(|b| NifFile::read(&b).ok())
        else {
            continue;
        };
        for shape in lodwright_nif::read_object_shapes(&nif).unwrap_or_default() {
            let key = texture_key(&shape.texture);
            let Some(size) = sizes.get(&key) else {
                continue;
            };
            let metric = MetricShape {
                uv: shape.uv,
                triangles: shape.triangles,
                ..MetricShape::default()
            };
            covers
                .entry(key)
                .or_insert_with(|| AtlasCover::new(*size))
                .add(&metric);
        }
    }
    covers
        .into_iter()
        .map(|(path, cover)| {
            let (width, height) = sizes[&path];
            AtlasUse {
                used: cover.used(),
                path,
                width,
                height,
            }
        })
        .collect()
}

/// Whether `data` is the data of a far lights shape (`objects.far_lights`): the file carries the marker name and the
/// data has vertex colours and no normals, which no object shape has.
fn is_far_lights_data(nif: &NifFile, data: &lodwright_nif::NiTriShapeData) -> bool {
    data.geometry.normals.is_none()
        && data.geometry.vertex_colors.is_some()
        && nif
            .header
            .strings
            .iter()
            .any(|s| s.as_slice() == lodwright_nif::FAR_LIGHTS_MARKER.as_bytes())
}

/// The vertices and triangle count of a block's object shapes (a far lights shape is not counted).
fn all_vertices_and_triangles(nif: &NifFile) -> (Vec<[f32; 3]>, usize) {
    let mut vertices = Vec::new();
    let mut triangles = 0;
    for b in &nif.blocks {
        if let Body::TriShapeData(d) = &b.body
            && !is_far_lights_data(nif, d)
        {
            vertices.extend_from_slice(d.geometry.vertices.as_deref().unwrap_or(&[]));
            triangles += d.triangles.as_deref().map_or(0, <[_]>::len);
        }
    }
    (vertices, triangles)
}

/// Measure every block mesh present in both `folder` and `reference` (names compared in lower case): the water `IoU`
/// of each terrain block with water on either side, and the agreement of each object block. A file that cannot be
/// read on either side is skipped; [`verify_folder`] and [`compare_folders`] report those.
#[must_use]
pub fn compare_blocks(folder: &Path, reference: &Path) -> BlockComparison {
    let lower = |root: &Path| -> BTreeMap<String, PathBuf> {
        files_below(root)
            .into_iter()
            .map(|(n, p)| (n.to_ascii_lowercase(), p))
            .filter(|(n, _)| n.starts_with("meshes/landscape/lod/") && n.ends_with(".nif"))
            .collect()
    };
    let (mine, theirs) = (lower(folder), lower(reference));
    let (my_sizes, their_sizes) = (atlas_sizes(folder), atlas_sizes(reference));
    let mut out = BlockComparison {
        atlases: atlas_use(&mine, &my_sizes),
        ..BlockComparison::default()
    };
    for (name, path) in &mine {
        let Some(other) = theirs.get(name) else {
            continue;
        };
        let read = |p: &PathBuf| std::fs::read(p).ok().and_then(|b| NifFile::read(&b).ok());
        let (Some(a), Some(b)) = (read(path), read(other)) else {
            continue;
        };
        if name.contains("/blocks/") {
            let (va, ta) = all_vertices_and_triangles(&a);
            let (vb, tb) = all_vertices_and_triangles(&b);
            out.objects.push(ObjectAgreement {
                path: name.clone(),
                vertices: va.len(),
                reference_vertices: vb.len(),
                triangles: ta,
                reference_triangles: tb,
                unmatched_vertices: unmatched_vertices(&va, &vb),
            });
            if let Some(error) = shape_error(
                &metric_shapes(&a, &my_sizes),
                &metric_shapes(&b, &their_sizes),
            ) {
                out.far_errors.push(FarBlockError {
                    path: name.clone(),
                    error,
                });
            }
        } else {
            if let Some((max, mean)) =
                height_difference(&ground_triangles(&a), &ground_triangles(&b))
            {
                out.terrain.push(TerrainHeightDifference {
                    path: name.clone(),
                    max,
                    mean,
                });
            }
            let (wa, wb) = (water_triangles(&a), water_triangles(&b));
            if !wa.is_empty() || !wb.is_empty() {
                out.water.push(WaterOverlap {
                    path: name.clone(),
                    iou: water_iou(&wa, &wb),
                    profile: WaterProfile {
                        vertices_mine: water_vertex_count(&a),
                        vertices_theirs: water_vertex_count(&b),
                        ..water_profile(&water_triangles3(&a), &water_triangles3(&b))
                    },
                });
            }
        }
    }
    out
}

/// The band within which a generated water plane counts as equal to the reference's: half to twice its area.
/// The planes are our own design (see docs/spec/terrain-lod.md, section 6), so only the covered ground is compared.
fn water_difference(mine: f64, theirs: f64) -> Option<String> {
    match (mine > 0.0, theirs > 0.0) {
        (false, false) => None,
        (false, true) => Some("no water plane, the reference has one".into()),
        (true, false) => Some("a water plane the reference does not have".into()),
        (true, true) => {
            let ratio = mine / theirs;
            (!(0.5..=2.0).contains(&ratio)).then(|| {
                format!(
                    "the water plane covers {:.0} percent of the reference's area",
                    ratio * 100.0
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Flat, compare_blocks, parse_block, unmatched_vertices, verify_folder, water_difference,
        water_iou,
    };
    use crate::{Options, generate_terrain};
    use lodwright_core::land::CellHeights;
    use lodwright_core::terrain::TerrainSettings;
    use lodwright_core::worldspace::Worldspace;
    use lodwright_testkit::{covers, ids};

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lodwright-verify-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A folder with one object block (a square of 100 units at height `z`, over the left half of a 64 by 64 atlas).
    fn object_folder(name: &str, z: f32) -> std::path::PathBuf {
        let dir = temp(name);
        let blocks = dir.join("meshes/landscape/lod/w/blocks");
        let textures = dir.join("textures/landscape/lod/w/blocks");
        std::fs::create_dir_all(&blocks).unwrap();
        std::fs::create_dir_all(&textures).unwrap();
        let vertices = [
            [0.0, 0.0, z],
            [100.0, 0.0, z],
            [100.0, 100.0, z],
            [0.0, 100.0, z],
        ];
        let uv = [[0.0, 0.0], [0.5, 0.0], [0.5, 1.0], [0.0, 1.0]];
        let mut segment_triangles = [0; lodwright_nif::SEGMENTS];
        segment_triangles[0] = 2;
        // the paths as a generated block writes them
        let paths = lodwright_core::layout::nif_atlas_paths("w").unwrap();
        let nif = lodwright_nif::object_block(&lodwright_nif::ObjectBlock {
            origin: (0.0, 0.0),
            vertices: &vertices,
            normals: &[[0.0, 0.0, 1.0]; 4],
            tangents: &[[1.0, 0.0, 0.0]; 4],
            bitangents: &[[0.0, 1.0, 0.0]; 4],
            uv: &uv,
            indices: &[0, 1, 2, 0, 2, 3],
            segment_triangles,
            textures: [&paths[0], &paths[1]],
        })
        .unwrap();
        std::fs::write(blocks.join("w.level4.x0.y0.nif"), nif.write().unwrap()).unwrap();
        let image = lodwright_dds::Image::new(64, 64, vec![128; 64 * 64 * 4]).unwrap();
        std::fs::write(
            textures.join("w.buildings.dds"),
            crate::objects::encode_bc3(&image, 1).unwrap(),
        )
        .unwrap();
        dir
    }

    #[test]
    fn object_blocks_get_error_numbers_and_their_atlas_its_use() {
        covers(&[ids::verify::folder::OBJECT_AGREEMENT]);
        let (mine, theirs) = (object_folder("far-a", 7.0), object_folder("far-b", 0.0));
        let compared = compare_blocks(&mine, &theirs);
        let [block] = compared.far_errors.as_slice() else {
            panic!("{:?}", compared.far_errors);
        };
        assert!((block.error.position_max - 7.0).abs() < 1e-6, "{block:?}");
        assert_eq!(block.error.uv, Some(0.0), "the same detail on both sides");
        let [atlas] = compared.atlases.as_slice() else {
            panic!("{:?}", compared.atlases);
        };
        assert_eq!((atlas.width, atlas.height), (64, 64));
        assert!((atlas.used - 0.5).abs() < 0.05, "{atlas:?}");
        for dir in [mine, theirs] {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn block_names_parse_with_negative_coordinates_and_the_high_marker() {
        covers(&[ids::verify::folder::REFERENCE_BLOCKS_BY_NAME]);
        assert_eq!(
            parse_block("w.level32.x-64.y0.nif"),
            Some((32, -64, 0, false))
        );
        assert_eq!(
            parse_block("w.n.level4.x4.y-8.dds"),
            Some((4, 4, -8, false))
        );
        assert_eq!(
            parse_block("w.level4.high.x8.y12.nif"),
            Some((4, 8, 12, true))
        );
        assert_eq!(parse_block("buildings.dds"), None);
    }

    #[test]
    fn a_generated_terrain_folder_verifies_clean_and_damage_is_found() {
        covers(&[ids::verify::folder::STRUCTURE_CHECKS]);
        let world = Worldspace::from_cells(
            1,
            "Mojave",
            (0..4).flat_map(|x| (0..4).map(move |y| ((x, y), CellHeights::flat(20.0 + x as f32)))),
        );
        let out = temp("clean");
        generate_terrain(
            &world,
            &TerrainSettings::default(),
            &out,
            &Options::default(),
        )
        .unwrap();
        let v = verify_folder(&out);
        assert_eq!(v.errors, []);
        assert_eq!((v.terrain_blocks, v.settings), (4, 1));
        // damage: remove a texture, truncate a block
        std::fs::remove_file(
            out.join("textures/landscape/lod/mojave/normals/mojave.n.level4.x0.y0.dds"),
        )
        .unwrap();
        let block = out.join("meshes/landscape/lod/mojave/mojave.level8.x0.y0.nif");
        let bytes = std::fs::read(&block).unwrap();
        std::fs::write(&block, &bytes[..bytes.len() / 2]).unwrap();
        let v = verify_folder(&out);
        let text: Vec<String> = v
            .errors
            .iter()
            .map(|f| format!("{}: {}", f.path, f.problem))
            .collect();
        assert!(
            text.iter().any(|t| t.contains("missing texture")),
            "{text:?}"
        );
        assert!(
            text.iter().any(|t| t.contains("not a readable NIF")),
            "{text:?}"
        );
    }

    #[test]
    fn water_planes_count_as_equal_within_half_to_twice_the_area() {
        covers(&[ids::verify::folder::WATER_IOU]);
        assert_eq!(water_difference(0.0, 0.0), None);
        assert_eq!(water_difference(150.0, 100.0), None);
        assert!(water_difference(0.0, 1.0).unwrap().starts_with("no water"));
        assert!(water_difference(1.0, 0.0).unwrap().starts_with("a water"));
        assert!(
            water_difference(300.0, 100.0)
                .unwrap()
                .contains("300 percent")
        );
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Flat> {
        vec![
            [[x0, y0], [x1, y0], [x1, y1]],
            [[x0, y0], [x1, y1], [x0, y1]],
        ]
    }

    #[test]
    fn equal_water_planes_have_an_iou_of_one() {
        covers(&[ids::verify::folder::WATER_IOU]);
        let a = rect(0.0, 0.0, 4096.0, 4096.0);
        assert!((water_iou(&a, &a) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn disjoint_water_planes_have_an_iou_of_zero() {
        covers(&[ids::verify::folder::WATER_IOU]);
        let a = rect(0.0, 0.0, 100.0, 100.0);
        let b = rect(200.0, 0.0, 300.0, 100.0);
        assert!(water_iou(&a, &b).abs() < 1e-12);
    }

    #[test]
    fn planes_overlapping_by_half_have_an_iou_of_about_a_third() {
        covers(&[ids::verify::folder::WATER_IOU]);
        let a = rect(0.0, 0.0, 2.0, 1.0);
        let b = rect(1.0, 0.0, 3.0, 1.0);
        let iou = water_iou(&a, &b);
        assert!((iou - 1.0 / 3.0).abs() < 0.005, "{iou}");
    }

    #[test]
    fn water_on_one_side_only_has_an_iou_of_zero() {
        covers(&[ids::verify::folder::WATER_IOU]);
        assert!(water_iou(&rect(0.0, 0.0, 1.0, 1.0), &[]).abs() < 1e-12);
        assert!(water_iou(&[], &rect(0.0, 0.0, 1.0, 1.0)).abs() < 1e-12);
    }

    #[test]
    fn the_iou_does_not_depend_on_the_triangle_order_or_winding() {
        covers(&[ids::verify::folder::WATER_IOU]);
        let a = rect(0.0, 0.0, 2.0, 1.0);
        let mut b: Vec<Flat> = rect(1.0, 0.0, 3.0, 1.0)
            .into_iter()
            .rev()
            .map(|[p, q, r]| [r, q, p])
            .collect();
        let forward = water_iou(&a, &b);
        b.reverse();
        assert!((forward - water_iou(&a, &b)).abs() < 1e-12);
        assert!((forward - water_iou(&b, &a)).abs() < 1e-12);
    }

    #[test]
    fn the_same_vertices_in_another_order_all_match() {
        covers(&[ids::verify::folder::OBJECT_AGREEMENT]);
        let a: Vec<[f32; 3]> = (0..50)
            .map(|i| [i as f32 * 3.0, (i % 7) as f32, -(i as f32)])
            .collect();
        let mut b = a.clone();
        b.reverse();
        b.rotate_left(13);
        assert_eq!(unmatched_vertices(&a, &b), 0);
    }

    #[test]
    fn a_vertex_moved_beyond_the_tolerance_is_unmatched_and_within_it_is_not() {
        covers(&[ids::verify::folder::OBJECT_AGREEMENT]);
        let a = vec![[10.0_f32, 10.0, 10.0], [20.0, 20.0, 20.0]];
        let near = vec![[10.04_f32, 10.0, 10.0], [20.0, 20.0, 20.0]];
        let far = vec![[10.06_f32, 10.0, 10.0], [20.0, 20.0, 20.0]];
        assert_eq!(unmatched_vertices(&a, &near), 0);
        assert_eq!(unmatched_vertices(&a, &far), 1);
    }

    #[test]
    fn the_water_profile_sees_walls_depth_vertices_and_height_differences() {
        let flat: Vec<super::Tri3> = vec![
            [[0.0, 0.0, 5.0], [100.0, 0.0, 5.0], [0.0, 100.0, 5.0]],
            [[100.0, 0.0, 5.0], [100.0, 100.0, 5.0], [0.0, 100.0, 5.0]],
        ];
        // the same plane with a vertical wall under its south edge, down to -50
        let mut walled = flat.clone();
        walled.push([[0.0, 0.0, 5.0], [100.0, 0.0, 5.0], [0.0, 0.0, -50.0]]);
        walled.push([[100.0, 0.0, 5.0], [100.0, 0.0, -50.0], [0.0, 0.0, -50.0]]);
        let same = super::water_profile(&flat, &flat);
        assert_eq!(same.depth_mine.to_bits(), 0.0_f64.to_bits());
        assert_eq!(same.wall_mine.to_bits(), 0.0_f64.to_bits());
        assert_eq!(same.height_agreement, Some(1.0));
        let p = super::water_profile(&flat, &walled);
        assert!((p.depth_theirs - 55.0).abs() < 1e-9);
        // two triangles of 100 by 55, half each
        assert!((p.wall_theirs - 5500.0).abs() < 1e-6, "{}", p.wall_theirs);
        assert!(p.wall_mine.abs() < 1e-12);
        assert_eq!(p.height_agreement, Some(1.0));
        // a plane one unit and a half higher agrees nowhere
        let raised: Vec<super::Tri3> = flat
            .iter()
            .map(|t| t.map(|v| [v[0], v[1], v[2] + 1.5]))
            .collect();
        assert_eq!(
            super::water_profile(&flat, &raised).height_agreement,
            Some(0.0)
        );
        // one side without water
        let none = super::water_profile(&flat, &[]);
        assert_eq!(none.height_agreement, None);
        assert!(none.depth_theirs.abs() < 1e-12);
    }

    #[test]
    fn a_block_with_the_placeholder_shape_has_twelve_blocks_and_passes_the_geometry_check() {
        let vertices = [
            [0.0, 0.0, 0.0],
            [16384.0, 0.0, 0.0],
            [16384.0, 16384.0, 0.0],
            [0.0, 16384.0, 0.0],
        ];
        let indices = [0, 1, 2, 0, 2, 3];
        let water = [[0.0, 0.0, 5.0], [128.0, 0.0, 5.0], [0.0, 128.0, 5.0]];
        let nif = lodwright_nif::terrain_block(&lodwright_nif::TerrainBlock {
            origin: (0.0, 0.0),
            size: 16384.0,
            vertices: &vertices,
            indices: &indices,
            textures: ["a.dds", "b.dds"],
            water: Some(lodwright_nif::WaterShape {
                vertices: &water,
                indices: &[0, 1, 2],
                segments: None,
                placeholder: true,
            }),
        })
        .unwrap();
        assert_eq!(nif.blocks.len(), 12);
        let mut ctx = super::Ctx {
            out: super::Verification::default(),
        };
        ctx.check_geometry("a.nif", &nif, Some(16384.0), false);
        assert!(ctx.out.errors.is_empty(), "{:?}", ctx.out.errors);
    }

    fn write_terrain(root: &std::path::Path, name: &str, water: Option<(f32, f32)>) {
        let vertices = [
            [0.0, 0.0, 0.0],
            [16384.0, 0.0, 0.0],
            [16384.0, 16384.0, 0.0],
            [0.0, 16384.0, 0.0],
        ];
        let indices = [0, 1, 2, 0, 2, 3];
        let water_vertices = water.map(|(x0, x1)| {
            [
                [x0, 0.0, 5.0],
                [x1, 0.0, 5.0],
                [x1, 16384.0, 5.0],
                [x0, 16384.0, 5.0],
            ]
        });
        let nif = lodwright_nif::terrain_block(&lodwright_nif::TerrainBlock {
            origin: (0.0, 0.0),
            size: 16384.0,
            vertices: &vertices,
            indices: &indices,
            textures: ["a.dds", "b.dds"],
            water: water_vertices.as_ref().map(|v| lodwright_nif::WaterShape {
                vertices: v,
                indices: &indices,
                segments: None,
                placeholder: false,
            }),
        })
        .unwrap();
        let path = root.join("meshes/landscape/lod/w").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, nif.write().unwrap()).unwrap();
    }

    fn write_object(root: &std::path::Path, vertices: &[[f32; 3]]) {
        let n = vertices.len();
        let up = vec![[0.0, 0.0, 1.0]; n];
        let along = vec![[1.0, 0.0, 0.0]; n];
        let side = vec![[0.0, 1.0, 0.0]; n];
        let uv = vec![[0.5, 0.5]; n];
        let nif = lodwright_nif::object_block(&lodwright_nif::ObjectBlock {
            origin: (0.0, 0.0),
            vertices,
            normals: &up,
            tangents: &along,
            bitangents: &side,
            uv: &uv,
            indices: &[0, 1, 2],
            segment_triangles: [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            textures: ["a.dds", "b.dds"],
        })
        .unwrap();
        let path = root.join("meshes/landscape/lod/w/blocks/w.level4.x0.y0.nif");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, nif.write().unwrap()).unwrap();
    }

    /// An object block named for cell (2, 1) with its origin on the shape (as the game's blocks have it) or, when
    /// `on_root`, on the root node (as 0.3.0 to 0.15.1 wrote it: the engine then draws none of the object LOD, #294).
    fn write_object_at_cell(root: &std::path::Path, on_root: bool) {
        let up = vec![[0.0, 0.0, 1.0]; 3];
        let along = vec![[1.0, 0.0, 0.0]; 3];
        let side = vec![[0.0, 1.0, 0.0]; 3];
        let uv = vec![[0.5, 0.5]; 3];
        let mut nif = lodwright_nif::object_block(&lodwright_nif::ObjectBlock {
            origin: (2.0 * 4096.0, 4096.0),
            vertices: &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: &up,
            tangents: &along,
            bitangents: &side,
            uv: &uv,
            indices: &[0, 1, 2],
            segment_triangles: [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            textures: ["a.dds", "b.dds"],
        })
        .unwrap();
        if on_root {
            let (lodwright_nif::Body::Opaque(a), lodwright_nif::Body::Opaque(b)) =
                (nif.blocks[0].body.clone(), nif.blocks[1].body.clone())
            else {
                panic!("root and shape are kept opaque");
            };
            let (mut a, mut b) = (a, b);
            let moved: Vec<u8> = a[16..28].to_vec();
            a[16..28].copy_from_slice(&b[16..28]);
            b[16..28].copy_from_slice(&moved);
            nif.blocks[0].body = lodwright_nif::Body::Opaque(a);
            nif.blocks[1].body = lodwright_nif::Body::Opaque(b);
        }
        let path = root.join("meshes/landscape/lod/w/blocks/w.level4.x2.y1.nif");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, nif.write().unwrap()).unwrap();
    }

    #[test]
    fn an_object_block_with_its_origin_on_the_root_node_is_an_error() {
        covers(&[ids::verify::folder::STRUCTURE_CHECKS]);
        let (good, bad) = (temp("origin-shape"), temp("origin-root"));
        write_object_at_cell(&good, false);
        write_object_at_cell(&bad, true);
        let problems = |root: &std::path::Path| -> Vec<String> {
            verify_folder(root)
                .errors
                .into_iter()
                .map(|e| e.problem)
                .filter(|p| p.contains("translation"))
                .collect()
        };
        assert!(problems(&good).is_empty(), "{:?}", problems(&good));
        let found = problems(&bad);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found
                .iter()
                .all(|p| p.contains("the origin sits on the shape"))
        );
        // and against the game's layout the block is listed as different, not as equal
        let diffs = super::compare_folders(&bad, &good);
        assert!(
            diffs
                .iter()
                .any(|f| f.problem.starts_with("node translations differ")),
            "{diffs:?}"
        );
        assert_eq!(
            super::compare_folders(&good, &good),
            [] as [super::Finding; 0]
        );
    }

    #[test]
    fn a_settings_file_wider_than_128_cells_is_an_error() {
        covers(&[ids::verify::folder::STRUCTURE_CHECKS]);
        let root = temp("wide-settings");
        let dir = root.join("lodsettings");
        std::fs::create_dir_all(&dir).unwrap();
        let wide = lodwright_core::lod::LodSettings::for_bounds((-99, -99), (97, 100), 32);
        std::fs::write(dir.join("w.dlodsettings"), wide.to_bytes()).unwrap();
        let narrow = lodwright_core::lod::LodSettings::for_bounds((-64, -64), (63, 63), 32);
        std::fs::write(dir.join("n.dlodsettings"), narrow.to_bytes()).unwrap();
        let errors = verify_folder(&root).errors;
        let wide_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.problem.contains("more than 128"))
            .collect();
        assert_eq!(wide_errors.len(), 1, "{errors:?}");
        assert!(wide_errors[0].path.ends_with("w.dlodsettings"));
    }

    fn grid_errors(world_dir: &str, settings_start: Option<(i16, i16)>, name: &str) -> usize {
        let root = temp(&format!("grid-{world_dir}-{}", settings_start.is_some()));
        let vertices = [
            [0.0, 0.0, 0.0],
            [16384.0, 0.0, 0.0],
            [16384.0, 16384.0, 0.0],
            [0.0, 16384.0, 0.0],
        ];
        let nif = lodwright_nif::terrain_block(&lodwright_nif::TerrainBlock {
            origin: (0.0, 0.0),
            size: 16384.0,
            vertices: &vertices,
            indices: &[0, 1, 2, 0, 2, 3],
            textures: ["a.dds", "b.dds"],
            water: None,
        })
        .unwrap();
        let path = root.join(format!("meshes/landscape/lod/{world_dir}/blocks/{name}"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, nif.write().unwrap()).unwrap();
        if let Some(start) = settings_start {
            let dir = root.join("lodsettings");
            std::fs::create_dir_all(&dir).unwrap();
            let settings =
                lodwright_core::lod::LodSettings::for_bounds(start, (start.0 + 7, start.1 + 7), 4);
            std::fs::write(
                dir.join(format!("{world_dir}.dlodsettings")),
                settings.to_bytes(),
            )
            .unwrap();
        }
        verify_folder(&root)
            .errors
            .iter()
            .filter(|e| e.problem.contains("not on the world's grid"))
            .count()
    }

    #[test]
    fn an_object_block_of_a_world_without_a_settings_file_is_not_checked_against_a_grid() {
        covers(&[ids::verify::folder::STRUCTURE_CHECKS]);
        // a world with no terrain: object blocks and no settings file (#245)
        assert_eq!(grid_errors("oasis", None, "oasis.level4.x-5.y27.nif"), 0);
    }

    #[test]
    fn a_block_off_the_grid_of_its_settings_file_is_still_an_error() {
        covers(&[ids::verify::folder::STRUCTURE_CHECKS]);
        // start (-5, 27): x-5 y27 is on the grid, x-4 is not
        assert_eq!(
            grid_errors("oasis", Some((-5, 27)), "oasis.level4.x-5.y27.nif"),
            0
        );
        assert_eq!(
            grid_errors("oasis", Some((-5, 27)), "oasis.level4.x-4.y27.nif"),
            1
        );
    }

    #[test]
    fn blocks_are_measured_against_the_reference_by_name() {
        covers(&[ids::verify::folder::REFERENCE_BLOCKS_BY_NAME]);
        let (mine, theirs) = (temp("blocks-mine"), temp("blocks-theirs"));
        write_terrain(&mine, "w.level4.x0.y0.nif", Some((0.0, 8192.0)));
        write_terrain(&theirs, "w.level4.x0.y0.nif", Some((4096.0, 12288.0)));
        write_terrain(&mine, "w.level4.x4.y0.nif", None);
        write_terrain(&theirs, "w.level4.x4.y0.nif", None);
        write_terrain(&mine, "w.level4.x8.y0.nif", Some((0.0, 16384.0)));
        write_terrain(&mine, "w.level4.x12.y0.nif", Some((0.0, 16384.0)));
        write_terrain(&theirs, "w.level4.x12.y0.nif", None);
        let triangle = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        write_object(&mine, &triangle);
        write_object(&theirs, &[triangle[2], triangle[0], [1.5, 0.0, 0.0]]);
        let got = compare_blocks(&mine, &theirs);
        let water: Vec<(&str, String)> = got
            .water
            .iter()
            .map(|w| (w.path.as_str(), format!("{:.3}", w.iou)))
            .collect();
        assert_eq!(
            water,
            [
                (
                    "meshes/landscape/lod/w/w.level4.x0.y0.nif",
                    // a third of the union, as sampled at the cell centres of the 400 by 400 grid
                    "0.335".to_owned()
                ),
                (
                    "meshes/landscape/lod/w/w.level4.x12.y0.nif",
                    "0.000".to_owned()
                ),
            ],
            "a block without water on either side and a block missing from the reference are not listed"
        );
        assert_eq!(got.objects.len(), 1);
        assert_eq!(got.objects[0].unmatched_vertices, 1);
        assert!(!got.objects[0].exact());
    }
}
