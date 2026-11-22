// SPDX-License-Identifier: GPL-3.0-only
//! The object LOD block NIF: the seven-block structure the game's own object blocks have.
//!
//! Layout from the row `game-lod-object-block-nif` in `provenance/sources.toml`. The segment table has eight
//! entries in the game's block (four empty, then four that partition the triangles); the meaning of the
//! partition is not established, so [`SEGMENTS`] entries are written with the triangles split by the caller.

#![allow(
    clippy::too_many_lines,
    reason = "one builder for one fixed file layout"
)]

use crate::block::{Body, GeometryData, NiTriShapeData};
use crate::error::{NifError, Result};
use crate::file::{Block, NifFile};
use crate::terrain::{
    ROOT_FLAGS, file_header, put_f32, put_u32, root_node, texture_set, tri_shape,
};

/// Segments in the table.
pub const SEGMENTS: usize = 8;

const TYPES: [&[u8]; 7] = [
    b"BSMultiBoundNode",
    b"BSSegmentedTriShape",
    b"BSShaderPPLightingProperty",
    b"BSShaderTextureSet",
    b"NiTriShapeData",
    b"BSMultiBound",
    b"BSMultiBoundAABB",
];

// source: game-lod-object-block-nif
/// The lighting property after its empty name, extra data and controller. Differs from the terrain block's in the
/// two shader flag words.
const LIGHTING_PROPERTY_TAIL: [u8; 42] = [
    0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x41, 0x00, 0x00, 0x80, 0x3f,
];

/// Everything an object block needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectBlock<'a> {
    /// South-west corner of the block in world units.
    pub origin: (f32, f32),
    /// Positions relative to the origin.
    pub vertices: &'a [[f32; 3]],
    /// Normals, one per vertex.
    pub normals: &'a [[f32; 3]],
    /// Tangents, one per vertex.
    pub tangents: &'a [[f32; 3]],
    /// Bitangents, one per vertex.
    pub bitangents: &'a [[f32; 3]],
    /// UVs, one per vertex.
    pub uv: &'a [[f32; 2]],
    /// Triangle corner indices, three per triangle.
    pub indices: &'a [u16],
    /// The triangle count of each of the last four segments (they must add up to the triangle count).
    pub segment_triangles: [u32; 4],
    /// The atlas and its normal atlas paths as written in the texture set.
    pub textures: [&'a str; 2],
}

fn shape_bytes(origin: (f32, f32), segments: [u32; 4]) -> Result<Vec<u8>> {
    // source: game-lod-object-block-nif
    let mut b = Body::TriShape(tri_shape_for_object(origin)).write(crate::block::Profile {
        user_version: 11,
        user_version2: 34,
    })?;
    put_u32(&mut b, u32::try_from(SEGMENTS).unwrap_or(8));
    let mut start = 0_u32;
    for n in [0, 0, 0, 0].into_iter().chain(segments) {
        b.push(0); // flags
        put_u32(&mut b, start * 3);
        put_u32(&mut b, n);
        start += n;
    }
    Ok(b)
}

/// The game's object block carries the block's origin on the shape; its root node has no translation (measured on
/// the game's blocks: the engine draws a block only when the translation is where the game puts it).
fn tri_shape_for_object(origin: (f32, f32)) -> crate::block::NiTriShape {
    let mut s = tri_shape();
    s.av.translation = [origin.0, origin.1, 0.0];
    s.data = 4;
    s
}

/// Build the NIF of one object block.
///
/// # Errors
/// [`NifError::Inconsistent`] when arrays disagree in length, there are no triangles, more than 65535 vertices,
/// an index is out of range, or the segment counts do not add up to the triangle count.
pub fn object_block(block: &ObjectBlock<'_>) -> Result<NifFile> {
    let n = block.vertices.len();
    if n == 0
        || n > usize::from(u16::MAX)
        || block.normals.len() != n
        || block.tangents.len() != n
        || block.bitangents.len() != n
        || block.uv.len() != n
    {
        return Err(NifError::Inconsistent(
            "vertex arrays disagree or are empty",
        ));
    }
    if block.indices.is_empty()
        || !block.indices.len().is_multiple_of(3)
        || block.indices.iter().any(|i| usize::from(*i) >= n)
    {
        return Err(NifError::Inconsistent("a block needs valid triangles"));
    }
    let triangles = u16::try_from(block.indices.len() / 3)
        .map_err(|_| NifError::Inconsistent("too many triangles"))?;
    if block
        .segment_triangles
        .iter()
        .map(|v| u64::from(*v))
        .sum::<u64>()
        != u64::from(triangles)
    {
        return Err(NifError::Inconsistent(
            "segments do not cover the triangles",
        ));
    }
    let (mut low, mut high) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for v in block.vertices {
        for k in 0..3 {
            low[k] = low[k].min(v[k]);
            high[k] = high[k].max(v[k]);
        }
    }
    let center = [
        f32::midpoint(low[0], high[0]),
        f32::midpoint(low[1], high[1]),
        f32::midpoint(low[2], high[2]),
    ];
    let radius = block
        .vertices
        .iter()
        .map(|v| {
            ((v[0] - center[0]).powi(2) + (v[1] - center[1]).powi(2) + (v[2] - center[2]).powi(2))
                .sqrt()
        })
        .fold(0.0_f32, f32::max);
    let data = NiTriShapeData {
        geometry: GeometryData {
            group_id: 0,
            num_vertices: u16::try_from(n)
                .map_err(|_| NifError::Inconsistent("too many vertices"))?,
            keep_flags: 51,
            compress_flags: 0,
            vertices: Some(block.vertices.to_vec()),
            vector_flags: 4097,
            material_crc: 0,
            normals: Some(block.normals.to_vec()),
            tangents: Some((block.tangents.to_vec(), block.bitangents.to_vec())),
            center,
            radius,
            vertex_colors: None,
            uv_sets: vec![block.uv.to_vec()],
            consistency: 0,
            additional_data: -1,
        },
        num_triangles: triangles,
        num_triangle_points: u32::from(triangles) * 3,
        triangles: Some(block.indices.as_chunks::<3>().0.to_vec()),
        match_groups: Vec::new(),
    };
    let mut lighting = Vec::with_capacity(54);
    put_u32(&mut lighting, u32::MAX);
    put_u32(&mut lighting, 0);
    put_u32(&mut lighting, u32::MAX);
    lighting.extend_from_slice(&LIGHTING_PROPERTY_TAIL);
    let mut aabb = Vec::with_capacity(24);
    for v in [
        block.origin.0 + center[0],
        block.origin.1 + center[1],
        center[2],
        (high[0] - low[0]) / 2.0,
        (high[1] - low[1]) / 2.0,
        (high[2] - low[2]) / 2.0,
    ] {
        put_f32(&mut aabb, v);
    }
    let _ = ROOT_FLAGS;
    let bodies = [
        Body::Opaque(root_node((0.0, 0.0), &[1], 5)),
        Body::Opaque(shape_bytes(block.origin, block.segment_triangles)?),
        Body::Opaque(lighting),
        Body::Opaque(texture_set(block.textures)),
        Body::TriShapeData(data),
        Body::Opaque(5_u32.to_le_bytes().to_vec()),
        Body::Opaque(aabb),
    ];
    // block 5 is the multibound, which points at block 6; the node above points at 5
    let mut bodies = bodies;
    bodies[5] = Body::Opaque(6_u32.to_le_bytes().to_vec());
    Ok(NifFile {
        header: file_header(&TYPES),
        blocks: bodies
            .into_iter()
            .enumerate()
            .map(|(i, body)| Block {
                type_index: u16::try_from(i).unwrap_or(0),
                body,
            })
            .collect(),
        roots: vec![0],
    })
}

#[cfg(test)]
#[allow(clippy::many_single_char_names)]
mod tests {
    use super::{ObjectBlock, SEGMENTS, object_block};
    use crate::{Body, NifFile};

    fn tri() -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u16>) {
        (
            vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 50.0]],
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            vec![0, 1, 2],
        )
    }

    #[test]
    fn an_object_block_has_seven_blocks_a_segment_table_and_reads_back() {
        let (v, uv, i) = tri();
        let n = vec![[0.0, 0.0, 1.0]; 3];
        let t = vec![[1.0, 0.0, 0.0]; 3];
        let b = vec![[0.0, 1.0, 0.0]; 3];
        let file = object_block(&ObjectBlock {
            origin: (98304.0, -49152.0),
            vertices: &v,
            normals: &n,
            tangents: &t,
            bitangents: &b,
            uv: &uv,
            indices: &i,
            segment_triangles: [0, 1, 0, 0],
            textures: ["Data\\a.dds", "Data\\b.dds"],
        })
        .unwrap();
        let bytes = file.write().unwrap();
        let back = NifFile::read(&bytes).unwrap();
        assert_eq!(back.blocks.len(), 7);
        assert_eq!(back.write().unwrap(), bytes);
        let Body::Opaque(shape) = &back.blocks[1].body else {
            panic!("the shape is kept opaque");
        };
        // 8 segments of 9 bytes at the end, after their count
        let tail = &shape[shape.len() - SEGMENTS * 9 - 4..];
        assert_eq!(&tail[0..4], &8_u32.to_le_bytes());
        // the origin is on the shape, the root node has no translation (as in the game's blocks)
        let Body::Opaque(root) = &back.blocks[0].body else {
            panic!("the root is kept opaque");
        };
        let at = |bytes: &[u8]| -> [f32; 3] {
            let f =
                |k: usize| f32::from_le_bytes(bytes[16 + 4 * k..20 + 4 * k].try_into().unwrap());
            [f(0), f(1), f(2)]
        };
        assert_eq!(at(root), [0.0, 0.0, 0.0]);
        assert_eq!(at(shape), [98304.0, -49152.0, 0.0]);
        let Body::TriShapeData(d) = &back.blocks[4].body else {
            panic!("data");
        };
        assert_eq!(d.geometry.vector_flags, 4097);
        assert_eq!(d.geometry.keep_flags, 51);
        assert!(d.geometry.tangents.is_some());
    }

    #[test]
    fn segments_must_cover_the_triangles() {
        let (v, uv, i) = tri();
        let z = vec![[0.0, 0.0, 1.0]; 3];
        let bad = object_block(&ObjectBlock {
            origin: (0.0, 0.0),
            vertices: &v,
            normals: &z,
            tangents: &z,
            bitangents: &z,
            uv: &uv,
            indices: &i,
            segment_triangles: [0, 0, 0, 0],
            textures: ["a", "b"],
        });
        assert!(bad.is_err());
    }
}
