// SPDX-License-Identifier: MPL-2.0
//! Object LOD generation from synthetic models, textures and references.

use std::collections::HashMap;

use lodwright_core::objects::Instance;
use lodwright_core::plugin::Placement;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode, read_header};
use lodwright_gen::{Assets, generate_objects};
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
    let node = NiNode {
        av: av(vec![]),
        children: vec![1],
        effects: vec![],
    };
    let shape = NiTriShape {
        av: av(vec![3]),
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
            uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
            consistency: 0,
            additional_data: -1,
        },
        num_triangles: 1,
        strips: Some(vec![vec![0, 1, 2]]),
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

fn instance(model: &str, x: f32, y: f32, rot_z: f32, reference: u32, high: bool) -> Instance {
    Instance {
        base: 1,
        signature: *b"STAT",
        model: model.to_owned(),
        placement: Placement {
            pos: [x, y, 10.0],
            rot: [0.0, 0.0, rot_z],
            scale: 2.0,
        },
        high_priority: high,
        has_tree_lod: false,
        reference,
        part: false,
    }
}

fn assets() -> Memory {
    let mut m = HashMap::new();
    m.insert(
        "meshes/a/wall_lod.nif".to_owned(),
        lod_model("Data\\Textures\\a\\wall.dds"),
    );
    m.insert("textures/a/wall.dds".to_owned(), texture(128, 200));
    Memory(m)
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-objects-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn read_block(out: &std::path::Path, rel: &str) -> NifFile {
    NifFile::read(&std::fs::read(out.join(rel)).unwrap()).unwrap()
}

#[test]
fn objects_are_merged_per_block_with_transform_scale_and_atlas_uvs() {
    let out = temp("merge");
    let list = vec![
        instance(
            "a\\Wall.nif",
            4096.0 * 5.0 + 100.0,
            4096.0 * 1.0 + 50.0,
            0.0,
            10,
            false,
        ),
        instance(
            "a\\Wall.nif",
            4096.0 * 6.0,
            4096.0 * 2.0,
            std::f32::consts::FRAC_PI_2,
            11,
            false,
        ),
    ];
    let report = generate_objects("Mojave", &list, &assets(), &out).unwrap();
    assert_eq!((report.instances, report.skipped), (2, 0));
    let block = read_block(
        &out,
        "meshes/landscape/lod/mojave/blocks/mojave.level4.x4.y0.nif",
    );
    assert_eq!(block.blocks.len(), 7);
    let Body::TriShapeData(d) = &block.blocks[4].body else {
        panic!("data block");
    };
    let v = d.geometry.vertices.as_ref().unwrap();
    assert_eq!(v.len(), 6);
    // first instance: scale 2, no rotation, block-local position
    assert_eq!(v[1], [4396.0, 4146.0, 10.0]);
    // second instance turned by the game's convention (Rz(-z)): the model's x axis points down the y axis
    assert!(
        (v[4][0] - 8192.0).abs() < 0.01 && (v[4][1] - 7992.0).abs() < 0.01,
        "{:?}",
        v[4]
    );
    let uv = &d.geometry.uv_sets[0];
    assert!(
        uv.iter()
            .all(|u| (0.0..=1.0).contains(&u[0]) && (0.0..=1.0).contains(&u[1]))
    );
    assert!(d.geometry.tangents.is_some() && d.geometry.normals.is_some());
}

#[test]
fn the_atlas_pair_is_written_as_bc3_and_the_normal_atlas_is_half_size() {
    let out = temp("atlas");
    generate_objects(
        "Mojave",
        &[instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, false)],
        &assets(),
        &out,
    )
    .unwrap();
    let d = std::fs::read(out.join("textures/landscape/lod/mojave/blocks/mojave.buildings.dds"))
        .unwrap();
    let n = std::fs::read(out.join("textures/landscape/lod/mojave/blocks/mojave.buildings_n.dds"))
        .unwrap();
    let (dh, nh) = (read_header(&d).unwrap(), read_header(&n).unwrap());
    assert_eq!((dh.width, dh.height, dh.format), (2048, 1024, Format::Bc3));
    assert_eq!((nh.width, nh.height), (1024, 512));
}

#[test]
fn a_high_priority_reference_goes_to_both_files_and_a_missing_model_is_skipped() {
    let out = temp("high");
    let list = vec![
        instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, true),
        instance("a\\Missing.nif", 200.0, 200.0, 0.0, 2, false),
    ];
    let report = generate_objects("Mojave", &list, &assets(), &out).unwrap();
    assert_eq!(report.skipped, 1);
    assert!(
        out.join("meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y0.nif")
            .is_file()
    );
    assert!(
        out.join("meshes/landscape/lod/mojave/blocks/mojave.level4.high.x0.y0.nif")
            .is_file()
    );
}

#[test]
fn nothing_is_written_when_no_model_can_be_read_and_output_is_deterministic() {
    let out = temp("none");
    let report = generate_objects(
        "Mojave",
        &[instance("zzz.nif", 1.0, 1.0, 0.0, 1, false)],
        &assets(),
        &out,
    )
    .unwrap();
    assert!(report.files.is_empty() && !out.exists());
    let (one, two) = (temp("d1"), temp("d2"));
    let list = vec![
        instance("a\\Wall.nif", 100.0, 100.0, 0.3, 1, false),
        instance("a\\Wall.nif", 5000.0, 90.0, 0.1, 2, false),
    ];
    let a = generate_objects("Mojave", &list, &assets(), &one).unwrap();
    let b = generate_objects("Mojave", &list, &assets(), &two).unwrap();
    assert_eq!(a, b);
}
