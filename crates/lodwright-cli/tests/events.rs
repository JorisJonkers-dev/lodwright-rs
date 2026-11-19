// SPDX-License-Identifier: GPL-3.0-only
//! `generate --events`: one line per finished terrain, object and tree block, from either form of the command.

use lodwright_cli::run;
use lodwright_contract::{BlockKind, Event, Request};
use lodwright_core::trees::TreeBase;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
use lodwright_gen::billboard_path;
use lodwright_nif::{
    AvObject, Block, Body, ExportInfo, GeometryData, Header, NiNode, NiTriShape, NiTriStripsData,
    NifFile, ObjectNet,
};
use lodwright_testkit::land::Heights;
use lodwright_testkit::plugin::{Group, PluginBuilder, Record};
use lodwright_testkit::world::{SyntheticWorld, world_group};
use std::path::{Path, PathBuf};

const WORLD: u32 = 0xB00;
const CELL: f32 = 4096.0;
/// `VISIBLE_WHEN_DISTANT` on a base object, and the flag that gives a base tree LOD.
const VISIBLE_WHEN_DISTANT: u32 = 0x8000;
const HAS_TREE_LOD: u32 = 0x40;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-events-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn av() -> AvObject {
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
        properties: vec![],
        collision_object: -1,
    }
}

/// A one-triangle LOD model without textures: node, strips shape, strips data.
fn lod_model() -> Vec<u8> {
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
            block_types: vec![
                b"NiNode".to_vec(),
                b"NiTriStrips".to_vec(),
                b"NiTriStripsData".to_vec(),
            ],
            strings: vec![],
            groups: vec![],
        },
        blocks: vec![
            Block {
                type_index: 0,
                body: Body::Node(NiNode {
                    av: av(),
                    children: vec![1],
                    effects: vec![],
                }),
            },
            Block {
                type_index: 1,
                body: Body::TriStrips(NiTriShape {
                    av: av(),
                    data: 2,
                    skin_instance: -1,
                    material_names: vec![],
                    material_extra_data: vec![],
                    active_material: 0,
                    material_needs_update: 0,
                    shader_property: -1,
                    alpha_property: -1,
                }),
            },
            Block {
                type_index: 2,
                body: Body::TriStripsData(NiTriStripsData {
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
                }),
            },
        ],
        roots: vec![0],
    }
    .write()
    .unwrap()
}

fn billboard() -> Vec<u8> {
    let px: Vec<u8> = (0..16 * 16).flat_map(|_| [90, 200, 40, 255]).collect();
    encode(
        &Image::new(16, 16, px).unwrap(),
        &EncodeOptions {
            format: Format::Bc3,
            mips: false,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )
    .unwrap()
}

fn base(signature: [u8; 4], id: u32, model: &str, flags: u32) -> Record {
    let mut r =
        Record::new(signature, id).with_subrecord(*b"MODL", format!("{model}\0").into_bytes());
    r.flags = flags;
    r
}

/// A reference to `base` in the middle of cell (`cx`, `cy`).
fn refr(id: u32, base: u32, cx: i32, cy: i32) -> Record {
    let mut data = Vec::new();
    #[allow(clippy::cast_precision_loss)] // small cell numbers
    for v in [
        (cx as f32 + 0.5) * CELL,
        (cy as f32 + 0.5) * CELL,
        0.0,
        0.0,
        0.0,
        0.0,
    ] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    Record::new(*b"REFR", id)
        .with_subrecord(*b"NAME", base.to_le_bytes().to_vec())
        .with_subrecord(*b"DATA", data)
}

/// A data folder with one world of two land cells, a wall in two object blocks and an oak in two tree blocks.
fn data(name: &str) -> PathBuf {
    let dir = temp(name);
    let world = SyntheticWorld::new(WORLD, "Island")
        .cell(2, 2, Heights::flat(16.0))
        .cell(3, 2, Heights::flat(24.0));
    let refs = Group::new(1, WORLD.to_le_bytes())
        .with(refr(0x200, 0x100, 2, 2))
        .with(refr(0x201, 0x100, 9, 2))
        .with(refr(0x202, 0x101, 2, 2))
        .with(refr(0x203, 0x101, 12, 2));
    let plugin = PluginBuilder::new()
        .group(Group::new(0, *b"STAT").with(base(
            *b"STAT",
            0x100,
            "a\\wall.nif",
            VISIBLE_WHEN_DISTANT,
        )))
        .group(Group::new(0, *b"TREE").with(base(*b"TREE", 0x101, "oak.spt", HAS_TREE_LOD)))
        .group(world_group(&world, false).with(refs))
        .build();
    let files: [(&str, Vec<u8>); 3] = [
        ("Island.esm", plugin),
        ("meshes/a/wall_lod.nif", lod_model()),
        (
            &billboard_path(
                "Island.esm",
                &TreeBase {
                    form_id: 0x101,
                    signature: *b"TREE",
                    model: "oak.spt".into(),
                    bounds: None,
                    plugin: 0,
                },
            ),
            billboard(),
        ),
    ];
    for (path, bytes) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
    dir
}

fn go(args: &[&str]) -> u8 {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    run(&args, &mut out, &mut err)
}

/// The lines of the events file, read back, with their kind, level, done and total.
fn lines(events: &Path) -> Vec<(Option<BlockKind>, u32, u64, u64)> {
    std::fs::read_to_string(events)
        .unwrap()
        .lines()
        .map(|l| match serde_json::from_str::<Event>(l).unwrap() {
            Event::Block {
                kind,
                world,
                level,
                done,
                of,
                ..
            } => {
                assert_eq!(world, "Island");
                (kind, level, done, of)
            }
        })
        .collect()
}

#[test]
fn object_and_tree_blocks_each_get_a_line_after_the_terrain_blocks_of_their_world() {
    let d = data("kinds");
    let out = temp("kinds-out");
    let events = temp("kinds.ndjson");
    let code = go(&[
        "generate",
        "--data",
        d.to_str().unwrap(),
        "--plugin",
        "Island.esm",
        "--out",
        out.to_str().unwrap(),
        "--threads",
        "1",
        "--events",
        events.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    let terrain = Some(BlockKind::Terrain);
    let objects = Some(BlockKind::Objects);
    let trees = Some(BlockKind::Trees);
    assert_eq!(
        lines(&events),
        [
            (terrain, 4, 1, 1),
            (terrain, 8, 1, 1),
            (terrain, 16, 1, 1),
            (terrain, 32, 1, 1),
            (objects, 4, 1, 2),
            (objects, 4, 2, 2),
            (trees, 8, 1, 2),
            (trees, 8, 2, 2),
        ]
    );
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_request_form_writes_the_events_file_too() {
    let d = data("request");
    let out = temp("request-out");
    let events = temp("request.ndjson");
    let mut request = Request::new(d.to_str().unwrap(), &["Island.esm"], out.to_str().unwrap());
    request.objects.enabled = false;
    request.trees.enabled = false;
    let file = temp("request.json");
    std::fs::write(&file, serde_json::to_string(&request).unwrap()).unwrap();
    let code = go(&[
        "generate",
        "--request",
        file.to_str().unwrap(),
        "--events",
        events.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    let kinds: Vec<_> = lines(&events).into_iter().map(|l| l.0).collect();
    assert_eq!(kinds, [Some(BlockKind::Terrain); 4]);
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&d);
}
