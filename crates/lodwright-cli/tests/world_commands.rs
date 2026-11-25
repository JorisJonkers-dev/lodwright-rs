// SPDX-License-Identifier: GPL-3.0-only
//! `plan` and `heightmap` against plugins built by the testkit in a temporary data folder.

use lodwright_cli::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, run};
use lodwright_testkit::golden::assert_golden;
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};
use lodwright_testkit::{covers, ids};
use std::path::{Path, PathBuf};

fn go(args: &[&str]) -> (u8, String, String) {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn data_folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let base = SyntheticWorld::new(0xA00, "Testland")
        .cell(
            0,
            0,
            Heights::from_fn(|x, y| 8.0 * f32::from(u8::try_from((x + y) % 40).unwrap())),
        )
        .cell(1, 0, Heights::flat(64.0))
        .cell(-5, 3, Heights::flat(-32.0));
    let patch = SyntheticWorld::new(0xA00, "Testland").cell(1, 0, Heights::flat(96.0));
    let other = SyntheticWorld::new(0xB00, "Island").cell(2, 2, Heights::flat(16.0));
    std::fs::write(dir.join("Base.esm"), world_plugin(&[], &base, false)).unwrap();
    std::fs::write(
        dir.join("PATCH.ESP"),
        world_plugin(&["base.esm"], &patch, true),
    )
    .unwrap();
    std::fs::write(dir.join("Other.esp"), world_plugin(&[], &other, false)).unwrap();
    dir
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

#[test]
fn plan_prints_the_text_report_for_one_world() {
    covers(&[ids::cli::commands::PLAN]);
    let dir = data_folder("text");
    let (code, out, err) = go(&[
        "plan",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Base.esm",
        "--plugin",
        "Patch.esp",
        "--world",
        "testland",
        "--blocks",
    ]);
    assert_eq!((code, err.as_str()), (EXIT_OK, ""));
    assert_golden(&golden("plan_text.txt"), &out);
}

#[test]
fn plan_prints_json_for_every_world() {
    covers(&[ids::cli::commands::PLAN]);
    let dir = data_folder("json");
    let (code, out, _) = go(&[
        "plan",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Base.esm",
        "--plugin",
        "Patch.esp",
        "--plugin",
        "Other.esp",
        "--format",
        "json",
    ]);
    assert_eq!(code, EXIT_OK);
    // The tool version changes with every release, so the golden file holds a placeholder.
    let out = out.replace(env!("CARGO_PKG_VERSION"), "VERSION");
    // Peak memory is measured on Linux only, so that one capability depends on the platform.
    let out = out.replace(
        &format!("\"peak_memory_reported\":{}", cfg!(target_os = "linux")),
        "\"peak_memory_reported\":\"PLATFORM\"",
    );
    assert_golden(&golden("plan.json"), &out);
}

#[test]
fn plan_with_blocks_in_json_lists_each_block_with_paths() {
    covers(&[ids::cli::commands::PLAN]);
    let dir = data_folder("jsonblocks");
    let (code, out, _) = go(&[
        "plan",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Other.esp",
        "--format",
        "json",
        "--blocks",
    ]);
    assert_eq!(code, EXIT_OK);
    assert!(out.starts_with("{\"schema_version\":1,\"contract\":\"lodwright-plan\","));
    assert!(out.contains("\"mesh\":\"meshes/landscape/lod/island/island.level4.x2.y2.nif\""));
    assert!(out.contains("\"texture_size\":[256,256]"));
}

#[test]
fn plan_reads_a_load_order_file() {
    covers(&[ids::cli::commands::PLAN]);
    let dir = data_folder("order");
    let order = dir.join("order.txt");
    std::fs::write(&order, "# order\n*Base.esm\n*Patch.esp\n").unwrap();
    let (code, out, _) = go(&[
        "plan",
        "--data",
        dir.to_str().unwrap(),
        "--load-order",
        order.to_str().unwrap(),
    ]);
    assert_eq!(code, EXIT_OK);
    assert!(out.starts_with("world Testland (form 00000A00): 3 cells"));
}

#[test]
fn plan_failures_name_the_problem_and_exit_one() {
    let dir = data_folder("fail");
    let d = dir.to_str().unwrap();
    let (code, _, err) = go(&["plan", "--data", d, "--plugin", "Missing.esm"]);
    assert_eq!(code, EXIT_FAILED);
    assert_eq!(
        err,
        "lodwright: plugin Missing.esm not found in the data folder\n"
    );
    let (code, _, err) = go(&["plan", "--data", d, "--plugin", "Patch.esp"]);
    assert_eq!(code, EXIT_FAILED);
    assert!(
        err.contains("master base.esm is not in the load order"),
        "{err}"
    );
    let (code, _, err) = go(&[
        "plan", "--data", d, "--plugin", "Base.esm", "--world", "Nowhere",
    ]);
    assert_eq!(code, EXIT_FAILED);
    assert!(
        err.contains("no worldspace named Nowhere; found: Testland"),
        "{err}"
    );
    let (code, _, err) = go(&["plan", "--data", d]);
    assert_eq!(code, EXIT_FAILED);
    assert!(err.contains("no plugins given"));
}

#[test]
fn plan_usage_errors_exit_two() {
    assert_eq!(go(&["plan"]).0, EXIT_USAGE);
    assert_eq!(go(&["plan", "--data", "d", "--bogus"]).0, EXIT_USAGE);
    assert_eq!(go(&["heightmap", "--level"]).0, EXIT_USAGE);
}

#[test]
fn a_plugin_without_worlds_is_reported() {
    let dir = data_folder("empty");
    let flat = lodwright_testkit::plugin::PluginBuilder::new().build();
    std::fs::write(dir.join("Empty.esp"), flat).unwrap();
    let (code, _, err) = go(&[
        "plan",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Empty.esp",
    ]);
    assert_eq!(code, EXIT_FAILED);
    assert!(err.contains("define no worldspace"));
}

#[test]
fn heightmap_writes_a_16_bit_pgm_north_up() {
    covers(&[ids::cli::commands::HEIGHTMAP]);
    let dir = data_folder("pgm");
    let out_file = dir.join("island.pgm");
    let (code, out, err) = go(&[
        "heightmap",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Other.esp",
        "--world",
        "Island",
        "--level",
        "4",
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert_eq!((code, err.as_str()), (EXIT_OK, ""));
    assert!(
        out.starts_with("wrote 129x129 samples; 0 is 16 and 65535 is 16 world units"),
        "{out}"
    );
    let bytes = std::fs::read(&out_file).unwrap();
    let header = b"P5\n129 129\n65535\n";
    assert_eq!(&bytes[..header.len()], header);
    assert_eq!(bytes.len(), header.len() + 129 * 129 * 2);
}

#[test]
fn heightmap_scales_between_the_lowest_and_highest_height() {
    covers(&[ids::cli::commands::HEIGHTMAP]);
    let dir = data_folder("scale");
    let out_file = dir.join("t.pgm");
    let (code, _, _) = go(&[
        "heightmap",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Base.esm",
        "--world",
        "Testland",
        "--level",
        "8",
        "--out",
        out_file.to_str().unwrap(),
    ]);
    assert_eq!(code, EXIT_OK);
    let bytes = std::fs::read(&out_file).unwrap();
    let header_len = bytes
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .nth(2)
        .unwrap()
        .0
        + 1;
    let words: Vec<u16> = bytes[header_len..]
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    assert_eq!(words.iter().max(), Some(&65535));
    assert!(words.contains(&0));
}

#[test]
fn heightmap_needs_a_single_world_and_an_output_file() {
    let dir = data_folder("heightfail");
    let d = dir.to_str().unwrap();
    let target = dir.join("x.pgm");
    let t = target.to_str().unwrap();
    let (code, _, err) = go(&["heightmap", "--data", d, "--plugin", "Other.esp"]);
    assert_eq!(code, EXIT_FAILED);
    assert!(err.contains("--out"), "{err}");
    let (code, _, err) = go(&[
        "heightmap",
        "--data",
        d,
        "--plugin",
        "Base.esm",
        "--plugin",
        "Other.esp",
        "--out",
        t,
    ]);
    assert_eq!(code, EXIT_FAILED);
    assert!(err.contains("choose one worldspace"), "{err}");
    let (code, _, err) = go(&[
        "heightmap",
        "--data",
        d,
        "--plugin",
        "Other.esp",
        "--world",
        "Island",
        "--out",
        "/definitely/not/here/x.pgm",
    ]);
    assert_eq!(code, EXIT_FAILED);
    assert!(
        err.starts_with("lodwright: cannot write the output file"),
        "{err}"
    );
}

#[test]
fn heightmap_refuses_to_write_inside_a_git_working_tree() {
    covers(&[ids::cli::commands::HEIGHTMAP]);
    let dir = data_folder("repo");
    let repo = dir.join("work");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let target = repo.join("h.pgm");
    let args = [
        "heightmap",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Other.esp",
        "--world",
        "Island",
        "--level",
        "4",
        "--out",
        target.to_str().unwrap(),
    ];
    let (code, _, err) = go(&args);
    assert_eq!(code, EXIT_FAILED);
    assert!(err.contains("refusing to write inside a git working tree"));
    assert!(!target.exists());
    let mut forced = args.to_vec();
    forced.push("--allow-in-repo");
    assert_eq!(go(&forced).0, EXIT_OK);
    assert!(target.exists());
}

#[test]
fn the_contract_the_plan_names_is_one_the_handshake_lists() {
    covers(&[ids::contract::documents::HANDSHAKE]);
    let dir = data_folder("handshake");
    let (_, plan, _) = go(&[
        "plan",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Other.esp",
        "--format",
        "json",
    ]);
    let (code, handshake, _) = go(&["contracts", "--json"]);
    assert_eq!(code, EXIT_OK);
    // The plan carries `"contract":"<name>"` and the handshake lists that name with the plan's version.
    assert!(plan.starts_with("{\"schema_version\":1,\"contract\":\"lodwright-plan\","));
    assert!(handshake.contains("{\"name\":\"lodwright-plan\",\"versions\":[1]}"));
}

#[test]
fn verify_passes_a_generated_folder_and_fails_a_damaged_one() {
    covers(&[ids::verify::folder::STRUCTURE_CHECKS]);
    let data = std::env::temp_dir().join(format!("lodwright-verify-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).unwrap();
    let world = SyntheticWorld::new(0xB00, "Island").cell(2, 2, Heights::flat(16.0));
    std::fs::write(data.join("Island.esm"), world_plugin(&[], &world, false)).unwrap();
    let out_dir =
        std::env::temp_dir().join(format!("lodwright-verify-cli-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out_dir);
    let run = |args: &[&str]| {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = lodwright_cli::run(&args, &mut out, &mut err);
        (code, String::from_utf8(out).unwrap())
    };
    let (code, _) = run(&[
        "generate",
        "--data",
        data.to_str().unwrap(),
        "--plugin",
        "Island.esm",
        "--out",
        out_dir.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code, 0);
    let (code, text) = run(&["verify", out_dir.to_str().unwrap()]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("0 errors"), "{text}");
    std::fs::remove_file(out_dir.join("lodsettings/island.dlodsettings")).unwrap();
    std::fs::write(out_dir.join("lodsettings/island.dlodsettings"), b"short").unwrap();
    let (code, text) = run(&["verify", out_dir.to_str().unwrap(), "--json"]);
    assert_eq!(code, 1);
    assert!(
        text.contains("\"ok\":false") && text.contains("24 bytes"),
        "{text}"
    );
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(lodwright_contract::check_verify(&report), Ok(()), "{text}");
    assert_eq!(run(&["verify"]).0, 2);
}

/// Generate a one-cell island whose cell has water above the land, with or without water planes, into a fresh folder.
fn generate_lake(name: &str, water: bool) -> PathBuf {
    let data = std::env::temp_dir().join(format!("lodwright-lake-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).unwrap();
    let world = SyntheticWorld::new(0xC00, "Lake")
        .cell(0, 0, Heights::flat(-64.0))
        .cell_water(0, 0, true, Some(0.0));
    std::fs::write(data.join("Lake.esm"), world_plugin(&[], &world, false)).unwrap();
    // the output must not lie inside the data folder
    let out = data.with_extension("out");
    let _ = std::fs::remove_dir_all(&out);
    let mut args = vec![
        "generate",
        "--data",
        data.to_str().unwrap(),
        "--plugin",
        "Lake.esm",
        "--out",
        out.to_str().unwrap(),
        "--json",
    ];
    if !water {
        args.push("--no-water");
    }
    let (code, text, _) = go(&args);
    assert_eq!(code, EXIT_OK, "{text}");
    out
}

#[test]
fn verify_reports_water_iou_and_object_agreement_against_an_equal_reference() {
    let (mine, theirs) = (
        generate_lake("equal-a", true),
        generate_lake("equal-b", true),
    );
    let (code, text, _) = go(&[
        "verify",
        mine.to_str().unwrap(),
        "--reference",
        theirs.to_str().unwrap(),
        "--water-iou",
        "0.95",
        "--json",
    ]);
    assert_eq!(code, EXIT_OK, "{text}");
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(lodwright_contract::check_verify(&report), Ok(()));
    let water = &report["water"];
    assert!(water["blocks"].as_u64().unwrap() > 0, "{text}");
    assert_eq!(water["under_threshold"], 0);
    assert_eq!(water["lowest"], 1.0);
    assert_eq!(water["mean"], 1.0);
    assert_eq!(report["objects"]["blocks"], report["objects"]["exact"]);
}

#[test]
fn a_block_under_the_water_iou_threshold_is_a_difference_and_exits_one() {
    covers(&[ids::verify::folder::WATER_IOU]);
    let (mine, theirs) = (generate_lake("dry-a", true), generate_lake("dry-b", false));
    let (code, text, _) = go(&[
        "verify",
        mine.to_str().unwrap(),
        "--reference",
        theirs.to_str().unwrap(),
        "--water-iou",
        "0.95",
    ]);
    assert_eq!(code, EXIT_FAILED, "{text}");
    assert!(text.contains("water IoU 0.000 is under 0.95"), "{text}");
    assert!(text.contains(", lowest 0.000, mean 0.000"), "{text}");
}

#[test]
fn a_known_differences_list_accepts_its_blocks_reports_stale_entries_and_prints_levels() {
    covers(&[ids::verify::folder::KNOWN_DIFFERENCES]);
    let (mine, theirs) = (
        generate_lake("known-a", true),
        generate_lake("known-b", false),
    );
    let list = std::env::temp_dir().join(format!("lodwright-known-{}.txt", std::process::id()));
    let entries: String = [4, 8, 16, 32]
        .iter()
        .map(|level| {
            format!(
                "meshes/landscape/lod/lake/lake.level{level}.x0.y0.nif\t0.7.0\tthe reference has no water here\n"
            )
        })
        .collect::<Vec<_>>()
        .concat();
    let mut text = format!("# path\trelease\treason\n{entries}");
    text.push_str("meshes/landscape/lod/lake/lake.level4.x4.y0.nif\t0.7.0\tfixed already\n");
    std::fs::write(&list, &text).unwrap();
    let args = |list: &std::path::Path| {
        vec![
            "verify".to_owned(),
            mine.to_str().unwrap().to_owned(),
            "--reference".to_owned(),
            theirs.to_str().unwrap().to_owned(),
            "--water-iou".to_owned(),
            "0.95".to_owned(),
            "--known-differences".to_owned(),
            list.to_str().unwrap().to_owned(),
        ]
    };
    let owned = args(&list);
    let (code, out, _) = go(&owned.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(code, EXIT_OK, "every difference is known: {out}");
    assert_golden(&golden("verify_known_differences.txt"), &out);
    let mut json_args = owned.clone();
    json_args.push("--json".to_owned());
    let (code, json, _) = go(&json_args.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(code, EXIT_OK);
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(lodwright_contract::check_verify(&report), Ok(()));
    let levels: Vec<u64> = report["water"]["per_level"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["level"].as_u64().unwrap())
        .collect();
    assert_eq!(levels, [4, 8, 16, 32]);
    assert_eq!(report["known"].as_array().unwrap().len(), 9);
    assert_eq!(report["stale"].as_array().unwrap().len(), 1);
    // the lake has water blocks at levels 4, 8, 16 and 32; without the level 16 entry, that block is a difference again
    let short = list.with_extension("short.txt");
    std::fs::write(
        &short,
        text.replace("lake.level16.x0.y0.nif", "elsewhere.nif"),
    )
    .unwrap();
    let owned = args(&short);
    let (code, out, _) = go(&owned.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(code, EXIT_FAILED, "{out}");
    assert!(
        out.contains("lake.level16.x0.y0.nif: water IoU 0.000 is under 0.95"),
        "{out}"
    );
    // a list needs a reference, and a malformed list is a usage error that names its line
    assert_eq!(
        go(&[
            "verify",
            mine.to_str().unwrap(),
            "--known-differences",
            list.to_str().unwrap()
        ])
        .0,
        EXIT_USAGE
    );
    std::fs::write(&short, "just-a-path.nif\n").unwrap();
    let owned = args(&short);
    let (code, _, err) = go(&owned.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(code, EXIT_USAGE);
    assert!(err.contains("line 1"), "{err}");
}

#[test]
fn verify_without_a_reference_prints_no_block_reports() {
    let mine = generate_lake("alone", true);
    let (code, text, _) = go(&["verify", mine.to_str().unwrap(), "--json"]);
    assert_eq!(code, EXIT_OK, "{text}");
    assert!(
        !text.contains("\"water\"") && !text.contains("\"objects\""),
        "{text}"
    );
}

#[test]
fn the_water_iou_threshold_needs_a_reference_and_a_number_from_zero_to_one() {
    let dir = std::env::temp_dir();
    let dir = dir.to_str().unwrap();
    assert_eq!(go(&["verify", dir, "--water-iou", "0.9"]).0, EXIT_USAGE);
    for bad in ["1.5", "-0.1", "x"] {
        assert_eq!(
            go(&["verify", dir, "--reference", dir, "--water-iou", bad]).0,
            EXIT_USAGE,
            "{bad}"
        );
    }
    assert_eq!(
        go(&["verify", dir, "--reference", dir, "--water-iou"]).0,
        EXIT_USAGE
    );
    // both ends of the range are accepted: two empty folders are equal, so the run is clean
    let empty = std::env::temp_dir().join(format!("lodwright-iou-edges-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    let empty = empty.to_str().unwrap();
    for edge in ["0", "1"] {
        assert_eq!(
            go(&["verify", empty, "--reference", empty, "--water-iou", edge]).0,
            EXIT_OK,
            "{edge}"
        );
    }
}
