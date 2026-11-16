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

fn files_below(root: &Path) -> Vec<(String, PathBuf)> {
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

    fn check_geometry(&mut self, path: &str, nif: &NifFile, size: Option<f32>, object: bool) {
        // a terrain block with a water plane has two more (`game-lod-water-shape`)
        let expected: &[usize] = if object { &[7] } else { &[8, 10] };
        if !expected.contains(&nif.blocks.len()) {
            self.err(
                path,
                format!("{} blocks, expected {}", nif.blocks.len(), expected[0]),
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
            self.check_data(path, data, size, object);
        }
    }

    fn check_data(
        &mut self,
        path: &str,
        data: &lodwright_nif::NiTriShapeData,
        size: Option<f32>,
        object: bool,
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
            let outside = verts
                .iter()
                .filter(|v| v[0] < -0.5 || v[1] < -0.5 || v[0] > size + 0.5 || v[1] > size + 0.5)
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
        if object {
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
                    let multiple = x.rem_euclid(i32::try_from(level).unwrap_or(1)) == 0
                        && y.rem_euclid(i32::try_from(level).unwrap_or(1)) == 0;
                    if !multiple {
                        ctx.err(rel, "the coordinates are not multiples of the level");
                    }
                    ctx.check_geometry(rel, &nif, (!object).then_some(size), object);
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
                    if s.width_cells
                        != u32::try_from(i32::from(s.north_east.0) - i32::from(s.south_west.0) + 1)
                            .unwrap_or(0)
                    {
                        ctx.err(rel, "the width disagrees with the corners");
                    }
                    if s.lowest_level != 4 || ![8, 16, 32].contains(&s.highest_level) {
                        ctx.err(rel, "the level range is not 4 to 8, 16 or 32");
                    }
                }
            }
        }
    }
    // every terrain block has both textures
    for (rel, level, x, y) in &terrain {
        let world = rel.split('/').nth(3).unwrap_or("");
        for kind in ["diffuse", "normals"] {
            let tex = format!(
                "textures/landscape/lod/{world}/{kind}/{world}.n.level{level}.x{x}.y{y}.dds"
            );
            if !names.contains_key(&tex) {
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

/// The ground area a terrain block's water plane covers: the summed area of the triangles of every data block
/// after the terrain's, seen from above.
fn water_area(nif: &NifFile) -> f64 {
    nif.blocks
        .iter()
        .filter_map(|b| match &b.body {
            Body::TriShapeData(d) => Some(d),
            _ => None,
        })
        .skip(1)
        .map(|d| {
            let v = d.geometry.vertices.as_deref().unwrap_or(&[]);
            d.triangles
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .filter_map(|t| {
                    let [a, b, c] = t.map(|i| v.get(usize::from(i)).copied());
                    let (a, b, c) = (a?, b?, c?);
                    let cross = f64::from(b[0] - a[0]) * f64::from(c[1] - a[1])
                        - f64::from(b[1] - a[1]) * f64::from(c[0] - a[0]);
                    Some(cross.abs() / 2.0)
                })
                .sum::<f64>()
        })
        .sum()
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
    use super::{parse_block, verify_folder, water_difference};
    use crate::{Options, generate_terrain};
    use lodwright_core::land::CellHeights;
    use lodwright_core::terrain::TerrainSettings;
    use lodwright_core::worldspace::Worldspace;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lodwright-verify-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn block_names_parse_with_negative_coordinates_and_the_high_marker() {
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
}
