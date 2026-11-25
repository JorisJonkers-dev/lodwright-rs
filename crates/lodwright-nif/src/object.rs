// SPDX-License-Identifier: GPL-3.0-only
//! The object LOD block NIF: the seven-block structure the game's own object blocks have.
//!
//! Layout from the row `game-lod-object-block-nif` in `provenance/sources.toml`. The segment table has sixteen
//! entries at most, one per cell of the block's four by four cells, x-major (`cell x * 4 + cell y`); each holds the
//! triangles of the objects whose reference stands in that cell, and the triangles are stored in segment order.

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
pub const SEGMENTS: usize = 16;

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

/// Where the lighting property's texture set reference sits: after the empty name, extra data and controller (12
/// bytes), the flags (2), the shader type, flags, second flags, environment map scale and clamp mode (4 each).
const TEXTURE_SET_REF: usize = 12 + 2 + 5 * 4;

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
    /// The triangle count of each segment, one per cell of the block, x-major (they must add up to the triangle
    /// count, and the triangles must be stored in this order).
    pub segment_triangles: [u32; SEGMENTS],
    /// The atlas and its normal atlas paths as written in the texture set.
    pub textures: [&'a str; 2],
}

fn shape_bytes(origin: (f32, f32), segments: [u32; SEGMENTS], first: i32) -> Result<Vec<u8>> {
    // source: game-lod-object-block-nif
    let mut b =
        Body::TriShape(tri_shape_for_object(origin, first)).write(crate::block::Profile {
            user_version: 11,
            user_version2: 34,
        })?;
    // the game's table ends with the last segment that holds a triangle
    let count = segments
        .iter()
        .rposition(|&n| n > 0)
        .map_or(1, |last| last + 1);
    put_u32(&mut b, u32::try_from(count).unwrap_or(16));
    let mut start = 0_u32;
    for &n in &segments[..count] {
        b.push(0); // flags
        // an empty segment starts at 0, as in the game's blocks
        put_u32(&mut b, if n == 0 { 0 } else { start * 3 });
        put_u32(&mut b, n);
        start += n;
    }
    Ok(b)
}

/// The game's object block carries the block's origin on the shape; its root node has no translation (measured on
/// the game's blocks: the engine draws a block only when the translation is where the game puts it).
fn tri_shape_for_object(origin: (f32, f32), first: i32) -> crate::block::NiTriShape {
    let mut s = tri_shape();
    s.av.translation = [origin.0, origin.1, 0.0];
    // the shape sits at block `first`; its lighting property follows it and its data is the third block after it
    s.av.properties = vec![first + 1];
    s.data = first + 3;
    s
}

/// Build the NIF of one object block.
///
/// # Errors
/// [`NifError::Inconsistent`] when arrays disagree in length, there are no triangles, more than 65535 vertices,
/// an index is out of range, or the segment counts do not add up to the triangle count.
pub fn object_block(block: &ObjectBlock<'_>) -> Result<NifFile> {
    object_block_shapes(std::slice::from_ref(block))
}

/// Build the NIF of one object block that holds several shapes (each its own segmented shape with its own lighting
/// property, texture set and data, as the game's blocks do when one shape would pass 65535 vertices or the block
/// draws from more than one atlas). `shapes` must not be empty and all carry the same origin.
///
/// # Errors
/// [`NifError::Inconsistent`] as [`object_block`] for any shape, or when `shapes` is empty or the origins differ.
pub fn object_block_shapes(shapes: &[ObjectBlock<'_>]) -> Result<NifFile> {
    object_block_with_lights(shapes, None)
}

/// The marker name of the far lights shape: a `NiStringExtraData` of this name on the shape tells the engine to
/// draw it only at night. One general name that any tool may write.
pub const FAR_LIGHTS_MARKER: &str = "FarLights";

/// The small unlit, additive light points of one object block (`objects.far_lights`), drawn as one plain shape
/// beside the block's object shapes.
#[derive(Debug, Clone, PartialEq)]
pub struct FarLightsShape<'a> {
    /// Positions relative to the block's origin.
    pub vertices: &'a [[f32; 3]],
    /// Colours, red, green, blue and alpha from 0 to 1, one per vertex.
    pub colors: &'a [[f32; 4]],
    /// UVs, one per vertex.
    pub uv: &'a [[f32; 2]],
    /// Triangle corner indices, three per triangle.
    pub indices: &'a [u16],
    /// The light point texture path as written in the shader property.
    pub texture: &'a str,
}

/// Block types a far lights shape adds to the block's type table, in this order after [`TYPES`].
const LIGHT_TYPES: [&[u8]; 4] = [
    b"NiTriShape",
    b"BSShaderNoLightingProperty",
    b"NiAlphaProperty",
    b"NiStringExtraData",
];

// source: nif-no-lighting-property-layout, nif-alpha-flags
/// The far lights shape's unlit property after its name, extra data and controller: `NiShadeProperty` flags (1),
/// shader type `SHADER_NOLIGHTING` (33), shader flags `ZBuffer_Test` only (bit 31: hidden behind nearer things), second
/// flags `Vertex_Colors` only (bit 5; `ZBuffer_Write` clear, so light points never hide each other), environment map
/// scale 1, texture clamp mode 3 (wrap both); then the file name and the four falloff numbers.
fn no_lighting_property(texture: &str) -> Vec<u8> {
    let mut b = Vec::with_capacity(64);
    put_u32(&mut b, u32::MAX); // name
    put_u32(&mut b, 0); // extra data
    put_u32(&mut b, u32::MAX); // controller
    b.extend_from_slice(&1_u16.to_le_bytes());
    put_u32(&mut b, 33);
    put_u32(&mut b, 0x8000_0000);
    put_u32(&mut b, 0x20);
    put_f32(&mut b, 1.0);
    put_u32(&mut b, 3);
    put_u32(&mut b, u32::try_from(texture.len()).unwrap_or(0));
    b.extend_from_slice(texture.as_bytes());
    // the falloff is left unset (-1 each), so the points are drawn the same from every side
    for _ in 0..4 {
        put_f32(&mut b, -1.0);
    }
    b
}

/// Build the NIF of one object block as [`object_block_shapes`], with the block's far light points as one more
/// shape when `lights` is given: a plain `NiTriShape` with vertex colours, an unlit property, an additive alpha
/// property (source `SRC_ALPHA`, destination `ONE`, no alpha test) and a `NiStringExtraData` named [`FAR_LIGHTS_MARKER`].
///
/// # Errors
/// As [`object_block_shapes`]; and [`NifError::Inconsistent`] when the light arrays disagree or hold no triangles.
pub fn object_block_with_lights(
    shapes: &[ObjectBlock<'_>],
    lights: Option<&FarLightsShape<'_>>,
) -> Result<NifFile> {
    let Some(first) = shapes.first() else {
        return Err(NifError::Inconsistent("a block needs a shape"));
    };
    if shapes.iter().any(|s| s.origin != first.origin) {
        return Err(NifError::Inconsistent(
            "the shapes of a block share an origin",
        ));
    }
    let count =
        u32::try_from(shapes.len()).map_err(|_| NifError::Inconsistent("too many shapes"))?;
    let (mut low, mut high) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    let mut bodies = vec![Body::Opaque(Vec::new())];
    let mut children = Vec::new();
    for (k, block) in shapes.iter().enumerate() {
        let (shape_bodies, lo, hi) = shape_blocks(block, i32::try_from(1 + 4 * k).unwrap_or(1))?;
        for c in 0..3 {
            low[c] = low[c].min(lo[c]);
            high[c] = high[c].max(hi[c]);
        }
        children.push(u32::try_from(1 + 4 * k).unwrap_or(0));
        bodies.extend(shape_bodies);
    }
    let multibound = 1 + 4 * count;
    bodies[0] = Body::Opaque(root_node((0.0, 0.0), &children, multibound));
    let center = [
        f32::midpoint(low[0], high[0]),
        f32::midpoint(low[1], high[1]),
        f32::midpoint(low[2], high[2]),
    ];
    let mut aabb = Vec::with_capacity(24);
    for v in [
        first.origin.0 + center[0],
        first.origin.1 + center[1],
        center[2],
        (high[0] - low[0]) / 2.0,
        (high[1] - low[1]) / 2.0,
        (high[2] - low[2]) / 2.0,
    ] {
        put_f32(&mut aabb, v);
    }
    let _ = ROOT_FLAGS;
    bodies.push(Body::Opaque((multibound + 1).to_le_bytes().to_vec()));
    bodies.push(Body::Opaque(aabb));
    // type indices: root 0, then per shape 1 to 4 (shape, lighting, texture set, data), then multibound and box
    let n = bodies.len();
    let type_of = |i: usize| -> u16 {
        if i == 0 {
            0
        } else if i == n - 2 {
            5
        } else if i == n - 1 {
            6
        } else {
            u16::try_from((i - 1) % 4 + 1).unwrap_or(0)
        }
    };
    let mut blocks: Vec<Block> = bodies
        .into_iter()
        .enumerate()
        .map(|(i, body)| Block {
            type_index: type_of(i),
            body,
        })
        .collect();
    let mut header = file_header(&TYPES);
    if let Some(lights) = lights {
        let at =
            i32::try_from(blocks.len()).map_err(|_| NifError::Inconsistent("too many blocks"))?;
        children.push(at.cast_unsigned());
        blocks[0].body = Body::Opaque(root_node((0.0, 0.0), &children, multibound));
        let types = u16::try_from(TYPES.len()).unwrap_or(0);
        for (body, type_index) in light_blocks(lights, first.origin, at)?.into_iter().zip([
            types,
            types + 1,
            types + 2,
            4,
            types + 3,
        ]) {
            blocks.push(Block { type_index, body });
        }
        header
            .block_types
            .extend(LIGHT_TYPES.iter().map(|t| t.to_vec()));
        header.strings.push(FAR_LIGHTS_MARKER.as_bytes().to_vec());
    }
    Ok(NifFile {
        header,
        blocks,
        roots: vec![0],
    })
}

/// The five blocks of the far lights shape, which starts at block `at`: the shape, its unlit property, its alpha
/// property, its data and its marker.
fn light_blocks(lights: &FarLightsShape<'_>, origin: (f32, f32), at: i32) -> Result<Vec<Body>> {
    let n = lights.vertices.len();
    if n == 0 || n > usize::from(u16::MAX) || lights.colors.len() != n || lights.uv.len() != n {
        return Err(NifError::Inconsistent(
            "light vertex arrays disagree or are empty",
        ));
    }
    if lights.indices.is_empty()
        || !lights.indices.len().is_multiple_of(3)
        || lights.indices.iter().any(|i| usize::from(*i) >= n)
    {
        return Err(NifError::Inconsistent("light points need valid triangles"));
    }
    let triangles = u16::try_from(lights.indices.len() / 3)
        .map_err(|_| NifError::Inconsistent("too many light triangles"))?;
    let (mut low, mut high) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for v in lights.vertices {
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
    let radius = lights
        .vertices
        .iter()
        .map(|v| {
            ((v[0] - center[0]).powi(2) + (v[1] - center[1]).powi(2) + (v[2] - center[2]).powi(2))
                .sqrt()
        })
        .fold(0.0_f32, f32::max);
    let mut shape = tri_shape();
    shape.av.translation = [origin.0, origin.1, 0.0];
    shape.av.properties = vec![at + 1, at + 2];
    shape.av.net.extra_data = vec![at + 4];
    shape.data = at + 3;
    // source: nif-alpha-flags
    // alpha blend on, source SRC_ALPHA (6), destination ONE (0), no alpha test: the points add their colour, scaled by
    // the alpha the engine fades them with by night, to what is behind
    let alpha = crate::block::NiAlphaProperty {
        net: crate::block::ObjectNet {
            name: -1,
            extra_data: Vec::new(),
            controller: -1,
        },
        flags: 0x000D,
        threshold: 0,
    };
    let data = NiTriShapeData {
        geometry: GeometryData {
            group_id: 0,
            num_vertices: u16::try_from(n)
                .map_err(|_| NifError::Inconsistent("too many vertices"))?,
            keep_flags: 0,
            compress_flags: 0,
            vertices: Some(lights.vertices.to_vec()),
            vector_flags: 1,
            material_crc: 0,
            normals: None,
            tangents: None,
            center,
            radius,
            vertex_colors: Some(lights.colors.to_vec()),
            uv_sets: vec![lights.uv.to_vec()],
            consistency: 0,
            additional_data: -1,
        },
        num_triangles: triangles,
        num_triangle_points: u32::from(triangles) * 3,
        triangles: Some(lights.indices.as_chunks::<3>().0.to_vec()),
        match_groups: Vec::new(),
    };
    // source: nif-string-extra-data-layout
    // NiExtraData name (string index 0, the marker), then the string data (none)
    let mut marker = Vec::with_capacity(8);
    put_u32(&mut marker, 0);
    put_u32(&mut marker, u32::MAX);
    Ok(vec![
        Body::TriShape(shape),
        Body::Opaque(no_lighting_property(lights.texture)),
        Body::AlphaProperty(alpha),
        Body::TriShapeData(data),
        Body::Opaque(marker),
    ])
}

/// The four blocks of one shape and the corners of its vertices' box.
fn shape_blocks(block: &ObjectBlock<'_>, first: i32) -> Result<(Vec<Body>, [f32; 3], [f32; 3])> {
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
    // the lighting property names the texture set of its own shape (the block after it); the tail as measured names
    // block 3, which is right only for the first shape of a block
    lighting[TEXTURE_SET_REF..TEXTURE_SET_REF + 4].copy_from_slice(&(first + 2).to_le_bytes());
    Ok((
        vec![
            Body::Opaque(shape_bytes(block.origin, block.segment_triangles, first)?),
            Body::Opaque(lighting),
            Body::Opaque(texture_set(block.textures)),
            Body::TriShapeData(data),
        ],
        low,
        high,
    ))
}

/// One shape of an object block as a reader sees it: where the block puts it, what it draws with and which
/// triangles belong to which cell segment.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectShape {
    /// The shape's translation: the south-west corner of the block; the vertices are relative to it.
    pub origin: [f32; 3],
    /// The diffuse texture path of the shape's texture set (as written, any case).
    pub texture: String,
    /// The normal texture path of the shape's texture set.
    pub normal_texture: String,
    /// Vertex positions relative to the origin.
    pub vertices: Vec<[f32; 3]>,
    /// The UVs of the first set, one per vertex; empty when the shape has none.
    pub uv: Vec<[f32; 2]>,
    /// Triangles, stored in segment order.
    pub triangles: Vec<[u16; 3]>,
    /// The triangle count of each entry of the segment table (the table ends at the last used segment).
    pub segment_triangles: Vec<u32>,
}

fn word(bytes: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn block_ref(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(word(bytes, at)?).ok()
}

/// Read the shapes of an object block: the shapes under the root node, each with its own lighting property,
/// texture set and data, as [`object_block_shapes`] writes them (the references are followed, not assumed).
///
/// # Errors
/// [`NifError::Inconsistent`] when the file is not laid out like an object block.
pub fn read_object_shapes(file: &NifFile) -> Result<Vec<ObjectShape>> {
    const BAD: NifError = NifError::Inconsistent("not an object block");
    let Some(Body::Opaque(root)) = file.blocks.first().map(|b| &b.body) else {
        return Err(BAD);
    };
    let count = file
        .blocks
        .iter()
        .filter(|b| matches!(b.body, Body::TriShapeData(_)))
        .count();
    // the root node ends with its children, an empty effect list and the multibound reference
    let children = root.len().checked_sub(8 + 4 * count).ok_or(BAD)?;
    if block_ref(root, children.checked_sub(4).ok_or(BAD)?) != Some(count) {
        return Err(BAD);
    }
    let mut shapes = Vec::with_capacity(count);
    for k in 0..count {
        let shape = block_ref(root, children + 4 * k).ok_or(BAD)?;
        // a far lights shape (a plain NiTriShape, see [`FarLightsShape`]) is not an object shape
        let segmented = file.blocks.get(shape).is_some_and(|b| {
            file.header
                .block_types
                .get(usize::from(b.type_index))
                .is_some_and(|t| t.as_slice() == TYPES[1])
        });
        if !segmented {
            continue;
        }
        let Some(Body::Opaque(shape_bytes)) = file.blocks.get(shape).map(|b| &b.body) else {
            return Err(BAD);
        };
        // name, extra data list, controller, flags (u32), translation, rotation, scale, then the property list
        let props = 4 + 4 + 4 + 4 + 12 + 36 + 4;
        let lighting = block_ref(shape_bytes, props + 4).ok_or(BAD)?;
        let data = block_ref(shape_bytes, props + 12).ok_or(BAD)?;
        let Some(Body::Opaque(lighting_bytes)) = file.blocks.get(lighting).map(|b| &b.body) else {
            return Err(BAD);
        };
        let set = block_ref(lighting_bytes, TEXTURE_SET_REF).ok_or(BAD)?;
        let Some(Body::Opaque(set_bytes)) = file.blocks.get(set).map(|b| &b.body) else {
            return Err(BAD);
        };
        let Some(Body::TriShapeData(d)) = file.blocks.get(data).map(|b| &b.body) else {
            return Err(BAD);
        };
        let mut at = 4;
        let mut paths = Vec::with_capacity(2);
        for _ in 0..2 {
            let len = block_ref(set_bytes, at).ok_or(BAD)?;
            let text = set_bytes.get(at + 4..at + 4 + len).ok_or(BAD)?;
            paths.push(String::from_utf8_lossy(text).into_owned());
            at += 4 + len;
        }
        let f = |n: usize| -> Option<f32> {
            Some(f32::from_le_bytes(
                shape_bytes.get(16 + 4 * n..20 + 4 * n)?.try_into().ok()?,
            ))
        };
        let origin = [f(0).ok_or(BAD)?, f(1).ok_or(BAD)?, f(2).ok_or(BAD)?];
        // the segment table ends the shape: a count, then per segment a flag byte, a start and a triangle count. The
        // shape before it has a fixed length, which tells the count (a word that merely equals a smaller count
        // somewhere in the table must not be taken for it)
        let segments = {
            let body = Body::TriShape(tri_shape_for_object((0.0, 0.0), 0))
                .write(crate::block::Profile {
                    user_version: 11,
                    user_version2: 34,
                })?
                .len();
            let table = shape_bytes.len().checked_sub(body + 4).ok_or(BAD)?;
            if table % 9 != 0 || block_ref(shape_bytes, body) != Some(table / 9) {
                return Err(BAD);
            }
            (0..table / 9)
                .map(|e| word(shape_bytes, body + 4 + 9 * e + 5).map(i32::cast_unsigned))
                .collect::<Option<Vec<u32>>>()
                .ok_or(BAD)?
        };
        let mut paths = paths.into_iter();
        shapes.push(ObjectShape {
            origin,
            texture: paths.next().unwrap_or_default(),
            normal_texture: paths.next().unwrap_or_default(),
            vertices: d.geometry.vertices.clone().ok_or(BAD)?,
            uv: d.geometry.uv_sets.first().cloned().unwrap_or_default(),
            triangles: d.triangles.clone().ok_or(BAD)?,
            segment_triangles: segments,
        });
    }
    Ok(shapes)
}

#[cfg(test)]
#[allow(clippy::many_single_char_names)]
mod tests {
    use super::{ObjectBlock, SEGMENTS, object_block};
    use crate::{Body, NifFile};
    use lodwright_testkit::{covers, ids};

    fn one_segment(k: usize) -> [u32; SEGMENTS] {
        let mut s = [0; SEGMENTS];
        s[k] = 1;
        s
    }

    fn tri() -> (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u16>) {
        (
            vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 50.0]],
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            vec![0, 1, 2],
        )
    }

    #[test]
    fn an_object_block_has_seven_blocks_a_segment_table_and_reads_back() {
        covers(&[ids::formats::meshes_and_images::NIF_OBJECT_BLOCKS]);
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
            segment_triangles: one_segment(5),
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
        // 9 bytes per segment at the end, after their count
        // segment 5 is the last used one, so the table has six segments
        let tail = &shape[shape.len() - 6 * 9 - 4..];
        assert_eq!(&tail[0..4], &6_u32.to_le_bytes());
        // the one triangle is in segment 5, the last entry
        assert_eq!(&tail[4 + 5 * 9 + 5..4 + 5 * 9 + 9], &1_u32.to_le_bytes());
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
        covers(&[ids::formats::meshes_and_images::NIF_OBJECT_BLOCKS]);
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
            segment_triangles: [0; SEGMENTS],
            textures: ["a", "b"],
        });
        assert!(bad.is_err());
    }
}

#[cfg(test)]
#[allow(clippy::many_single_char_names)]
mod shapes_tests {
    use super::{ObjectBlock, SEGMENTS, object_block_shapes};
    use crate::{Body, NifFile};

    #[test]
    fn a_block_of_two_shapes_links_each_shape_to_its_own_blocks_and_reads_back() {
        covers(&[ids::formats::meshes_and_images::NIF_OBJECT_BLOCKS]);
        let v = vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 50.0]];
        let uv = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let n = vec![[0.0, 0.0, 1.0]; 3];
        let t = vec![[1.0, 0.0, 0.0]; 3];
        let b = vec![[0.0, 1.0, 0.0]; 3];
        let i = vec![0_u16, 1, 2];
        let mut seg = [0; SEGMENTS];
        seg[2] = 1;
        let one = ObjectBlock {
            origin: (4096.0, 0.0),
            vertices: &v,
            normals: &n,
            tangents: &t,
            bitangents: &b,
            uv: &uv,
            indices: &i,
            segment_triangles: seg,
            textures: ["Data\\a.dds", "Data\\b.dds"],
        };
        let file = object_block_shapes(&[one.clone(), one]).unwrap();
        let bytes = file.write().unwrap();
        let back = NifFile::read(&bytes).unwrap();
        // root, two times (shape, lighting, texture set, data), multibound and its box
        assert_eq!(back.blocks.len(), 11);
        assert_eq!(back.write().unwrap(), bytes);
        let Body::Opaque(root) = &back.blocks[0].body else {
            panic!("root");
        };
        // the root's children are blocks 1 and 5
        let children =
            |k: usize| u32::from_le_bytes(root[root.len() - 16 + 4 * k..][..4].try_into().unwrap());
        assert_eq!((children(0), children(1)), (1, 5));
        for (shape, data) in [(1_usize, 4_usize), (5, 8)] {
            assert!(matches!(back.blocks[data].body, Body::TriShapeData(_)));
            assert!(matches!(back.blocks[shape + 2].body, Body::Opaque(_)));
        }
    }

    // regression: docs/regressions/2026-11-22-second-atlas-page.md
    #[test]
    fn each_shape_of_a_block_names_its_own_texture_set() {
        covers(&[ids::formats::meshes_and_images::NIF_OBJECT_BLOCKS]);
        let v = vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 50.0]];
        let uv = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let n = vec![[0.0, 0.0, 1.0]; 3];
        let t = vec![[1.0, 0.0, 0.0]; 3];
        let b = vec![[0.0, 1.0, 0.0]; 3];
        let i = vec![0_u16, 1, 2];
        let mut seg = [0; SEGMENTS];
        seg[2] = 1;
        let page = |textures: [&'static str; 2]| ObjectBlock {
            origin: (4096.0, 0.0),
            vertices: &v,
            normals: &n,
            tangents: &t,
            bitangents: &b,
            uv: &uv,
            indices: &i,
            segment_triangles: seg,
            textures,
        };
        let file = object_block_shapes(&[
            page(["Data\\a.dds", "Data\\a_n.dds"]),
            page(["Data\\a01.dds", "Data\\a01_n.dds"]),
            page(["Data\\a02.dds", "Data\\a02_n.dds"]),
        ])
        .unwrap();
        let back = NifFile::read(&file.write().unwrap()).unwrap();
        let int =
            |bytes: &[u8], at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        for (k, name) in ["a.dds", "a01.dds", "a02.dds"].iter().enumerate() {
            let shape = 1 + 4 * k;
            // the shape's property is the lighting block after it ...
            let Body::Opaque(lighting) = &back.blocks[shape + 1].body else {
                panic!("lighting property");
            };
            // ... whose texture set is the block after that, holding this shape's textures
            let set = usize::try_from(int(lighting, super::TEXTURE_SET_REF)).unwrap();
            assert_eq!(set, shape + 2, "shape {k}");
            let Body::Opaque(textures) = &back.blocks[set].body else {
                panic!("texture set");
            };
            assert!(
                String::from_utf8_lossy(textures).contains(name),
                "shape {k} names {}",
                String::from_utf8_lossy(textures)
            );
        }
    }

    #[test]
    fn a_reader_returns_each_shape_with_its_texture_triangles_and_segments() {
        let v = vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 50.0]];
        let uv = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let n = vec![[0.0, 0.0, 1.0]; 3];
        let t = vec![[1.0, 0.0, 0.0]; 3];
        let b = vec![[0.0, 1.0, 0.0]; 3];
        let i = vec![0_u16, 1, 2];
        let page = |textures: [&'static str; 2], segment: usize| {
            let mut seg = [0; SEGMENTS];
            seg[segment] = 1;
            ObjectBlock {
                origin: (4096.0, -8192.0),
                vertices: &v,
                normals: &n,
                tangents: &t,
                bitangents: &b,
                uv: &uv,
                indices: &i,
                segment_triangles: seg,
                textures,
            }
        };
        let file = object_block_shapes(&[
            page(["Data\\a.dds", "Data\\a_n.dds"], 2),
            page(["Data\\a01.dds", "Data\\a01_n.dds"], 5),
        ])
        .unwrap();
        let back = NifFile::read(&file.write().unwrap()).unwrap();
        let shapes = super::read_object_shapes(&back).unwrap();
        assert_eq!(shapes.len(), 2);
        assert_eq!(shapes[0].texture, "Data\\a.dds");
        assert_eq!(shapes[1].texture, "Data\\a01.dds");
        assert_eq!(shapes[1].normal_texture, "Data\\a01_n.dds");
        assert_eq!(shapes[0].origin, [4096.0, -8192.0, 0.0]);
        assert_eq!(shapes[0].vertices, v);
        assert_eq!(shapes[0].triangles, vec![[0, 1, 2]]);
        assert_eq!(shapes[0].segment_triangles, vec![0, 0, 1]);
        assert_eq!(shapes[1].segment_triangles, vec![0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn a_long_segment_table_is_read_whole_even_when_a_word_in_it_looks_like_a_smaller_count() {
        let v = vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 50.0]];
        let uv = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let n = vec![[0.0, 0.0, 1.0]; 3];
        let t = vec![[1.0, 0.0, 0.0]; 3];
        let b = vec![[0.0, 1.0, 0.0]; 3];
        // 15 triangles, one in each of the first fifteen segments: every count word is 1, as a table count of 1 is
        let indices: Vec<u16> = (0..15).flat_map(|_| [0_u16, 1, 2]).collect();
        let mut seg = [1; SEGMENTS];
        seg[15] = 0;
        let file = object_block_shapes(&[ObjectBlock {
            origin: (0.0, 0.0),
            vertices: &v,
            normals: &n,
            tangents: &t,
            bitangents: &b,
            uv: &uv,
            indices: &indices,
            segment_triangles: seg,
            textures: ["Data\\a.dds", "Data\\a_n.dds"],
        }])
        .unwrap();
        let back = NifFile::read(&file.write().unwrap()).unwrap();
        let shapes = super::read_object_shapes(&back).unwrap();
        assert_eq!(shapes[0].segment_triangles, vec![1; 15]);
    }

    #[test]
    fn a_block_without_a_shape_is_refused() {
        covers(&[ids::formats::meshes_and_images::NIF_OBJECT_BLOCKS]);
        assert!(object_block_shapes(&[]).is_err());
    }
}

#[cfg(test)]
mod lights_tests {
    use super::{
        FAR_LIGHTS_MARKER, FarLightsShape, LIGHT_TYPES, ObjectBlock, SEGMENTS, TYPES,
        object_block_with_lights, read_object_shapes,
    };
    use crate::{Body, NifFile};

    fn object<'a>(
        v: &'a [[f32; 3]],
        n: &'a [[f32; 3]],
        uv: &'a [[f32; 2]],
        i: &'a [u16],
    ) -> ObjectBlock<'a> {
        let mut segments = [0; SEGMENTS];
        segments[0] = 1;
        ObjectBlock {
            origin: (4096.0, 8192.0),
            vertices: v,
            normals: n,
            tangents: n,
            bitangents: n,
            uv,
            indices: i,
            segment_triangles: segments,
            textures: ["a.dds", "a_n.dds"],
        }
    }

    #[test]
    fn a_block_with_light_points_carries_one_marked_unlit_additive_shape_and_still_reads_its_objects()
     {
        let v = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 50.0]];
        let n = [[0.0, 0.0, 1.0]; 3];
        let uv = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let i = [0, 1, 2];
        let block = object(&v, &n, &uv, &i);
        let lv = [
            [10.0, 10.0, 500.0],
            [30.0, 10.0, 500.0],
            [10.0, 30.0, 500.0],
        ];
        let colors = [[0.5, 0.5, 0.25, 1.0]; 3];
        let lights = FarLightsShape {
            vertices: &lv,
            colors: &colors,
            uv: &uv,
            indices: &i,
            texture: "textures\\landscape\\lod\\farlight.dds",
        };
        let file = object_block_with_lights(std::slice::from_ref(&block), Some(&lights)).unwrap();
        let bytes = file.write().unwrap();
        let back = NifFile::read(&bytes).unwrap();
        assert_eq!(back.write().unwrap(), bytes);
        assert_eq!(back.blocks.len(), 7 + 5);
        assert_eq!(
            back.header.strings,
            vec![FAR_LIGHTS_MARKER.as_bytes().to_vec()]
        );
        let type_name =
            |k: usize| back.header.block_types[usize::from(back.blocks[k].type_index)].clone();
        assert_eq!(type_name(7), b"NiTriShape".to_vec());
        assert_eq!(type_name(8), b"BSShaderNoLightingProperty".to_vec());
        assert_eq!(type_name(9), b"NiAlphaProperty".to_vec());
        assert_eq!(type_name(10), b"NiTriShapeData".to_vec());
        assert_eq!(type_name(11), b"NiStringExtraData".to_vec());
        let Body::TriShape(shape) = &back.blocks[7].body else {
            panic!("the light shape is a NiTriShape");
        };
        assert_eq!(shape.av.properties, vec![8, 9]);
        assert_eq!(shape.av.net.extra_data, vec![11]);
        assert_eq!(shape.data, 10);
        assert_eq!(shape.av.translation, [4096.0, 8192.0, 0.0]);
        let Body::AlphaProperty(alpha) = &back.blocks[9].body else {
            panic!("alpha");
        };
        // blend on, source SRC_ALPHA, destination ONE, no alpha test
        assert_eq!(alpha.flags, 0x000D);
        let Body::Opaque(unlit) = &back.blocks[8].body else {
            panic!("unlit property");
        };
        let word = |at: usize| u32::from_le_bytes(unlit[at..at + 4].try_into().unwrap());
        // after name, extra data, controller (12) and the shade flags (2): type, flags, second flags
        assert_eq!((word(14), word(18), word(22)), (33, 0x8000_0000, 0x20));
        let Body::TriShapeData(data) = &back.blocks[10].body else {
            panic!("data");
        };
        assert_eq!(data.geometry.vertex_colors.as_deref(), Some(&colors[..]));
        let Body::Opaque(marker) = &back.blocks[11].body else {
            panic!("marker");
        };
        assert_eq!(&marker[..4], &0_u32.to_le_bytes());
        // the reader still finds the one object shape and nothing else
        let shapes = read_object_shapes(&back).unwrap();
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].texture, "a.dds");
    }

    #[test]
    fn without_light_points_the_block_has_no_marker_and_no_light_blocks() {
        let v = [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 50.0]];
        let n = [[0.0, 0.0, 1.0]; 3];
        let uv = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let i = [0, 1, 2];
        let block = object(&v, &n, &uv, &i);
        let nif = object_block_with_lights(std::slice::from_ref(&block), None).unwrap();
        assert_eq!(nif.header.strings, Vec::<Vec<u8>>::new());
        assert_eq!(nif.header.block_types.len(), TYPES.len());
        assert!(
            nif.header
                .block_types
                .iter()
                .all(|t| !LIGHT_TYPES.contains(&t.as_slice()))
        );
        assert_eq!(
            nif.blocks.len(),
            7,
            "root, four blocks of the shape, multibound and its box"
        );
        let back = NifFile::read(&nif.write().unwrap()).unwrap();
        assert_eq!(read_object_shapes(&back).unwrap().len(), 1);
    }
}
