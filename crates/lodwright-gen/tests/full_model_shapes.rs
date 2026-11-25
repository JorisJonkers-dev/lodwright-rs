// SPDX-License-Identifier: GPL-3.0-only
//! Far shapes from full models for objects without a LOD model (`objects.shapes_from_full_models`), on synthetic models.

use lodwright_testkit::{covers, ids};
use std::collections::HashMap;

use lodwright_core::objects::Instance;
use lodwright_core::plugin::Placement;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, encode};
use lodwright_gen::{
    Assets, FarLimits, ObjectReport, RuleShapes, Surfaces, generate_objects_with_progress,
};
use lodwright_nif::{
    AvObject, Block, Body, ExportInfo, GeometryData, Header, NiNode, NiTriShape, NiTriShapeData,
    NifFile, ObjectNet,
};

const BLOCK: &str = "meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y0.nif";
const LIMITS: FarLimits = FarLimits {
    keep: 0.25,
    max_error: 0.05,
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
        flags: 0,
        translation: [0.0; 3],
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        properties,
        collision_object: -1,
    }
}

/// A model with one flat grid of `size` squares, textured; `alpha` makes the shape see-through.
fn grid_model(size: u16, alpha: bool) -> Vec<u8> {
    grid_model_with_first_height(size, alpha, 0.0)
}

/// The same model with the height of its first vertex given.
fn grid_model_with_first_height(size: u16, alpha: bool, first_height: f32) -> Vec<u8> {
    grid_model_full(size, alpha, first_height, false)
}

/// The same model; `unlit` also gives the shape a `BSShaderNoLightingProperty`, as light beams and glows have.
#[allow(clippy::too_many_lines, reason = "one literal NIF")]
fn grid_model_full(size: u16, alpha: bool, first_height: f32, unlit: bool) -> Vec<u8> {
    let row = size + 1;
    let mut vertices: Vec<[f32; 3]> = (0..row)
        .flat_map(|j| (0..row).map(move |i| [f32::from(i) * 4.0, f32::from(j) * 4.0, 0.0]))
        .collect();
    vertices[0][2] = first_height;
    let uv: Vec<[f32; 2]> = (0..row)
        .flat_map(|j| {
            (0..row).map(move |i| {
                [
                    f32::from(i) / f32::from(size),
                    f32::from(j) / f32::from(size),
                ]
            })
        })
        .collect();
    let triangles: Vec<[u16; 3]> = (0..size)
        .flat_map(|j| {
            (0..size).flat_map(move |i| {
                let a = j * row + i;
                [[a, a + 1, a + row + 1], [a, a + row + 1, a + row]]
            })
        })
        .collect();
    let node = NiNode {
        av: av(vec![]),
        children: vec![1],
        effects: vec![],
    };
    let shape = NiTriShape {
        av: av(match (alpha, unlit) {
            (false, false) => vec![3],
            (true, false) => vec![3, 5],
            (false, true) => vec![3, 6],
            (true, true) => vec![3, 5, 6],
        }),
        data: 2,
        skin_instance: -1,
        material_names: vec![],
        material_extra_data: vec![],
        active_material: 0,
        material_needs_update: 0,
        shader_property: -1,
        alpha_property: -1,
    };
    let data = NiTriShapeData {
        geometry: GeometryData {
            group_id: 0,
            num_vertices: u16::try_from(vertices.len()).unwrap(),
            keep_flags: 0,
            compress_flags: 0,
            vertices: Some(vertices),
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
        num_triangles: u16::try_from(triangles.len()).unwrap(),
        num_triangle_points: u32::try_from(triangles.len() * 3).unwrap(),
        triangles: Some(triangles),
        match_groups: vec![],
    };
    let mut lighting = Vec::new();
    for v in [u32::MAX, 0, u32::MAX] {
        lighting.extend_from_slice(&v.to_le_bytes());
    }
    lighting.extend_from_slice(&[0; 42]);
    lighting[34..38].copy_from_slice(&4_u32.to_le_bytes());
    let mut set = 6_u32.to_le_bytes().to_vec();
    for p in ["textures\\a\\grid.dds", ""] {
        set.extend_from_slice(&u32::try_from(p.len()).unwrap().to_le_bytes());
        set.extend_from_slice(p.as_bytes());
    }
    set.extend_from_slice(&[0; 16]);
    let types: Vec<Vec<u8>> = [
        &b"NiNode"[..],
        b"NiTriShape",
        b"NiTriShapeData",
        b"BSShaderPPLightingProperty",
        b"BSShaderTextureSet",
        b"NiAlphaProperty",
        b"BSShaderNoLightingProperty",
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
                body: Body::TriShape(shape),
            },
            Block {
                type_index: 2,
                body: Body::TriShapeData(data),
            },
            Block {
                type_index: 3,
                body: Body::Opaque(lighting),
            },
            Block {
                type_index: 4,
                body: Body::Opaque(set),
            },
            Block {
                type_index: 5,
                body: Body::Opaque(alpha_property()),
            },
            Block {
                type_index: 6,
                body: Body::Opaque(vec![0; 16]),
            },
        ],
        roots: vec![0],
    }
    .write()
    .unwrap()
}

/// `NiAlphaProperty`: no name, no extra data, no controller, then its flags and threshold.
fn alpha_property() -> Vec<u8> {
    let mut bytes = Vec::new();
    for v in [u32::MAX, 0, u32::MAX] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&[0xED, 0x00, 0x80]);
    bytes
}

fn texture() -> Vec<u8> {
    let px: Vec<u8> = (0..64 * 64).flat_map(|_| [120, 120, 120, 255]).collect();
    encode(
        &Image::new(64, 64, px).unwrap(),
        &EncodeOptions {
            format: Format::Bc1,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )
    .unwrap()
}

fn instance(model: &str, x: f32, reference: u32, part: bool) -> Instance {
    Instance {
        base: 1,
        signature: *b"STAT",
        model: model.to_owned(),
        placement: Placement {
            pos: [x, 100.0, 10.0],
            rot: [0.0; 3],
            scale: 1.0,
        },
        high_priority: false,
        has_tree_lod: false,
        reference,
        reference_plugin: 0,
        base_plugin: 0,
        flagged_by: 0,
        part,
        cell: None,
    }
}

fn assets(models: &[(&str, Vec<u8>)]) -> Memory {
    let mut m: HashMap<String, Vec<u8>> = models
        .iter()
        .map(|(path, bytes)| ((*path).to_owned(), bytes.clone()))
        .collect();
    m.insert("textures/a/grid.dds".to_owned(), texture());
    Memory(m)
}

fn run(
    name: &str,
    list: &[Instance],
    assets: &Memory,
    full_models: Option<FarLimits>,
    threads: usize,
) -> (ObjectReport, std::path::PathBuf) {
    run_with_rules(name, list, assets, full_models, None, threads)
}

fn run_with_rules(
    name: &str,
    list: &[Instance],
    assets: &Memory,
    full_models: Option<FarLimits>,
    rule_shapes: Option<RuleShapes<'_>>,
    threads: usize,
) -> (ObjectReport, std::path::PathBuf) {
    let out = std::env::temp_dir().join(format!(
        "lodwright-full-shapes-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&out);
    let surfaces = Surfaces {
        full_models,
        rule_shapes,
        threads,
        ..Surfaces::default()
    };
    let report =
        generate_objects_with_progress("Mojave", list, assets, &out, surfaces, &mut |_, _| {})
            .unwrap();
    (report, out)
}

fn block_triangles(out: &std::path::Path) -> usize {
    let file = NifFile::read(&std::fs::read(out.join(BLOCK)).unwrap()).unwrap();
    file.blocks
        .iter()
        .map(|b| match &b.body {
            Body::TriShapeData(d) => usize::from(d.num_triangles),
            _ => 0,
        })
        .sum()
}

fn kinds(report: &ObjectReport) -> Vec<&str> {
    report.records.iter().map(|r| r.kind.as_str()).collect()
}

#[test]
fn with_the_switch_off_a_model_without_a_lod_model_stays_out() {
    let store = assets(&[("meshes/a/grid.nif", grid_model(16, false))]);
    let (report, out) = run(
        "off",
        &[instance("a\\Grid.nif", 100.0, 1, false)],
        &store,
        None,
        1,
    );
    assert!(!out.join(BLOCK).exists());
    assert_eq!(kinds(&report), ["far_model_without_lod_model"]);
}

#[test]
fn with_the_switch_on_it_is_drawn_with_fewer_triangles_and_reported() {
    covers(&[ids::objects::far_shapes::FULL_MODEL_SHAPES]);
    let store = assets(&[("meshes/a/grid.nif", grid_model(16, false))]);
    let (report, out) = run(
        "on",
        &[instance("a\\Grid.nif", 100.0, 1, false)],
        &store,
        Some(LIMITS),
        1,
    );
    let triangles = block_triangles(&out);
    assert!(
        triangles > 0 && triangles <= 512 / 4,
        "{triangles} triangles"
    );
    assert_eq!(kinds(&report), ["far_model_simplified"]);
    assert_eq!(report.records[0].subjects, ["meshes/a/grid.nif"]);
}

#[test]
fn an_authored_lod_model_is_used_instead_of_the_full_model() {
    let store = assets(&[
        ("meshes/a/grid.nif", grid_model(16, false)),
        ("meshes/a/grid_lod.nif", grid_model(1, false)),
    ]);
    let (report, out) = run(
        "authored",
        &[instance("a\\Grid.nif", 100.0, 1, false)],
        &store,
        Some(LIMITS),
        1,
    );
    assert_eq!(block_triangles(&out), 2);
    assert!(kinds(&report).is_empty(), "{:?}", kinds(&report));
}

#[test]
fn a_model_whose_shapes_all_have_an_alpha_property_stays_out_and_is_reported() {
    let store = assets(&[("meshes/a/glass.nif", grid_model(8, true))]);
    let (report, out) = run(
        "see-through",
        &[instance("a\\Glass.nif", 100.0, 1, false)],
        &store,
        Some(LIMITS),
        1,
    );
    assert!(!out.join(BLOCK).exists());
    assert_eq!(kinds(&report), ["far_model_no_solid_shape"]);
}

#[test]
fn a_solid_model_that_cannot_be_simplified_is_not_called_see_through() {
    let store = assets(&[(
        "meshes/a/broken.nif",
        grid_model_with_first_height(8, false, f32::NAN),
    )]);
    let (report, out) = run(
        "cannot-simplify",
        &[instance("a\\Broken.nif", 100.0, 1, false)],
        &store,
        Some(LIMITS),
        1,
    );
    assert!(!out.join(BLOCK).exists());
    assert_eq!(kinds(&report), ["far_model_without_lod_model"]);
}

#[test]
fn a_static_collection_with_parts_is_not_also_drawn_as_its_own_model() {
    covers(&[ids::objects::selection::STATIC_COLLECTIONS]);
    let store = assets(&[
        ("meshes/a/collection.nif", grid_model(16, false)),
        ("meshes/a/piece.nif", grid_model(8, false)),
    ]);
    let list = [
        instance("a\\Collection.nif", 100.0, 1, false),
        instance("a\\Piece.nif", 100.0, 1, true),
    ];
    let (report, out) = run("collection", &list, &store, Some(LIMITS), 1);
    let simplified = report
        .records
        .iter()
        .find(|r| r.kind == "far_model_simplified")
        .unwrap();
    assert_eq!(simplified.subjects, ["meshes/a/piece.nif"]);
    let (_, alone) = run(
        "collection-piece-alone",
        &[instance("a\\Piece.nif", 100.0, 1, false)],
        &store,
        Some(LIMITS),
        1,
    );
    assert_eq!(block_triangles(&out), block_triangles(&alone));
}

#[test]
fn the_bytes_do_not_depend_on_the_thread_count() {
    let store = assets(&[
        ("meshes/a/one.nif", grid_model(16, false)),
        ("meshes/a/two.nif", grid_model(12, false)),
    ]);
    let list = [
        instance("a\\One.nif", 100.0, 1, false),
        instance("a\\Two.nif", 300.0, 2, false),
    ];
    let (_, one) = run("threads-1", &list, &store, Some(LIMITS), 1);
    let (_, four) = run("threads-4", &list, &store, Some(LIMITS), 4);
    assert_eq!(
        std::fs::read(one.join(BLOCK)).unwrap(),
        std::fs::read(four.join(BLOCK)).unwrap()
    );
}

/// The limits a rule gives the objects of reference 1 only, as an include rule's `generate` does for what it names.
fn first_reference_only(i: &Instance) -> Option<FarLimits> {
    (i.reference == 1).then_some(LIMITS)
}

fn record<'a>(
    report: &'a ObjectReport,
    kind: &str,
) -> Option<&'a lodwright_contract::WarningRecord> {
    report.records.iter().find(|r| r.kind == kind)
}

#[test]
fn a_rule_gives_a_shape_to_what_it_names_and_to_no_other_model() {
    covers(&[ids::objects::far_shapes::FULL_MODEL_RULES]);
    let store = assets(&[
        ("meshes/a/lamp.nif", grid_model(16, false)),
        ("meshes/a/house.nif", grid_model(12, false)),
    ]);
    let list = [
        instance("a\\Lamp.nif", 100.0, 1, false),
        instance("a\\House.nif", 300.0, 2, false),
    ];
    let (report, out) = run_with_rules("rule", &list, &store, None, Some(&first_reference_only), 1);
    let (_, lamp_alone) = run(
        "rule-lamp-alone",
        &[instance("a\\Lamp.nif", 100.0, 1, false)],
        &store,
        Some(LIMITS),
        1,
    );
    // the block holds the lamp's shape and nothing of the house
    assert_eq!(
        std::fs::read(out.join(BLOCK)).unwrap(),
        std::fs::read(lamp_alone.join(BLOCK)).unwrap()
    );
    assert_eq!(
        record(&report, "far_model_simplified").unwrap().subjects,
        ["meshes/a/lamp.nif"]
    );
    let missing = record(&report, "far_model_without_lod_model").unwrap();
    assert_eq!(missing.subjects, ["meshes/a/house.nif"]);
    assert_eq!(missing.count, 1);
}

#[test]
fn another_object_of_the_same_model_that_no_rule_names_stays_out() {
    covers(&[ids::objects::far_shapes::FULL_MODEL_RULES]);
    let store = assets(&[("meshes/a/lamp.nif", grid_model(16, false))]);
    let list = [
        instance("a\\Lamp.nif", 100.0, 1, false),
        instance("a\\Lamp.nif", 300.0, 2, false),
    ];
    let (report, out) = run_with_rules(
        "same-model",
        &list,
        &store,
        None,
        Some(&first_reference_only),
        1,
    );
    let (_, one) = run(
        "same-model-one",
        &[instance("a\\Lamp.nif", 100.0, 1, false)],
        &store,
        Some(LIMITS),
        1,
    );
    assert_eq!(block_triangles(&out), block_triangles(&one));
    let missing = record(&report, "far_model_without_lod_model").unwrap();
    assert_eq!(missing.subjects, ["meshes/a/lamp.nif"]);
    assert_eq!(missing.count, 1);
    // with the switch for every object on as well, both are drawn
    let (_, both) = run_with_rules(
        "same-model-both",
        &list,
        &store,
        Some(LIMITS),
        Some(&first_reference_only),
        1,
    );
    assert_eq!(block_triangles(&both), 2 * block_triangles(&one));
}

#[test]
fn a_rule_that_names_nothing_changes_no_byte() {
    let store = assets(&[
        ("meshes/a/house.nif", grid_model(12, false)),
        ("meshes/a/tower.nif", grid_model(16, false)),
        ("meshes/a/tower_lod.nif", grid_model(2, false)),
    ]);
    let list = [
        instance("a\\House.nif", 100.0, 7, false),
        instance("a\\Tower.nif", 300.0, 8, false),
    ];
    let (plain, a) = run("no-rule", &list, &store, None, 1);
    let (ruled, b) = run_with_rules(
        "rule-names-nothing",
        &list,
        &store,
        None,
        Some(&first_reference_only),
        1,
    );
    assert_eq!(
        std::fs::read(a.join(BLOCK)).unwrap(),
        std::fs::read(b.join(BLOCK)).unwrap()
    );
    assert_eq!(kinds(&plain), kinds(&ruled));
}

#[test]
fn a_rules_limits_win_over_the_switch_for_every_object() {
    covers(&[ids::objects::far_shapes::FULL_MODEL_RULES]);
    let store = assets(&[("meshes/a/lamp.nif", grid_model(16, false))]);
    let list = [instance("a\\Lamp.nif", 100.0, 1, false)];
    let fine = |_: &Instance| {
        Some(FarLimits {
            keep: 1.0,
            max_error: 0.0,
        })
    };
    let (_, coarse) = run("limits-switch", &list, &store, Some(LIMITS), 1);
    let (_, ruled) = run_with_rules("limits-rule", &list, &store, Some(LIMITS), Some(&fine), 1);
    assert!(block_triangles(&ruled) > block_triangles(&coarse));
}

#[test]
fn a_rule_does_not_call_an_unnamed_see_through_object_see_through() {
    let store = assets(&[("meshes/a/glass.nif", grid_model(8, true))]);
    let list = [
        instance("a\\Glass.nif", 100.0, 1, false),
        instance("a\\Glass.nif", 300.0, 2, false),
    ];
    let (report, out) =
        run_with_rules("glass", &list, &store, None, Some(&first_reference_only), 1);
    assert!(!out.join(BLOCK).exists());
    assert_eq!(
        record(&report, "far_model_no_solid_shape").unwrap().count,
        1
    );
    assert_eq!(
        record(&report, "far_model_without_lod_model")
            .unwrap()
            .count,
        1
    );
}

#[test]
fn a_model_drawn_without_lighting_gets_no_far_shape() {
    covers(&[ids::objects::far_shapes::SEE_THROUGH_MODELS_STAY_OUT]);
    // a light beam: one shape without an alpha property, drawn without lighting
    let store = assets(&[("meshes/a/beam.nif", grid_model_full(8, false, 0.0, true))]);
    let (report, out) = run(
        "unlit",
        &[instance("a\\Beam.nif", 100.0, 1, false)],
        &store,
        Some(LIMITS),
        1,
    );
    assert!(!out.join(BLOCK).exists());
    assert_eq!(kinds(&report), ["far_model_no_solid_shape"]);
    // the same through a rule's limits
    let (report, out) = run_with_rules(
        "unlit-rule",
        &[instance("a\\Beam.nif", 100.0, 1, false)],
        &store,
        None,
        Some(&first_reference_only),
        1,
    );
    assert!(!out.join(BLOCK).exists());
    assert_eq!(kinds(&report), ["far_model_no_solid_shape"]);
}

#[test]
fn a_rules_shapes_do_not_depend_on_the_thread_count() {
    let store = assets(&[
        ("meshes/a/one.nif", grid_model(16, false)),
        ("meshes/a/two.nif", grid_model(12, false)),
    ]);
    let list = [
        instance("a\\One.nif", 100.0, 1, false),
        instance("a\\Two.nif", 300.0, 1, false),
        instance("a\\Two.nif", 500.0, 2, false),
    ];
    let rule = Some(&first_reference_only as RuleShapes<'_>);
    let (_, one) = run_with_rules("rule-threads-1", &list, &store, None, rule, 1);
    let (_, four) = run_with_rules("rule-threads-4", &list, &store, None, rule, 4);
    assert_eq!(
        std::fs::read(one.join(BLOCK)).unwrap(),
        std::fs::read(four.join(BLOCK)).unwrap()
    );
}
