// SPDX-License-Identifier: MPL-2.0
//! Golden documents, schema drift and the compatibility rules of the contract.

use std::path::{Path, PathBuf};

use lodwright_contract::{
    Bounds, Capabilities, DocumentKind, LevelPlan, Plan, PlannedBlock, ResultDoc, Status, ToolInfo,
    WorldPlan, Written, check_plan, check_result, schema_file_name, schema_json, sha256_digest,
};
use lodwright_testkit::golden::assert_golden;

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn schemas(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas")
        .join(name)
}

fn sample_plan(with_blocks: bool) -> Plan {
    let block = PlannedBlock {
        level: 4,
        x: 0,
        y: 0,
        step: 1,
        vertices: 4,
        triangles: 2,
        max_error: 0.0,
        mesh: "meshes/landscape/lod/Island/blocks/Island.4.0.0.btr".into(),
        diffuse: "textures/landscape/lod/Island/diffuse/Island.4.0.0.dds".into(),
        normal: "textures/landscape/lod/Island/normals/Island.4.0.0_n.dds".into(),
        texture_size: [512, 512],
        mips: 10,
    };
    Plan {
        schema_version: 1,
        contract: "lodwright-plan".into(),
        tool: ToolInfo::new("0.0.0-test"),
        arguments_digest: sha256_digest(b"arguments"),
        inputs_digest: sha256_digest(b"inputs"),
        capabilities: Capabilities::default(),
        worlds: vec![WorldPlan {
            name: "Island".into(),
            form_id: "02000B00".into(),
            cells: 1,
            bounds: Some(Bounds {
                west: 2,
                south: 2,
                east: 2,
                north: 2,
            }),
            max_seam_error: 0.0,
            settings_path: "lodsettings/Island.lod".into(),
            levels: vec![LevelPlan {
                level: 4,
                blocks: 1,
                vertices: 4,
                triangles: 2,
                max_error: 0.0,
            }],
            blocks: with_blocks.then(|| vec![block]),
        }],
        warnings: vec![],
    }
}

fn pretty<T: serde::Serialize>(value: &T) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    text
}

#[test]
fn the_plan_document_equals_its_golden_file() {
    assert_golden(&golden("plan.json"), &pretty(&sample_plan(false)));
}

#[test]
fn the_plan_document_with_blocks_equals_its_golden_file() {
    assert_golden(&golden("plan_blocks.json"), &pretty(&sample_plan(true)));
}

#[test]
fn the_unsupported_result_equals_its_golden_file() {
    let mut doc = ResultDoc::new(
        "0.0.0-test",
        Status::Unsupported,
        "capability_missing",
        "generate cannot write terrain_mesh yet",
    );
    doc.arguments_digest = Some(sha256_digest(b"arguments"));
    doc.inputs_digest = Some(sha256_digest(b"inputs"));
    doc.peak_memory_mib = Some(12);
    doc.missing_capabilities = Capabilities::default().missing();
    assert_golden(&golden("result_unsupported.json"), &pretty(&doc));
}

#[test]
fn the_ok_result_equals_its_golden_file() {
    let mut doc = ResultDoc::new("0.0.0-test", Status::Ok, "ok", "written");
    doc.written = vec![Written {
        world: "Island".into(),
        kind: "terrain_mesh".into(),
        files: 3,
        bytes: 1024,
    }];
    doc.manifest = Some("manifest.json".into());
    assert_golden(&golden("result_ok.json"), &pretty(&doc));
}

#[test]
fn committed_schemas_equal_the_generated_ones() {
    for kind in [DocumentKind::Plan, DocumentKind::Result] {
        let generated = schema_json(kind, 1).unwrap();
        // UPDATE_GOLDEN=1 rewrites the committed file; without it a drift fails here.
        assert_golden(&schemas(&schema_file_name(kind, 1)), &generated);
    }
}

#[test]
fn golden_documents_pass_their_own_schema_check() {
    for (name, kind) in [
        ("plan.json", DocumentKind::Plan),
        ("plan_blocks.json", DocumentKind::Plan),
        ("result_unsupported.json", DocumentKind::Result),
        ("result_ok.json", DocumentKind::Result),
    ] {
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(golden(name)).unwrap()).unwrap();
        let outcome = match kind {
            DocumentKind::Plan => check_plan(&value),
            DocumentKind::Result => check_result(&value),
        };
        assert_eq!(outcome, Ok(()), "{name}");
    }
}

#[test]
fn a_golden_plan_with_version_two_is_refused() {
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(golden("plan.json")).unwrap()).unwrap();
    value["schema_version"] = 2.into();
    assert!(
        check_plan(&value)
            .unwrap_err()
            .to_string()
            .contains("not supported")
    );
}

#[test]
fn a_plan_with_unknown_fields_is_read_and_the_fields_are_dropped() {
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(golden("plan.json")).unwrap()).unwrap();
    value["added_later"] = "x".into();
    value["worlds"][0]["also_later"] = 1.into();
    let plan: Plan = serde_json::from_value(value).unwrap();
    assert_eq!(plan, sample_plan(false));
}

#[test]
fn no_zero_stands_in_for_a_missing_bound() {
    let mut plan = sample_plan(false);
    plan.worlds[0].bounds = None;
    let text = serde_json::to_string(&plan).unwrap();
    assert!(!text.contains("bounds"));
}
