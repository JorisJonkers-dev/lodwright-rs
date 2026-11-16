// SPDX-License-Identifier: GPL-3.0-only
//! Shared builders: turn testkit shape specs into NIF files.

use lodwright_nif::{
    AvObject, Block, Body, ExportInfo, GeometryData, Header, NiAlphaProperty, NiNode, NiTriShape,
    NiTriShapeData, NifFile, ObjectNet, names,
};
use lodwright_testkit::nif::ShapeSpec;

pub const USER_VERSION: u32 = 11;
pub const USER_VERSION2: u32 = 34;

fn net(name: i32) -> ObjectNet {
    ObjectNet {
        name,
        extra_data: vec![],
        controller: -1,
    }
}

fn av(name: i32, translation: [f32; 3], properties: Vec<i32>) -> AvObject {
    AvObject {
        net: net(name),
        flags: 14,
        translation,
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        properties,
        collision_object: -1,
    }
}

fn type_index(types: &mut Vec<Vec<u8>>, name: &[u8]) -> u16 {
    let at = types.iter().position(|t| t == name).unwrap_or_else(|| {
        types.push(name.to_vec());
        types.len() - 1
    });
    u16::try_from(at).unwrap()
}

/// One root node with every shape below it; shapes with an alpha spec get a property block.
pub fn build(specs: &[ShapeSpec]) -> NifFile {
    let mut types = Vec::new();
    let mut strings = vec![b"Root".to_vec()];
    let mut blocks = Vec::new();
    let mut children = Vec::new();
    for spec in specs {
        strings.push(spec.name.clone().into_bytes());
        let name = i32::try_from(strings.len() - 1).unwrap();
        let base = i32::try_from(1 + blocks.len()).unwrap();
        let (shape_at, data_at) = (base, base + 1);
        let alpha_at = spec.alpha.map(|_| base + 2);
        children.push(shape_at);
        blocks.push(Block {
            type_index: type_index(&mut types, names::TRI_SHAPE),
            body: Body::TriShape(NiTriShape {
                av: av(name, spec.translation, alpha_at.into_iter().collect()),
                data: data_at,
                skin_instance: -1,
                material_names: vec![],
                material_extra_data: vec![],
                active_material: -1,
                material_needs_update: 0,
                shader_property: -1,
                alpha_property: -1,
            }),
        });
        let n = u16::try_from(spec.vertices.len()).unwrap();
        let t = u16::try_from(spec.triangles.len()).unwrap();
        blocks.push(Block {
            type_index: type_index(&mut types, names::TRI_SHAPE_DATA),
            body: Body::TriShapeData(NiTriShapeData {
                geometry: GeometryData {
                    group_id: 0,
                    num_vertices: n,
                    keep_flags: 0,
                    compress_flags: 0,
                    vertices: Some(spec.vertices.clone()),
                    vector_flags: u16::from(spec.uvs.is_some()),
                    material_crc: 0,
                    normals: spec.normals.clone(),
                    tangents: None,
                    center: [0.0; 3],
                    radius: 1.0,
                    vertex_colors: None,
                    uv_sets: spec.uvs.iter().cloned().collect(),
                    consistency: 0x4000,
                    additional_data: -1,
                },
                num_triangles: t,
                num_triangle_points: u32::from(t) * 3,
                triangles: Some(spec.triangles.clone()),
                match_groups: vec![],
            }),
        });
        if let Some((flags, threshold)) = spec.alpha {
            blocks.push(Block {
                type_index: type_index(&mut types, names::ALPHA_PROPERTY),
                body: Body::AlphaProperty(NiAlphaProperty {
                    net: net(-1),
                    flags,
                    threshold,
                }),
            });
        }
    }
    let root = Block {
        type_index: type_index(&mut types, names::NODE),
        body: Body::Node(NiNode {
            av: av(0, [0.0; 3], vec![]),
            children,
            effects: vec![],
        }),
    };
    blocks.insert(0, root);
    NifFile {
        header: Header {
            user_version: USER_VERSION,
            bethesda: Some((
                USER_VERSION2,
                ExportInfo {
                    author: b"synthetic\0".to_vec(),
                    unknown_int: 0,
                    process_script: b"\0".to_vec(),
                    export_script: b"\0".to_vec(),
                    max_filepath: vec![],
                },
            )),
            block_types: types,
            strings,
            groups: vec![],
        },
        blocks,
        roots: vec![0],
    }
}
