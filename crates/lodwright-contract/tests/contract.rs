// SPDX-License-Identifier: GPL-3.0-only
//! Golden documents, schema drift and the compatibility rules of the contract.

use std::path::{Path, PathBuf};

use lodwright_contract::{
    Bounds, Capabilities, DocumentKind, LevelPlan, Plan, PlannedBlock, Request, ResultDoc, Status,
    ToolInfo, VerifyDoc, VerifyFinding, WorldPlan, Written, check_plan, check_request, check_result,
    check_verify, schema_file_name, schema_json, sha256_digest,
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
        mesh: "meshes/landscape/lod/island/island.level4.x0.y0.nif".into(),
        diffuse: "textures/landscape/lod/island/diffuse/island.n.level4.x0.y0.dds".into(),
        normal: "textures/landscape/lod/island/normals/island.n.level4.x0.y0.dds".into(),
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

fn sample_verify() -> VerifyDoc {
    let mut doc = VerifyDoc::new();
    doc.ok = false;
    doc.terrain_blocks = 4;
    doc.object_blocks = 1;
    doc.textures = 8;
    doc.settings = 1;
    doc.errors = vec![VerifyFinding {
        path: "lodsettings/island.dlodsettings".into(),
        problem: "the width disagrees with the corners".into(),
    }];
    doc.differences = vec![VerifyFinding {
        path: "meshes/landscape/lod/island/island.level4.x0.y0.nif".into(),
        problem: "block types differ".into(),
    }];
    doc
}

#[test]
fn the_verify_document_equals_its_golden_file() {
    assert_golden(&golden("verify.json"), &pretty(&sample_verify()));
}

/// The request every existing output folder was most likely made with: defaults everywhere.
fn default_request() -> Request {
    Request::new("/data", &["FalloutNV.esm"], "/out")
}

/// The digest of [`default_request`] as released. Not rewritten by `UPDATE_GOLDEN`: when this test fails, a change
/// made every existing manifest's `request_digest` stale, and every output folder would be rebuilt for no reason. A
/// new request option must not serialise while it has its default value (see docs/contracts.md).
const DEFAULT_REQUEST_DIGEST: &str =
    "sha256:029c0350d99a3a289ddf1ab79d491a5bc818c961099cb026cee70ef323ba5642";

#[test]
fn the_default_request_keeps_its_released_digest() {
    assert_eq!(default_request().digest(), DEFAULT_REQUEST_DIGEST);
}

#[test]
fn the_default_request_document_equals_its_golden_file() {
    assert_golden(&golden("request_default.json"), &pretty(&default_request()));
}

#[test]
fn a_request_read_from_its_golden_file_keeps_the_released_digest() {
    let request: Request =
        serde_json::from_str(&std::fs::read_to_string(golden("request_default.json")).unwrap())
            .unwrap();
    assert_eq!(request.digest(), DEFAULT_REQUEST_DIGEST);
}

#[test]
fn committed_schemas_equal_the_generated_ones() {
    for kind in DocumentKind::ALL {
        for &version in kind.versions() {
            let generated = schema_json(kind, version).unwrap();
            // UPDATE_GOLDEN=1 rewrites the committed file; without it a drift fails here.
            assert_golden(&schemas(&schema_file_name(kind, version)), &generated);
        }
    }
}

#[test]
fn every_committed_schema_belongs_to_a_kind_this_build_speaks() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut expected: Vec<String> = DocumentKind::ALL
        .into_iter()
        .flat_map(|kind| {
            kind.versions()
                .iter()
                .map(move |&v| schema_file_name(kind, v))
        })
        .collect();
    expected.sort();
    assert_eq!(names, expected);
}

#[test]
fn golden_documents_pass_their_own_schema_check() {
    for (name, kind) in [
        ("plan.json", DocumentKind::Plan),
        ("plan_blocks.json", DocumentKind::Plan),
        ("result_unsupported.json", DocumentKind::Result),
        ("result_ok.json", DocumentKind::Result),
        ("verify.json", DocumentKind::Verify),
        ("request_default.json", DocumentKind::Request),
    ] {
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(golden(name)).unwrap()).unwrap();
        let outcome = match kind {
            DocumentKind::Plan => check_plan(&value),
            DocumentKind::Result => check_result(&value),
            DocumentKind::Verify => check_verify(&value),
            DocumentKind::Request => check_request(&value),
            other => unreachable!("no golden {}", other.name()),
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
