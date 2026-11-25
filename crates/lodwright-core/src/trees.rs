// SPDX-License-Identifier: MPL-2.0
//! Tree LOD: the `.dtl` block files, the `treetypes.lst` list and the choice of which references become trees.
//!
//! The file layouts and the selection rules are translated from `Core/wbLOD.pas` of xEdit (MPL-2.0, revision
//! 081cd8672bd366306eaafbbc8771f3a72c6cea7a), and every layout fact was re-measured on tree LOD files of the owner's
//! install (rows `xedit-tree-lod-files`, `xedit-tree-lod-selection`, `game-tree-lod-dtl`, `game-tree-lod-lst`).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "cell indices and 24-bit fractions are small"
)]

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::Error;
use crate::objects::HAS_TREE_LOD;
use crate::plugin::Plugin;
use crate::worldspace::{absolute, master_table};

/// Cells per tree block edge in the games with `.dtl` files.
pub const TREE_BLOCK_CELLS: i32 = 8;
/// Size of one `treetypes.lst` entry.
pub const TYPE_LEN: usize = 32;
/// Size of one tree reference in a `.dtl` file.
pub const TREE_LEN: usize = 32;

const INITIALLY_DISABLED: u32 = 0x0000_0800;
const DELETED: u32 = 0x0000_0020;
const CELL_UNITS: f32 = 4096.0;

/// One entry of `treetypes.lst`: a billboard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeType {
    /// The type index that `.dtl` groups refer to.
    pub index: u32,
    /// Billboard width in world units.
    pub width: f32,
    /// Billboard height in world units.
    pub height: f32,
    /// Atlas rectangle as fractions: left, top, right, bottom.
    pub uv: [f32; 4],
}

// source: xedit-tree-lod-files, game-tree-lod-lst
/// The bytes of a `treetypes.lst`: a count, then one 32-byte entry per type (index, width, height, four atlas
/// fractions and a zero).
#[must_use]
pub fn write_types(types: &[TreeType]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + types.len() * TYPE_LEN);
    out.extend_from_slice(&u32::try_from(types.len()).unwrap_or(u32::MAX).to_le_bytes());
    for t in types {
        out.extend_from_slice(&t.index.to_le_bytes());
        out.extend_from_slice(&t.width.to_le_bytes());
        out.extend_from_slice(&t.height.to_le_bytes());
        for v in t.uv {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&0_u32.to_le_bytes());
    }
    out
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    f32::from_bits(u32_at(b, at))
}

// source: xedit-tree-lod-files, game-tree-lod-lst
/// Read a `treetypes.lst`.
///
/// # Errors
/// [`Error::Malformed`] when the file is shorter than its count says.
pub fn parse_types(bytes: &[u8]) -> Result<Vec<TreeType>, Error> {
    if bytes.len() < 4 {
        return Ok(Vec::new());
    }
    let n = u32_at(bytes, 0) as usize;
    if n.checked_mul(TYPE_LEN).and_then(|v| v.checked_add(4)) > Some(bytes.len()) {
        return Err(Error::Malformed(
            "a tree type list is shorter than its count",
        ));
    }
    Ok((0..n)
        .map(|i| {
            let at = 4 + i * TYPE_LEN;
            TreeType {
                index: u32_at(bytes, at),
                width: f32_at(bytes, at + 4),
                height: f32_at(bytes, at + 8),
                uv: [
                    f32_at(bytes, at + 12),
                    f32_at(bytes, at + 16),
                    f32_at(bytes, at + 20),
                    f32_at(bytes, at + 24),
                ],
            }
        })
        .collect())
}

/// One tree in a block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tree {
    /// World position.
    pub pos: [f32; 3],
    /// Rotation about the vertical axis in radians, 0 to 2 pi.
    pub rotation: f32,
    /// Scale.
    pub scale: f32,
    /// The reference's form id as the game's own files write it.
    pub reference: u32,
}

/// The trees of one block, grouped by type in the order the types first appear.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TreeBlock {
    /// Groups: the type index and its trees.
    pub groups: Vec<(u32, Vec<Tree>)>,
}

impl TreeBlock {
    /// Add a tree of `type_index`.
    pub fn push(&mut self, type_index: u32, tree: Tree) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.0 == type_index) {
            g.1.push(tree);
        } else {
            self.groups.push((type_index, vec![tree]));
        }
    }

    /// Number of trees.
    #[must_use]
    pub fn len(&self) -> usize {
        self.groups.iter().map(|g| g.1.len()).sum()
    }

    /// No tree at all (the game still reads such a file: it overrides a block of an earlier archive).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.iter().all(|g| g.1.is_empty())
    }
}

// source: xedit-tree-lod-files, game-tree-lod-dtl
/// The bytes of a `.dtl` block: the number of groups, then for each group its type index, its tree count and its
/// 32-byte trees (x, y, z, rotation, scale, reference, two zeros).
#[must_use]
pub fn write_block(block: &TreeBlock) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + block.len() * TREE_LEN + block.groups.len() * 8);
    out.extend_from_slice(
        &u32::try_from(block.groups.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    for (index, trees) in &block.groups {
        out.extend_from_slice(&index.to_le_bytes());
        out.extend_from_slice(&u32::try_from(trees.len()).unwrap_or(u32::MAX).to_le_bytes());
        for t in trees {
            for v in [t.pos[0], t.pos[1], t.pos[2], t.rotation, t.scale] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&t.reference.to_le_bytes());
            out.extend_from_slice(&[0_u8; 8]);
        }
    }
    out
}

// source: xedit-tree-lod-files, game-tree-lod-dtl
/// Read a `.dtl` block.
///
/// # Errors
/// [`Error::Malformed`] when the file ends inside a group or has bytes left over.
pub fn parse_block(bytes: &[u8]) -> Result<TreeBlock, Error> {
    if bytes.len() < 4 {
        return Ok(TreeBlock::default());
    }
    let groups = u32_at(bytes, 0) as usize;
    let mut at = 4;
    let mut block = TreeBlock::default();
    for _ in 0..groups {
        if at + 8 > bytes.len() {
            return Err(Error::Malformed("a tree block ends inside a group header"));
        }
        let (index, n) = (u32_at(bytes, at), u32_at(bytes, at + 4) as usize);
        at += 8;
        if n.checked_mul(TREE_LEN).and_then(|v| v.checked_add(at)) > Some(bytes.len()) {
            return Err(Error::Malformed("a tree block ends inside a group"));
        }
        let trees = (0..n)
            .map(|i| {
                let a = at + i * TREE_LEN;
                Tree {
                    pos: [f32_at(bytes, a), f32_at(bytes, a + 4), f32_at(bytes, a + 8)],
                    rotation: f32_at(bytes, a + 12),
                    scale: f32_at(bytes, a + 16),
                    reference: u32_at(bytes, a + 20),
                }
            })
            .collect();
        at += n * TREE_LEN;
        block.groups.push((index, trees));
    }
    if at != bytes.len() {
        return Err(Error::Malformed("a tree block has bytes after its groups"));
    }
    Ok(block)
}

/// A base record that has a tree billboard: flagged "has tree LOD", a static, an activator or a tree.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeBase {
    /// Absolute form id.
    pub form_id: u32,
    /// Record signature.
    pub signature: [u8; 4],
    /// Model path, as the record states it.
    pub model: String,
    /// Object bounds of the winning record, if any.
    pub bounds: Option<[i16; 6]>,
    /// Load-order index of the plugin that defines the record.
    pub plugin: usize,
}

type Winner = (u32, Option<[i16; 6]>, Option<String>);
type Placed = (u32, u32, crate::plugin::Placement, bool, usize);

fn signature_rank(sig: [u8; 4]) -> usize {
    match &sig {
        b"STAT" => 0,
        b"ACTI" => 1,
        _ => 2,
    }
}

// source: xedit-tree-lod-selection
/// Every base record that is flagged has-tree-LOD in its winning version, in the order the generator collects the
/// billboards: plugins in load order, within a plugin statics, then activators, then trees, each in file order.
/// Only the plugin that defines a record contributes it.
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn tree_bases(plugins: &[(String, Plugin)]) -> Result<Vec<TreeBase>, Error> {
    let mut winners: HashMap<u32, Winner> = HashMap::new();
    let mut defined: Vec<Vec<(u32, [u8; 4])>> = vec![Vec::new(); plugins.len()];
    for (index, (_, plugin)) in plugins.iter().enumerate() {
        let table = master_table(plugins, plugin)?;
        let own = u32::try_from(index).map_err(|_| Error::Malformed("too many plugins"))?;
        for base in &plugin.objects {
            let id = absolute(base.form_id, &table, own);
            winners.insert(id, (base.flags, base.bounds, base.model.clone()));
            if usize::try_from(id >> 24).is_ok_and(|d| d == index)
                && [*b"STAT", *b"ACTI", *b"TREE"].contains(&base.signature)
            {
                defined[index].push((id, base.signature));
            }
        }
    }
    let mut out = Vec::new();
    for (index, ids) in defined.iter().enumerate() {
        let mut ids = ids.clone();
        ids.sort_by_key(|(_, sig)| signature_rank(*sig));
        for (id, signature) in ids {
            let Some((flags, bounds, Some(model))) = winners.get(&id).cloned() else {
                continue;
            };
            if flags & HAS_TREE_LOD != 0 {
                out.push(TreeBase {
                    form_id: id,
                    signature,
                    model,
                    bounds,
                    plugin: index,
                });
            }
        }
    }
    Ok(out)
}

/// A reference to a tree base, with what the block files need of it.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRef {
    /// Absolute form id of the base.
    pub base: u32,
    /// World position.
    pub pos: [f32; 3],
    /// Scale (1 when the reference states none).
    pub scale: f32,
    /// The form id written to the block file.
    pub reference: u32,
    /// Initially disabled, deleted, or tied to an enable parent: the block exists, the tree does not.
    pub hidden: bool,
    /// Lying on its side: too far tilted to be a billboard.
    pub fallen: bool,
}

// source: xedit-tree-lod-selection
/// The references in world `world` whose base is one of `bases`, in load order and record order, after overrides.
///
/// The form id written for a reference that its own plugin defines is the absolute id; for one a later plugin
/// overrides it carries load-order index 1 (the game's own files do the same).
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn tree_references(
    plugins: &[(String, Plugin)],
    world: u32,
    bases: &[TreeBase],
) -> Result<Vec<TreeRef>, Error> {
    let wanted: HashSet<u32> = bases.iter().map(|b| b.form_id).collect();
    let mut refs: HashMap<u32, Placed> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    for (index, (_, plugin)) in plugins.iter().enumerate() {
        let table = master_table(plugins, plugin)?;
        let own = u32::try_from(index).map_err(|_| Error::Malformed("too many plugins"))?;
        for source in plugin
            .worlds
            .iter()
            .filter(|w| absolute(w.form_id, &table, own) == world)
        {
            for r in &source.refs {
                let id = absolute(r.form_id, &table, own);
                let value = (
                    absolute(r.base, &table, own),
                    r.flags,
                    r.placement,
                    r.enable_parent.is_some(),
                    index,
                );
                if refs.insert(id, value).is_none() {
                    order.push(id);
                }
            }
        }
    }
    let mut out = Vec::new();
    for id in order {
        let (base, flags, placement, enable_parent, plugin) = refs[&id];
        if !wanted.contains(&base) {
            continue;
        }
        let deg = |r: f32| r.to_degrees();
        let tilted = |d: f32| d > 30.0 && d < 330.0;
        let fallen = tilted(deg(placement.rot[0])) || tilted(deg(placement.rot[1]));
        let defined_here = usize::try_from(id >> 24).is_ok_and(|d| d == plugin);
        out.push(TreeRef {
            base,
            pos: placement.pos,
            scale: placement.scale,
            reference: if defined_here {
                id
            } else {
                0x0100_0000 | (id & 0x00FF_FFFF)
            },
            hidden: flags & (INITIALLY_DISABLED | DELETED) != 0 || enable_parent,
            fallen,
        });
    }
    Ok(out)
}

/// Cell of a world position.
#[must_use]
pub fn cell_of(v: f32) -> i32 {
    (v / CELL_UNITS).floor() as i32
}

// source: xedit-tree-lod-selection
/// The south-west cell of the block that holds cell (`cx`, `cy`), when the world's own south-west cell is
/// `south_west`; `None` for a cell outside the world's range.
#[must_use]
pub fn block_of(cx: i32, cy: i32, south_west: (i32, i32)) -> Option<(i32, i32)> {
    let bx = south_west.0 + (cx - south_west.0).div_euclid(TREE_BLOCK_CELLS) * TREE_BLOCK_CELLS;
    let by = south_west.1 + (cy - south_west.1).div_euclid(TREE_BLOCK_CELLS) * TREE_BLOCK_CELLS;
    (bx >= south_west.0 && by >= south_west.1).then_some((bx, by))
}

/// A deterministic stand-in for the random rotation the game's generator draws: the same reference always gets
/// the same angle in 0 to 2 pi.
#[must_use]
pub fn rotation_for(reference: u32) -> f32 {
    // splitmix64 of the form id
    let mut z = u64::from(reference).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let unit = (z >> 40) as f32 / (1_u64 << 24) as f32;
    unit * std::f32::consts::TAU
}

/// Blocks keyed by their south-west cell, built from references; `type_of` gives the type index of a base (and
/// `None` for one without a billboard), `shift` the billboard's position shift and scale factor.
///
/// A reference whose object id was already used is dropped (counted in the second value).
pub fn assemble(
    refs: &[TreeRef],
    south_west: (i32, i32),
    type_of: &dyn Fn(u32) -> Option<(u32, [f32; 3], f32)>,
) -> (BTreeMap<(i32, i32), TreeBlock>, usize) {
    let mut blocks: BTreeMap<(i32, i32), TreeBlock> = BTreeMap::new();
    let mut seen: HashSet<u32> = HashSet::new();
    let mut duplicates = 0;
    for r in refs {
        let Some((index, shift, factor)) = type_of(r.base) else {
            continue;
        };
        let Some(block) = block_of(cell_of(r.pos[0]), cell_of(r.pos[1]), south_west) else {
            continue;
        };
        let entry = blocks.entry(block).or_default();
        if r.hidden || r.fallen {
            continue;
        }
        if !seen.insert(r.reference & 0x00FF_FFFF) {
            duplicates += 1;
            continue;
        }
        entry.push(
            index,
            Tree {
                pos: [
                    r.pos[0] + shift[0],
                    r.pos[1] + shift[1],
                    r.pos[2] + shift[2],
                ],
                rotation: rotation_for(r.reference),
                scale: r.scale * factor,
                reference: r.reference,
            },
        );
    }
    (blocks, duplicates)
}

#[cfg(test)]
mod tests {
    use super::{
        Tree, TreeBlock, TreeType, assemble, block_of, parse_block, parse_types, tree_bases,
        tree_references, write_block, write_types,
    };
    use crate::plugin::{ReadOptions, read_with};
    use lodwright_testkit::plugin::{Group, PluginBuilder, Record};
    use lodwright_testkit::{covers, ids};

    #[test]
    fn a_block_and_a_type_list_round_trip_and_have_the_sizes_the_layout_gives() {
        covers(&[ids::trees::lod::LIST_BLOCKS_AND_ATLAS]);
        let mut block = TreeBlock::default();
        let t = |x: f32, r: u32| Tree {
            pos: [x, 2.0, 3.0],
            rotation: 1.5,
            scale: 1.25,
            reference: r,
        };
        block.push(3, t(1.0, 0x10));
        block.push(0, t(2.0, 0x11));
        block.push(3, t(3.0, 0x12));
        let bytes = write_block(&block);
        assert_eq!(bytes.len(), 4 + 2 * 8 + 3 * 32);
        assert_eq!(parse_block(&bytes).unwrap(), block);
        assert_eq!(block.groups[0].0, 3, "groups keep first-seen order");
        let types = vec![TreeType {
            index: 2,
            width: 150.0,
            height: 127.0,
            uv: [0.0, 0.53125, 0.328_125, 0.78125],
        }];
        let list = write_types(&types);
        assert_eq!(list.len(), 4 + 32);
        assert_eq!(parse_types(&list).unwrap(), types);
    }

    #[test]
    fn an_empty_block_is_four_zero_bytes_and_truncated_files_are_refused() {
        assert_eq!(write_block(&TreeBlock::default()), vec![0, 0, 0, 0]);
        assert!(parse_block(&[1, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0]).is_err());
        assert!(parse_types(&[2, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn blocks_align_to_the_world_corner_and_cells_below_it_are_out_of_range() {
        covers(&[ids::trees::lod::BLOCKS_ALIGN_TO_THE_WORLD_CORNER]);
        assert_eq!(block_of(-2, -2, (-2, -2)), Some((-2, -2)));
        assert_eq!(block_of(5, 5, (-2, -2)), Some((-2, -2)));
        assert_eq!(block_of(6, 6, (-2, -2)), Some((6, 6)));
        assert_eq!(block_of(-3, 0, (-2, -2)), None);
    }

    fn tree_plugin() -> crate::plugin::Plugin {
        let mut stat = Record::new(*b"STAT", 0x100).with_subrecord(*b"MODL", b"t.nif\0".to_vec());
        stat.flags = 0x40;
        let mut plain = Record::new(*b"STAT", 0x101).with_subrecord(*b"MODL", b"u.nif\0".to_vec());
        plain.flags = 0;
        let mut tree = Record::new(*b"TREE", 0x102).with_subrecord(*b"MODL", b"v.spt\0".to_vec());
        tree.flags = 0x40;
        let refr = |id: u32, base: u32, x: f32, flags: u32, rot_x: f32| {
            let mut data = Vec::new();
            for v in [x, 100.0, 50.0, rot_x, 0.0, 0.0] {
                data.extend_from_slice(&v.to_le_bytes());
            }
            let mut r = Record::new(*b"REFR", id)
                .with_subrecord(*b"NAME", base.to_le_bytes().to_vec())
                .with_subrecord(*b"DATA", data);
            r.flags = flags;
            r
        };
        let world = Group::new(1, 0x0000_0A00_u32.to_le_bytes())
            .with(refr(0x200, 0x100, 10.0, 0, 0.0))
            .with(refr(0x201, 0x101, 20.0, 0, 0.0))
            .with(refr(0x202, 0x102, 30.0, 0x800, 0.0))
            .with(refr(0x203, 0x100, 40.0, 0, 1.5));
        let bytes = PluginBuilder::new()
            .group(Group::new(0, *b"STAT").with(stat).with(plain))
            .group(Group::new(0, *b"TREE").with(tree))
            .group(world)
            .build();
        read_with(
            &bytes,
            ReadOptions {
                objects: true,
                ..ReadOptions::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn only_flagged_bases_become_types_and_hidden_or_fallen_references_leave_empty_blocks() {
        covers(&[ids::trees::lod::ONLY_FLAGGED_BASES]);
        let plugins = vec![("A.esm".to_owned(), tree_plugin())];
        let bases = tree_bases(&plugins).unwrap();
        assert_eq!(
            bases.iter().map(|b| b.form_id).collect::<Vec<_>>(),
            [0x100, 0x102]
        );
        let refs = tree_references(&plugins, 0x0A00, &bases).unwrap();
        assert_eq!(refs.len(), 3, "the unflagged static has no tree");
        assert!(refs[1].hidden && refs[2].fallen);
        let (blocks, duplicates) = assemble(&refs, (0, 0), &|base| {
            Some((u32::from(base != 0x100), [0.0, 0.0, -1.0], 2.0))
        });
        assert_eq!(duplicates, 0);
        let block = &blocks[&(0, 0)];
        assert_eq!(block.len(), 1);
        let tree = block.groups[0].1[0];
        assert_eq!(
            (tree.pos[2], tree.scale, tree.reference),
            (49.0, 2.0, 0x200)
        );
        assert!((0.0..std::f32::consts::TAU).contains(&tree.rotation));
    }
}
