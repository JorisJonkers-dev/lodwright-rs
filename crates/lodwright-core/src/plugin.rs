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
use crate::layers::{CellLayers, LayerReader};
use crate::texture::{LandTexture, TextureSet};

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
    /// Normals, colours and texture layers; `Some` only when read with [`ReadOptions::land_layers`].
    /// Texture `FormID`s are as stored in this plugin.
    pub layers: Option<CellLayers>,
    /// Whether the CELL's `DATA` has the has-water flag.
    pub has_water: bool,
    /// The CELL's `XCLW` water height, when it has one.
    pub water_height: Option<f32>,
}

/// The corners of a worldspace as stored in `NAM0` and `NAM9` (unit not established, see `wrld-bounds`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeclaredBounds {
    /// `NAM0`: x and y of the minimum corner.
    pub min: [f32; 2],
    /// `NAM9`: x and y of the maximum corner.
    pub max: [f32; 2],
}

/// The fields of a `WRLD` record the pipeline uses; `FormID`s as stored in the plugin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldRecord {
    /// `WNAM`: parent worldspace, when the record has one.
    pub parent: Option<u32>,
    /// `PNAM`: which data the child takes from its parent.
    pub parent_flags: Option<u16>,
    /// `DNAM`: default land height.
    pub default_land_height: Option<f32>,
    /// `DNAM`: default water height.
    pub default_water_height: Option<f32>,
    /// `NAM0` and `NAM9`, when both are present.
    pub bounds: Option<DeclaredBounds>,
}

/// A position, rotation (radians) and uniform scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// World position.
    pub pos: [f32; 3],
    /// Rotation about x, y and z in radians.
    pub rot: [f32; 3],
    /// Uniform scale.
    pub scale: f32,
}

/// A record that has a model and can appear in the distance: a static, a static collection and the like.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseObject {
    /// `FormID` as stored in this plugin.
    pub form_id: u32,
    /// Record signature, for example `STAT`.
    pub signature: [u8; 4],
    /// Editor id.
    pub editor_id: Option<String>,
    /// `MODL`: the model path relative to `meshes`.
    pub model: Option<String>,
    /// Record flags (bit 15 visible when distant, bit 6 has tree LOD).
    pub flags: u32,
    /// For a static collection: each part's static (`FormID` as stored) and its placements.
    pub parts: Vec<(u32, Vec<Placement>)>,
}

/// A placed reference.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedRef {
    /// `FormID` of the reference as stored in this plugin.
    pub form_id: u32,
    /// `NAME`: the base object, as stored in this plugin.
    pub base: u32,
    /// Record flags (bit 15 visible when distant, bit 16 high priority LOD, bit 11 initially disabled).
    pub flags: u32,
    /// Where it stands.
    pub placement: Placement,
}

/// A worldspace as one plugin defines or overrides it.
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    /// `FormID` of the `WRLD` record as stored in the file.
    pub form_id: u32,
    /// Editor id (`EDID`), when the plugin carries the `WRLD` record.
    pub editor_id: Option<String>,
    /// The `WRLD` record's fields, when the plugin carries the record.
    pub record: Option<WorldRecord>,
    /// Exterior cells with LAND, in file order.
    pub cells: Vec<LandCell>,
    /// Placed references of the world's cells; only when read with [`ReadOptions::objects`].
    pub refs: Vec<PlacedRef>,
}

/// What [`read`] returns.
#[derive(Debug, Clone, PartialEq)]
pub struct Plugin {
    /// Master file names, in order.
    pub masters: Vec<String>,
    /// Worldspaces this plugin touches.
    pub worlds: Vec<World>,
    /// `LTEX` records of this plugin (`FormID`s as stored).
    pub land_textures: Vec<LandTexture>,
    /// `TXST` records of this plugin (`FormID`s as stored).
    pub texture_sets: Vec<TextureSet>,
    /// Records with a model that may appear in the distance; only when read with [`ReadOptions::objects`].
    pub objects: Vec<BaseObject>,
}

/// What [`read_with`] keeps beyond heights.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadOptions {
    /// Keep vertex normals, vertex colours and texture layers of every LAND record. Off by default: they are
    /// about six times the size of the heights, so planning does not pay for them.
    pub land_layers: bool,
    /// Keep the placed references of worlds and the base objects that can be seen from afar.
    pub objects: bool,
}

/// Read a plugin.
///
/// # Errors
/// [`Error::Malformed`] when the bytes are not a plugin, [`Error::Truncated`] when they end early,
/// [`Error::Decompress`] for a compressed record that does not inflate, and the VHGT errors of
/// [`crate::land::decode_vhgt`].
pub fn read(bytes: &[u8]) -> Result<Plugin, Error> {
    read_with(bytes, ReadOptions::default())
}

/// Read a plugin, keeping what `options` asks for.
///
/// # Errors
/// As [`read`], plus the decode errors of the LAND layer subrecords when `options.land_layers` is set.
pub fn read_with(bytes: &[u8], options: ReadOptions) -> Result<Plugin, Error> {
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
    let mut walker = Walker {
        options,
        ..Walker::default()
    };
    walker.walk(rest, 0)?;
    Ok(Plugin {
        masters,
        worlds: walker.worlds,
        land_textures: walker.land_textures,
        texture_sets: walker.texture_sets,
        objects: walker.objects,
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
        // source: game-plugin-zlib-checksum
        let stream = &self.data[4..];
        // Some of the game's own records carry a zlib checksum that does not match their contents; the
        // deflate data inside is still sound, so a checksum mismatch falls back to inflating without the
        // two-byte header and the checksum, and the claimed size remains the check.
        let plain = match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stream, claimed) {
            Ok(plain) => plain,
            Err(e) if e.status == miniz_oxide::inflate::TINFLStatus::Adler32Mismatch => {
                miniz_oxide::inflate::decompress_to_vec_with_limit(
                    stream.get(2..).ok_or(Error::Decompress)?,
                    claimed,
                )
                .map_err(|_| Error::Decompress)?
            }
            Err(_) => return Err(Error::Decompress),
        };
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
    options: ReadOptions,
    land_textures: Vec<LandTexture>,
    texture_sets: Vec<TextureSet>,
    objects: Vec<BaseObject>,
    worlds: Vec<World>,
    world: Option<usize>,
    cell: Option<CellInfo>,
}

#[derive(Clone, Copy)]
struct CellInfo {
    x: i32,
    y: i32,
    has_water: bool,
    water_height: Option<f32>,
}

fn le_f32(bytes: &[u8], at: usize) -> Option<f32> {
    let chunk = bytes.get(at..at + 4)?;
    Some(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])).filter(|v| v.is_finite())
}

/// The fields of a `WRLD` record's subrecords.
fn world_record(data: &[u8]) -> Result<(Option<String>, WorldRecord), Error> {
    let mut editor_id = None;
    let mut record = WorldRecord::default();
    let (mut min, mut max) = (None, None);
    for (signature, payload) in subrecords(data)? {
        match &signature {
            // source: wrld-edid
            b"EDID" => editor_id = Some(zstring(payload)),
            // source: wrld-parent
            b"WNAM" if payload.len() >= 4 => {
                record.parent = Some(le_u32(payload, 0)).filter(|id| *id != 0);
            }
            b"PNAM" if payload.len() >= 2 => {
                record.parent_flags = Some(u16::from_le_bytes([payload[0], payload[1]]));
            }
            // source: wrld-land-data
            b"DNAM" if payload.len() >= 8 => {
                record.default_land_height = le_f32(payload, 0);
                record.default_water_height = le_f32(payload, 4);
            }
            // source: wrld-bounds
            b"NAM0" => {
                min = le_f32(payload, 0)
                    .zip(le_f32(payload, 4))
                    .map(|(x, y)| [x, y]);
            }
            b"NAM9" => {
                max = le_f32(payload, 0)
                    .zip(le_f32(payload, 4))
                    .map(|(x, y)| [x, y]);
            }
            _ => {}
        }
    }
    record.bounds = min.zip(max).map(|(min, max)| DeclaredBounds { min, max });
    Ok((editor_id, record))
}

impl Walker {
    fn world_index(&mut self, form_id: u32) -> usize {
        if let Some(index) = self.worlds.iter().position(|w| w.form_id == form_id) {
            return index;
        }
        self.worlds.push(World {
            form_id,
            editor_id: None,
            record: None,
            cells: Vec::new(),
            refs: Vec::new(),
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

    #[allow(clippy::too_many_lines)] // one arm per record type
    fn record(&mut self, record: &RawRecord<'_>) -> Result<(), Error> {
        if record.deleted() {
            return Ok(());
        }
        match &record.signature {
            b"WRLD" => {
                let index = self.world_index(record.form_id);
                let data = record.data()?;
                let (editor_id, fields) = world_record(&data)?;
                if editor_id.is_some() {
                    self.worlds[index].editor_id = editor_id;
                }
                self.worlds[index].record = Some(fields);
            }
            b"CELL" => {
                let data = record.data()?;
                self.cell = None;
                let (mut grid, mut has_water, mut water_height) = (None, false, None);
                for (signature, payload) in subrecords(&data)? {
                    match &signature {
                        // source: cell-xclc-grid
                        b"XCLC" if payload.len() >= 8 => {
                            grid = Some((
                                i32::from_le_bytes([
                                    payload[0], payload[1], payload[2], payload[3],
                                ]),
                                i32::from_le_bytes([
                                    payload[4], payload[5], payload[6], payload[7],
                                ]),
                            ));
                        }
                        // source: cell-water
                        b"DATA" => has_water = payload.first().is_some_and(|f| f & 0x02 != 0),
                        b"XCLW" => water_height = le_f32(payload, 0),
                        _ => {}
                    }
                }
                self.cell = grid.map(|(x, y)| CellInfo {
                    x,
                    y,
                    has_water,
                    water_height,
                });
            }
            b"LAND" => {
                let (Some(world), Some(cell)) = (self.world, self.cell) else {
                    return Ok(());
                };
                let data = record.data()?;
                let mut heights = None;
                let mut layers = self.options.land_layers.then(LayerReader::default);
                for (signature, payload) in subrecords(&data)? {
                    if &signature == b"VHGT" {
                        heights = Some(decode_vhgt(payload)?);
                    } else if let Some(reader) = &mut layers {
                        reader.subrecord(signature, payload)?;
                    }
                }
                if let Some(heights) = heights {
                    self.worlds[world].cells.push(LandCell {
                        x: cell.x,
                        y: cell.y,
                        heights,
                        layers: layers.map(LayerReader::finish),
                        has_water: cell.has_water,
                        water_height: cell.water_height,
                    });
                }
            }
            b"LTEX" => {
                let data = record.data()?;
                let mut texture = LandTexture::new(record.form_id);
                // source: ltex-texture-set
                for (signature, payload) in subrecords(&data)? {
                    match &signature {
                        b"EDID" => texture.editor_id = Some(zstring(payload)),
                        b"ICON" => texture.icon = Some(zstring(payload)),
                        b"TNAM" if payload.len() >= 4 => {
                            texture.texture_set = Some(le_u32(payload, 0));
                        }
                        _ => {}
                    }
                }
                self.land_textures.push(texture);
            }
            b"TXST" => {
                let data = record.data()?;
                let mut set = TextureSet::new(record.form_id);
                // source: txst-texture-paths
                for (signature, payload) in subrecords(&data)? {
                    match &signature {
                        b"EDID" => set.editor_id = Some(zstring(payload)),
                        b"TX00" => set.diffuse = Some(zstring(payload)),
                        b"TX01" => set.normal = Some(zstring(payload)),
                        _ => {}
                    }
                }
                self.texture_sets.push(set);
            }
            b"REFR" if self.options.objects => {
                let Some(world) = self.world else {
                    return Ok(());
                };
                let data = record.data()?;
                if let Some(placed) = placed_ref(record, &data)? {
                    self.worlds[world].refs.push(placed);
                }
            }
            sig if self.options.objects && BASE_SIGNATURES.contains(sig) => {
                let data = record.data()?;
                if let Some(base) = base_object(*sig, record, &data)? {
                    self.objects.push(base);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

// source: fnv-object-records
/// Record types whose base objects can carry the visible-when-distant flag and a model.
const BASE_SIGNATURES: [[u8; 4]; 9] = [
    *b"STAT", *b"SCOL", *b"ACTI", *b"DOOR", *b"MSTT", *b"FURN", *b"CONT", *b"TERM", *b"TREE",
];

fn placement_at(bytes: &[u8], at: usize) -> Option<Placement> {
    let f = |i: usize| le_f32(bytes, at + i * 4);
    Some(Placement {
        pos: [f(0)?, f(1)?, f(2)?],
        rot: [f(3)?, f(4)?, f(5)?],
        scale: 1.0,
    })
}

// source: fnv-object-records
fn placed_ref(record: &RawRecord<'_>, data: &[u8]) -> Result<Option<PlacedRef>, Error> {
    let (mut base, mut placement, mut scale) = (None, None, 1.0);
    for (signature, payload) in subrecords(data)? {
        match &signature {
            b"NAME" if payload.len() >= 4 => base = Some(le_u32(payload, 0)),
            b"DATA" if payload.len() >= 24 => placement = placement_at(payload, 0),
            b"XSCL" => scale = le_f32(payload, 0).unwrap_or(1.0),
            _ => {}
        }
    }
    Ok(base.zip(placement).map(|(base, mut placement)| {
        placement.scale = scale;
        PlacedRef {
            form_id: record.form_id,
            base,
            flags: record.flags,
            placement,
        }
    }))
}

// source: fnv-object-records
fn base_object(
    signature: [u8; 4],
    record: &RawRecord<'_>,
    data: &[u8],
) -> Result<Option<BaseObject>, Error> {
    let mut base = BaseObject {
        form_id: record.form_id,
        signature,
        editor_id: None,
        model: None,
        flags: record.flags,
        parts: Vec::new(),
    };
    for (sub, payload) in subrecords(data)? {
        match &sub {
            b"EDID" => base.editor_id = Some(zstring(payload)),
            b"MODL" => base.model = Some(zstring(payload)),
            b"ONAM" if signature == *b"SCOL" && payload.len() >= 4 => {
                base.parts.push((le_u32(payload, 0), Vec::new()));
            }
            b"DATA" if signature == *b"SCOL" => {
                if let Some(part) = base.parts.last_mut() {
                    for at in (0..payload.len() / 28).map(|i| i * 28) {
                        if let (Some(mut p), Some(sc)) =
                            (placement_at(payload, at), le_f32(payload, at + 24))
                        {
                            p.scale = sc;
                            part.1.push(p);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok((base.model.is_some() || !base.parts.is_empty()).then_some(base))
}

#[cfg(test)]
mod tests {
    use super::{ReadOptions, read, read_with};
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
    fn statics_and_placed_references_are_read_with_flags_and_placements() {
        use super::{ReadOptions, read_with};
        let f32s = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let stat = Record::new(*b"STAT", 0x0100_0001)
            .with_subrecord(*b"EDID", b"Wall\0".to_vec())
            .with_subrecord(*b"MODL", b"architecture\\wall.nif\0".to_vec());
        let mut stat = stat;
        stat.flags = 0x8000;
        let mut refr = Record::new(*b"REFR", 0x0100_0100)
            .with_subrecord(*b"NAME", 0x0100_0001_u32.to_le_bytes().to_vec())
            .with_subrecord(*b"XSCL", 2.0_f32.to_le_bytes().to_vec())
            .with_subrecord(*b"DATA", f32s(&[10.0, 20.0, 30.0, 0.0, 0.0, 1.5]));
        refr.flags = 0x8000;
        let world = Group::new(1, 0x0100_0010_u32.to_le_bytes()).with(refr);
        let bytes = PluginBuilder::new()
            .group(Group::new(0, *b"STAT").with(stat))
            .group(world)
            .build();
        let plugin = read_with(
            &bytes,
            ReadOptions {
                objects: true,
                ..ReadOptions::default()
            },
        )
        .unwrap();
        assert_eq!(plugin.objects.len(), 1);
        assert_eq!(
            plugin.objects[0].model.as_deref(),
            Some("architecture\\wall.nif")
        );
        assert_eq!(plugin.objects[0].flags & 0x8000, 0x8000);
        let r = &plugin.worlds[0].refs[0];
        assert_eq!((r.base, r.flags), (0x0100_0001, 0x8000));
        assert_eq!(r.placement.pos, [10.0, 20.0, 30.0]);
        assert_eq!(r.placement.scale, 2.0);
        let off = read_with(&bytes, ReadOptions::default()).unwrap();
        assert!(off.objects.is_empty() && off.worlds.iter().all(|w| w.refs.is_empty()));
    }

    #[test]
    fn a_static_collection_keeps_its_parts_and_placements() {
        use super::{ReadOptions, read_with};
        let mut data = Vec::new();
        for v in [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0, 0.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let scol = Record::new(*b"SCOL", 0x0100_0002)
            .with_subrecord(*b"MODL", b"c.nif\0".to_vec())
            .with_subrecord(*b"ONAM", 0x0100_0001_u32.to_le_bytes().to_vec())
            .with_subrecord(*b"DATA", data);
        let bytes = PluginBuilder::new()
            .group(Group::new(0, *b"SCOL").with(scol))
            .build();
        let plugin = read_with(
            &bytes,
            ReadOptions {
                objects: true,
                ..ReadOptions::default()
            },
        )
        .unwrap();
        let parts = &plugin.objects[0].parts;
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].1.len(), 1);
        assert_eq!(parts[0].1[0].scale, 0.5);
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
            for _ in 0..=rng.below(3) {
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

    fn layered_world() -> SyntheticWorld {
        use lodwright_testkit::land::{
            encode_atxt, encode_btxt, encode_vclr, encode_vnml, encode_vtxt,
        };
        let grid = vec![[1_u8, 2, 3]; 33 * 33];
        SyntheticWorld::new(0x0100_0010, "Mojave").cell_with_subrecords(
            0,
            0,
            Heights::flat(8.0),
            vec![
                (*b"VNML", encode_vnml(&grid)),
                (*b"VCLR", encode_vclr(&grid)),
                (*b"BTXT", encode_btxt(0x0100_0AAA, 0, 0)),
                (*b"ATXT", encode_atxt(0x0000_0BBB, 1, 1)),
                (*b"VTXT", encode_vtxt(&[(1, 0.5)])),
                (*b"ZZZZ", vec![9; 3]),
            ],
        )
    }

    #[test]
    fn layers_are_kept_only_when_asked_for() {
        let bytes = world_plugin(&["FalloutNV.esm"], &layered_world(), true);
        assert_eq!(read(&bytes).unwrap().worlds[0].cells[0].layers, None);
        let plugin = read_with(
            &bytes,
            ReadOptions {
                land_layers: true,
                ..ReadOptions::default()
            },
        )
        .unwrap();
        let layers = plugin.worlds[0].cells[0].layers.as_ref().unwrap();
        assert_eq!(layers.normals.as_ref().unwrap().get(3, 3), [1, 2, 3]);
        assert_eq!(layers.colors.as_ref().unwrap().get(0, 0), [1, 2, 3]);
        assert_eq!(layers.base[0], Some(0x0100_0AAA));
        assert_eq!(layers.alpha.len(), 1);
        assert_eq!(layers.alpha[0].quadrant, 1);
        assert_eq!(layers.alpha[0].point(1, 0), 0.5);
        // the heights are unchanged by asking for layers
        assert_eq!(plugin.worlds[0].cells[0].heights.get(1, 1), 8.0);
    }

    #[test]
    fn unknown_land_subrecords_are_still_skipped_with_layers_on() {
        let bytes = world_plugin(&[], &layered_world(), false);
        let plugin = read_with(
            &bytes,
            ReadOptions {
                land_layers: true,
                ..ReadOptions::default()
            },
        )
        .unwrap();
        assert_eq!(plugin.worlds[0].cells.len(), 1);
    }

    #[test]
    fn a_bad_layer_subrecord_is_an_error_only_when_layers_are_read() {
        let world = SyntheticWorld::new(0x10, "W").cell_with_subrecords(
            0,
            0,
            Heights::flat(8.0),
            vec![(*b"VNML", vec![0; 5])],
        );
        let bytes = world_plugin(&[], &world, false);
        assert!(read(&bytes).is_ok());
        assert_eq!(
            read_with(
                &bytes,
                ReadOptions {
                    land_layers: true,
                    ..ReadOptions::default()
                }
            ),
            Err(Error::SubrecordLength {
                what: "VNML",
                found: 5
            })
        );
    }

    #[test]
    fn ltex_and_txst_records_are_read_with_their_subrecords() {
        use lodwright_testkit::world::{ltex_record, record_group, txst_record};
        let bytes = PluginBuilder::new()
            .group(record_group(
                *b"TXST",
                vec![txst_record(0x20, "Set", Some("a.dds"), Some("a_n.dds"))],
            ))
            .group(record_group(
                *b"LTEX",
                vec![ltex_record(0x30, "Tex", Some("old.dds"), Some(0x20))],
            ))
            .build();
        let plugin = read(&bytes).unwrap();
        assert_eq!(plugin.texture_sets[0].diffuse.as_deref(), Some("a.dds"));
        assert_eq!(plugin.texture_sets[0].normal.as_deref(), Some("a_n.dds"));
        assert_eq!(plugin.land_textures[0].icon.as_deref(), Some("old.dds"));
        assert_eq!(plugin.land_textures[0].texture_set, Some(0x20));
        assert_eq!(plugin.land_textures[0].editor_id.as_deref(), Some("Tex"));
    }
}
