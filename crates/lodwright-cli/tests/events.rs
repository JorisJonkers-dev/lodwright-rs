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
use lodwright_testkit::{covers, ids};
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
    covers(&[ids::contract::events::BLOCK_LINES]);
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
fn a_recorded_run_with_terrain_objects_and_trees_has_a_run_number_that_never_goes_back() {
    use lodwright_api::{CancelToken, GenerateOutcome, GenerateRequest, ProgressEvent, Step};
    covers(&[ids::contract::events::RUN_WIDE_PROGRESS]);
    let d = data("run");
    let out = temp("run-out");
    let mut request = Request::new(d.to_str().unwrap(), &["Island.esm"], out.to_str().unwrap());
    request.limits.threads = Some(1);
    let mut events: Vec<ProgressEvent> = Vec::new();
    let mut record = |e: &ProgressEvent| events.push(e.clone());
    let outcome = lodwright_api::generate(
        &GenerateRequest::new(request),
        &mut record,
        &CancelToken::new(),
    )
    .unwrap();
    assert!(matches!(outcome, GenerateOutcome::Done { .. }));
    let steps: Vec<Step> = {
        let mut s: Vec<Step> = events.iter().map(|e| e.step).collect();
        s.dedup();
        s
    };
    assert_eq!(
        steps,
        [
            Step::ReadPlugins,
            Step::Terrain,
            Step::Objects,
            Step::Trees,
            Step::Manifest
        ]
    );
    let done: Vec<u64> = events.iter().map(|e| e.run_done.unwrap()).collect();
    assert!(done.windows(2).all(|w| w[0] <= w[1]), "{done:?}");
    assert!(events.iter().all(|e| e.run_total == Some(10_000)));
    // every phase has a stable total and its own index: read, terrain, objects, trees, manifest
    for (step, index) in steps.iter().zip(1_u32..) {
        let of_step: Vec<&ProgressEvent> = events.iter().filter(|e| e.step == *step).collect();
        assert!(of_step.iter().all(|e| e.phase_index == Some(index)));
        let totals: Vec<_> = of_step.iter().map(|e| e.phase_total).collect();
        assert!(totals.windows(2).all(|w| w[0] == w[1]));
    }
    assert!(events[1..].iter().all(|e| e.phase_count == Some(5)));
    // the last block of each phase fills the phase: the next phase starts where it ended
    for step in [Step::Terrain, Step::Objects, Step::Trees] {
        let last = events
            .iter()
            .rfind(|e| e.step == step && e.done.is_some())
            .unwrap();
        if step != Step::Terrain {
            assert_eq!(last.phase_done, last.phase_total);
        }
    }
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_events_file_lines_carry_the_same_run_numbers_as_the_callback() {
    covers(&[ids::contract::events::RUN_WIDE_PROGRESS]);
    let d = data("filerun");
    let out = temp("filerun-out");
    let events = temp("filerun.ndjson");
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
    let mut last = 0;
    let mut phases = Vec::new();
    for l in std::fs::read_to_string(&events).unwrap().lines() {
        let Event::Block {
            phase,
            phase_index,
            phase_count,
            phase_done,
            phase_total,
            run_done,
            run_total,
            ..
        } = serde_json::from_str::<Event>(l).unwrap();
        assert_eq!((phase_count, run_total), (Some(5), Some(10_000)));
        assert!(phase_done <= phase_total);
        let run = run_done.unwrap();
        assert!(run >= last);
        last = run;
        phases.push((phase.unwrap(), phase_index.unwrap()));
    }
    phases.dedup();
    assert_eq!(
        phases,
        [
            ("terrain".to_owned(), 2),
            ("objects".to_owned(), 3),
            ("trees".to_owned(), 4)
        ]
    );
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_request_form_with_the_default_workers_writes_the_same_object_and_tree_lines() {
    covers(&[ids::cli::commands::GENERATE_COMMAND]);
    let d = data("request");
    let out = temp("request-out");
    let events = temp("request.ndjson");
    // no thread limit: terrain blocks may finish in any order, object and tree lines may not
    let request = Request::new(d.to_str().unwrap(), &["Island.esm"], out.to_str().unwrap());
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
    let all = lines(&events);
    let (terrain, rest) = all.split_at(4);
    let mut levels: Vec<u32> = terrain
        .iter()
        .map(|l| {
            assert_eq!((l.0, l.2, l.3), (Some(BlockKind::Terrain), 1, 1));
            l.1
        })
        .collect();
    levels.sort_unstable();
    assert_eq!(levels, [4, 8, 16, 32]);
    let objects = Some(BlockKind::Objects);
    let trees = Some(BlockKind::Trees);
    assert_eq!(
        rest,
        [
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
fn a_request_that_drops_faces_under_the_ground_counts_them_in_the_result() {
    covers(&[ids::objects::blocks::FACES_UNDER_GROUND_AND_WATER_DROPPED]);
    // the wall in cell (2, 2) stands at height 0 on ground at 16, the one in cell (9, 2) has no land under it
    let d = data("drop");
    let out = temp("drop-out");
    let mut request = Request::new(d.to_str().unwrap(), &["Island.esm"], out.to_str().unwrap());
    request.objects.drop_under_terrain = true;
    let file = temp("drop.json");
    std::fs::write(&file, serde_json::to_string(&request).unwrap()).unwrap();
    let args: Vec<String> = ["generate", "--request", file.to_str().unwrap(), "--json"]
        .iter()
        .map(ToString::to_string)
        .collect();
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    assert_eq!(run(&args, &mut stdout, &mut stderr), 0);
    let text = String::from_utf8(stdout).unwrap();
    let doc: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(doc["dropped_faces"]["under_terrain"], 1, "{doc}");
    assert_eq!(doc["dropped_faces"]["under_water"], 0, "{doc}");
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&d);
}
