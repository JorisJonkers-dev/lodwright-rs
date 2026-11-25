// SPDX-License-Identifier: GPL-3.0-only
//! `lodwright bands`: the minimum and maximum of each statistic over the verify reports of several runs.

use lodwright_testkit::{covers, ids};
use std::path::{Path, PathBuf};

use lodwright_cli::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, run};
use lodwright_contract::{BandsDoc, ObjectBlock, ObjectReport, VerifyDoc, WaterBlock, WaterReport};
use lodwright_testkit::golden::assert_golden;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-bands-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn go(args: &[&str]) -> (u8, String) {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &mut out, &mut err);
    (code, String::from_utf8(out).unwrap())
}

/// A verify report of one run: one water block with `iou`, one object block with `unmatched` vertices.
fn report(iou: f64, unmatched: u64) -> VerifyDoc {
    let mut doc = VerifyDoc::new();
    doc.terrain_blocks = 4;
    doc.object_blocks = 1;
    doc.textures = 8;
    doc.settings = 1;
    doc.water = Some(WaterReport {
        blocks: 1,
        threshold: 0.95,
        under_threshold: u64::from(iou < 0.95),
        lowest: Some(iou),
        mean: Some(iou),
        per_block: vec![WaterBlock {
            path: "meshes/landscape/lod/w/w.level4.x0.y0.nif".into(),
            iou,
        }],
        per_level: Vec::new(),
        profile: Vec::new(),
    });
    doc.objects = Some(ObjectReport {
        blocks: 1,
        exact: u64::from(unmatched == 0),
        per_block: vec![ObjectBlock {
            path: "meshes/landscape/lod/w/blocks/w.level4.x0.y0.nif".into(),
            vertices: 3,
            reference_vertices: 3,
            triangles: 1,
            reference_triangles: 1,
            unmatched_vertices: unmatched,
            exact: unmatched == 0,
        }],
    });
    doc
}

fn write(dir: &Path, name: &str, doc: &VerifyDoc) -> String {
    let file = dir.join(name);
    std::fs::write(&file, serde_json::to_string(doc).unwrap()).unwrap();
    file.to_string_lossy().into_owned()
}

#[test]
fn identical_runs_give_zero_width_bands() {
    covers(&[ids::verify::measure::BANDS]);
    let dir = temp("same");
    let files: Vec<String> = (0..3)
        .map(|i| write(&dir, &format!("run{i}.json"), &report(0.912_345, 0)))
        .collect();
    let mut args = vec!["bands", "--json"];
    args.extend(files.iter().map(String::as_str));
    let (code, out) = go(&args);
    assert_eq!(code, EXIT_OK);
    let doc: BandsDoc = serde_json::from_str(&out).unwrap();
    assert_eq!(doc.runs, 3);
    assert!(
        doc.bands
            .iter()
            .all(|b| (b.min - b.max).abs() < f64::EPSILON && b.runs == 3)
    );
}

#[test]
fn a_changed_run_gives_a_band_and_the_text_report_equals_its_golden_file() {
    let dir = temp("changed");
    let a = write(&dir, "a.json", &report(0.9, 0));
    let b = write(&dir, "b.json", &report(0.954_9, 2));
    let (code, out) = go(&["bands", &a, &b]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("water.mean 0.9 0.955 (2 runs)"), "{out}");
    assert_golden(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/bands.txt"),
        &out,
    );
}

#[test]
fn bands_needs_reports_and_refuses_a_file_that_is_not_one() {
    covers(&[ids::verify::measure::BANDS]);
    assert_eq!(go(&["bands"]).0, EXIT_USAGE);
    assert_eq!(go(&["bands", "--frobnicate"]).0, EXIT_USAGE);
    let dir = temp("bad");
    let file = dir.join("plan.json");
    std::fs::write(
        &file,
        b"{\"schema_version\":1,\"contract\":\"lodwright-plan\"}",
    )
    .unwrap();
    assert_eq!(go(&["bands", file.to_str().unwrap()]).0, EXIT_FAILED);
    assert_eq!(
        go(&["bands", dir.join("missing.json").to_str().unwrap()]).0,
        EXIT_FAILED
    );
}
