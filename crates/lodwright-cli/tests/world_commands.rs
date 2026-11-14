// SPDX-License-Identifier: MPL-2.0
//! `plan` and `heightmap` against plugins built by the testkit in a temporary data folder.

use lodwright_cli::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, run};
use lodwright_testkit::golden::assert_golden;
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};
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
    assert_golden(&golden("plan.json"), &out);
}

#[test]
fn plan_with_blocks_in_json_lists_each_block_with_paths() {
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
    assert!(out.starts_with("{\"schema_version\":1,\"worlds\":["));
    assert!(out.contains("\"mesh\":\"meshes/landscape/lod/Island/blocks/Island.4.0.0.btr\""));
    assert!(out.contains("\"texture_size\":[512,512]"));
}

#[test]
fn plan_reads_a_load_order_file() {
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
