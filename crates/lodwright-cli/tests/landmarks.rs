// SPDX-License-Identifier: GPL-3.0-only
//! `landmarks`: the presence report of named landmarks in a generated output, end to end on a synthetic data folder.

use lodwright_cli::run;
use lodwright_contract::Request;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
use lodwright_nif::{
    AvObject, Block, Body, ExportInfo, GeometryData, Header, NiNode, NiTriShape, NiTriStripsData,
    NifFile, ObjectNet,
};
use lodwright_testkit::land::Heights;
use lodwright_testkit::plugin::{Group, PluginBuilder, Record};
use lodwright_testkit::world::{SyntheticWorld, record_group, world_group};
use lodwright_testkit::{covers, ids};
use std::path::PathBuf;

const VISIBLE_WHEN_DISTANT: u32 = 0x8000;
const INITIALLY_DISABLED: u32 = 0x0800;

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

fn temp(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("lodwright-landmarks-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn stat(id: u32, model: &str) -> Record {
    let mut r =
        Record::new(*b"STAT", id).with_subrecord(*b"MODL", format!("{model}\0").into_bytes());
    r.flags = VISIBLE_WHEN_DISTANT;
    r
}

fn refr(id: u32, base: u32, x: f32, flags: u32) -> Record {
    let mut data = Vec::new();
    for v in [x, 100.0, 1000.0, 0.0, 0.0, 0.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    let mut r = Record::new(*b"REFR", id)
        .with_subrecord(*b"NAME", base.to_le_bytes().to_vec())
        .with_subrecord(*b"DATA", data);
    r.flags = flags;
    r
}

/// A data folder with a world, one far model and its texture, and references: two that stand, one initially
/// disabled and one whose base has no far model. Returns the request file and the output folder, generated.
fn generated(name: &str) -> (PathBuf, PathBuf) {
    let data = temp(&format!("{name}-data"));
    std::fs::create_dir_all(data.join("meshes/arch")).unwrap();
    std::fs::create_dir_all(data.join("textures/arch")).unwrap();
    let model = lod_model_tris(
        "Data\\Textures\\arch\\wall.dds",
        av(vec![]),
        av(vec![3]),
        vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        1,
    );
    std::fs::write(data.join("meshes/arch/wall_lod.nif"), model).unwrap();
    std::fs::write(data.join("meshes/arch/nolod.nif"), b"not a model").unwrap();
    std::fs::write(data.join("textures/arch/wall.dds"), texture(64, 150)).unwrap();
    let world = SyntheticWorld::new(0xB00, "Island").cell(0, 0, Heights::flat(16.0));
    let refs = Group::new(1, 0xB00_u32.to_le_bytes())
        .with(refr(0xC00, 0x100, 100.0, 0))
        .with(refr(0xC01, 0x100, 500.0, 0))
        .with(refr(0xC02, 0x100, 900.0, INITIALLY_DISABLED))
        .with(refr(0xC03, 0x101, 1300.0, 0));
    let plugin = PluginBuilder::new()
        .group(record_group(
            *b"STAT",
            vec![
                stat(0x100, "arch\\wall.nif"),
                stat(0x101, "arch\\nolod.nif"),
            ],
        ))
        .group(world_group(&world, false))
        .group(refs)
        .build();
    std::fs::write(data.join("Base.esm"), plugin).unwrap();
    let out = temp(&format!("{name}-out"));
    let mut request = Request::new(data.to_str().unwrap(), &["Base.esm"], out.to_str().unwrap());
    request.terrain.enabled = false;
    request.trees.enabled = false;
    request.objects.enabled = true;
    request.limits.threads = Some(1);
    let file = data.with_extension("request.json");
    std::fs::write(&file, serde_json::to_string(&request).unwrap()).unwrap();
    let (code, text) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!(code, 0);
    let result: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    let steps: Vec<&str> = result["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["step"].as_str().unwrap())
        .collect();
    // every phase of the object step is profiled, in the order it runs
    assert_eq!(
        steps,
        [
            "read_plugins",
            "setup",
            "assets_index",
            "objects.collect",
            "objects.models",
            "objects.textures",
            "objects.atlas",
            "objects.blocks",
            "manifest"
        ]
    );
    (file, out)
}

fn go(args: &[&str]) -> (u8, String) {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &mut out, &mut err);
    let mut text = String::from_utf8(out).unwrap();
    text.push_str(&String::from_utf8(err).unwrap());
    (code, text)
}

const LIST: &str = include_str!("fixtures/landmarks-synthetic.tsv");

fn statuses(report: &serde_json::Value) -> Vec<(String, String)> {
    report["landmarks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| {
            (
                l["name"].as_str().unwrap().to_owned(),
                l["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn each_landmark_is_written_missing_excluded_without_a_far_model_or_unresolved() {
    covers(&[ids::verify::measure::LANDMARKS]);
    let (request, out) = generated("report");
    let list = request.with_extension("landmarks.tsv");
    std::fs::write(&list, LIST).unwrap();
    let (code, text) = go(&[
        "landmarks",
        "--request",
        request.to_str().unwrap(),
        "--landmarks",
        list.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code, 0, "{text}");
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    let want = |n: &str, s: &str| (n.to_owned(), s.to_owned());
    assert_eq!(
        statuses(&report),
        [
            want("Wall West", "written"),
            want("Wall East", "written"),
            want("Hidden Wall", "excluded"),
            want("Plain Wall", "written"),
            want("Empty Spot", "missing"),
            want("No Far Model", "no_far_model"),
            want("Gone", "unresolved"),
        ]
    );
    let west = &report["landmarks"][0];
    assert_eq!(west["triangles"], 1);
    assert_eq!(west["reference"], "00000C00");
    let found = &west["found_in"][0];
    assert!(
        found["block"]
            .as_str()
            .unwrap()
            .ends_with("mojave.level4.x0.y0.nif")
            || found["block"]
                .as_str()
                .unwrap()
                .ends_with("island.level4.x0.y0.nif")
    );
    assert!(
        found["texture"]
            .as_str()
            .unwrap()
            .to_ascii_lowercase()
            .contains("buildings")
    );
    assert_eq!(found["segment_ok"], true);
    assert_eq!(report["landmarks"][2]["reason"], "initially disabled");
    assert_eq!(report["summary"]["by_status"]["written"], 3);
    let _ = out;
}

#[test]
fn a_baseline_passes_when_nothing_is_lost_and_fails_when_a_written_landmark_is_gone() {
    covers(&[ids::verify::measure::LANDMARK_BASELINE]);
    let (request, _out) = generated("baseline");
    let list = request.with_extension("landmarks.tsv");
    std::fs::write(&list, LIST).unwrap();
    let baseline = request.with_extension("baseline.json");
    let args = |extra: &[&'static str]| {
        let mut a = vec![
            "landmarks".to_owned(),
            "--request".to_owned(),
            request.to_str().unwrap().to_owned(),
            "--landmarks".to_owned(),
            list.to_str().unwrap().to_owned(),
        ];
        a.extend(extra.iter().map(|s| (*s).to_owned()));
        a
    };
    let go_owned = |a: Vec<String>| go(&a.iter().map(String::as_str).collect::<Vec<_>>());
    let b = baseline.to_str().unwrap().to_owned();
    let mut saved = args(&["--write-baseline"]);
    saved.push(b.clone());
    assert_eq!(go_owned(saved).0, 0);
    // the same output against its own baseline
    let mut same = args(&["--baseline"]);
    same.push(b.clone());
    let (code, text) = go_owned(same.clone());
    assert_eq!(code, 0, "{text}");
    assert!(text.ends_with("no landmark changed\n"), "{text}");
    // the baseline says a landmark was written where nothing is now
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&baseline).unwrap()).unwrap();
    doc["landmarks"][4]["status"] = "written".into();
    doc["landmarks"][4]["triangles"] = 1.into();
    std::fs::write(&baseline, serde_json::to_string(&doc).unwrap()).unwrap();
    let (code, text) = go_owned(same);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("LOST: Empty Spot"), "{text}");
}

#[test]
fn a_bad_landmark_file_or_a_missing_option_is_a_usage_error() {
    let (request, _out) = generated("usage");
    let list = request.with_extension("landmarks.tsv");
    std::fs::write(&list, "only a name\n").unwrap();
    let (code, text) = go(&[
        "landmarks",
        "--request",
        request.to_str().unwrap(),
        "--landmarks",
        list.to_str().unwrap(),
    ]);
    assert_eq!(code, 2, "{text}");
    assert!(text.contains("line 1: needs a name"), "{text}");
    assert_eq!(go(&["landmarks", "--request", "x"]).0, 2);
}
