// SPDX-License-Identifier: MPL-2.0
//! Synthetic worldspaces: a `WRLD` record and exterior cells with `LAND` heights, nested in groups the way a
//! real plugin nests them (top group, world children, exterior block, sub-block, cell children, temporary).

use crate::land::{Heights, encode_vhgt};
use crate::plugin::{Group, PluginBuilder, Record};

// source: plugin-group-types
const TOP: u32 = 0;
const WORLD_CHILDREN: u32 = 1;
const EXTERIOR_BLOCK: u32 = 4;
const EXTERIOR_SUB_BLOCK: u32 = 5;
const CELL_CHILDREN: u32 = 6;
const CELL_TEMPORARY: u32 = 9;

/// One exterior cell with its heights.
#[derive(Debug, Clone)]
pub struct SyntheticCell {
    /// Grid x.
    pub x: i32,
    /// Grid y.
    pub y: i32,
    /// Heights; `None` writes a cell without a LAND record.
    pub heights: Option<Heights>,
    /// Further LAND subrecords written after VHGT (VNML, VCLR, BTXT, ATXT, VTXT, anything else).
    pub extra: Vec<([u8; 4], Vec<u8>)>,
}

/// A worldspace to write.
#[derive(Debug, Clone)]
pub struct SyntheticWorld {
    /// In-file `FormID` of the `WRLD` record.
    pub form_id: u32,
    /// Editor id (`EDID`).
    pub editor_id: String,
    /// Exterior cells.
    pub cells: Vec<SyntheticCell>,
}

impl SyntheticWorld {
    /// An empty world.
    #[must_use]
    pub fn new(form_id: u32, editor_id: &str) -> Self {
        Self {
            form_id,
            editor_id: editor_id.to_owned(),
            cells: Vec::new(),
        }
    }

    /// Add a cell with LAND heights.
    #[must_use]
    pub fn cell(mut self, x: i32, y: i32, heights: Heights) -> Self {
        self.cells.push(SyntheticCell {
            x,
            y,
            heights: Some(heights),
            extra: Vec::new(),
        });
        self
    }

    /// Add a cell with LAND heights and further LAND subrecords after VHGT.
    #[must_use]
    pub fn cell_with_subrecords(
        mut self,
        x: i32,
        y: i32,
        heights: Heights,
        extra: Vec<([u8; 4], Vec<u8>)>,
    ) -> Self {
        self.cells.push(SyntheticCell {
            x,
            y,
            heights: Some(heights),
            extra,
        });
        self
    }

    /// Add a cell with no LAND record.
    #[must_use]
    pub fn cell_without_land(mut self, x: i32, y: i32) -> Self {
        self.cells.push(SyntheticCell {
            x,
            y,
            heights: None,
            extra: Vec::new(),
        });
        self
    }
}

fn zstring(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn i16_pair(a: i32, b: i32) -> [u8; 4] {
    let a = i16::try_from(a).unwrap_or(0).to_le_bytes();
    let b = i16::try_from(b).unwrap_or(0).to_le_bytes();
    [a[0], a[1], b[0], b[1]]
}

// source: plugin-group-types, cell-xclc-grid, wrld-edid
/// The top-level `WRLD` group of `world`. Cell `FormID`s are `world.form_id + 1 + index`; LAND ones follow them
/// at `world.form_id + 0x1000 + index`. `compress_land` writes the LAND records zlib-compressed.
#[must_use]
pub fn world_group(world: &SyntheticWorld, compress_land: bool) -> Group {
    let mut children = Group::new(WORLD_CHILDREN, world.form_id.to_le_bytes());
    for (index, cell) in world.cells.iter().enumerate() {
        let offset = u32::try_from(index).unwrap_or(0);
        let cell_id = world.form_id + 1 + offset;
        let mut xclc = Vec::new();
        xclc.extend_from_slice(&cell.x.to_le_bytes());
        xclc.extend_from_slice(&cell.y.to_le_bytes());
        xclc.extend_from_slice(&0_u32.to_le_bytes());
        let cell_record = Record::new(*b"CELL", cell_id)
            .with_subrecord(*b"DATA", vec![0])
            .with_subrecord(*b"XCLC", xclc);
        let mut cell_children = Group::new(CELL_CHILDREN, cell_id.to_le_bytes());
        if let Some(heights) = &cell.heights {
            let mut land = Record::new(*b"LAND", world.form_id + 0x1000 + offset)
                .with_subrecord(*b"DATA", 0x1_u32.to_le_bytes().to_vec())
                .with_subrecord(*b"VHGT", encode_vhgt(heights));
            for (signature, bytes) in &cell.extra {
                land = land.with_subrecord(*signature, bytes.clone());
            }
            if compress_land {
                land = land.compressed();
            }
            cell_children =
                cell_children.with(Group::new(CELL_TEMPORARY, cell_id.to_le_bytes()).with(land));
        }
        let sub_block = Group::new(EXTERIOR_SUB_BLOCK, i16_pair(cell.y >> 2, cell.x >> 2))
            .with(cell_record)
            .with(cell_children);
        children = children
            .with(Group::new(EXTERIOR_BLOCK, i16_pair(cell.y >> 3, cell.x >> 3)).with(sub_block));
    }
    Group::new(TOP, *b"WRLD")
        .with(
            Record::new(*b"WRLD", world.form_id)
                .with_subrecord(*b"EDID", zstring(&world.editor_id)),
        )
        .with(children)
}

// source: ltex-texture-set
/// A `LTEX` record: editor id, optional `ICON` path, optional `TNAM` texture-set `FormID`.
#[must_use]
pub fn ltex_record(
    form_id: u32,
    editor_id: &str,
    icon: Option<&str>,
    texture_set: Option<u32>,
) -> Record {
    let mut record = Record::new(*b"LTEX", form_id).with_subrecord(*b"EDID", zstring(editor_id));
    if let Some(icon) = icon {
        record = record.with_subrecord(*b"ICON", zstring(icon));
    }
    if let Some(set) = texture_set {
        record = record.with_subrecord(*b"TNAM", set.to_le_bytes().to_vec());
    }
    record
}

// source: txst-texture-paths
/// A `TXST` record with the base image (`TX00`) and normal map (`TX01`) paths.
#[must_use]
pub fn txst_record(
    form_id: u32,
    editor_id: &str,
    diffuse: Option<&str>,
    normal: Option<&str>,
) -> Record {
    let mut record = Record::new(*b"TXST", form_id).with_subrecord(*b"EDID", zstring(editor_id));
    if let Some(path) = diffuse {
        record = record.with_subrecord(*b"TX00", zstring(path));
    }
    if let Some(path) = normal {
        record = record.with_subrecord(*b"TX01", zstring(path));
    }
    record
}

/// A top-level group of flat records (for example the `LTEX` or `TXST` group).
#[must_use]
pub fn record_group(label: [u8; 4], records: Vec<Record>) -> Group {
    records
        .into_iter()
        .fold(Group::new(TOP, label), Group::with)
}

/// A complete plugin with `masters` and one worldspace.
#[must_use]
pub fn world_plugin(masters: &[&str], world: &SyntheticWorld, compress_land: bool) -> Vec<u8> {
    let mut builder = PluginBuilder::new();
    for master in masters {
        builder = builder.master(master);
    }
    builder.group(world_group(world, compress_land)).build()
}

#[cfg(test)]
mod tests {
    use super::{SyntheticWorld, world_group, world_plugin};
    use crate::land::Heights;
    use crate::plugin::Node;

    #[test]
    fn a_world_group_holds_the_wrld_record_then_the_world_children() {
        let world = SyntheticWorld::new(0x0100_0000, "Mojave").cell(0, 0, Heights::flat(0.0));
        let group = world_group(&world, false);
        assert_eq!(&group.label, b"WRLD");
        assert!(matches!(&group.children[0], Node::Record(r) if &r.signature == b"WRLD"));
        assert!(matches!(&group.children[1], Node::Group(g) if g.group_type == 1));
    }

    #[test]
    fn a_cell_without_land_has_no_temporary_group() {
        let world = SyntheticWorld::new(0x10, "W").cell_without_land(3, 4);
        let bytes = world_plugin(&[], &world, false);
        assert!(!bytes.windows(4).any(|w| w == b"LAND"));
        assert!(bytes.windows(4).any(|w| w == b"CELL"));
    }

    #[test]
    fn negative_cells_fit_the_block_labels() {
        let world = SyntheticWorld::new(0x10, "W").cell(-9, -17, Heights::flat(8.0));
        assert!(world_plugin(&["A.esm"], &world, true).len() > 100);
    }
}
