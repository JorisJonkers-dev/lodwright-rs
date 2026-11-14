// SPDX-License-Identifier: MPL-2.0
//! Typed blocks: `NiNode`, `NiTriShape`, `NiTriShapeData` and the common property blocks, plus the opaque
//! carrier for every other block type.
//!
//! Field order and the conditions that switch fields on or off are taken from the record definitions in xEdit's
//! `wbDataFormatNif.pas`, at the revision named in each `source:` row. Only the 20.2.0.7 branch is implemented.
//! Derived from: `Core/wbDataFormatNif.pas` and `Core/wbDataFormatNifTypes.pas` in TES5Edit/TES5Edit at revision
//! `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b` (MPL-2.0).
//!
//! Every string is an index into the header string table (`-1` for none) and every link is a block index
//! (`-1` for none); neither is resolved here.

use crate::error::{NifError, Result};
use crate::io::{Reader, len16, len32, put_f32s, put_i32, put_i32s_counted, put_u16, put_u32};

/// Block type names this crate decodes. Any other name is carried as [`Body::Opaque`].
pub mod names {
    /// `NiNode`.
    pub const NODE: &[u8] = b"NiNode";
    /// `NiTriShape`.
    pub const TRI_SHAPE: &[u8] = b"NiTriShape";
    /// `NiTriShapeData`.
    pub const TRI_SHAPE_DATA: &[u8] = b"NiTriShapeData";
    /// `NiAlphaProperty`.
    pub const ALPHA_PROPERTY: &[u8] = b"NiAlphaProperty";
    /// `NiMaterialProperty`.
    pub const MATERIAL_PROPERTY: &[u8] = b"NiMaterialProperty";
    /// `NiZBufferProperty`.
    pub const Z_BUFFER_PROPERTY: &[u8] = b"NiZBufferProperty";
}

/// The two header numbers that decide which optional fields a block has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// Header user version.
    pub user_version: u32,
    /// Header user version 2 (zero when the header has none).
    pub user_version2: u32,
}

/// A 3-vector.
pub type Vec3 = [f32; 3];

/// `NiObjectNET`: name, extra data and controller.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectNet {
    /// String table index of the name, or `-1`.
    pub name: i32,
    /// Extra data links.
    pub extra_data: Vec<i32>,
    /// Controller link, or `-1`.
    pub controller: i32,
}

/// `NiAVObject`: the scene-graph part of nodes and shapes.
#[derive(Debug, Clone, PartialEq)]
pub struct AvObject {
    /// The `NiObjectNET` part.
    pub net: ObjectNet,
    /// Flags (two bytes on the wire when user version 2 is 26 or less, so the value must fit then).
    pub flags: u32,
    /// Translation.
    pub translation: Vec3,
    /// Rotation matrix, nine floats in the stored order.
    pub rotation: [f32; 9],
    /// Uniform scale.
    pub scale: f32,
    /// Property links (present on the wire when user version 2 is 34 or less; must be empty otherwise).
    pub properties: Vec<i32>,
    /// Collision object link, or `-1`.
    pub collision_object: i32,
}

/// `NiNode`.
#[derive(Debug, Clone, PartialEq)]
pub struct NiNode {
    /// The `NiAVObject` part.
    pub av: AvObject,
    /// Child links.
    pub children: Vec<i32>,
    /// Effect links (present when user version 2 is below 130; must be empty otherwise).
    pub effects: Vec<i32>,
}

/// `NiTriShape` (the `NiGeometry` fields; the shape adds none).
#[derive(Debug, Clone, PartialEq)]
pub struct NiTriShape {
    /// The `NiAVObject` part.
    pub av: AvObject,
    /// Data link, or `-1`.
    pub data: i32,
    /// Skin instance link, or `-1`.
    pub skin_instance: i32,
    /// Material name string indices.
    pub material_names: Vec<i32>,
    /// Material extra data links, one per material name.
    pub material_extra_data: Vec<i32>,
    /// Active material index, or `-1`.
    pub active_material: i32,
    /// Material-needs-update byte.
    pub material_needs_update: u8,
    /// Shader property link (present when user version is 12).
    pub shader_property: i32,
    /// Alpha property link (present when user version is 12).
    pub alpha_property: i32,
}

/// The `NiGeometryData` fields shared by every geometry data block.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryData {
    /// Group id.
    pub group_id: i32,
    /// Vertex count.
    pub num_vertices: u16,
    /// Keep flags.
    pub keep_flags: u8,
    /// Compress flags.
    pub compress_flags: u8,
    /// Vertices, when the has-vertices byte is non-zero.
    pub vertices: Option<Vec<Vec3>>,
    /// Vector flags (bit 0 is the UV set count, bit 12 says tangents follow the normals).
    pub vector_flags: u16,
    /// Material CRC (present when user version is 12).
    pub material_crc: u32,
    /// Normals, when the has-normals byte is non-zero.
    pub normals: Option<Vec<Vec3>>,
    /// Tangents and bitangents (present when there are normals and bit 12 of the vector flags is set).
    pub tangents: Option<(Vec<Vec3>, Vec<Vec3>)>,
    /// Bounding sphere centre.
    pub center: Vec3,
    /// Bounding sphere radius.
    pub radius: f32,
    /// Vertex colours (RGBA floats), when the has-vertex-colours byte is non-zero.
    pub vertex_colors: Option<Vec<[f32; 4]>>,
    /// UV sets; their number is bit 0 of the vector flags.
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    /// Consistency flags.
    pub consistency: u16,
    /// Additional data link, or `-1`.
    pub additional_data: i32,
}

/// `NiTriShapeData`.
#[derive(Debug, Clone, PartialEq)]
pub struct NiTriShapeData {
    /// The geometry data part.
    pub geometry: GeometryData,
    /// Triangle count.
    pub num_triangles: u16,
    /// Triangle point count, three times the triangle count.
    pub num_triangle_points: u32,
    /// Triangles, when the has-triangles byte is non-zero.
    pub triangles: Option<Vec<[u16; 3]>>,
    /// Match groups: lists of vertex indices.
    pub match_groups: Vec<Vec<u16>>,
}

/// `NiAlphaProperty`.
#[derive(Debug, Clone, PartialEq)]
pub struct NiAlphaProperty {
    /// The `NiObjectNET` part.
    pub net: ObjectNet,
    /// Blend and test flags.
    pub flags: u16,
    /// Alpha test threshold.
    pub threshold: u8,
}

/// `NiMaterialProperty`.
#[derive(Debug, Clone, PartialEq)]
pub struct NiMaterialProperty {
    /// The `NiObjectNET` part.
    pub net: ObjectNet,
    /// Ambient colour (present when user version 2 is below 26).
    pub ambient: Vec3,
    /// Diffuse colour (present when user version 2 is below 26).
    pub diffuse: Vec3,
    /// Specular colour.
    pub specular: Vec3,
    /// Emissive colour.
    pub emissive: Vec3,
    /// Glossiness.
    pub glossiness: f32,
    /// Alpha.
    pub alpha: f32,
    /// Emissive multiplier (present when user version 2 is above 21).
    pub emissive_mult: f32,
}

/// `NiZBufferProperty`.
#[derive(Debug, Clone, PartialEq)]
pub struct NiZBufferProperty {
    /// The `NiObjectNET` part.
    pub net: ObjectNet,
    /// Flags.
    pub flags: u16,
}

/// The content of one block.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// A `NiNode`.
    Node(NiNode),
    /// A `NiTriShape`.
    TriShape(NiTriShape),
    /// A `NiTriShapeData`.
    TriShapeData(NiTriShapeData),
    /// A `NiAlphaProperty`.
    AlphaProperty(NiAlphaProperty),
    /// A `NiMaterialProperty`.
    MaterialProperty(NiMaterialProperty),
    /// A `NiZBufferProperty`.
    ZBufferProperty(NiZBufferProperty),
    /// Any other block, as the exact bytes the header sizes it by.
    Opaque(Vec<u8>),
}

impl Body {
    /// The block type name this body is written under, or `None` for [`Body::Opaque`] (any name fits).
    #[must_use]
    pub fn type_name(&self) -> Option<&'static [u8]> {
        match self {
            Self::Node(_) => Some(names::NODE),
            Self::TriShape(_) => Some(names::TRI_SHAPE),
            Self::TriShapeData(_) => Some(names::TRI_SHAPE_DATA),
            Self::AlphaProperty(_) => Some(names::ALPHA_PROPERTY),
            Self::MaterialProperty(_) => Some(names::MATERIAL_PROPERTY),
            Self::ZBufferProperty(_) => Some(names::Z_BUFFER_PROPERTY),
            Self::Opaque(_) => None,
        }
    }

    /// Decode `bytes` as the block type `name`. Names this crate does not decode (and the geometry blocks of
    /// Bethesda streams from user version 2 of 100, whose layout differs) are kept opaque.
    ///
    /// # Errors
    /// Truncation, counts that cannot fit, or a layout that does not use all of `bytes`.
    pub fn parse(name: &[u8], bytes: &[u8], profile: Profile) -> Result<(Self, usize)> {
        let mut r = Reader::new(bytes);
        let geometry_ok = profile.user_version2 < 100;
        let body = if name == names::NODE {
            Self::Node(read_node(&mut r, profile)?)
        } else if name == names::TRI_SHAPE && geometry_ok {
            Self::TriShape(read_tri_shape(&mut r, profile)?)
        } else if name == names::TRI_SHAPE_DATA && geometry_ok {
            Self::TriShapeData(read_tri_shape_data(&mut r, profile)?)
        } else if name == names::ALPHA_PROPERTY {
            Self::AlphaProperty(NiAlphaProperty {
                net: read_net(&mut r)?,
                flags: r.u16()?,
                threshold: r.u8()?,
            })
        } else if name == names::MATERIAL_PROPERTY {
            Self::MaterialProperty(read_material(&mut r, profile)?)
        } else if name == names::Z_BUFFER_PROPERTY {
            Self::ZBufferProperty(NiZBufferProperty {
                net: read_net(&mut r)?,
                flags: r.u16()?,
            })
        } else {
            return Ok((Self::Opaque(bytes.to_vec()), bytes.len()));
        };
        Ok((body, r.pos()))
    }

    /// Encode the block.
    ///
    /// # Errors
    /// [`NifError::Inconsistent`] when counts disagree with arrays or a number does not fit its field.
    pub fn write(&self, profile: Profile) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        match self {
            Self::Node(n) => write_node(&mut out, n, profile)?,
            Self::TriShape(s) => write_tri_shape(&mut out, s, profile)?,
            Self::TriShapeData(d) => write_tri_shape_data(&mut out, d, profile)?,
            Self::AlphaProperty(p) => {
                write_net(&mut out, &p.net)?;
                put_u16(&mut out, p.flags);
                out.push(p.threshold);
            }
            Self::MaterialProperty(m) => write_material(&mut out, m, profile)?,
            Self::ZBufferProperty(p) => {
                write_net(&mut out, &p.net)?;
                put_u16(&mut out, p.flags);
            }
            Self::Opaque(b) => out.extend_from_slice(b),
        }
        Ok(out)
    }
}

// source: nif-objectnet-avobject-ninode
fn read_net(r: &mut Reader<'_>) -> Result<ObjectNet> {
    let name = r.i32()?;
    let n = r.count32("extra data links", 4)?;
    let extra_data = r.i32s(n)?;
    let controller = r.i32()?;
    Ok(ObjectNet {
        name,
        extra_data,
        controller,
    })
}

fn write_net(out: &mut Vec<u8>, net: &ObjectNet) -> Result<()> {
    put_i32(out, net.name);
    put_i32s_counted(out, &net.extra_data, "extra data links")?;
    put_i32(out, net.controller);
    Ok(())
}

// source: nif-objectnet-avobject-ninode
fn read_av(r: &mut Reader<'_>, p: Profile) -> Result<AvObject> {
    let net = read_net(r)?;
    let flags = if p.user_version2 > 26 {
        r.u32()?
    } else {
        u32::from(r.u16()?)
    };
    let translation = r.f32s::<3>()?;
    let rotation = r.f32s::<9>()?;
    let scale = r.f32()?;
    let properties = if p.user_version2 <= 34 {
        let n = r.count32("property links", 4)?;
        r.i32s(n)?
    } else {
        Vec::new()
    };
    let collision_object = r.i32()?;
    Ok(AvObject {
        net,
        flags,
        translation,
        rotation,
        scale,
        properties,
        collision_object,
    })
}

fn write_av(out: &mut Vec<u8>, av: &AvObject, p: Profile) -> Result<()> {
    write_net(out, &av.net)?;
    if p.user_version2 > 26 {
        put_u32(out, av.flags);
    } else {
        put_u16(
            out,
            u16::try_from(av.flags).map_err(|_| NifError::Inconsistent("flags exceed 16 bits"))?,
        );
    }
    put_f32s(out, &av.translation);
    put_f32s(out, &av.rotation);
    put_f32s(out, &[av.scale]);
    if p.user_version2 <= 34 {
        put_i32s_counted(out, &av.properties, "property links")?;
    } else if !av.properties.is_empty() {
        return Err(NifError::Inconsistent(
            "properties on a stream without them",
        ));
    }
    put_i32(out, av.collision_object);
    Ok(())
}

// source: nif-objectnet-avobject-ninode
fn read_node(r: &mut Reader<'_>, p: Profile) -> Result<NiNode> {
    let av = read_av(r, p)?;
    let n = r.count32("children", 4)?;
    let children = r.i32s(n)?;
    let effects = if p.user_version2 < 130 {
        let n = r.count32("effects", 4)?;
        r.i32s(n)?
    } else {
        Vec::new()
    };
    Ok(NiNode {
        av,
        children,
        effects,
    })
}

fn write_node(out: &mut Vec<u8>, node: &NiNode, p: Profile) -> Result<()> {
    write_av(out, &node.av, p)?;
    put_i32s_counted(out, &node.children, "children")?;
    if p.user_version2 < 130 {
        put_i32s_counted(out, &node.effects, "effects")?;
    } else if !node.effects.is_empty() {
        return Err(NifError::Inconsistent("effects on a stream without them"));
    }
    Ok(())
}

// source: nif-trishape-geometry
fn read_tri_shape(r: &mut Reader<'_>, p: Profile) -> Result<NiTriShape> {
    let av = read_av(r, p)?;
    let data = r.i32()?;
    let skin_instance = r.i32()?;
    let n = r.count32("materials", 8)?;
    let material_names = r.i32s(n)?;
    let material_extra_data = r.i32s(n)?;
    let active_material = r.i32()?;
    let material_needs_update = r.u8()?;
    let (shader_property, alpha_property) = if p.user_version == 12 {
        (r.i32()?, r.i32()?)
    } else {
        (-1, -1)
    };
    Ok(NiTriShape {
        av,
        data,
        skin_instance,
        material_names,
        material_extra_data,
        active_material,
        material_needs_update,
        shader_property,
        alpha_property,
    })
}

fn write_tri_shape(out: &mut Vec<u8>, s: &NiTriShape, p: Profile) -> Result<()> {
    if s.material_names.len() != s.material_extra_data.len() {
        return Err(NifError::Inconsistent(
            "material names and extra data differ in length",
        ));
    }
    write_av(out, &s.av, p)?;
    put_i32(out, s.data);
    put_i32(out, s.skin_instance);
    put_u32(out, len32(s.material_names.len(), "materials")?);
    for v in s.material_names.iter().chain(&s.material_extra_data) {
        put_i32(out, *v);
    }
    put_i32(out, s.active_material);
    out.push(s.material_needs_update);
    if p.user_version == 12 {
        put_i32(out, s.shader_property);
        put_i32(out, s.alpha_property);
    }
    Ok(())
}

// source: nif-geometry-data-layout
fn read_geometry_data(r: &mut Reader<'_>, p: Profile) -> Result<GeometryData> {
    let group_id = r.i32()?;
    let num_vertices = r.u16()?;
    let n = usize::from(num_vertices);
    let keep_flags = r.u8()?;
    let compress_flags = r.u8()?;
    let vertices = if r.u8()? != 0 {
        r.check_count("vertices", u32::from(num_vertices), 12)?;
        Some(r.vecs::<3>(n)?)
    } else {
        None
    };
    let vector_flags = r.u16()?;
    let material_crc = if p.user_version == 12 { r.u32()? } else { 0 };
    let has_normals = r.u8()? != 0;
    let (normals, tangents) = if has_normals {
        r.check_count("normals", u32::from(num_vertices), 12)?;
        let normals = r.vecs::<3>(n)?;
        let tangents = if vector_flags & 0x1000 != 0 {
            r.check_count("tangents", u32::from(num_vertices), 24)?;
            Some((r.vecs::<3>(n)?, r.vecs::<3>(n)?))
        } else {
            None
        };
        (Some(normals), tangents)
    } else {
        (None, None)
    };
    let center = r.f32s::<3>()?;
    let radius = r.f32()?;
    let vertex_colors = if r.u8()? != 0 {
        r.check_count("vertex colors", u32::from(num_vertices), 16)?;
        Some(r.vecs::<4>(n)?)
    } else {
        None
    };
    let uv_count = usize::from(vector_flags & 1);
    let mut uv_sets = Vec::new();
    for _ in 0..uv_count {
        r.check_count("uvs", u32::from(num_vertices), 8)?;
        uv_sets.push(r.vecs::<2>(n)?);
    }
    let consistency = r.u16()?;
    let additional_data = r.i32()?;
    Ok(GeometryData {
        group_id,
        num_vertices,
        keep_flags,
        compress_flags,
        vertices,
        vector_flags,
        material_crc,
        normals,
        tangents,
        center,
        radius,
        vertex_colors,
        uv_sets,
        consistency,
        additional_data,
    })
}

fn put_array<const N: usize>(
    out: &mut Vec<u8>,
    vs: &[[f32; N]],
    expect: usize,
    what: &'static str,
) -> Result<()> {
    if vs.len() != expect {
        return Err(NifError::Inconsistent(what));
    }
    for v in vs {
        put_f32s(out, v);
    }
    Ok(())
}

fn write_geometry_data(out: &mut Vec<u8>, g: &GeometryData, p: Profile) -> Result<()> {
    let n = usize::from(g.num_vertices);
    put_i32(out, g.group_id);
    put_u16(out, g.num_vertices);
    out.push(g.keep_flags);
    out.push(g.compress_flags);
    out.push(u8::from(g.vertices.is_some()));
    if let Some(v) = &g.vertices {
        put_array(out, v, n, "vertex count differs from the vertex array")?;
    }
    put_u16(out, g.vector_flags);
    if p.user_version == 12 {
        put_u32(out, g.material_crc);
    }
    out.push(u8::from(g.normals.is_some()));
    let wants_tangents = g.normals.is_some() && g.vector_flags & 0x1000 != 0;
    if g.tangents.is_some() != wants_tangents {
        return Err(NifError::Inconsistent(
            "tangents disagree with normals and vector flags",
        ));
    }
    if let Some(v) = &g.normals {
        put_array(out, v, n, "vertex count differs from the normal array")?;
    }
    if let Some((t, b)) = &g.tangents {
        put_array(out, t, n, "vertex count differs from the tangent array")?;
        put_array(out, b, n, "vertex count differs from the bitangent array")?;
    }
    put_f32s(out, &g.center);
    put_f32s(out, &[g.radius]);
    out.push(u8::from(g.vertex_colors.is_some()));
    if let Some(v) = &g.vertex_colors {
        put_array(out, v, n, "vertex count differs from the colour array")?;
    }
    if g.uv_sets.len() != usize::from(g.vector_flags & 1) {
        return Err(NifError::Inconsistent(
            "uv set count disagrees with the vector flags",
        ));
    }
    for set in &g.uv_sets {
        put_array(out, set, n, "vertex count differs from a uv set")?;
    }
    put_u16(out, g.consistency);
    put_i32(out, g.additional_data);
    Ok(())
}

// source: nif-geometry-data-layout
fn read_tri_shape_data(r: &mut Reader<'_>, p: Profile) -> Result<NiTriShapeData> {
    let geometry = read_geometry_data(r, p)?;
    let num_triangles = r.u16()?;
    let num_triangle_points = r.u32()?;
    let triangles = if r.u8()? != 0 {
        r.check_count("triangles", u32::from(num_triangles), 6)?;
        let mut t = Vec::with_capacity(usize::from(num_triangles));
        for _ in 0..num_triangles {
            t.push([r.u16()?, r.u16()?, r.u16()?]);
        }
        Some(t)
    } else {
        None
    };
    let groups = r.count16("match groups", 2)?;
    let mut match_groups = Vec::with_capacity(groups);
    for _ in 0..groups {
        let n = r.count16("match group indices", 2)?;
        match_groups.push((0..n).map(|_| r.u16()).collect::<Result<Vec<_>>>()?);
    }
    Ok(NiTriShapeData {
        geometry,
        num_triangles,
        num_triangle_points,
        triangles,
        match_groups,
    })
}

fn write_tri_shape_data(out: &mut Vec<u8>, d: &NiTriShapeData, p: Profile) -> Result<()> {
    write_geometry_data(out, &d.geometry, p)?;
    put_u16(out, d.num_triangles);
    put_u32(out, d.num_triangle_points);
    out.push(u8::from(d.triangles.is_some()));
    if let Some(t) = &d.triangles {
        if t.len() != usize::from(d.num_triangles) {
            return Err(NifError::Inconsistent(
                "triangle count differs from the triangle array",
            ));
        }
        for tri in t {
            for i in tri {
                put_u16(out, *i);
            }
        }
    }
    put_u16(out, len16(d.match_groups.len(), "match groups")?);
    for g in &d.match_groups {
        put_u16(out, len16(g.len(), "match group indices")?);
        for i in g {
            put_u16(out, *i);
        }
    }
    Ok(())
}

// source: nif-property-layout
fn read_material(r: &mut Reader<'_>, p: Profile) -> Result<NiMaterialProperty> {
    let net = read_net(r)?;
    let (ambient, diffuse) = if p.user_version2 < 26 {
        (r.f32s::<3>()?, r.f32s::<3>()?)
    } else {
        ([1.0; 3], [1.0; 3])
    };
    let specular = r.f32s::<3>()?;
    let emissive = r.f32s::<3>()?;
    let glossiness = r.f32()?;
    let alpha = r.f32()?;
    let emissive_mult = if p.user_version2 > 21 { r.f32()? } else { 1.0 };
    Ok(NiMaterialProperty {
        net,
        ambient,
        diffuse,
        specular,
        emissive,
        glossiness,
        alpha,
        emissive_mult,
    })
}

fn write_material(out: &mut Vec<u8>, m: &NiMaterialProperty, p: Profile) -> Result<()> {
    write_net(out, &m.net)?;
    if p.user_version2 < 26 {
        put_f32s(out, &m.ambient);
        put_f32s(out, &m.diffuse);
    }
    put_f32s(out, &m.specular);
    put_f32s(out, &m.emissive);
    put_f32s(out, &[m.glossiness, m.alpha]);
    if p.user_version2 > 21 {
        put_f32s(out, &[m.emissive_mult]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net() -> ObjectNet {
        ObjectNet {
            name: -1,
            extra_data: vec![],
            controller: -1,
        }
    }

    fn av(properties: Vec<i32>) -> AvObject {
        AvObject {
            net: net(),
            flags: 14,
            translation: [0.0; 3],
            rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
            properties,
            collision_object: -1,
        }
    }

    fn round_trip(body: &Body, p: Profile) -> Body {
        let bytes = body.write(p).unwrap();
        let name = body.type_name().unwrap();
        let (back, used) = Body::parse(name, &bytes, p).unwrap();
        assert_eq!(used, bytes.len());
        back
    }

    const FNV: Profile = Profile {
        user_version: 11,
        user_version2: 34,
    };

    #[test]
    fn a_node_without_properties_round_trips_on_a_newer_stream() {
        let p = Profile {
            user_version: 12,
            user_version2: 83,
        };
        let node = Body::Node(NiNode {
            av: av(vec![]),
            children: vec![1, 2],
            effects: vec![],
        });
        assert_eq!(round_trip(&node, p), node);
    }

    #[test]
    fn a_node_with_properties_is_refused_on_a_stream_that_has_none() {
        let p = Profile {
            user_version: 12,
            user_version2: 83,
        };
        let node = Body::Node(NiNode {
            av: av(vec![3]),
            children: vec![],
            effects: vec![],
        });
        assert!(node.write(p).is_err());
    }

    #[test]
    fn effects_are_refused_on_a_stream_from_user_version_two_of_130() {
        let p = Profile {
            user_version: 12,
            user_version2: 130,
        };
        let node = Body::Node(NiNode {
            av: av(vec![]),
            children: vec![],
            effects: vec![1],
        });
        assert!(node.write(p).is_err());
    }

    #[test]
    fn flags_use_two_bytes_on_an_old_stream_and_must_fit() {
        let p = Profile {
            user_version: 0,
            user_version2: 0,
        };
        let mut a = av(vec![]);
        a.flags = 70_000;
        assert!(
            Body::Node(NiNode {
                av: a,
                children: vec![],
                effects: vec![]
            })
            .write(p)
            .is_err()
        );
        let node = Body::Node(NiNode {
            av: av(vec![4]),
            children: vec![],
            effects: vec![0],
        });
        assert_eq!(round_trip(&node, p), node);
    }

    #[test]
    fn a_material_property_round_trips_with_and_without_the_ambient_fields() {
        let m = |ambient: Vec3, mult: f32| {
            Body::MaterialProperty(NiMaterialProperty {
                net: net(),
                ambient,
                diffuse: ambient,
                specular: [0.5; 3],
                emissive: [0.0; 3],
                glossiness: 10.0,
                alpha: 1.0,
                emissive_mult: mult,
            })
        };
        let old = Profile {
            user_version: 0,
            user_version2: 20,
        };
        assert_eq!(round_trip(&m([0.25; 3], 1.0), old), m([0.25; 3], 1.0));
        assert_eq!(round_trip(&m([1.0; 3], 2.0), FNV), m([1.0; 3], 2.0));
    }

    #[test]
    fn a_z_buffer_property_and_an_alpha_property_round_trip() {
        let z = Body::ZBufferProperty(NiZBufferProperty {
            net: net(),
            flags: 3,
        });
        assert_eq!(round_trip(&z, FNV), z);
        let a = Body::AlphaProperty(NiAlphaProperty {
            net: net(),
            flags: 4844,
            threshold: 128,
        });
        assert_eq!(round_trip(&a, FNV), a);
    }

    #[test]
    fn a_shape_on_user_version_12_carries_shader_and_alpha_links() {
        let p = Profile {
            user_version: 12,
            user_version2: 83,
        };
        let s = Body::TriShape(NiTriShape {
            av: av(vec![]),
            data: 2,
            skin_instance: -1,
            material_names: vec![5],
            material_extra_data: vec![-1],
            active_material: 0,
            material_needs_update: 1,
            shader_property: 7,
            alpha_property: 8,
        });
        assert_eq!(round_trip(&s, p), s);
    }

    #[test]
    fn a_shape_whose_material_arrays_differ_in_length_cannot_be_written() {
        let s = Body::TriShape(NiTriShape {
            av: av(vec![]),
            data: 2,
            skin_instance: -1,
            material_names: vec![5, 6],
            material_extra_data: vec![-1],
            active_material: 0,
            material_needs_update: 0,
            shader_property: -1,
            alpha_property: -1,
        });
        assert!(s.write(FNV).is_err());
    }

    fn data(vector_flags: u16, tangents: bool) -> Body {
        let v = vec![[0.0, 1.0, 2.0]; 2];
        Body::TriShapeData(NiTriShapeData {
            geometry: GeometryData {
                group_id: 0,
                num_vertices: 2,
                keep_flags: 0,
                compress_flags: 0,
                vertices: Some(v.clone()),
                vector_flags,
                material_crc: 9,
                normals: Some(v.clone()),
                tangents: tangents.then(|| (v.clone(), v.clone())),
                center: [0.0; 3],
                radius: 1.0,
                vertex_colors: Some(vec![[1.0, 0.5, 0.25, 1.0]; 2]),
                uv_sets: vec![vec![[0.0, 1.0]; 2]; usize::from(vector_flags & 1)],
                consistency: 0,
                additional_data: -1,
            },
            num_triangles: 1,
            num_triangle_points: 3,
            triangles: Some(vec![[0, 1, 0]]),
            match_groups: vec![vec![0, 1], vec![]],
        })
    }

    #[test]
    fn geometry_data_with_tangents_colours_and_material_crc_round_trips() {
        let p = Profile {
            user_version: 12,
            user_version2: 83,
        };
        let d = data(0x1001, true);
        assert_eq!(round_trip(&d, p), d);
    }

    #[test]
    fn tangents_without_the_vector_flag_cannot_be_written() {
        assert!(data(1, true).write(FNV).is_err());
        assert!(data(0x1000, false).write(FNV).is_err());
    }

    #[test]
    fn a_uv_set_count_that_disagrees_with_the_vector_flags_cannot_be_written() {
        let Body::TriShapeData(mut d) = data(1, false) else {
            unreachable!()
        };
        d.geometry.uv_sets.clear();
        assert!(Body::TriShapeData(d).write(FNV).is_err());
    }

    #[test]
    fn a_triangle_array_of_the_wrong_length_cannot_be_written() {
        let Body::TriShapeData(mut d) = data(0, false) else {
            unreachable!()
        };
        d.triangles = Some(vec![]);
        assert!(Body::TriShapeData(d).write(FNV).is_err());
    }

    #[test]
    fn geometry_blocks_of_a_newer_stream_stay_opaque() {
        let p = Profile {
            user_version: 12,
            user_version2: 130,
        };
        let (b, used) = Body::parse(names::TRI_SHAPE_DATA, &[1, 2, 3], p).unwrap();
        assert_eq!((b, used), (Body::Opaque(vec![1, 2, 3]), 3));
    }
}
