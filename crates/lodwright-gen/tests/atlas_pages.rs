// SPDX-License-Identifier: GPL-3.0-only
//! Objects whose textures land on a second atlas page: the block they are written into must draw them with that page.
//! The check follows the references a reader follows (shape, its lighting property, that property's texture set),
//! then samples the named page at each triangle's UVs.

use std::collections::HashMap;

use lodwright_core::objects::Instance;
use lodwright_core::plugin::Placement;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
use lodwright_gen::{Assets, Surfaces, generate_objects_with_atlas_limit};
use lodwright_nif::{
    AvObject, Block, Body, ExportInfo, GeometryData, Header, NiNode, NiTriShape, NiTriStripsData,
    NifFile, ObjectNet,
};

struct Memory(HashMap<String, Vec<u8>>);

impl Assets for Memory {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0
            .get(&path.to_ascii_lowercase().replace('\\', "/"))
            .cloned()
    }
}

fn av(properties: Vec<i32>) -> AvObject {
    AvObject {
        net: ObjectNet {
            name: -1,
            extra_data: vec![],
            controller: -1,
        },
        flags: 14,
        translation: [0.0; 3],
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        properties,
        collision_object: -1,
    }
}

/// A one-triangle LOD model with a texture set: node, strips shape, strips data, lighting property, texture set.
#[allow(clippy::too_many_lines)] // one literal NIF
fn lod_model(texture: &str) -> Vec<u8> {
    let (node_av, shape_av, triangles) = (av(vec![]), av(vec![3]), 1_u16);
    let uv = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let node = NiNode {
        av: node_av,
        children: vec![1],
        effects: vec![],
    };
    let shape = NiTriShape {
        av: shape_av,
        data: 2,
        skin_instance: -1,
        material_names: vec![],
        material_extra_data: vec![],
        active_material: 0,
        material_needs_update: 0,
        shader_property: -1,
        alpha_property: -1,
    };
    let data = NiTriStripsData {
        geometry: GeometryData {
            group_id: 0,
            num_vertices: 3,
            keep_flags: 0,
            compress_flags: 0,
            vertices: Some(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]]),
            vector_flags: 1,
            material_crc: 0,
            normals: None,
            tangents: None,
            center: [0.0; 3],
            radius: 0.0,
            vertex_colors: None,
            uv_sets: vec![uv],
            consistency: 0,
            additional_data: -1,
        },
        num_triangles: triangles,
        strips: Some(vec![(0..triangles + 2).map(|k| k % 3).collect()]),
    };
    let mut lighting = Vec::new();
    for v in [u32::MAX, 0, u32::MAX] {
        lighting.extend_from_slice(&v.to_le_bytes());
    }
    lighting.extend_from_slice(&[0; 42]);
    lighting[34..38].copy_from_slice(&4_u32.to_le_bytes());
    let mut set = 6_u32.to_le_bytes().to_vec();
    for p in [texture, ""] {
        set.extend_from_slice(&u32::try_from(p.len()).unwrap().to_le_bytes());
        set.extend_from_slice(p.as_bytes());
    }
    set.extend_from_slice(&[0; 16]);
    let types: Vec<Vec<u8>> = [
        &b"NiNode"[..],
        b"NiTriStrips",
        b"NiTriStripsData",
        b"BSShaderPPLightingProperty",
        b"BSShaderTextureSet",
    ]
    .iter()
    .map(|t| t.to_vec())
    .collect();
    NifFile {
        header: Header {
            user_version: 11,
            bethesda: Some((
                34,
                ExportInfo {
                    author: vec![0],
                    unknown_int: 0,
                    process_script: vec![0],
                    export_script: vec![0],
                    max_filepath: vec![],
                },
            )),
            block_types: types,
            strings: vec![],
            groups: vec![],
        },
        blocks: vec![
            Block {
                type_index: 0,
                body: Body::Node(node),
            },
            Block {
                type_index: 1,
                body: Body::TriStrips(shape),
            },
            Block {
                type_index: 2,
                body: Body::TriStripsData(data),
            },
            Block {
                type_index: 3,
                body: Body::Opaque(lighting),
            },
            Block {
                type_index: 4,
                body: Body::Opaque(set),
            },
        ],
        roots: vec![0],
    }
    .write()
    .unwrap()
}

fn texture(size: u32, shade: u8) -> Vec<u8> {
    let px: Vec<u8> = (0..size * size)
        .flat_map(|_| [shade, shade, shade, 255])
        .collect();
    encode(
        &Image::new(size, size, px).unwrap(),
        &EncodeOptions {
            format: Format::Bc1,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )
    .unwrap()
}

fn instance(model: &str, x: f32, reference: u32) -> Instance {
    Instance {
        base: 1,
        signature: *b"STAT",
        model: model.to_owned(),
        placement: Placement {
            pos: [x, 100.0, 10.0],
            rot: [0.0; 3],
            scale: 2.0,
        },
        high_priority: false,
        has_tree_lod: false,
        reference,
        reference_plugin: 0,
        base_plugin: 0,
        part: false,
        cell: None,
    }
}

fn int(bytes: &[u8], at: usize) -> usize {
    usize::try_from(i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())).unwrap()
}

/// Per shape of a block: the diffuse texture its lighting property names (by reference, as a reader follows it) and
/// the shade of that page at the centre of each triangle's UVs.
fn shades_per_shape(out: &std::path::Path, world: &str, block: &str) -> Vec<(String, Vec<u8>)> {
    let nif = NifFile::read(
        &std::fs::read(out.join(format!("meshes/landscape/lod/{world}/blocks/{block}"))).unwrap(),
    )
    .unwrap();
    let Body::Opaque(root) = &nif.blocks[0].body else {
        panic!("root node");
    };
    // the root node's last fields: the child count and the children, an empty effect list, the multibound
    let mut result = Vec::new();
    let count = nif
        .blocks
        .iter()
        .filter(|b| matches!(b.body, Body::TriShapeData(_)))
        .count();
    let children = root.len() - 8 - 4 * count;
    assert_eq!(int(root, children - 4), count, "child count");
    for k in 0..count {
        let shape = int(root, children + 4 * k);
        let Body::Opaque(shape_bytes) = &nif.blocks[shape].body else {
            panic!("shape");
        };
        // a NiTriShape for this version: name, extra data list, controller, flags (u32), translation, rotation,
        // scale, property count and refs, collision, data
        let props = 4 + 4 + 4 + 4 + 12 + 36 + 4;
        assert_eq!(int(shape_bytes, props), 1, "one property");
        let lighting = int(shape_bytes, props + 4);
        let data = int(shape_bytes, props + 12);
        let Body::Opaque(lighting_bytes) = &nif.blocks[lighting].body else {
            panic!("lighting property");
        };
        let set = int(lighting_bytes, 34);
        let Body::Opaque(set_bytes) = &nif.blocks[set].body else {
            panic!("texture set");
        };
        let len = int(set_bytes, 4);
        let name = String::from_utf8_lossy(&set_bytes[8..8 + len]).to_ascii_lowercase();
        let file = name.rsplit('\\').next().unwrap().to_owned();
        let dds = std::fs::read(out.join(format!("textures/landscape/lod/{world}/blocks/{file}")))
            .unwrap();
        let image = &lodwright_dds::decode(&dds).unwrap().levels[0];
        let Body::TriShapeData(shape_data) = &nif.blocks[data].body else {
            panic!("shape data");
        };
        let uv = &shape_data.geometry.uv_sets[0];
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let shades = shape_data
            .triangles
            .as_ref()
            .unwrap()
            .iter()
            .map(|t| {
                let corners = t.map(|i| uv[usize::from(i)]);
                let centre = |k: usize| corners.iter().map(|c| c[k]).sum::<f32>() / 3.0;
                let column = (centre(0) * image.width() as f32) as u32;
                let row = (centre(1) * image.height() as f32) as u32;
                image.rgba()[((row * image.width() + column) * 4) as usize]
            })
            .collect();
        result.push((file, shades));
    }
    result
}

// regression: docs/regressions/2026-11-22-second-atlas-page.md
#[test]
fn objects_on_a_second_atlas_page_are_drawn_with_their_own_texture() {
    // with atlases of at most 2048 by 2048, four textures of 1024 by 1024 fill the first; the fifth goes on a second.
    let out = std::env::temp_dir().join(format!("lodwright-atlas-pages-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let shade = |k: u8| 40 + 40 * k;
    let mut m = HashMap::new();
    let mut list = Vec::new();
    for k in 0..5_u8 {
        m.insert(
            format!("meshes/a/wall{k}_lod.nif"),
            lod_model(&format!("Data\\Textures\\a\\wall{k}.dds")),
        );
        m.insert(format!("textures/a/wall{k}.dds"), texture(1024, shade(k)));
        list.push(instance(
            &format!("a\\Wall{k}.nif"),
            100.0 + f32::from(k),
            u32::from(k) + 1,
        ));
    }
    generate_objects_with_atlas_limit(
        "Mojave",
        &list,
        &Memory(m),
        &out,
        Surfaces::default(),
        (2048, 2048),
        &mut |_, _| {},
    )
    .unwrap();
    let shapes = shades_per_shape(&out, "mojave", "mojave.level4.x0.y0.nif");
    let _ = std::fs::remove_dir_all(&out);
    let names: Vec<&str> = shapes.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["mojave.buildings.dds", "mojave.buildings01.dds"]);
    let mut seen: Vec<u8> = shapes.iter().flat_map(|(_, s)| s.iter().copied()).collect();
    seen.sort_unstable();
    assert_eq!(seen.len(), 5, "{shapes:?}");
    for (k, s) in (0..5_u8).zip(&seen) {
        assert!(
            s.abs_diff(shade(k)) <= 4,
            "triangle {k} shows {s}: {shapes:?}"
        );
    }
}
