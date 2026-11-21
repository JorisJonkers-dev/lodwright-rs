// SPDX-License-Identifier: GPL-3.0-only
//! The terrain LOD block NIF: the eight-block structure the game's own terrain blocks have.
//!
//! Layout from the row `game-lod-terrain-block-nif` in `provenance/sources.toml`. Blocks whose bytes are
//! constants (the lighting property, its flags, the multibound) are written from the named constants below; the
//! meaning of some flag words is not known, only that every measured block carries them unchanged.

use crate::block::{AvObject, Body, GeometryData, NiTriShape, NiTriShapeData, ObjectNet};
use crate::error::{NifError, Result};
use crate::file::{Block, ExportInfo, Header, NifFile};

/// Block order in the file.
const TYPES: [&[u8]; 8] = [
    b"BSMultiBoundNode",
    b"NiTriShape",
    b"BSShaderPPLightingProperty",
    b"BSShaderTextureSet",
    b"NiTriShapeData",
    b"NiAdditionalGeometryData",
    b"BSMultiBound",
    b"BSMultiBoundAABB",
];

// source: game-lod-terrain-block-nif
/// Node flags of the root, and of the shape.
pub(crate) const ROOT_FLAGS: u32 = 0x0008_080e;
/// Flags of the terrain shape (the same word as the root's in the measured files).
pub(crate) const SHAPE_FLAGS: u32 = 524_302;

// source: game-lod-terrain-block-nif
/// The `BSShaderPPLightingProperty` after its empty name, extra data and controller: flags, shader type and the
/// remaining words, identical in every measured block.
const LIGHTING_PROPERTY_TAIL: [u8; 42] = [
    0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x30, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x41, 0x00, 0x00, 0x80, 0x3f,
];

/// Everything a terrain block needs.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainBlock<'a> {
    /// South-west corner of the block in world units (the root node's translation).
    pub origin: (f32, f32),
    /// Edge length of the block in world units (level times 4096).
    pub size: f32,
    /// Vertices in block-local units: x and y within `0..=size`, z the height.
    pub vertices: &'a [[f32; 3]],
    /// Triangle corner indices, three per triangle.
    pub indices: &'a [u16],
    /// The diffuse and the normal map path written into the texture set.
    pub textures: [&'a str; 2],
    /// A water plane written as a second shape, when the block has one.
    pub water: Option<WaterShape<'a>>,
}

/// The water plane of a terrain block (`game-lod-water-shape`): a shape without properties whose data has
/// positions only, relative to the block origin like the terrain's.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterShape<'a> {
    /// Positions relative to the block origin.
    pub vertices: &'a [[f32; 3]],
    /// Triangle corner indices, three per triangle.
    pub indices: &'a [u16],
    /// For a level 4 block, the triangle count of each of its 16 cells in the order x * 4 + y (the triangles
    /// must come in that order); the shape is then a `BSSegmentedTriShape`. `None` writes a plain `NiTriShape`.
    pub segments: Option<[u32; 16]>,
    /// Write the game's empty one-vertex segmented shape between the terrain and the water.
    pub placeholder: bool,
}

pub(crate) fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn root_node(origin: (f32, f32), children: &[u32], multibound: u32) -> Vec<u8> {
    // source: game-lod-terrain-block-nif
    let mut b = Vec::with_capacity(92);
    put_u32(&mut b, u32::MAX); // name
    put_u32(&mut b, 0); // extra data
    put_u32(&mut b, u32::MAX); // controller
    put_u32(&mut b, ROOT_FLAGS);
    for v in [origin.0, origin.1, 0.0] {
        put_f32(&mut b, v);
    }
    for v in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0] {
        put_f32(&mut b, v); // rotation, then scale
    }
    put_u32(&mut b, 0); // properties
    put_u32(&mut b, u32::MAX); // collision
    put_u32(&mut b, u32::try_from(children.len()).unwrap_or(0)); // children
    for c in children {
        put_u32(&mut b, *c); // the shapes
    }
    put_u32(&mut b, 0); // effects
    put_u32(&mut b, multibound); // the multibound
    b
}

pub(crate) fn texture_set(paths: [&str; 2]) -> Vec<u8> {
    // source: game-lod-terrain-block-nif
    let mut b = Vec::new();
    put_u32(&mut b, 6);
    for p in paths {
        put_u32(&mut b, u32::try_from(p.len()).unwrap_or(u32::MAX));
        b.extend_from_slice(p.as_bytes());
    }
    b.extend_from_slice(&[0; 16]); // four empty slots
    b
}

fn additional_data(heights: &[f32]) -> Result<Vec<u8>> {
    // source: game-lod-terrain-block-nif
    let n =
        u16::try_from(heights.len()).map_err(|_| NifError::Inconsistent("too many vertices"))?;
    let bytes = u32::from(n) * 4;
    let mut b = Vec::with_capacity(56 + heights.len() * 4);
    b.extend_from_slice(&n.to_le_bytes());
    for v in [1, 1, 4, bytes, 4, 0, 0] {
        put_u32(&mut b, v); // block infos, type, unit size, total size, stride, index, offset
    }
    b.push(2);
    put_u32(&mut b, 1); // blocks
    b.push(1); // has data
    put_u32(&mut b, bytes);
    for v in [1, 0, 1, 4] {
        put_u32(&mut b, v); // offsets (count, 0), data sizes (count, 4)
    }
    for h in heights {
        put_f32(&mut b, *h);
    }
    Ok(b)
}

pub(crate) fn tri_shape() -> NiTriShape {
    NiTriShape {
        av: AvObject {
            net: ObjectNet {
                name: -1,
                extra_data: Vec::new(),
                controller: -1,
            },
            flags: SHAPE_FLAGS,
            translation: [0.0; 3],
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
            properties: vec![2],
            collision_object: -1,
        },
        data: 4,
        skin_instance: -1,
        material_names: Vec::new(),
        material_extra_data: Vec::new(),
        active_material: 0,
        material_needs_update: 0,
        shader_property: -1,
        alpha_property: -1,
    }
}

pub(crate) fn file_header(types: &[&[u8]]) -> Header {
    Header {
        user_version: 11,
        bethesda: Some((
            34,
            ExportInfo {
                author: vec![0],
                unknown_int: 0,
                process_script: vec![0],
                export_script: vec![0],
                max_filepath: Vec::new(),
            },
        )),
        block_types: types.iter().map(|t| t.to_vec()).collect(),
        strings: Vec::new(),
        groups: Vec::new(),
    }
}

const SEGMENTED: &[u8] = b"BSSegmentedTriShape";

/// The water shape's segment table (if any) and data block.
fn water_blocks(water: &WaterShape<'_>) -> Result<(Option<[u32; 16]>, NiTriShapeData)> {
    let verts = water.vertices;
    if verts.is_empty() || water.indices.is_empty() || !water.indices.len().is_multiple_of(3) {
        return Err(NifError::Inconsistent("a water plane needs triangles"));
    }
    if water.indices.iter().any(|i| usize::from(*i) >= verts.len()) {
        return Err(NifError::Inconsistent(
            "a water triangle index is out of range",
        ));
    }
    let triangles = u16::try_from(water.indices.len() / 3)
        .map_err(|_| NifError::Inconsistent("too many water triangles"))?;
    if let Some(segments) = water.segments
        && segments.iter().sum::<u32>() != u32::from(triangles)
    {
        return Err(NifError::Inconsistent(
            "the water segments do not add up to the triangle count",
        ));
    }
    let (mut low, mut high) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for v in verts {
        for k in 0..3 {
            low[k] = low[k].min(v[k]);
            high[k] = high[k].max(v[k]);
        }
    }
    let center = [0, 1, 2].map(|k| f32::midpoint(low[k], high[k]));
    let radius = verts
        .iter()
        .map(|v| {
            ((v[0] - center[0]).powi(2) + (v[1] - center[1]).powi(2) + (v[2] - center[2]).powi(2))
                .sqrt()
        })
        .fold(0.0_f32, f32::max);
    let data = NiTriShapeData {
        geometry: GeometryData {
            group_id: 0,
            num_vertices: u16::try_from(verts.len())
                .map_err(|_| NifError::Inconsistent("too many water vertices"))?,
            keep_flags: 0,
            compress_flags: 0,
            vertices: Some(verts.to_vec()),
            vector_flags: 0,
            material_crc: 0,
            normals: None,
            tangents: None,
            center,
            radius,
            vertex_colors: None,
            uv_sets: Vec::new(),
            consistency: 0,
            additional_data: -1,
        },
        num_triangles: triangles,
        num_triangle_points: u32::from(triangles) * 3,
        triangles: Some(water.indices.as_chunks::<3>().0.to_vec()),
        match_groups: Vec::new(),
    };
    Ok((water.segments, data))
}

/// A `BSSegmentedTriShape`: the shape, then 16 segments of a flag byte, the first triangle point and the triangle
/// count; an empty segment starts at 0 as in the measured blocks.
fn segmented(shape: &NiTriShape, segments: &[u32]) -> Result<Vec<u8>> {
    // source: game-lod-water-shape
    let mut b = Body::TriShape(shape.clone()).write(crate::block::Profile {
        user_version: 11,
        user_version2: 34,
    })?;
    put_u32(&mut b, u32::try_from(segments.len()).unwrap_or(0));
    let mut start = 0_u32;
    for n in segments {
        b.push(0);
        put_u32(&mut b, if *n == 0 { 0 } else { start * 3 });
        put_u32(&mut b, *n);
        start += n;
    }
    Ok(b)
}

/// The data of the game's empty shape: one vertex at the origin and one triangle of three zero indices.
fn placeholder_data() -> NiTriShapeData {
    NiTriShapeData {
        geometry: GeometryData {
            group_id: 0,
            num_vertices: 1,
            keep_flags: 0,
            compress_flags: 0,
            vertices: Some(vec![[0.0; 3]]),
            vector_flags: 0,
            material_crc: 0,
            normals: None,
            tangents: None,
            center: [0.0; 3],
            radius: 0.0,
            vertex_colors: None,
            uv_sets: Vec::new(),
            consistency: 0,
            additional_data: -1,
        },
        num_triangles: 1,
        num_triangle_points: 3,
        triangles: Some(vec![[0, 0, 0]]),
        match_groups: Vec::new(),
    }
}

/// Append the water shape and its data, adding the segmented shape type when one is written. With `placeholder` the
/// game's empty segmented shape (three empty segments, translation zero, one zero vertex) comes first, as blocks 6
/// and 7, and the water follows as blocks 8 and 9.
fn push_water(
    blocks: &mut Vec<(u16, Body)>,
    types: &mut Vec<&[u8]>,
    origin: (f32, f32),
    (water_shape, water_data): (Option<[u32; 16]>, NiTriShapeData),
    placeholder: bool,
) -> Result<()> {
    // source: game-lod-water-shape
    if water_shape.is_some() || placeholder {
        types.insert(6, SEGMENTED);
    }
    let mut data_index = 7;
    if placeholder {
        // source: game-lod-water-placeholder
        let mut empty = tri_shape();
        empty.av.properties = Vec::new();
        empty.data = 7;
        blocks.push((6, Body::Opaque(segmented(&empty, &[0; 3])?)));
        blocks.push((4, Body::TriShapeData(placeholder_data())));
        data_index = 9;
    }
    let mut shape = tri_shape();
    shape.av.translation = [origin.0, origin.1, 0.0];
    shape.av.properties = Vec::new();
    shape.data = data_index;
    let shape = match water_shape {
        Some(segments) => (6, Body::Opaque(segmented(&shape, &segments)?)),
        None => (1, Body::TriShape(shape)),
    };
    blocks.push(shape);
    blocks.push((4, Body::TriShapeData(water_data)));
    Ok(())
}

/// Build the NIF of one terrain block.
///
/// # Errors
/// [`NifError::Inconsistent`] when the mesh has no vertices, more than 65535 vertices or triangles, an index out
/// of range, or an index count that is not a multiple of three, and the same for the water plane, whose
/// segments must add up to its triangle count.
pub fn terrain_block(block: &TerrainBlock<'_>) -> Result<NifFile> {
    let verts = block.vertices;
    if verts.is_empty() || block.indices.is_empty() || !block.indices.len().is_multiple_of(3) {
        return Err(NifError::Inconsistent("a terrain block needs triangles"));
    }
    if block.indices.iter().any(|i| usize::from(*i) >= verts.len()) {
        return Err(NifError::Inconsistent("a triangle index is out of range"));
    }
    let triangles = u16::try_from(block.indices.len() / 3)
        .map_err(|_| NifError::Inconsistent("too many triangles"))?;
    let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
    for v in verts {
        low = low.min(v[2]);
        high = high.max(v[2]);
    }
    let half = block.size / 2.0;
    let mid = f32::midpoint(low, high);
    let radius = (half * half * 2.0 + (high - mid) * (high - mid)).sqrt();
    // The UVs are mirrored in u: u = 1 - x / size, v = y / size.
    // source: game-lod-terrain-block-nif
    let uv: Vec<[f32; 2]> = verts
        .iter()
        .map(|v| [1.0 - v[0] / block.size, v[1] / block.size])
        .collect();
    let data = NiTriShapeData {
        geometry: GeometryData {
            group_id: 0,
            num_vertices: u16::try_from(verts.len())
                .map_err(|_| NifError::Inconsistent("too many vertices"))?,
            keep_flags: 0,
            compress_flags: 0,
            vertices: Some(verts.to_vec()),
            vector_flags: 1,
            material_crc: 0,
            normals: None,
            tangents: None,
            center: [half, half, mid],
            radius,
            vertex_colors: None,
            uv_sets: vec![uv],
            consistency: 0,
            additional_data: 5,
        },
        num_triangles: triangles,
        num_triangle_points: u32::from(triangles) * 3,
        triangles: Some(block.indices.as_chunks::<3>().0.to_vec()),
        match_groups: Vec::new(),
    };
    let shape = tri_shape();
    let mut lighting = Vec::with_capacity(54);
    put_u32(&mut lighting, u32::MAX);
    put_u32(&mut lighting, 0);
    put_u32(&mut lighting, u32::MAX);
    lighting.extend_from_slice(&LIGHTING_PROPERTY_TAIL);
    let heights: Vec<f32> = verts.iter().map(|v| v[2]).collect();
    let mut aabb = Vec::with_capacity(24);
    for v in [
        block.origin.0 + half,
        block.origin.1 + half,
        mid,
        half,
        half,
        (high - low) / 2.0,
    ] {
        put_f32(&mut aabb, v);
    }
    let water = block.water.as_ref().map(water_blocks).transpose()?;
    let placeholder = block.water.as_ref().is_some_and(|w| w.placeholder);
    let (children, multibound): (&[u32], u32) = match (water.is_some(), placeholder) {
        (true, true) => (&[1, 6, 8], 10),
        (true, false) => (&[1, 6], 8),
        _ => (&[1], 6),
    };
    let mut blocks = vec![
        (
            0,
            Body::Opaque(root_node(block.origin, children, multibound)),
        ),
        (1, Body::TriShape(shape)),
        (2, Body::Opaque(lighting)),
        (3, Body::Opaque(texture_set(block.textures))),
        (4, Body::TriShapeData(data)),
        (5, Body::Opaque(additional_data(&heights)?)),
    ];
    let mut types: Vec<&[u8]> = TYPES.to_vec();
    if let Some(water) = water {
        push_water(&mut blocks, &mut types, block.origin, water, placeholder)?;
    }
    let tail = u16::try_from(types.len() - 2).unwrap_or(0);
    // The multibound holds the index of the box block that follows it: 7 without water, 9 with one water shape,
    // 11 with the placeholder too (measured on the game's blocks, `game-lod-multibound-aabb-ref`).
    // source: game-lod-multibound-aabb-ref
    let aabb_index =
        u32::try_from(blocks.len() + 1).map_err(|_| NifError::Inconsistent("too many blocks"))?;
    blocks.push((tail, Body::Opaque(aabb_index.to_le_bytes().to_vec())));
    blocks.push((tail + 1, Body::Opaque(aabb)));
    Ok(NifFile {
        header: file_header(&types),
        blocks: blocks
            .into_iter()
            .map(|(type_index, body)| Block { type_index, body })
            .collect(),
        roots: vec![0],
    })
}

#[cfg(test)]
mod tests {
    use super::{TerrainBlock, terrain_block};
    use crate::NifFile;

    fn quad(heights: [f32; 4]) -> (Vec<[f32; 3]>, Vec<u16>) {
        (
            vec![
                [0.0, 0.0, heights[0]],
                [16384.0, 0.0, heights[1]],
                [0.0, 16384.0, heights[2]],
                [16384.0, 16384.0, heights[3]],
            ],
            vec![0, 1, 2, 2, 1, 3],
        )
    }

    /// The multibound block `at` holds the index of the box block `aabb`, and that block is the box.
    fn assert_multibound_names_the_box(file: &NifFile, at: usize, aabb: u32) {
        let kind =
            |n: usize| file.header.block_types[usize::from(file.blocks[n].type_index)].clone();
        assert_eq!(kind(at), b"BSMultiBound");
        assert_eq!(kind(aabb as usize), b"BSMultiBoundAABB");
        let crate::Body::Opaque(bytes) = &file.blocks[at].body else {
            panic!("opaque multibound");
        };
        assert_eq!(bytes.as_slice(), &aabb.to_le_bytes());
    }

    fn block<'a>(v: &'a [[f32; 3]], i: &'a [u16]) -> TerrainBlock<'a> {
        TerrainBlock {
            origin: (-16384.0, 0.0),
            size: 16384.0,
            vertices: v,
            indices: i,
            textures: ["Data\\a.dds", "Data\\b.dds"],
            water: None,
        }
    }

    #[test]
    fn the_file_has_the_eight_measured_blocks_and_reads_back() {
        let (v, i) = quad([1.0, 2.0, 3.0, 4.0]);
        let bytes = terrain_block(&block(&v, &i)).unwrap().write().unwrap();
        let back = NifFile::read(&bytes).unwrap();
        assert_eq!(back.blocks.len(), 8);
        assert_multibound_names_the_box(&back, 6, 7);
        assert_eq!(back.roots, [0]);
        assert_eq!(back.header.user_version, 11);
        assert_eq!(back.header.bethesda.as_ref().unwrap().0, 34);
        assert_eq!(back.write().unwrap(), bytes);
    }

    #[test]
    fn the_data_block_keeps_vertices_triangles_and_mirrored_uvs() {
        let (v, i) = quad([1.0, 2.0, 3.0, 4.0]);
        let back = NifFile::read(&terrain_block(&block(&v, &i)).unwrap().write().unwrap()).unwrap();
        let crate::Body::TriShapeData(d) = &back.blocks[4].body else {
            panic!("block 4 is the data");
        };
        assert_eq!(d.geometry.vertices.as_deref(), Some(&v[..]));
        assert_eq!(d.num_triangles, 2);
        assert_eq!(d.geometry.uv_sets[0][1], [0.0, 0.0]);
        assert_eq!(d.geometry.uv_sets[0][2], [1.0, 1.0]);
        assert!(d.geometry.normals.is_none() && d.geometry.vertex_colors.is_none());
        assert_eq!(d.geometry.center, [8192.0, 8192.0, 2.5]);
    }

    #[test]
    fn the_additional_data_holds_one_height_per_vertex_after_a_56_byte_header() {
        let (v, i) = quad([5.0, 6.0, 7.0, 8.0]);
        let file = terrain_block(&block(&v, &i)).unwrap();
        let crate::Body::Opaque(b) = &file.blocks[5].body else {
            panic!("opaque");
        };
        assert_eq!(b.len(), 56 + 16);
        assert_eq!(&b[0..2], &4_u16.to_le_bytes());
        assert_eq!(&b[56..60], &5.0_f32.to_le_bytes());
    }

    #[test]
    fn the_root_translation_is_the_block_origin() {
        let (v, i) = quad([0.0; 4]);
        let file = terrain_block(&block(&v, &i)).unwrap();
        let crate::Body::Opaque(b) = &file.blocks[0].body else {
            panic!("opaque");
        };
        assert_eq!(b.len(), 92);
        assert_eq!(&b[16..20], &(-16384.0_f32).to_le_bytes());
    }

    #[test]
    fn a_water_plane_adds_a_shape_and_its_data_and_reads_back() {
        let (v, i) = quad([0.0; 4]);
        let water_v = [[0.0, 0.0, 5.0], [128.0, 0.0, 5.0], [0.0, 128.0, 5.0]];
        let mut segments = [0; 16];
        segments[1] = 1;
        for (segments, shape_type) in [
            (Some(segments), &b"BSSegmentedTriShape"[..]),
            (None, b"NiTriShape"),
        ] {
            let mut plain = block(&v, &i);
            plain.water = Some(super::WaterShape {
                vertices: &water_v,
                indices: &[0, 1, 2],
                segments,
                placeholder: false,
            });
            let bytes = terrain_block(&plain).unwrap().write().unwrap();
            let back = NifFile::read(&bytes).unwrap();
            assert_eq!(back.blocks.len(), 10);
            assert_eq!(back.write().unwrap(), bytes);
            let kind = &back.header.block_types[usize::from(back.blocks[6].type_index)];
            assert_eq!(kind.as_slice(), shape_type);
            let crate::Body::TriShapeData(data) = &back.blocks[7].body else {
                panic!("block 7 is the water data");
            };
            assert_eq!(data.geometry.vertices.as_deref(), Some(&water_v[..]));
            assert!(data.geometry.uv_sets.is_empty() && data.geometry.additional_data == -1);
            assert_eq!(data.geometry.center, [64.0, 64.0, 5.0]);
            let crate::Body::Opaque(root) = &back.blocks[0].body else {
                panic!("opaque root");
            };
            // two children, then the multibound at 8, which names the box at 9
            assert_eq!(&root[76..80], &2_u32.to_le_bytes());
            assert_eq!(&root[92..96], &8_u32.to_le_bytes());
            assert_multibound_names_the_box(&back, 8, 9);
        }
        let mut plain = block(&v, &i);
        plain.water = Some(super::WaterShape {
            vertices: &water_v,
            indices: &[0, 1, 2],
            segments: Some([0; 16]),
            placeholder: false,
        });
        assert!(terrain_block(&plain).is_err());
    }

    #[test]
    fn the_placeholder_shape_comes_between_the_terrain_and_the_water() {
        let (v, i) = quad([0.0; 4]);
        let water_v = [[0.0, 0.0, 5.0], [128.0, 0.0, 5.0], [0.0, 128.0, 5.0]];
        let mut one = [0_u32; 16];
        one[1] = 1;
        for segments in [None, Some(one)] {
            let mut plain = block(&v, &i);
            plain.water = Some(super::WaterShape {
                vertices: &water_v,
                indices: &[0, 1, 2],
                segments,
                placeholder: true,
            });
            let bytes = terrain_block(&plain).unwrap().write().unwrap();
            let back = NifFile::read(&bytes).unwrap();
            assert_eq!(back.blocks.len(), 12);
            assert_eq!(back.write().unwrap(), bytes);
            let kind =
                |n: usize| back.header.block_types[usize::from(back.blocks[n].type_index)].clone();
            assert_eq!(kind(6), b"BSSegmentedTriShape");
            let crate::Body::TriShapeData(empty) = &back.blocks[7].body else {
                panic!("block 7 is the placeholder data");
            };
            assert_eq!(empty.geometry.vertices.as_deref(), Some(&[[0.0; 3]][..]));
            assert_eq!(empty.triangles.as_deref(), Some(&[[0_u16, 0, 0]][..]));
            // the water follows, its data at 9
            assert_eq!(
                kind(8),
                if segments.is_some() {
                    &b"BSSegmentedTriShape"[..]
                } else {
                    b"NiTriShape"
                }
            );
            let crate::Body::TriShapeData(data) = &back.blocks[9].body else {
                panic!("block 9 is the water data");
            };
            assert_eq!(data.geometry.vertices.as_deref(), Some(&water_v[..]));
            let crate::Body::Opaque(root) = &back.blocks[0].body else {
                panic!("opaque root");
            };
            // three children (1, 6, 8), then the multibound at 10, which names the box at 11
            assert_multibound_names_the_box(&back, 10, 11);
            assert_eq!(&root[76..80], &3_u32.to_le_bytes());
            assert_eq!(&root[80..84], &1_u32.to_le_bytes());
            assert_eq!(&root[84..88], &6_u32.to_le_bytes());
            assert_eq!(&root[88..92], &8_u32.to_le_bytes());
            assert_eq!(&root[96..100], &10_u32.to_le_bytes());
            // the placeholder shape: no properties, translation zero, data block 7, three empty segments
            let crate::Body::Opaque(shape) = &back.blocks[6].body else {
                panic!("opaque shape");
            };
            assert_eq!(&shape[16..28], &[0_u8; 12]);
            let tail = &shape[shape.len() - (4 + 3 * 9)..];
            assert_eq!(&tail[..4], &3_u32.to_le_bytes());
            assert!(tail[4..].iter().all(|b| *b == 0));
        }
    }

    #[test]
    fn bad_meshes_are_refused() {
        let (v, _) = quad([0.0; 4]);
        assert!(terrain_block(&block(&v, &[])).is_err());
        assert!(terrain_block(&block(&v, &[0, 1])).is_err());
        assert!(terrain_block(&block(&v, &[0, 1, 9])).is_err());
        assert!(terrain_block(&block(&[], &[0, 1, 2])).is_err());
    }
}
