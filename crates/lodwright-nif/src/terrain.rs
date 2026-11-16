// SPDX-License-Identifier: MPL-2.0
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
const ROOT_FLAGS: u32 = 0x0008_080e;
/// Flags of the terrain shape (the same word as the root's in the measured files).
const SHAPE_FLAGS: u32 = 524_302;

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
}

fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn root_node(origin: (f32, f32)) -> Vec<u8> {
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
    put_u32(&mut b, 1); // children
    put_u32(&mut b, 1); // the shape
    put_u32(&mut b, 0); // effects
    put_u32(&mut b, 6); // the multibound
    b
}

fn texture_set(paths: [&str; 2]) -> Vec<u8> {
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

fn tri_shape() -> NiTriShape {
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

fn file_header() -> Header {
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
        block_types: TYPES.iter().map(|t| t.to_vec()).collect(),
        strings: Vec::new(),
        groups: Vec::new(),
    }
}

/// Build the NIF of one terrain block.
///
/// # Errors
/// [`NifError::Inconsistent`] when the mesh has no vertices, more than 65535 vertices or triangles, an index out
/// of range, or an index count that is not a multiple of three.
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
    // source: game-lod-terrain-block-nif (u mirrored: u = 1 - x / size, v = y / size)
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
    let blocks = vec![
        (0, Body::Opaque(root_node(block.origin))),
        (1, Body::TriShape(shape)),
        (2, Body::Opaque(lighting)),
        (3, Body::Opaque(texture_set(block.textures))),
        (4, Body::TriShapeData(data)),
        (5, Body::Opaque(additional_data(&heights)?)),
        (6, Body::Opaque(7_u32.to_le_bytes().to_vec())),
        (7, Body::Opaque(aabb)),
    ];
    Ok(NifFile {
        header: file_header(),
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

    fn block<'a>(v: &'a [[f32; 3]], i: &'a [u16]) -> TerrainBlock<'a> {
        TerrainBlock {
            origin: (-16384.0, 0.0),
            size: 16384.0,
            vertices: v,
            indices: i,
            textures: ["Data\\a.dds", "Data\\b.dds"],
        }
    }

    #[test]
    fn the_file_has_the_eight_measured_blocks_and_reads_back() {
        let (v, i) = quad([1.0, 2.0, 3.0, 4.0]);
        let bytes = terrain_block(&block(&v, &i)).unwrap().write().unwrap();
        let back = NifFile::read(&bytes).unwrap();
        assert_eq!(back.blocks.len(), 8);
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
    fn bad_meshes_are_refused() {
        let (v, _) = quad([0.0; 4]);
        assert!(terrain_block(&block(&v, &[])).is_err());
        assert!(terrain_block(&block(&v, &[0, 1])).is_err());
        assert!(terrain_block(&block(&v, &[0, 1, 9])).is_err());
        assert!(terrain_block(&block(&[], &[0, 1, 2])).is_err());
    }
}
