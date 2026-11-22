// SPDX-License-Identifier: GPL-3.0-only
//! Object LOD generation from synthetic models, textures and references.

use std::collections::HashMap;

use lodwright_core::objects::Instance;
use lodwright_core::plugin::Placement;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode, read_header};
use lodwright_gen::{
    Assets, Surfaces, generate_objects, generate_objects_with_atlas_limit,
    generate_objects_with_progress,
};
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
fn lod_model(texture: &str) -> Vec<u8> {
    lod_model_with(texture, av(vec![]), av(vec![3]))
}

/// The same model with the given node and shape transforms.
fn lod_model_with(texture: &str, node_av: AvObject, shape_av: AvObject) -> Vec<u8> {
    lod_model_full(
        texture,
        node_av,
        shape_av,
        vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
    )
}

/// The same model with the given node and shape transforms and UVs.
fn lod_model_full(
    texture: &str,
    node_av: AvObject,
    shape_av: AvObject,
    uv: Vec<[f32; 2]>,
) -> Vec<u8> {
    lod_model_tris(texture, node_av, shape_av, uv, 1)
}

/// The same with a strip of `triangles` triangles over the same three vertices.
#[allow(clippy::too_many_lines)] // one literal NIF
fn lod_model_tris(
    texture: &str,
    node_av: AvObject,
    shape_av: AvObject,
    uv: Vec<[f32; 2]>,
    triangles: u16,
) -> Vec<u8> {
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
        reference_plugin: 0,
        base_plugin: 0,
        part: false,
        cell: None,
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
    // the segment table: one segment per cell of the block, x-major. The first object stands in cell (1, 1) of the
    // block, the second in cell (2, 2), so segments 5 and 10 hold one triangle each and the table ends with 10.
    let Body::Opaque(shape) = &block.blocks[1].body else {
        panic!("shape block");
    };
    let count = 11;
    let table = &shape[shape.len() - count * 9 - 4..];
    assert_eq!(&table[..4], &u32::try_from(count).unwrap().to_le_bytes());
    let triangles =
        |k: usize| u32::from_le_bytes(table[4 + k * 9 + 5..4 + k * 9 + 9].try_into().unwrap());
    let counts: Vec<u32> = (0..count).map(triangles).collect();
    assert_eq!(counts, [0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
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
fn each_block_file_reports_once_in_order_and_the_count_reaches_the_total() {
    let out = temp("progress");
    let list = vec![
        instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, true),
        instance("a\\Wall.nif", 4096.0 * 9.0, 100.0, 0.0, 2, false),
    ];
    let mut seen = Vec::new();
    let report = generate_objects_with_progress(
        "Mojave",
        &list,
        &assets(),
        &out,
        Surfaces::default(),
        &mut |done, total| {
            seen.push((done, total));
        },
    )
    .unwrap();
    // block x0 has a normal and a high-priority file, block x8 a normal one
    assert_eq!(seen, [(1, 3), (2, 3), (3, 3)]);
    assert_eq!(
        report.files.len(),
        2 + 3,
        "the atlas pair and three block files"
    );
}

#[test]
fn a_world_without_a_usable_model_reports_no_block() {
    let out = temp("progress-none");
    let mut seen = Vec::new();
    generate_objects_with_progress(
        "Mojave",
        &[instance("zzz.nif", 1.0, 1.0, 0.0, 1, false)],
        &assets(),
        &out,
        Surfaces::default(),
        &mut |done, total| seen.push((done, total)),
    )
    .unwrap();
    assert_eq!(seen, Vec::<(u64, u64)>::new());
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

// source: game-object-shape-offset
#[test]
fn the_shape_offset_is_not_scaled_and_node_rotations_are_ignored() {
    let out = temp("offset");
    // the root node turns the model a quarter turn about z, which the game's blocks ignore
    let mut node = av(vec![]);
    node.rotation = [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
    let mut shape = av(vec![3]);
    shape.translation = [10.0, 20.0, 30.0];
    let mut m = assets();
    m.0.insert(
        "meshes/a/post_lod.nif".to_owned(),
        lod_model_with("Data\\Textures\\a\\wall.dds", node, shape),
    );
    let list = vec![instance(
        "a\\Post.nif",
        1000.0,
        1000.0,
        std::f32::consts::FRAC_PI_2,
        1,
        false,
    )];
    generate_objects("Mojave", &list, &m, &out).unwrap();
    let block = read_block(
        &out,
        "meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y0.nif",
    );
    let Body::TriShapeData(d) = &block.blocks[4].body else {
        panic!("data block");
    };
    let v = d.geometry.vertices.as_ref().unwrap();
    // pos + R * (s * v + T) with v = (100, 0, 0), s = 2, T = (10, 20, 30) and R the reference's quarter turn
    // (Rz(-z): x goes to -y): (1000, 1000, 10) + (20, -210, 30)
    let want = [1020.0, 790.0, 40.0];
    assert!(
        v[1].iter().zip(want).all(|(a, b)| (a - b).abs() < 0.01),
        "{:?}",
        v[1]
    );
}

// source: game-object-block-cell
#[test]
fn a_temporary_reference_goes_to_the_block_of_its_cell() {
    let out = temp("cell");
    let mut held = instance("a\\Wall.nif", 100.0, -0.0001, 0.0, 1, false);
    held.cell = Some((0, 0));
    let loose = instance("a\\Wall.nif", 200.0, -0.0001, 0.0, 2, false);
    generate_objects("Mojave", &[held, loose], &assets(), &out).unwrap();
    let count = |rel: &str| {
        let Body::TriShapeData(d) = &read_block(&out, rel).blocks[4].body else {
            panic!("data block");
        };
        d.geometry.vertices.as_ref().unwrap().len()
    };
    assert_eq!(
        count("meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y0.nif"),
        3
    );
    assert_eq!(
        count("meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y-4.nif"),
        3
    );
}

/// Generate `list` with the given surfaces and return the report and the vertex count of block x0 y0, if written.
fn with_surfaces(
    name: &str,
    list: &[Instance],
    surfaces: Surfaces,
) -> (lodwright_gen::ObjectReport, Option<usize>) {
    let out = temp(name);
    let report =
        generate_objects_with_progress("Mojave", list, &assets(), &out, surfaces, &mut |_, _| {})
            .unwrap();
    let path = out.join("meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y0.nif");
    let vertices = path.is_file().then(|| {
        let block = NifFile::read(&std::fs::read(path).unwrap()).unwrap();
        block
            .blocks
            .iter()
            .find_map(|b| match &b.body {
                Body::TriShapeData(d) => d.geometry.vertices.as_ref().map(Vec::len),
                _ => None,
            })
            .unwrap_or(0)
    });
    (report, vertices)
}

#[test]
fn faces_wholly_under_the_ground_are_dropped_and_faces_above_or_crossing_it_are_kept() {
    // a flat triangle at height 10, one at height 10 turned upright (it reaches 200 units up), one at height 90
    let mut upright = instance("a\\Wall.nif", 300.0, 100.0, 0.0, 2, false);
    upright.placement.rot = [-std::f32::consts::FRAC_PI_2, 0.0, 0.0];
    let mut high = instance("a\\Wall.nif", 500.0, 100.0, 0.0, 3, false);
    high.placement.pos[2] = 90.0;
    let list = vec![
        instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, false),
        upright,
        high,
    ];
    let ground = |_: f32, _: f32| Some(50.0);
    let (report, vertices) = with_surfaces(
        "under-ground",
        &list,
        Surfaces {
            ground: Some(&ground),
            water: None,
            ..Surfaces::default()
        },
    );
    assert_eq!(
        (report.dropped_under_terrain, report.dropped_under_water),
        (1, 0)
    );
    assert_eq!(
        vertices,
        Some(6),
        "the dropped face's three vertices are gone"
    );
    let (all, kept) = with_surfaces("no-surfaces", &list, Surfaces::default());
    assert_eq!((all.dropped_under_terrain, kept), (0, Some(9)));
}

#[test]
fn faces_under_water_are_dropped_only_in_cells_that_have_water() {
    // the same low triangle in a cell with water at 50 and in a cell without water
    let list = vec![
        instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, false),
        instance("a\\Wall.nif", 4096.0 + 100.0, 100.0, 0.0, 2, false),
    ];
    let water = |x: f32, _: f32| (x < 4096.0).then_some(50.0);
    let (report, vertices) = with_surfaces(
        "under-water",
        &list,
        Surfaces {
            ground: None,
            water: Some(&water),
            ..Surfaces::default()
        },
    );
    assert_eq!(
        (report.dropped_under_terrain, report.dropped_under_water),
        (0, 1)
    );
    assert_eq!(vertices, Some(3));
}

#[test]
fn a_model_with_every_face_dropped_writes_no_block_and_counts_its_faces() {
    let list = vec![instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, false)];
    let ground = |_: f32, _: f32| Some(1000.0);
    let (report, vertices) = with_surfaces(
        "all-dropped",
        &list,
        Surfaces {
            ground: Some(&ground),
            water: None,
            ..Surfaces::default()
        },
    );
    assert_eq!(report.dropped_under_terrain, 1);
    assert_eq!(vertices, None);
}

#[test]
fn object_blocks_start_at_the_grid_anchor() {
    // an anchor of (-6, -5): the cell (0, 0) lies in the block that starts at (-2, -1)
    let list = vec![instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, false)];
    let out = std::env::temp_dir().join(format!("lodwright-anchor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    generate_objects_with_progress(
        "Mojave",
        &list,
        &assets(),
        &out,
        Surfaces {
            grid_anchor: (-6, -5),
            ..Surfaces::default()
        },
        &mut |_, _| {},
    )
    .unwrap();
    let mut found = Vec::new();
    for entry in walk(&out) {
        found.push(entry);
    }
    assert!(
        found.iter().any(|f| f.contains("level4.x-2.y-1")),
        "{found:?}"
    );
}

fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p.to_string_lossy().into_owned());
        }
    }
    out
}

#[test]
fn a_uv_outside_the_texture_is_held_at_its_edge_not_wrapped_to_the_far_edge() {
    // a vertex a hair outside the texture (-0.02, 1.02) must not jump to the other side of the tile, and a texture
    // meant to repeat (2.5, -3.0) is held at the edge, as the game's own blocks do
    let out = temp("uvclamp");
    let mut m = HashMap::new();
    m.insert(
        "meshes/a/wall_lod.nif".to_owned(),
        lod_model_full(
            "Data\\Textures\\a\\wall.dds",
            av(vec![]),
            av(vec![3]),
            vec![[-0.02, 1.02], [2.5, 0.5], [0.5, -3.0]],
        ),
    );
    m.insert("textures/a/wall.dds".to_owned(), texture(128, 200));
    let list = vec![instance("a\\Wall.nif", 4096.0 * 5.0, 4096.0, 0.0, 1, false)];
    generate_objects("Mojave", &list, &Memory(m), &out).unwrap();
    let block = read_block(
        &out,
        "meshes/landscape/lod/mojave/blocks/mojave.level4.x4.y0.nif",
    );
    let Body::TriShapeData(d) = &block.blocks[4].body else {
        panic!("data block");
    };
    let uv = &d.geometry.uv_sets[0];
    // the three corners of the one tile: the atlas holds only this texture, so the tile starts at (0, 0)
    let (u0, v1) = (uv[0][0], uv[0][1]);
    let (u_far, v_mid) = (uv[1][0], uv[1][1]);
    let (u_mid, v0) = (uv[2][0], uv[2][1]);
    assert!(u0 <= u_mid && u_mid <= u_far, "{uv:?}");
    assert!(v0 <= v_mid && v_mid <= v1, "{uv:?}");
    // held at the edges: -0.02 is the tile's left edge, 1.02 its bottom edge, 2.5 its right edge, -3.0 its top edge
    assert!((u_mid - f32::midpoint(u0, u_far)).abs() < 1e-4, "{uv:?}");
    assert!((v_mid - f32::midpoint(v0, v1)).abs() < 1e-4, "{uv:?}");
}

#[test]
fn a_model_that_is_missing_or_unreadable_is_named_in_a_warning() {
    let out = temp("warn-missing");
    let report = generate_objects(
        "Mojave",
        &[
            instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, false),
            instance("zzz.nif", 1.0, 1.0, 0.0, 2, false),
        ],
        &assets(),
        &out,
    )
    .unwrap();
    let said: Vec<&String> = report
        .warnings
        .iter()
        .filter(|w| w.contains("LOD models are missing"))
        .collect();
    assert_eq!(said.len(), 1, "{:?}", report.warnings);
    assert!(
        said[0].contains("zzz_lod.nif") && said[0].contains("left out"),
        "{said:?}"
    );
}

/// An uncompressed 32 bit DDS with a legacy header and the usual BGRA-free (RGBA mask) layout, as LOD resource mods ship.
fn uncompressed_texture(size: u32, shade: u8) -> Vec<u8> {
    let mut b = vec![0_u8; 128];
    b[0..4].copy_from_slice(b"DDS ");
    b[4..8].copy_from_slice(&124_u32.to_le_bytes());
    b[8..12].copy_from_slice(&0x0002_100F_u32.to_le_bytes());
    b[12..16].copy_from_slice(&size.to_le_bytes());
    b[16..20].copy_from_slice(&size.to_le_bytes());
    b[20..24].copy_from_slice(&(size * 4).to_le_bytes());
    b[28..32].copy_from_slice(&1_u32.to_le_bytes());
    b[76..80].copy_from_slice(&32_u32.to_le_bytes());
    b[80..84].copy_from_slice(&0x41_u32.to_le_bytes());
    b[88..92].copy_from_slice(&32_u32.to_le_bytes());
    b[92..96].copy_from_slice(&0x0000_00FF_u32.to_le_bytes());
    b[96..100].copy_from_slice(&0x0000_FF00_u32.to_le_bytes());
    b[100..104].copy_from_slice(&0x00FF_0000_u32.to_le_bytes());
    b[104..108].copy_from_slice(&0xFF00_0000_u32.to_le_bytes());
    b[108..112].copy_from_slice(&0x1000_u32.to_le_bytes());
    for _ in 0..size * size {
        b.extend_from_slice(&[shade, shade, shade, 255]);
    }
    b
}

#[test]
fn an_uncompressed_32_bit_texture_is_used_not_replaced_by_a_flat_colour() {
    let out = temp("uncompressed");
    let mut m = HashMap::new();
    m.insert(
        "meshes/a/wall_lod.nif".to_owned(),
        lod_model("Data\\Textures\\a\\wall.dds"),
    );
    m.insert(
        "textures/a/wall.dds".to_owned(),
        uncompressed_texture(64, 40),
    );
    let report = generate_objects(
        "Mojave",
        &[instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, false)],
        &Memory(m),
        &out,
    )
    .unwrap();
    assert!(
        !report.warnings.iter().any(|w| w.contains("texture")),
        "{:?}",
        report.warnings
    );
    let atlas =
        std::fs::read(out.join("textures/landscape/lod/mojave/blocks/mojave.buildings.dds"))
            .unwrap();
    let image = lodwright_dds::decode_image(&atlas).unwrap();
    let p = image.pixel(2, 2).unwrap();
    assert!(
        p[0].abs_diff(40) < 8,
        "the texture's shade is kept, got {p:?}"
    );
}

#[test]
fn a_texture_that_is_absent_and_one_that_cannot_be_decoded_get_different_warnings() {
    let out = temp("texture-faults");
    let mut m = HashMap::new();
    m.insert(
        "meshes/a/wall_lod.nif".to_owned(),
        lod_model("Data\\Textures\\a\\wall.dds"),
    );
    m.insert(
        "meshes/a/gone_lod.nif".to_owned(),
        lod_model("Data\\Textures\\a\\gone.dds"),
    );
    m.insert("textures/a/wall.dds".to_owned(), b"not a dds".to_vec());
    let report = generate_objects(
        "Mojave",
        &[
            instance("a\\Wall.nif", 100.0, 100.0, 0.0, 1, false),
            instance("a\\Gone.nif", 200.0, 200.0, 0.0, 2, false),
        ],
        &Memory(m),
        &out,
    )
    .unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("textures/a/gone.dds not found")),
        "{:?}",
        report.warnings
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("textures/a/wall.dds found but not readable")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn a_block_over_the_16_bit_vertex_limit_is_written_as_several_shapes_and_nothing_is_left_out() {
    // 25000 objects of three vertices each: 75000 vertices in one block
    let out = temp("many-shapes");
    let list: Vec<Instance> = (0..25_000_u32)
        .map(|k| {
            instance(
                "a\\Wall.nif",
                4096.0 * 4.0 + f32::from(u16::try_from(k % 1000).unwrap()),
                4096.0 * 4.0 + f32::from(u16::try_from(k / 1000).unwrap()),
                0.0,
                k + 1,
                false,
            )
        })
        .collect();
    let report = generate_objects("Mojave", &list, &assets(), &out).unwrap();
    assert_eq!((report.instances, report.skipped), (25_000, 0));
    assert!(
        !report.warnings.iter().any(|w| w.contains("left out")),
        "{:?}",
        report.warnings
    );
    assert!(
        report.warnings.iter().any(|w| w.contains("2 shapes")),
        "{:?}",
        report.warnings
    );
    let block = read_block(
        &out,
        "meshes/landscape/lod/mojave/blocks/mojave.level4.x4.y4.nif",
    );
    let (mut vertices, mut triangles, mut shapes) = (0, 0, 0);
    for b in &block.blocks {
        if let Body::TriShapeData(d) = &b.body {
            let n = d.geometry.vertices.as_ref().unwrap().len();
            assert!(n <= 65_535);
            vertices += n;
            triangles += d.triangles.as_ref().unwrap().len();
            shapes += 1;
        }
    }
    assert_eq!((vertices, triangles, shapes), (75_000, 25_000, 2));
}

#[test]
fn a_block_over_the_16_bit_triangle_limit_is_written_as_several_shapes_though_its_vertices_fit() {
    // 40 objects of three vertices and 3000 triangles each: 120 vertices, 120000 triangles in one block
    let out = temp("many-triangles");
    let mut m = HashMap::new();
    m.insert(
        "meshes/a/wall_lod.nif".to_owned(),
        lod_model_tris(
            "Data\\Textures\\a\\wall.dds",
            av(vec![]),
            av(vec![3]),
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            3000,
        ),
    );
    m.insert("textures/a/wall.dds".to_owned(), texture(128, 200));
    let list: Vec<Instance> = (0..40_u32)
        .map(|k| {
            instance(
                "a\\Wall.nif",
                4096.0 * 4.0 + f32::from(u16::try_from(k).unwrap()),
                4096.0 * 4.0,
                0.0,
                k + 1,
                false,
            )
        })
        .collect();
    let report = generate_objects("Mojave", &list, &Memory(m), &out).unwrap();
    assert_eq!((report.instances, report.skipped), (40, 0));
    assert!(
        !report.warnings.iter().any(|w| w.contains("left out")),
        "{:?}",
        report.warnings
    );
    let block = read_block(
        &out,
        "meshes/landscape/lod/mojave/blocks/mojave.level4.x4.y4.nif",
    );
    let (mut triangles, mut shapes) = (0, 0);
    for b in &block.blocks {
        if let Body::TriShapeData(d) = &b.body {
            let n = d.triangles.as_ref().unwrap().len();
            assert!(n <= 65_535);
            triangles += n;
            shapes += 1;
        }
    }
    assert_eq!((triangles, shapes), (120_000, 2));
}

#[test]
fn textures_that_do_not_fit_one_atlas_keep_their_size_on_a_second_atlas_with_its_own_shape() {
    // with atlases of at most 2048 by 2048, four textures of 1024 by 1024 fill the first; the fifth goes on a second
    let out = temp("atlas-pages");
    let mut m = HashMap::new();
    let mut list = Vec::new();
    for k in 0..5_u8 {
        m.insert(
            format!("meshes/a/wall{k}_lod.nif"),
            lod_model(&format!("Data\\Textures\\a\\wall{k}.dds")),
        );
        m.insert(
            format!("textures/a/wall{k}.dds"),
            texture(1024, 40 + 40 * k),
        );
        list.push(instance(
            &format!("a\\Wall{k}.nif"),
            100.0 + f32::from(k),
            100.0,
            0.0,
            u32::from(k) + 1,
            false,
        ));
    }
    let report = generate_objects_with_atlas_limit(
        "Mojave",
        &list,
        &Memory(m),
        &out,
        Surfaces::default(),
        (2048, 2048),
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(report.instances, 5);
    assert!(
        !report.warnings.iter().any(|w| w.contains("shape")),
        "{:?}",
        report.warnings
    );
    let dir = out.join("textures/landscape/lod/mojave/blocks");
    let first = read_header(&std::fs::read(dir.join("mojave.buildings.dds")).unwrap()).unwrap();
    let second = read_header(&std::fs::read(dir.join("mojave.buildings01.dds")).unwrap()).unwrap();
    assert!(dir.join("mojave.buildings01_n.dds").is_file());
    // native size: four 1024 textures fill the 2048 by 2048 atlas; the fifth gets the smallest atlas
    assert_eq!((first.width, first.height), (2048, 2048));
    assert_eq!((second.width, second.height), (2048, 1024));
    // one shape per atlas, each naming its own atlas
    let block =
        std::fs::read(out.join("meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y0.nif"))
            .unwrap();
    let text = String::from_utf8_lossy(&block);
    assert!(text.contains("Mojave.Buildings.dds") && text.contains("Mojave.Buildings01.dds"));
    let nif = NifFile::read(&block).unwrap();
    let shapes = nif
        .blocks
        .iter()
        .filter(|b| matches!(b.body, Body::TriShapeData(_)))
        .count();
    assert_eq!(shapes, 2);
}

#[test]
fn a_lod_shape_without_a_texture_takes_the_full_models_texture_and_says_so() {
    let out = temp("no-texture");
    let mut m = HashMap::new();
    // the LOD model's texture set names no diffuse; the full model names a dark texture
    m.insert("meshes/a/pole_lod.nif".to_owned(), lod_model(""));
    m.insert(
        "meshes/a/pole.nif".to_owned(),
        lod_model("textures\\a\\pole.dds"),
    );
    m.insert("textures/a/pole.dds".to_owned(), texture(64, 30));
    // a second model has no texture in either: it stays a flat colour, said differently
    m.insert("meshes/a/bare_lod.nif".to_owned(), lod_model(""));
    m.insert("meshes/a/bare.nif".to_owned(), lod_model(""));
    let list = vec![
        instance("a\\Pole.nif", 100.0, 100.0, 0.0, 1, false),
        instance("a\\Bare.nif", 200.0, 100.0, 0.0, 2, false),
    ];
    let report = generate_objects("Mojave", &list, &Memory(m), &out).unwrap();
    assert!(
        report.warnings.iter().any(|w| w
            == "model meshes/a/pole_lod.nif has a shape with no diffuse texture, the texture textures/a/pole.dds of meshes/a/pole.nif is used"),
        "{:?}",
        report.warnings
    );
    assert!(
        report.warnings.iter().any(|w| w
            == "model meshes/a/bare_lod.nif has a shape with no diffuse texture and its full model names none, a flat colour is used"),
        "{:?}",
        report.warnings
    );
    // the pole's corner samples the dark texture, not the flat grey
    let atlas = lodwright_dds::decode(
        &std::fs::read(out.join("textures/landscape/lod/mojave/blocks/mojave.buildings.dds"))
            .unwrap(),
    )
    .unwrap();
    let block = read_block(
        &out,
        "meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y0.nif",
    );
    let Body::TriShapeData(d) = &block.blocks[4].body else {
        panic!("data block");
    };
    let image = &atlas.levels[0];
    // the texel under each UV: the atlas is 2048 by 1024, so these are small exact values
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "UVs lie in 0 to 1 and the atlas is small"
    )]
    let texel = |u: f32, size: u32| ((u * size as f32) as u32).min(size - 1);
    let shades: Vec<u8> = d.geometry.uv_sets[0]
        .iter()
        .map(|uv| {
            image
                .pixel(texel(uv[0], image.width()), texel(uv[1], image.height()))
                .unwrap()[0]
        })
        .collect();
    assert!(shades.iter().any(|&s| s.abs_diff(30) <= 4), "{shades:?}");
    assert!(shades.iter().any(|&s| s.abs_diff(128) <= 4), "{shades:?}");
}
