// SPDX-License-Identifier: MPL-2.0
//! Tree LOD generation: billboard atlas, `treetypes.lst` and the `.dtl` block files of one world.
//!
//! Translated from `TwbLodTES5TreeList` and `wbGenerateLODTES5` in `Core/wbLOD.pas` of xEdit (MPL-2.0, revision
//! 081cd8672bd366306eaafbbc8771f3a72c6cea7a): which bases have billboards, where the billboard textures and their
//! settings are looked up, how the atlas is packed and how the lists are numbered (rows `xedit-tree-lod-files`,
//! `xedit-tree-lod-selection`, `xedit-tree-lod-atlas`). The atlas is written as BC3 with box-filtered mips where
//! the original writes BC2 with Lanczos mips; the game reads both.

use std::path::Path;

use lodwright_core::layout;
use lodwright_core::trees::{TreeBase, TreeRef, TreeType, assemble, write_block, write_types};
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, decode_image, encode};

use crate::assets::Assets;
use crate::error::GenError;
use crate::terrain::{FileRecord, write_file};

/// The largest atlas edge tried; the generator's default.
const MAX_ATLAS: u32 = 8192;
/// Space kept between billboards in the atlas, in pixels: measured gaps of the game's own atlases are 12 to 16
/// pixels, which also keeps mip levels from bleeding one billboard into another.
const PADDING: u32 = 16;

/// What tree generation produced.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreeReport {
    /// Files written, sorted by path.
    pub files: Vec<FileRecord>,
    /// Things that did not go in and why, in a stable order.
    pub warnings: Vec<String>,
    /// Trees written into blocks.
    pub trees: usize,
    /// Billboard types in the list.
    pub types: usize,
    /// References to flagged bases without a billboard texture (they are drawn as object LOD models, if at all).
    pub unbilled: usize,
}

struct Billboard {
    base: u32,
    index: u32,
    width: f32,
    height: f32,
    shift: [f32; 3],
    factor: f32,
    crc: u32,
    image: Image,
}

// source: xedit-tree-lod-atlas
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for b in data {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn stem(model: &str) -> String {
    let name = model.rsplit(['\\', '/']).next().unwrap_or(model);
    name.rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .to_owned()
}

// source: xedit-tree-lod-atlas
/// Where the billboard texture of `base` is looked up, relative to the data folder.
#[must_use]
pub fn billboard_path(plugin_name: &str, base: &TreeBase) -> String {
    format!(
        "textures/terrain/lodgen/{}/{}_{:08X}.dds",
        plugin_name.to_ascii_lowercase(),
        stem(&base.model).to_ascii_lowercase(),
        base.form_id & 0x00FF_FFFF
    )
}

/// The `[LOD]` keys of a billboard's settings file.
#[derive(Default)]
struct BillboardSettings {
    width: f32,
    height: f32,
    shift: [f32; 3],
    scale: f32,
}

// source: xedit-tree-lod-atlas
fn parse_settings(text: &str) -> BillboardSettings {
    let mut s = BillboardSettings {
        scale: 1.0,
        ..BillboardSettings::default()
    };
    let mut in_lod = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_lod = section.eq_ignore_ascii_case("LOD");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !in_lod {
            continue;
        }
        let Ok(v) = value.trim().parse::<f32>() else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "width" => s.width = v,
            "height" => s.height = v,
            "shiftx" => s.shift[0] = v,
            "shifty" => s.shift[1] = v,
            "shiftz" => s.shift[2] = v,
            "scale" => s.scale = v,
            _ => {}
        }
    }
    s
}

fn load_billboards(
    bases: &[TreeBase],
    plugin_names: &[String],
    assets: &dyn Assets,
) -> (Vec<Billboard>, Vec<u32>) {
    let mut found = Vec::new();
    let mut missing = Vec::new();
    let mut next = 0_u32;
    for base in bases {
        let name = plugin_names.get(base.plugin).map_or("", String::as_str);
        let path = billboard_path(name, base);
        let Some(bytes) = assets.read(&path) else {
            missing.push(base.form_id);
            continue;
        };
        let Some(image) = decode_image(&bytes).ok() else {
            missing.push(base.form_id);
            continue;
        };
        // the default size is the object bounds; the settings file overrides each size that it states as nonzero
        let (mut width, mut height) = base.bounds.map_or((0.0, 0.0), |b| {
            (
                f32::from(b[3]) - f32::from(b[0]),
                f32::from(b[5]) - f32::from(b[2]),
            )
        });
        let mut shift = [0.0; 3];
        let mut factor = 1.0;
        if let Some(text) = assets
            .read(&format!("{}txt", path.trim_end_matches("dds")))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
        {
            let s = parse_settings(&text);
            if s.width.abs() > 1e-6 {
                width = s.width;
            }
            if s.height.abs() > 1e-6 {
                height = s.height;
            }
            shift = s.shift;
            factor = s.scale;
        }
        found.push(Billboard {
            base: base.form_id,
            index: next,
            width,
            height,
            shift,
            factor,
            crc: crc32(&bytes),
            image,
        });
        next += 1;
    }
    (found, missing)
}

struct Node {
    used: bool,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    down: Option<usize>,
    right: Option<usize>,
}

struct Slot {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    crc: u32,
}

// source: xedit-tree-lod-atlas
fn find(nodes: &[Node], at: usize, w: u32, h: u32) -> Option<usize> {
    if nodes[at].used {
        nodes[at]
            .right
            .and_then(|r| find(nodes, r, w, h))
            .or_else(|| nodes[at].down.and_then(|d| find(nodes, d, w, h)))
    } else if w <= nodes[at].w && h <= nodes[at].h {
        Some(at)
    } else {
        None
    }
}

// source: xedit-tree-lod-atlas
fn fit(slots: &mut [Slot], width: u32, height: u32) -> bool {
    let mut nodes = vec![Node {
        used: false,
        x: 0,
        y: 0,
        w: width,
        h: height,
        down: None,
        right: None,
    }];
    let mut all = true;
    for s in slots.iter_mut() {
        let Some(at) = find(&nodes, 0, s.w, s.h) else {
            all = false;
            continue;
        };
        let (x, y, nw, nh) = (nodes[at].x, nodes[at].y, nodes[at].w, nodes[at].h);
        nodes[at].used = true;
        nodes.push(Node {
            used: false,
            x,
            y: y + s.h + PADDING,
            w: nw,
            h: nh.saturating_sub(s.h + PADDING),
            down: None,
            right: None,
        });
        nodes[at].down = Some(nodes.len() - 1);
        nodes.push(Node {
            used: false,
            x: x + s.w + PADDING,
            y,
            w: nw.saturating_sub(s.w + PADDING),
            h: s.h,
            down: None,
            right: None,
        });
        nodes[at].right = Some(nodes.len() - 1);
        s.x = x;
        s.y = y;
    }
    all
}

// source: xedit-tree-lod-atlas
/// Pack billboards: each distinct picture (by checksum) once, largest side first (stable), into an atlas that
/// starts at 512 by 512 and doubles its smaller side until everything fits.
fn pack(found: &[Billboard]) -> Option<(u32, u32, Vec<Slot>)> {
    let mut slots: Vec<Slot> = Vec::new();
    for (i, b) in found.iter().enumerate() {
        if found[..i].iter().any(|o| o.crc == b.crc) {
            continue;
        }
        slots.push(Slot {
            x: 0,
            y: 0,
            // block compression covers whole 4 by 4 blocks: the measured rectangles round the width up to four
            w: b.image.width().next_multiple_of(4),
            h: b.image.height().next_multiple_of(4),
            crc: b.crc,
        });
    }
    if slots.is_empty() {
        return None;
    }
    // bubble sort by the longer side, descending, equal sides keep their order
    let side = |s: &Slot| s.w.max(s.h);
    for i in 1..slots.len() {
        let mut j = i;
        while j > 0 && side(&slots[j - 1]) < side(&slots[j]) {
            slots.swap(j - 1, j);
            j -= 1;
        }
    }
    let (mut w, mut h) = (512.min(MAX_ATLAS), 512.min(MAX_ATLAS));
    while !fit(&mut slots, w, h) {
        if w <= h {
            w *= 2;
        } else {
            h *= 2;
        }
        if w > MAX_ATLAS || h > MAX_ATLAS {
            return None;
        }
    }
    Some((w, h, slots))
}

fn copy_into(canvas: &mut [u8], width: u32, src: &Image, x: u32, y: u32) {
    for sy in 0..src.height() {
        for sx in 0..src.width() {
            if let Some(p) = src.pixel(sx, sy) {
                let at = (((y + sy) * width + x + sx) * 4) as usize;
                if at + 4 <= canvas.len() {
                    canvas[at..at + 4].copy_from_slice(&p);
                }
            }
        }
    }
}

/// Generate the tree LOD of one world into `out`.
///
/// `bases` and `refs` come from [`lodwright_core::trees::tree_bases`] and
/// [`lodwright_core::trees::tree_references`]; `plugin_names` are the plugin file names in load order (a base's
/// billboard is looked up under its defining plugin's name); `south_west` is the world's south-west cell. Nothing is
/// written when no billboard exists or no tree stands in the world.
///
/// # Errors
/// [`GenError`] for a world name that cannot be a folder, an atlas that cannot be encoded or a write that fails.
pub fn generate_trees(
    world: &str,
    south_west: (i32, i32),
    bases: &[TreeBase],
    refs: &[TreeRef],
    plugin_names: &[String],
    assets: &dyn Assets,
    out: &Path,
) -> Result<TreeReport, GenError> {
    generate_trees_with_progress(
        world,
        south_west,
        bases,
        refs,
        plugin_names,
        assets,
        out,
        &mut |_, _| {},
    )
}

/// [`generate_trees`], calling `block_done(done, total)` after each block file is written; `total` is the number
/// of block files, known before the first is written, and `done` reaches it. A world that writes nothing reports
/// no block.
///
/// # Errors
/// As [`generate_trees`].
#[allow(clippy::too_many_arguments)] // generate_trees's inputs plus the callback
pub fn generate_trees_with_progress(
    world: &str,
    south_west: (i32, i32),
    bases: &[TreeBase],
    refs: &[TreeRef],
    plugin_names: &[String],
    assets: &dyn Assets,
    out: &Path,
    block_done: &mut dyn FnMut(u64, u64),
) -> Result<TreeReport, GenError> {
    layout::check_world_name(world)?;
    let mut report = TreeReport::default();
    let (found, missing) = load_billboards(bases, plugin_names, assets);
    let by_base = |base: u32| found.iter().find(|b| b.base == base);
    let (blocks, duplicates) = assemble(refs, south_west, &|base| {
        by_base(base).map(|b| (b.index, b.shift, b.factor))
    });
    report.trees = blocks
        .values()
        .map(lodwright_core::trees::TreeBlock::len)
        .sum();
    report.unbilled = refs.iter().filter(|r| missing.contains(&r.base)).count();
    if duplicates > 0 {
        report.warnings.push(format!(
            "{world}: {duplicates} tree references repeat an object id and are left out of tree LOD"
        ));
    }
    if report.trees == 0 {
        return Ok(report);
    }
    let Some((width, height, slots)) = pack(&found) else {
        report
            .warnings
            .push(format!("{world}: the tree billboards do not fit one atlas"));
        report.trees = 0;
        return Ok(report);
    };
    let mut canvas = vec![0_u8; (width * height * 4) as usize];
    for b in &found {
        if let Some(s) = slots.iter().find(|s| s.crc == b.crc)
            && found
                .iter()
                .find(|o| o.crc == b.crc)
                .is_some_and(|o| o.base == b.base)
        {
            copy_into(&mut canvas, width, &b.image, s.x, s.y);
        }
    }
    let types: Vec<TreeType> = found
        .iter()
        .filter_map(|b| {
            let s = slots.iter().find(|s| s.crc == b.crc)?;
            Some(TreeType {
                index: b.index,
                width: b.width,
                height: b.height,
                uv: [
                    s.x as f32 / width as f32,
                    s.y as f32 / height as f32,
                    (s.x + s.w) as f32 / width as f32,
                    (s.y + s.h) as f32 / height as f32,
                ],
            })
        })
        .collect();
    report.types = types.len();
    let atlas = Image::new(width, height, canvas)?;
    let dds = encode(
        &atlas,
        &EncodeOptions {
            format: Format::Bc3,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )?;
    report
        .files
        .push(write_file(out, layout::tree_atlas_path(world), &dds)?);
    report.files.push(write_file(
        out,
        &layout::tree_list_path(world)?,
        &write_types(&types),
    )?);
    let total = blocks.len() as u64;
    for (done, ((x, y), block)) in (1_u64..).zip(&blocks) {
        report.files.push(write_file(
            out,
            &layout::tree_block_path(world, *x, *y)?,
            &write_block(block),
        )?);
        block_done(done, total);
    }
    report.files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::{
        billboard_path, crc32, generate_trees, generate_trees_with_progress, parse_settings,
    };
    use crate::assets::Assets;
    use lodwright_core::trees::{TreeBase, TreeRef, parse_block, parse_types};
    use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
    use std::collections::HashMap;

    struct Memory(HashMap<String, Vec<u8>>);

    impl Assets for Memory {
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.0
                .get(&path.to_ascii_lowercase().replace('\\', "/"))
                .cloned()
        }
    }

    fn billboard(w: u32, h: u32, shade: u8) -> Vec<u8> {
        let px: Vec<u8> = (0..w * h).flat_map(|_| [shade, 200, 40, 255]).collect();
        encode(
            &Image::new(w, h, px).unwrap(),
            &EncodeOptions {
                format: Format::Bc3,
                mips: false,
                filter: Filter::Box,
                quality: Quality::Fast,
            },
        )
        .unwrap()
    }

    fn base(id: u32, model: &str) -> TreeBase {
        TreeBase {
            form_id: id,
            signature: *b"TREE",
            model: model.to_owned(),
            bounds: Some([-100, -100, 0, 100, 100, 300]),
            plugin: 0,
        }
    }

    fn at(base: u32, x: f32, reference: u32) -> TreeRef {
        TreeRef {
            base,
            pos: [x, 100.0, 5.0],
            scale: 1.0,
            reference,
            hidden: false,
            fallen: false,
        }
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lodwright-trees-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn billboard_names_use_the_defining_plugin_the_model_stem_and_the_form_id_without_load_index() {
        let b = base(0x0100_3D42, "trees\\Oak01.spt");
        assert_eq!(
            billboard_path("FalloutNV.esm", &b),
            "textures/terrain/lodgen/falloutnv.esm/oak01_00003D42.dds"
        );
    }

    #[test]
    fn the_settings_file_overrides_the_bounds_only_for_sizes_it_states() {
        let s = parse_settings("[LOD]\nWidth=150.5\nHeight=0\nShiftZ=-1\nScale=1.5\n");
        assert_eq!(
            (s.width, s.height, s.shift[2], s.scale),
            (150.5, 0.0, -1.0, 1.5)
        );
    }

    #[test]
    fn trees_are_written_with_a_list_an_atlas_and_one_block_per_cell_group() {
        let mut files = HashMap::new();
        files.insert(
            "textures/terrain/lodgen/a.esm/oak_00000100.dds".to_owned(),
            billboard(64, 128, 10),
        );
        files.insert(
            "textures/terrain/lodgen/a.esm/oak_00000100.txt".to_owned(),
            b"[LOD]\nWidth=150\nHeight=127\nShiftZ=-1\n".to_vec(),
        );
        files.insert(
            "textures/terrain/lodgen/a.esm/pine_00000101.dds".to_owned(),
            billboard(32, 32, 90),
        );
        let bases = vec![
            base(0x100, "oak.spt"),
            base(0x101, "pine.spt"),
            base(0x102, "none.spt"),
        ];
        let refs = vec![
            at(0x100, 10.0, 0x200),
            at(0x101, 20.0, 0x201),
            at(0x102, 30.0, 0x202),
            at(0x100, 9000.0, 0x203),
            at(0x100, 40_000.0, 0x204),
        ];
        let out = temp("write");
        let report = generate_trees(
            "Mojave",
            (0, 0),
            &bases,
            &refs,
            &["A.esm".to_owned()],
            &Memory(files),
            &out,
        )
        .unwrap();
        assert_eq!(report.trees, 4);
        assert_eq!(report.types, 2);
        assert_eq!(report.unbilled, 1, "one tree has no billboard");
        let paths: Vec<_> = report.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "meshes/landscape/lod/mojave/trees/mojave.level8.x0.y0.dtl",
                "meshes/landscape/lod/mojave/trees/mojave.level8.x8.y0.dtl",
                "meshes/landscape/lod/mojave/trees/treetypes.lst",
                "textures/landscape/trees/treedeadlod.dds",
            ]
        );
        let verification = crate::verify_folder(&out);
        assert_eq!(
            verification.errors,
            vec![],
            "a generated folder verifies clean"
        );
        assert_eq!(verification.tree_blocks, 2);
        let list = parse_types(&std::fs::read(out.join(paths[2])).unwrap()).unwrap();
        assert_eq!(list[0].index, 0);
        assert_eq!((list[0].width, list[0].height), (150.0, 127.0));
        let first = parse_block(&std::fs::read(out.join(paths[0])).unwrap()).unwrap();
        assert_eq!(first.len(), 3);
        assert_eq!(first.groups[0].1[0].pos[2], 4.0, "the shift applies");
        let mut damaged = std::fs::read(out.join(paths[0])).unwrap();
        damaged.push(0);
        std::fs::write(out.join(paths[0]), damaged).unwrap();
        assert!(
            crate::verify_folder(&out)
                .errors
                .iter()
                .any(|f| f.problem.contains("not a readable tree block")),
            "trailing bytes are found"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn each_tree_block_reports_once_in_order_and_a_world_without_billboards_reports_none() {
        let mut files = HashMap::new();
        files.insert(
            "textures/terrain/lodgen/a.esm/oak_00000100.dds".to_owned(),
            billboard(16, 16, 1),
        );
        let bases = vec![base(0x100, "oak.spt")];
        let refs = vec![
            at(0x100, 10.0, 0x200),
            at(0x100, 9000.0, 0x201),
            at(0x100, 40_000.0, 0x202),
        ];
        let names = ["A.esm".to_owned()];
        let out = temp("progress");
        let mut seen = Vec::new();
        generate_trees_with_progress(
            "Mojave",
            (0, 0),
            &bases,
            &refs,
            &names,
            &Memory(files),
            &out,
            &mut |done, total| seen.push((done, total)),
        )
        .unwrap();
        assert_eq!(seen, [(1, 2), (2, 2)]);
        seen.clear();
        generate_trees_with_progress(
            "Mojave",
            (0, 0),
            &bases,
            &refs,
            &names,
            &Memory(HashMap::new()),
            &out,
            &mut |done, total| seen.push((done, total)),
        )
        .unwrap();
        assert_eq!(seen, Vec::<(u64, u64)>::new());
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn a_dlc4_world_uses_the_swamp_atlas_and_a_world_without_billboards_writes_nothing() {
        let mut files = HashMap::new();
        files.insert(
            "textures/terrain/lodgen/a.esm/oak_00000100.dds".to_owned(),
            billboard(16, 16, 1),
        );
        let out = temp("swamp");
        let bases = vec![base(0x100, "oak.spt")];
        let refs = vec![at(0x100, 10.0, 0x200)];
        let names = ["A.esm".to_owned()];
        let r = generate_trees(
            "DLC4Bog",
            (0, 0),
            &bases,
            &refs,
            &names,
            &Memory(files),
            &out,
        )
        .unwrap();
        assert!(
            r.files
                .iter()
                .any(|f| f.path == "textures/landscape/trees/treeswamplod.dds")
        );
        let none = generate_trees(
            "Mojave",
            (0, 0),
            &bases,
            &refs,
            &names,
            &Memory(HashMap::new()),
            &out,
        )
        .unwrap();
        assert!(none.files.is_empty() && none.trees == 0);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn identical_billboards_share_one_atlas_rectangle_and_output_is_repeatable() {
        let same = billboard(24, 24, 7);
        let mut files = HashMap::new();
        files.insert(
            "textures/terrain/lodgen/a.esm/a_00000100.dds".to_owned(),
            same.clone(),
        );
        files.insert(
            "textures/terrain/lodgen/a.esm/b_00000101.dds".to_owned(),
            same,
        );
        let bases = vec![base(0x100, "a.spt"), base(0x101, "b.spt")];
        let refs = vec![at(0x100, 10.0, 0x200), at(0x101, 20.0, 0x201)];
        let names = ["A.esm".to_owned()];
        let assets = Memory(files);
        let (o1, o2) = (temp("rep1"), temp("rep2"));
        let a = generate_trees("W", (0, 0), &bases, &refs, &names, &assets, &o1).unwrap();
        let b = generate_trees("W", (0, 0), &bases, &refs, &names, &assets, &o2).unwrap();
        assert_eq!(a, b);
        let list = parse_types(
            &std::fs::read(o1.join("meshes/landscape/lod/w/trees/treetypes.lst")).unwrap(),
        )
        .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].uv, list[1].uv);
        let _ = std::fs::remove_dir_all(&o1);
        let _ = std::fs::remove_dir_all(&o2);
    }
}
