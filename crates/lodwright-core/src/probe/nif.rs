// SPDX-License-Identifier: MPL-2.0
//! NIF probe: the header, the block table with type counts, and per-shape counts, for version 20.2.0.7.
//!
//! The layouts come from the provenance rows named in the `source:` comments. A block whose fields do not add up
//! to the size the block table gives it is reported with `layout_matches: false`, so a wrong row shows up as a
//! number. Block type names are printed only when they are identifiers that start with `Ni`, `BS`, `bhk` or `hk`;
//! anything else is counted as `other`. Texture paths are counted and measured, never printed.

use std::collections::BTreeMap;

use super::reader::Reader;
use super::{Ident, ProbeError, Value};

const MAGIC: &[u8] = b"Gamebryo File Format, Version ";
const VERSION_20_2_0_7: u32 = 0x1402_0007;
const MAX_BLOCKS: usize = 1_000_000;
const MAX_LENGTHS_LISTED: usize = 256;
const TRUNCATED: ProbeError = ProbeError::new("the NIF file ends inside its header");

/// Probe the NIF `data`.
///
/// # Errors
/// [`ProbeError`] when `data` is not a NIF with a Gamebryo header line, or ends inside the header or the block data.
pub fn probe(data: &[u8]) -> Result<Value, ProbeError> {
    // source: nif-header-layout
    let line_end = data
        .iter()
        .take(128)
        .position(|b| *b == 0x0A)
        .ok_or(ProbeError::new("not a Gamebryo NIF: no header text line"))?;
    let text_version = data[..line_end]
        .strip_prefix(MAGIC)
        .and_then(Ident::version)
        .ok_or(ProbeError::new(
            "not a Gamebryo NIF: unexpected header text",
        ))?;
    let mut r = Reader::new(&data[line_end + 1..]);
    let version = r.u32().ok_or(TRUNCATED)?;
    let mut report = vec![
        ("header_version", Value::Ident(text_version)),
        ("version", Value::Uint(u64::from(version))),
        ("file_bytes", Value::count(data.len())),
    ];
    if version != VERSION_20_2_0_7 {
        report.push(("layout_supported", Value::Bool(false)));
        return Ok(Value::Map(report));
    }
    let endian = r.u8().ok_or(TRUNCATED)?;
    report.push(("endian_type", Value::Uint(u64::from(endian))));
    if endian != 1 {
        report.push(("layout_supported", Value::Bool(false)));
        return Ok(Value::Map(report));
    }
    report.push(("layout_supported", Value::Bool(true)));
    let user_version = r.u32().ok_or(TRUNCATED)?;
    let block_count = r.len32().ok_or(TRUNCATED)?;
    report.push(("user_version", Value::Uint(u64::from(user_version))));
    report.push(("block_count", Value::count(block_count)));
    if user_version >= 3 {
        let user_version_2 = r.u32().ok_or(TRUNCATED)?;
        report.push(("user_version_2", Value::Uint(u64::from(user_version_2))));
        skip_export_info(&mut r, user_version_2)?;
    }

    let table = read_table(&mut r, block_count)?;
    report.push(("string_count", Value::count(table.string_count)));
    report.push((
        "max_string_length",
        Value::Uint(u64::from(table.max_string_length)),
    ));
    report.push(("group_count", Value::count(table.group_count)));
    let (type_names, type_of, sizes) = (table.type_names, table.type_of, table.sizes);

    let blocks_start = data.len() - r.remaining();
    let mut total = 0_usize;
    for size in &sizes {
        total = total
            .checked_add(*size)
            .ok_or(ProbeError::new("the NIF block sizes overflow"))?;
    }
    let blocks_end = blocks_start
        .checked_add(total)
        .filter(|end| *end <= data.len())
        .ok_or(ProbeError::new("the NIF file ends inside its block data"))?;
    report.push(("block_bytes", Value::count(total)));
    report.push(("trailing_bytes", Value::count(data.len() - blocks_end)));

    let mut counts: BTreeMap<Option<Ident>, u64> = BTreeMap::new();
    let mut shapes = vec![];
    let mut texture = TextureStats::default();
    let mut at = blocks_start;
    for (index, (type_index, size)) in type_of.iter().zip(&sizes).enumerate() {
        let name = *type_names.get(*type_index).ok_or(ProbeError::new(
            "a NIF block names a block type that does not exist",
        ))?;
        let block = &data[at..at + size];
        at += size;
        *counts.entry(printable_type(name)).or_default() += 1;
        match name {
            b"NiTriShapeData" => shapes.push(shape(index, "NiTriShapeData", block, false)),
            b"NiTriStripsData" => shapes.push(shape(index, "NiTriStripsData", block, true)),
            b"BSShaderTextureSet" => texture.add(block),
            _ => {}
        }
    }
    report.push((
        "block_types",
        Value::List(
            counts
                .into_iter()
                .map(|(name, count)| {
                    Value::Map(vec![
                        ("name", name.map_or(Value::Label("other"), Value::Ident)),
                        ("count", Value::Uint(count)),
                    ])
                })
                .collect(),
        ),
    ));
    report.push(("geometry", Value::List(shapes)));
    report.push(("texture_paths", texture.into_value()));
    Ok(Value::Map(report))
}

/// The block table of the header.
struct Table<'a> {
    type_names: Vec<&'a [u8]>,
    type_of: Vec<usize>,
    sizes: Vec<usize>,
    string_count: usize,
    max_string_length: u32,
    group_count: usize,
}

// source: probe-nif-block-table
fn read_table<'a>(r: &mut Reader<'a>, block_count: usize) -> Result<Table<'a>, ProbeError> {
    if block_count > MAX_BLOCKS {
        return Err(ProbeError::new("the NIF claims an impossible block count"));
    }
    let type_count = usize::from(r.u16().ok_or(TRUNCATED)?);
    if type_count.saturating_mul(4) > r.remaining() {
        return Err(TRUNCATED);
    }
    let mut type_names = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let len = r.len32().ok_or(TRUNCATED)?;
        type_names.push(r.take(len).ok_or(TRUNCATED)?);
    }
    if block_count.saturating_mul(6) > r.remaining() {
        return Err(TRUNCATED);
    }
    let mut type_of = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        type_of.push(usize::from(r.u16().ok_or(TRUNCATED)?));
    }
    let mut sizes = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        sizes.push(r.len32().ok_or(TRUNCATED)?);
    }
    let string_count = r.len32().ok_or(TRUNCATED)?;
    let max_string_length = r.u32().ok_or(TRUNCATED)?;
    if string_count.saturating_mul(4) > r.remaining() {
        return Err(TRUNCATED);
    }
    for _ in 0..string_count {
        let len = r.len32().ok_or(TRUNCATED)?;
        r.skip(len).ok_or(TRUNCATED)?;
    }
    let group_count = r.len32().ok_or(TRUNCATED)?;
    r.skip_items(group_count, 4).ok_or(TRUNCATED)?;
    Ok(Table {
        type_names,
        type_of,
        sizes,
        string_count,
        max_string_length,
        group_count,
    })
}

// source: nif-header-layout
fn skip_export_info(r: &mut Reader<'_>, user_version_2: u32) -> Result<(), ProbeError> {
    skip_short(r)?; // author
    if user_version_2 > 130 {
        r.skip(4).ok_or(TRUNCATED)?;
    }
    if user_version_2 < 131 {
        skip_short(r)?; // process script
    }
    skip_short(r)?; // export script
    if user_version_2 >= 103 {
        skip_short(r)?; // max filepath
    }
    Ok(())
}

fn skip_short(r: &mut Reader<'_>) -> Result<(), ProbeError> {
    let len = usize::from(r.u8().ok_or(TRUNCATED)?);
    r.skip(len).ok_or(TRUNCATED)
}

/// The block type name when it is safe to print, `None` (counted as `other`) otherwise.
fn printable_type(name: &[u8]) -> Option<Ident> {
    const PREFIXES: [&[u8]; 4] = [b"Ni", b"BS", b"bhk", b"hk"];
    if PREFIXES.iter().any(|p| name.starts_with(p)) {
        Ident::checked(name)
    } else {
        None
    }
}

/// What was read of one geometry data block.
#[derive(Default)]
struct Geometry {
    vertices: Option<u16>,
    keep_flags: Option<u8>,
    compress_flags: Option<u8>,
    vector_flags: Option<u16>,
    has_normals: Option<bool>,
    has_vertex_colors: Option<bool>,
    triangles: Option<u16>,
    triangle_points: Option<u32>,
    strips: Option<u16>,
    strip_points: Option<u64>,
}

// source: probe-nif-geometry-data-layout, nif-trishape-strips-data
fn read_geometry(r: &mut Reader<'_>, strips: bool, g: &mut Geometry) -> Option<()> {
    r.skip(4)?; // group id
    let vertices = r.u16()?;
    g.vertices = Some(vertices);
    let n = usize::from(vertices);
    g.keep_flags = Some(r.u8()?);
    g.compress_flags = Some(r.u8()?);
    if r.u8()? != 0 {
        r.skip_items(n, 12)?; // vertices
    }
    let flags = r.u16()?;
    g.vector_flags = Some(flags);
    let has_normals = r.u8()? != 0;
    g.has_normals = Some(has_normals);
    if has_normals {
        r.skip_items(n, 12)?; // normals
        if flags & 4096 != 0 {
            r.skip_items(n, 24)?; // tangents and bitangents
        }
    }
    r.skip(16)?; // bounding sphere
    let has_colors = r.u8()? != 0;
    g.has_vertex_colors = Some(has_colors);
    if has_colors {
        r.skip_items(n, 16)?;
    }
    if flags & 1 != 0 {
        r.skip_items(n, 8)?; // the one UV set
    }
    r.skip(2)?; // consistency flags
    r.skip(4)?; // additional data reference
    g.triangles = Some(r.u16()?);
    if strips {
        let count = r.u16()?;
        g.strips = Some(count);
        let mut lengths = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            lengths.push(r.u16()?);
        }
        g.strip_points = Some(lengths.iter().map(|l| u64::from(*l)).sum());
        if r.u8()? != 0 {
            for length in lengths {
                r.skip_items(usize::from(length), 2)?;
            }
        }
    } else {
        g.triangle_points = Some(r.u32()?);
        if r.u8()? != 0 {
            r.skip_items(usize::from(g.triangles?), 6)?;
        }
        let groups = r.u16()?;
        for _ in 0..groups {
            let members = r.u16()?;
            r.skip_items(usize::from(members), 2)?;
        }
    }
    Some(())
}

fn shape(index: usize, type_name: &'static str, block: &[u8], strips: bool) -> Value {
    let mut g = Geometry::default();
    let mut r = Reader::new(block);
    let complete = read_geometry(&mut r, strips, &mut g).is_some() && r.remaining() == 0;
    let mut fields = vec![
        ("block", Value::count(index)),
        ("type", Value::Label(type_name)),
        (
            "topology",
            Value::Label(if strips { "strip" } else { "list" }),
        ),
        ("index_bits", Value::Uint(16)),
    ];
    let mut put = |name: &'static str, value: Option<Value>| {
        if let Some(value) = value {
            fields.push((name, value));
        }
    };
    put("vertices", g.vertices.map(|v| Value::Uint(u64::from(v))));
    put("triangles", g.triangles.map(|v| Value::Uint(u64::from(v))));
    put(
        "triangle_points",
        g.triangle_points.map(|v| Value::Uint(u64::from(v))),
    );
    put("strips", g.strips.map(|v| Value::Uint(u64::from(v))));
    put("strip_points", g.strip_points.map(Value::Uint));
    put(
        "vector_flags",
        g.vector_flags.map(|v| Value::Uint(u64::from(v))),
    );
    put(
        "uv_sets",
        g.vector_flags.map(|v| Value::Uint(u64::from(v & 1))),
    );
    put("has_normals", g.has_normals.map(Value::Bool));
    put(
        "has_tangents",
        g.vector_flags
            .zip(g.has_normals)
            .map(|(flags, normals)| Value::Bool(normals && flags & 4096 != 0)),
    );
    put("has_vertex_colors", g.has_vertex_colors.map(Value::Bool));
    put(
        "keep_flags",
        g.keep_flags.map(|v| Value::Uint(u64::from(v))),
    );
    put(
        "compress_flags",
        g.compress_flags.map(|v| Value::Uint(u64::from(v))),
    );
    fields.push(("layout_matches", Value::Bool(complete)));
    Value::Map(fields)
}

/// Texture path statistics over every `BSShaderTextureSet` block.
#[derive(Default)]
struct TextureStats {
    sets: u64,
    paths: u64,
    lengths: Vec<Value>,
    layout_matches: bool,
    seen_mismatch: bool,
}

impl TextureStats {
    // source: nif-texture-set
    fn add(&mut self, block: &[u8]) {
        self.sets += 1;
        let mut r = Reader::new(block);
        let ok = self.read(&mut r).is_some() && r.remaining() == 0;
        self.seen_mismatch |= !ok;
        self.layout_matches = !self.seen_mismatch;
    }

    fn read(&mut self, r: &mut Reader<'_>) -> Option<()> {
        let count = r.len32()?;
        if count.saturating_mul(4) > r.remaining() {
            return None;
        }
        for _ in 0..count {
            let len = r.len32()?;
            r.skip(len)?;
            if len > 0 {
                self.paths += 1;
                if self.lengths.len() < MAX_LENGTHS_LISTED {
                    self.lengths.push(Value::count(len));
                }
            }
        }
        Some(())
    }

    fn into_value(self) -> Value {
        Value::Map(vec![
            ("sets", Value::Uint(self.sets)),
            ("paths", Value::Uint(self.paths)),
            ("lengths", Value::List(self.lengths)),
            (
                "layout_matches",
                Value::Bool(self.sets == 0 || self.layout_matches),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::probe;
    use crate::probe::Value;
    use lodwright_testkit::probe_nif::{
        GeometryOptions, NifBuilder, texture_set, tri_shape_data, tri_strips_data,
    };

    fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
        let Value::Map(fields) = value else {
            panic!("not a map: {value:?}")
        };
        &fields.iter().find(|(n, _)| *n == name).unwrap().1
    }

    fn sample() -> Vec<u8> {
        NifBuilder::new()
            .string("alpha")
            .block("NiNode", vec![0; 8])
            .block(
                "NiTriShapeData",
                tri_shape_data(4, 2, GeometryOptions::default()),
            )
            .block(
                "NiTriStripsData",
                tri_strips_data(5, &[4, 3], GeometryOptions::default()),
            )
            .block("BSShaderTextureSet", texture_set(&["a.dds", "", "bcd.dds"]))
            .block("NiNode", vec![0; 8])
            .build()
    }

    #[test]
    fn the_header_and_block_table_are_reported_as_numbers() {
        let report = probe(&sample()).unwrap();
        assert_eq!(field(&report, "version"), &Value::Uint(0x1402_0007));
        assert_eq!(field(&report, "user_version"), &Value::Uint(11));
        assert_eq!(field(&report, "user_version_2"), &Value::Uint(34));
        assert_eq!(field(&report, "block_count"), &Value::Uint(5));
        assert_eq!(field(&report, "string_count"), &Value::Uint(1));
        assert_eq!(field(&report, "trailing_bytes"), &Value::Uint(4));
        let json = report.to_json();
        assert!(json.contains(r#"{"name":"NiNode","count":2}"#), "{json}");
        assert!(
            json.contains(r#"{"name":"NiTriShapeData","count":1}"#),
            "{json}"
        );
    }

    #[test]
    fn shapes_report_counts_index_width_and_topology() {
        let report = probe(&sample()).unwrap();
        let Value::List(shapes) = field(&report, "geometry") else {
            panic!("no geometry")
        };
        assert_eq!(shapes.len(), 2);
        let list = shapes[0].to_json();
        assert!(
            list.contains(r#""vertices":4,"triangles":2,"triangle_points":6"#),
            "{list}"
        );
        assert!(
            list.contains(r#""topology":"list","index_bits":16"#),
            "{list}"
        );
        assert!(list.contains(r#""layout_matches":true"#), "{list}");
        let strip = shapes[1].to_json();
        assert!(strip.contains(r#""topology":"strip""#), "{strip}");
        assert!(strip.contains(r#""strips":2,"strip_points":7"#), "{strip}");
        assert!(strip.contains(r#""layout_matches":true"#), "{strip}");
    }

    #[test]
    fn attribute_flags_are_reported() {
        let options = GeometryOptions {
            vector_flags: 1 | 4096,
            has_normals: true,
            has_vertex_colors: true,
        };
        let bytes = NifBuilder::new()
            .block("NiTriShapeData", tri_shape_data(3, 1, options))
            .build();
        let report = probe(&bytes).unwrap();
        let json = report.to_json();
        assert!(json.contains(r#""vector_flags":4097,"uv_sets":1,"has_normals":true,"has_tangents":true,"has_vertex_colors":true"#), "{json}");
    }

    #[test]
    fn texture_paths_are_counted_and_measured_never_printed() {
        let report = probe(&sample()).unwrap();
        let paths = field(&report, "texture_paths");
        assert_eq!(field(paths, "sets"), &Value::Uint(1));
        assert_eq!(field(paths, "paths"), &Value::Uint(2));
        assert_eq!(
            field(paths, "lengths"),
            &Value::List(vec![Value::Uint(5), Value::Uint(7)])
        );
        assert!(!report.to_json().contains("a.dds"));
    }

    #[test]
    fn a_block_that_does_not_match_the_layout_is_flagged() {
        let mut shape = tri_shape_data(4, 2, GeometryOptions::default());
        shape.push(0); // one byte too many
        let bytes = NifBuilder::new().block("NiTriShapeData", shape).build();
        let json = probe(&bytes).unwrap().to_json();
        assert!(json.contains(r#""layout_matches":false"#), "{json}");
        let short = NifBuilder::new()
            .block("NiTriShapeData", vec![0, 0, 0, 0, 9, 0])
            .build();
        let json = probe(&short).unwrap().to_json();
        assert!(json.contains(r#""vertices":9"#) && json.contains(r#""layout_matches":false"#));
    }

    #[test]
    fn free_text_never_reaches_the_report() {
        let marker = "ZZMARKERZZ";
        let bytes = NifBuilder::new()
            .author(marker)
            .string(marker)
            .block(marker, vec![1, 2, 3])
            .block("NiMARKERwith space", vec![])
            .block("BSShaderTextureSet", texture_set(&[marker]))
            .build();
        let report = probe(&bytes).unwrap();
        for text in [report.to_json(), report.to_text()] {
            assert!(!text.contains(marker), "{text}");
            assert!(!text.contains("MARKER"), "{text}");
        }
        assert!(report.to_json().contains(r#"{"name":"other","count":2}"#));
    }

    #[test]
    fn an_unsupported_version_reports_the_number_only() {
        let mut bytes = NifBuilder::new().build();
        bytes[39..43].copy_from_slice(&0x0A01_0001_u32.to_le_bytes());
        let report = probe(&bytes).unwrap();
        assert_eq!(field(&report, "version"), &Value::Uint(0x0A01_0001));
        assert_eq!(field(&report, "layout_supported"), &Value::Bool(false));
        let mut big = NifBuilder::new().build();
        big[43] = 0; // big endian
        assert_eq!(
            field(&probe(&big).unwrap(), "layout_supported"),
            &Value::Bool(false)
        );
    }

    #[test]
    fn garbage_and_truncation_are_errors_at_every_length() {
        for garbage in [
            &b""[..],
            b"not a nif",
            &[0xFF; 300][..],
            b"Gamebryo File Format, Version abc\n",
        ] {
            assert!(probe(garbage).is_err());
        }
        let bytes = sample();
        for cut in 0..bytes.len() - 4 {
            assert!(probe(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        assert!(probe(&bytes).is_ok());
        assert!(probe(&bytes[..bytes.len() - 4]).is_ok());
    }

    #[test]
    fn impossible_counts_are_refused_without_allocating() {
        let mut bytes = NifBuilder::new().build();
        bytes[48..52].copy_from_slice(&u32::MAX.to_le_bytes()); // block count
        assert!(probe(&bytes).is_err());
    }
}
