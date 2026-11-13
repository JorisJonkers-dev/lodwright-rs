// SPDX-License-Identifier: MPL-2.0
//! Plugin (`.esm`/`.esp`) reader: the TES4 header and master list, the group tree, zlib-compressed records, and
//! the three record types the terrain pipeline needs (`WRLD`, `CELL`, `LAND`). Everything else is skipped
//! unread.
//!
//! The reader keeps only exterior cells that have a LAND heightfield, grouped by worldspace. `FormID`s are kept as
//! stored in the file (the high byte indexes this plugin's master list); [`crate::worldspace`] resolves them
//! through the load order.
//!
//! Derived from: the UESP and `Tes4Mod` mod-file-format pages (`provenance/sources.toml`: `plugin-*`, `cell-xclc-grid`,
//! `wrld-edid`). Never from decompiled or unlicensed code.

use crate::Error;
use crate::land::{CellHeights, decode_vhgt};

// source: plugin-record-header
const RECORD_HEADER: usize = 24;
// source: plugin-group-layout
const GROUP_HEADER: usize = 24;
// source: plugin-record-compression
const COMPRESSED_FLAG: u32 = 0x0004_0000;
// source: plugin-deleted-flag
const DELETED_FLAG: u32 = 0x0000_0020;
// source: plugin-group-types
const GROUP_WORLD_CHILDREN: u32 = 1;
const MAX_DEPTH: usize = 32;
/// Largest decompressed record this reader accepts; the records it decodes are far smaller.
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

/// A cell with a LAND heightfield.
#[derive(Debug, Clone, PartialEq)]
pub struct LandCell {
    /// Cell grid x.
    pub x: i32,
    /// Cell grid y.
    pub y: i32,
    /// The decoded VHGT heights.
    pub heights: CellHeights,
}

/// A worldspace as one plugin defines or overrides it.
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    /// `FormID` of the `WRLD` record as stored in the file.
    pub form_id: u32,
    /// Editor id (`EDID`), when the plugin carries the `WRLD` record.
    pub editor_id: Option<String>,
    /// Exterior cells with LAND, in file order.
    pub cells: Vec<LandCell>,
}

/// What [`read`] returns.
#[derive(Debug, Clone, PartialEq)]
pub struct Plugin {
    /// Master file names, in order.
    pub masters: Vec<String>,
    /// Worldspaces this plugin touches.
    pub worlds: Vec<World>,
}

/// Read a plugin.
///
/// # Errors
/// [`Error::Malformed`] when the bytes are not a plugin, [`Error::Truncated`] when they end early,
/// [`Error::Decompress`] for a compressed record that does not inflate, and the VHGT errors of
/// [`crate::land::decode_vhgt`].
pub fn read(bytes: &[u8]) -> Result<Plugin, Error> {
    let (header, rest) = split_record(bytes, "the TES4 header")?;
    if &header.signature != b"TES4" {
        return Err(Error::Malformed("the first record is not TES4"));
    }
    let data = header.data()?;
    let mut masters = Vec::new();
    for (signature, payload) in subrecords(&data)? {
        // source: plugin-subrecord-layout
        if &signature == b"MAST" {
            masters.push(zstring(payload));
        }
    }
    let mut walker = Walker::default();
    walker.walk(rest, 0)?;
    Ok(Plugin {
        masters,
        worlds: walker.worlds,
    })
}

struct RawRecord<'a> {
    signature: [u8; 4],
    flags: u32,
    form_id: u32,
    data: &'a [u8],
}

impl RawRecord<'_> {
    fn deleted(&self) -> bool {
        self.flags & DELETED_FLAG != 0
    }

    /// The record data, inflated when the record is compressed.
    fn data(&self) -> Result<std::borrow::Cow<'_, [u8]>, Error> {
        if self.flags & COMPRESSED_FLAG == 0 {
            return Ok(std::borrow::Cow::Borrowed(self.data));
        }
        let Some(size_bytes) = self.data.first_chunk::<4>() else {
            return Err(Error::Decompress);
        };
        let claimed =
            usize::try_from(u32::from_le_bytes(*size_bytes)).map_err(|_| Error::Decompress)?;
        if claimed > MAX_RECORD_BYTES {
            return Err(Error::Decompress);
        }
        let plain =
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&self.data[4..], claimed)
                .map_err(|_| Error::Decompress)?;
        if plain.len() != claimed {
            return Err(Error::Decompress);
        }
        Ok(std::borrow::Cow::Owned(plain))
    }
}

fn le_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn split_record<'a>(
    bytes: &'a [u8],
    what: &'static str,
) -> Result<(RawRecord<'a>, &'a [u8]), Error> {
    if bytes.len() < RECORD_HEADER {
        return Err(Error::Truncated { what });
    }
    let size = usize::try_from(le_u32(bytes, 4)).map_err(|_| Error::Malformed("record size"))?;
    let end = RECORD_HEADER
        .checked_add(size)
        .filter(|end| *end <= bytes.len())
        .ok_or(Error::Truncated { what })?;
    let record = RawRecord {
        signature: [bytes[0], bytes[1], bytes[2], bytes[3]],
        flags: le_u32(bytes, 8),
        form_id: le_u32(bytes, 12),
        data: &bytes[RECORD_HEADER..end],
    };
    Ok((record, &bytes[end..]))
}

/// The subrecords of record data: signature and payload.
type Subrecord<'a> = ([u8; 4], &'a [u8]);

fn subrecords(data: &[u8]) -> Result<Vec<Subrecord<'_>>, Error> {
    let mut out = Vec::new();
    let mut at = 0;
    // source: plugin-xxxx-subrecord
    let mut extended: Option<usize> = None;
    while at < data.len() {
        if data.len() - at < 6 {
            return Err(Error::Truncated {
                what: "a subrecord header",
            });
        }
        let signature = [data[at], data[at + 1], data[at + 2], data[at + 3]];
        let short = usize::from(u16::from_le_bytes([data[at + 4], data[at + 5]]));
        let size = extended.take().unwrap_or(short);
        let start = at + 6;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= data.len())
            .ok_or(Error::Truncated {
                what: "a subrecord",
            })?;
        if &signature == b"XXXX" && size == 4 {
            extended = Some(
                usize::try_from(le_u32(data, start)).map_err(|_| Error::Malformed("XXXX size"))?,
            );
        } else {
            out.push((signature, &data[start..end]));
        }
        at = end;
    }
    Ok(out)
}

fn zstring(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[derive(Default)]
struct Walker {
    worlds: Vec<World>,
    world: Option<usize>,
    cell: Option<(i32, i32)>,
}

impl Walker {
    fn world_index(&mut self, form_id: u32) -> usize {
        if let Some(index) = self.worlds.iter().position(|w| w.form_id == form_id) {
            return index;
        }
        self.worlds.push(World {
            form_id,
            editor_id: None,
            cells: Vec::new(),
        });
        self.worlds.len() - 1
    }

    fn walk(&mut self, mut data: &[u8], depth: usize) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Malformed("groups nested too deeply"));
        }
        while !data.is_empty() {
            if data.len() < RECORD_HEADER {
                return Err(Error::Truncated {
                    what: "a record header",
                });
            }
            if &data[0..4] == b"GRUP" {
                let size =
                    usize::try_from(le_u32(data, 4)).map_err(|_| Error::Malformed("group size"))?;
                if size < GROUP_HEADER || size > data.len() {
                    return Err(Error::Malformed("group size is outside its parent"));
                }
                // source: plugin-group-types
                let group_type = le_u32(data, 12);
                let saved = (self.world, self.cell);
                if group_type == GROUP_WORLD_CHILDREN {
                    let label = le_u32(data, 8);
                    self.world = Some(self.world_index(label));
                    self.cell = None;
                }
                self.walk(&data[GROUP_HEADER..size], depth + 1)?;
                if group_type == GROUP_WORLD_CHILDREN {
                    (self.world, self.cell) = saved;
                }
                data = &data[size..];
            } else {
                let (record, rest) = split_record(data, "a record")?;
                self.record(&record)?;
                data = rest;
            }
        }
        Ok(())
    }

    fn record(&mut self, record: &RawRecord<'_>) -> Result<(), Error> {
        if record.deleted() {
            return Ok(());
        }
        match &record.signature {
            b"WRLD" => {
                let index = self.world_index(record.form_id);
                let data = record.data()?;
                // source: wrld-edid
                for (signature, payload) in subrecords(&data)? {
                    if &signature == b"EDID" {
                        self.worlds[index].editor_id = Some(zstring(payload));
                    }
                }
            }
            b"CELL" => {
                let data = record.data()?;
                self.cell = None;
                for (signature, payload) in subrecords(&data)? {
                    // source: cell-xclc-grid
                    if &signature == b"XCLC" && payload.len() >= 8 {
                        self.cell = Some((
                            i32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]),
                            i32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]),
                        ));
                    }
                }
            }
            b"LAND" => {
                let (Some(world), Some((x, y))) = (self.world, self.cell) else {
                    return Ok(());
                };
                let data = record.data()?;
                for (signature, payload) in subrecords(&data)? {
                    if &signature == b"VHGT" {
                        let heights = decode_vhgt(payload)?;
                        self.worlds[world].cells.push(LandCell { x, y, heights });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::read;
    use crate::Error;
    use crate::land::SCALE;
    use lodwright_testkit::land::Heights;
    use lodwright_testkit::plugin::{Group, PluginBuilder, Record};
    use lodwright_testkit::rng::Rng;
    use lodwright_testkit::world::{SyntheticWorld, world_plugin};

    fn two_cell_world() -> SyntheticWorld {
        SyntheticWorld::new(0x0100_0010, "Mojave")
            .cell(0, 0, Heights::flat(64.0))
            .cell(-1, 2, Heights::flat(-16.0))
    }

    #[test]
    fn masters_are_read_in_order() {
        let bytes = PluginBuilder::new()
            .master("FalloutNV.esm")
            .master("DeadMoney.esm")
            .build();
        assert_eq!(
            read(&bytes).unwrap().masters,
            ["FalloutNV.esm", "DeadMoney.esm"]
        );
    }

    #[test]
    fn a_world_with_cells_is_read_with_heights_and_editor_id() {
        let plugin = read(&world_plugin(&["FalloutNV.esm"], &two_cell_world(), false)).unwrap();
        assert_eq!(plugin.worlds.len(), 1);
        let world = &plugin.worlds[0];
        assert_eq!(world.form_id, 0x0100_0010);
        assert_eq!(world.editor_id.as_deref(), Some("Mojave"));
        assert_eq!(world.cells.len(), 2);
        assert_eq!((world.cells[1].x, world.cells[1].y), (-1, 2));
        assert_eq!(world.cells[0].heights.get(5, 5), 64.0);
        assert_eq!(world.cells[1].heights.get(32, 32), -16.0);
        assert!((world.cells[1].heights.get(0, 0) + 16.0).abs() < SCALE);
    }

    #[test]
    fn compressed_land_records_read_the_same_as_plain_ones() {
        let plain = read(&world_plugin(&[], &two_cell_world(), false)).unwrap();
        let packed = read(&world_plugin(&[], &two_cell_world(), true)).unwrap();
        assert_eq!(plain, packed);
    }

    #[test]
    fn cells_without_land_are_left_out() {
        let world =
            SyntheticWorld::new(0x10, "W")
                .cell_without_land(1, 1)
                .cell(2, 2, Heights::flat(8.0));
        let plugin = read(&world_plugin(&[], &world, false)).unwrap();
        assert_eq!(plugin.worlds[0].cells.len(), 1);
        assert_eq!(plugin.worlds[0].cells[0].x, 2);
    }

    #[test]
    fn a_plugin_without_groups_has_no_worlds() {
        let bytes = PluginBuilder::new()
            .record(Record::new(*b"STAT", 1))
            .build();
        assert_eq!(read(&bytes).unwrap().worlds.len(), 0);
    }

    #[test]
    fn a_deleted_land_record_is_skipped() {
        let world = SyntheticWorld::new(0x10, "W").cell(0, 0, Heights::flat(8.0));
        let mut bytes = world_plugin(&[], &world, false);
        let at = bytes.windows(4).position(|w| w == b"LAND").unwrap();
        bytes[at + 8] |= 0x20;
        assert_eq!(read(&bytes).unwrap().worlds[0].cells.len(), 0);
    }

    #[test]
    fn a_land_record_outside_a_cell_is_ignored() {
        let land = Record::new(*b"LAND", 5).with_subrecord(*b"VHGT", vec![0; 3]);
        let bytes = PluginBuilder::new()
            .group(Group::new(0, *b"LAND").with(land))
            .build();
        assert_eq!(read(&bytes).unwrap().worlds.len(), 0);
    }

    #[test]
    fn a_bad_vhgt_length_is_an_error() {
        let cell = Record::new(*b"CELL", 2).with_subrecord(*b"XCLC", [0_u8; 12].to_vec());
        let land = Record::new(*b"LAND", 3).with_subrecord(*b"VHGT", vec![0; 3]);
        let world = Group::new(1, 0x10_u32.to_le_bytes()).with(cell).with(land);
        let bytes = PluginBuilder::new().group(world).build();
        assert_eq!(read(&bytes), Err(Error::VhgtLength { found: 3 }));
    }

    #[test]
    fn extended_size_subrecords_are_honoured() {
        let big = vec![9_u8; 70_000];
        let mut data = Vec::new();
        data.extend_from_slice(b"XXXX");
        data.extend_from_slice(&4_u16.to_le_bytes());
        data.extend_from_slice(&70_000_u32.to_le_bytes());
        data.extend_from_slice(b"BIGB");
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(&big);
        data.extend_from_slice(b"EDID");
        data.extend_from_slice(&2_u16.to_le_bytes());
        data.extend_from_slice(b"x\0");
        let subs = super::subrecords(&data).unwrap();
        assert_eq!(subs.len(), 2);
        assert_eq!(subs[0].1.len(), 70_000);
        assert_eq!(subs[1].0, *b"EDID");
    }

    #[test]
    fn a_file_that_does_not_start_with_tes4_is_malformed() {
        let mut bytes = world_plugin(&[], &two_cell_world(), false);
        bytes[0] = b'X';
        assert_eq!(
            read(&bytes),
            Err(Error::Malformed("the first record is not TES4"))
        );
    }

    #[test]
    fn empty_and_tiny_inputs_are_truncated_not_panics() {
        assert!(matches!(read(&[]), Err(Error::Truncated { .. })));
        assert!(matches!(read(&[b'T'; 10]), Err(Error::Truncated { .. })));
    }

    #[test]
    fn every_prefix_of_a_valid_plugin_is_an_error_or_a_plugin_never_a_panic() {
        let bytes = world_plugin(&["A.esm"], &two_cell_world(), true);
        for len in 0..bytes.len() {
            let _ = read(&bytes[..len]);
        }
    }

    #[test]
    fn random_corruption_never_panics() {
        let original = world_plugin(&["A.esm"], &two_cell_world(), true);
        let mut rng = Rng::new(0x00C0_FFEE);
        for _ in 0..300 {
            let mut bytes = original.clone();
            for _ in 0..1 + rng.below(4) {
                let at = usize::try_from(rng.below(u32::try_from(bytes.len()).unwrap())).unwrap();
                bytes[at] = u8::try_from(rng.below(256)).unwrap();
            }
            let _ = read(&bytes);
        }
    }

    #[test]
    fn a_group_that_claims_more_than_its_parent_is_malformed() {
        let mut bytes = world_plugin(&[], &two_cell_world(), false);
        let at = bytes.windows(4).position(|w| w == b"GRUP").unwrap();
        bytes[at + 4..at + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            read(&bytes),
            Err(Error::Malformed("group size is outside its parent"))
        );
    }

    #[test]
    fn a_corrupt_compressed_record_is_a_decompress_error() {
        let mut bytes = world_plugin(&[], &two_cell_world(), true);
        let at = bytes.windows(4).position(|w| w == b"LAND").unwrap();
        // the stored plain length no longer matches the stream
        bytes[at + 24] ^= 0x01;
        assert_eq!(read(&bytes), Err(Error::Decompress));
    }

    #[test]
    fn groups_nested_beyond_the_limit_are_rejected() {
        let mut group = Group::new(0, *b"STAT");
        for _ in 0..40 {
            group = Group::new(0, *b"STAT").with(group);
        }
        let bytes = PluginBuilder::new().group(group).build();
        assert_eq!(
            read(&bytes),
            Err(Error::Malformed("groups nested too deeply"))
        );
    }
}
