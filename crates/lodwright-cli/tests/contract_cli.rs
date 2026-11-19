// SPDX-License-Identifier: GPL-3.0-only
//! The JSON contract as a caller sees it: documents, exit codes, versions and what never appears.

use lodwright_cli::{EXIT_FAILED, EXIT_OK, EXIT_UNSUPPORTED, EXIT_USAGE, run};
use lodwright_contract::{DocumentKind, check_plan, check_result, schema_json};
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};
use std::path::{Path, PathBuf};
use std::process::Command;

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
    let dir =
        std::env::temp_dir().join(format!("lodwright-contract-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let world = SyntheticWorld::new(0xB00, "Island")
        .cell(2, 2, Heights::flat(16.0))
        .cell(3, 2, Heights::flat(24.0));
    std::fs::write(dir.join("Island.esm"), world_plugin(&[], &world, false)).unwrap();
    dir
}

fn listing(dir: &Path) -> Vec<(String, u64)> {
    let mut names: Vec<(String, u64)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                e.metadata().unwrap().len(),
            )
        })
        .collect();
    names.sort();
    names
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn json_is_an_alias_of_format_json_and_the_document_passes_the_plan_check() {
    let dir = data_folder("alias");
    let d = dir.to_str().unwrap();
    let base = ["plan", "--data", d, "--plugin", "Island.esm"];
    let (code, with_flag, err) = go(&[&base[..], &["--json"]].concat());
    assert_eq!((code, err.as_str()), (EXIT_OK, ""));
    let (_, with_format, _) = go(&[&base[..], &["--format", "json"]].concat());
    assert_eq!(with_flag, with_format);
    let doc = json(&with_flag);
    assert_eq!(check_plan(&doc), Ok(()));
    assert_eq!(doc["tool"]["name"], "lodwright");
    assert_eq!(doc["contract"], "lodwright-plan");
    assert_eq!(doc["capabilities"]["terrain_mesh"], true);
    assert!(
        doc["arguments_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(
        doc["inputs_digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert!(
        doc["worlds"][0].get("blocks").is_none(),
        "paths only with --blocks"
    );
    assert_eq!(doc["worlds"][0]["bounds"]["west"], 2);
    assert_eq!(doc["worlds"][0]["bounds"]["east"], 3);
}

#[test]
fn schema_version_one_prints_the_default_document_and_other_numbers_exit_two() {
    let dir = data_folder("version");
    let d = dir.to_str().unwrap();
    let base = ["plan", "--data", d, "--plugin", "Island.esm", "--json"];
    let (_, default, _) = go(&base);
    let (code, one, _) = go(&[&base[..], &["--schema-version", "1"]].concat());
    assert_eq!(code, EXIT_OK);
    assert_eq!(one, default);
    for bad in ["2", "0"] {
        let (code, out, err) = go(&[&base[..], &["--schema-version", bad]].concat());
        assert_eq!((code, out.as_str()), (EXIT_USAGE, ""));
        assert!(err.contains("not supported"), "{err}");
    }
    let (code, _, _) = go(&["schema", "plan", "--schema-version", "2"]);
    assert_eq!(code, EXIT_USAGE);
    let (code, _, _) = go(&["generate", "--json", "--schema-version", "2"]);
    assert_eq!(code, EXIT_USAGE);
}

#[test]
fn the_digests_do_not_depend_on_where_the_data_folder_is() {
    let (a, b) = (data_folder("digest-a"), data_folder("digest-b"));
    let run_plan = |dir: &Path| {
        let (_, out, _) = go(&[
            "plan",
            "--data",
            dir.to_str().unwrap(),
            "--plugin",
            "island.ESM",
            "--json",
        ]);
        let doc = json(&out);
        (
            doc["arguments_digest"].clone(),
            doc["inputs_digest"].clone(),
        )
    };
    assert_eq!(run_plan(&a), run_plan(&b));
}

#[test]
fn schema_prints_the_generated_schema_for_both_documents() {
    for (name, kind) in [
        ("plan", DocumentKind::Plan),
        ("result", DocumentKind::Result),
    ] {
        let (code, out, err) = go(&["schema", name]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""));
        assert_eq!(out, schema_json(kind, 1).unwrap());
        let (_, explicit, _) = go(&["schema", name, "--schema-version", "1"]);
        assert_eq!(explicit, out);
    }
}

#[test]
fn schema_rejects_a_missing_or_unknown_document_name() {
    for args in [
        vec!["schema"],
        vec!["schema", "nope"],
        vec!["schema", "plan", "result"],
        vec!["schema", "plan", "--schema-version"],
        vec!["schema", "plan", "--schema-version", "x"],
    ] {
        assert_eq!(go(&args).0, EXIT_USAGE, "{args:?}");
    }
}

#[test]
fn version_json_names_the_tool_and_the_contracts_it_speaks() {
    for args in [["--version", "--json"], ["--json", "-V"]] {
        let (code, out, _) = go(&args);
        assert_eq!(code, EXIT_OK);
        let doc = json(&out);
        assert_eq!(doc["tool"], "lodwright");
        assert_eq!(doc["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            doc["contracts"],
            serde_json::json!([
                {"name": "lodwright-plan", "versions": [1]},
                {"name": "lodwright-result", "versions": [1]},
                {"name": "lodwright-request", "versions": [1]},
                {"name": "lodwright-manifest", "versions": [1]},
                {"name": "lodwright-verify", "versions": [1]},
                {"name": "lodwright-events", "versions": [1]},
                {"name": "lodwright-api", "versions": [1]}
            ])
        );
    }
}

#[test]
fn generate_writes_the_terrain_files_and_reports_counts_per_kind() {
    let dir = data_folder("generate");
    let output = std::env::temp_dir().join(format!(
        "lodwright-contract-{}-generate-out",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&output);
    let before = listing(&dir);
    let (code, out, err) = go(&[
        "generate",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Island.esm",
        "--out",
        output.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!((code, err.as_str()), (0, ""));
    let doc = json(&out);
    assert_eq!(check_result(&doc), Ok(()));
    assert_eq!(doc["status"], "ok");
    assert!(doc["inputs_digest"].as_str().is_some());
    let written = doc["written"].as_array().unwrap();
    let kinds: Vec<&str> = written
        .iter()
        .map(|w| w["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["terrain_mesh", "terrain_texture", "settings"]);
    assert_eq!(written[0]["files"], 4);
    assert_eq!(written[1]["files"], 8);
    assert_eq!(written[2]["files"], 1);
    assert!(output.join("lodsettings/island.dlodsettings").is_file());
    assert_eq!(listing(&dir), before, "the data folder is untouched");
}

#[test]
fn generate_refuses_an_output_folder_inside_the_data_folder() {
    let dir = data_folder("generate-inside");
    let inside = dir.join("lod-out");
    let (code, out, _) = go(&[
        "generate",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Island.esm",
        "--out",
        inside.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code, 1);
    assert_eq!(json(&out)["code"], "output_in_data");
    assert!(!inside.exists());
}

#[test]
fn generate_without_arguments_still_answers_unsupported() {
    let (code, out, _) = go(&["generate", "--json"]);
    assert_eq!(code, EXIT_UNSUPPORTED);
    let doc = json(&out);
    assert_eq!(doc["status"], "unsupported");
    assert!(doc.get("inputs_digest").is_none());
    assert!(doc.get("arguments_digest").is_none());
}

#[test]
fn generate_without_json_puts_the_reason_on_stderr_and_the_document_on_stdout() {
    let (code, out, err) = go(&["generate"]);
    assert_eq!(code, EXIT_UNSUPPORTED);
    assert_eq!(json(&out)["status"], "unsupported");
    assert!(err.contains("generate needs"), "{err}");
}

#[test]
fn a_failure_with_json_prints_the_error_document_on_stdout_and_no_data_path() {
    let dir = data_folder("failure");
    let d = dir.to_str().unwrap();
    let (code, out, err) = go(&["plan", "--data", d, "--plugin", "Missing.esm", "--json"]);
    assert_eq!(code, EXIT_FAILED);
    let doc = json(&out);
    assert_eq!(check_result(&doc), Ok(()));
    assert_eq!(doc["status"], "failed");
    assert_eq!(doc["code"], "plugin_not_found");
    assert!(!out.contains(d) && !err.contains(d));
    // generate reads the same plugins and fails the same way, with exit 1
    let (code, out, _) = go(&["generate", "--data", d, "--plugin", "Missing.esm", "--json"]);
    assert_eq!(code, EXIT_FAILED);
    assert_eq!(json(&out)["code"], "plugin_not_found");
}

#[test]
fn a_failure_without_json_prints_no_document() {
    let dir = data_folder("failure-text");
    let (code, out, err) = go(&[
        "plan",
        "--data",
        dir.to_str().unwrap(),
        "--plugin",
        "Missing.esm",
    ]);
    assert_eq!((code, out.as_str()), (EXIT_FAILED, ""));
    assert!(err.starts_with("lodwright: plugin Missing.esm not found"));
}

#[test]
fn result_and_error_documents_reach_stdout_and_never_stderr_in_the_real_binary() {
    let dir = data_folder("process");
    let d = dir.to_str().unwrap();
    for (args, status) in [
        (vec!["generate", "--json"], 3),
        (
            vec!["generate", "--data", d, "--plugin", "Island.esm", "--json"],
            3,
        ),
        (
            vec!["plan", "--data", d, "--plugin", "Nope.esm", "--json"],
            1,
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_lodwright"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(status), "{args:?}");
        let doc = json(&String::from_utf8(output.stdout).unwrap());
        assert!(check_result(&doc).is_ok());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            !stderr.contains("\"schema_version\""),
            "no document on stderr: {stderr}"
        );
    }
}

#[test]
fn no_output_contains_a_data_path_or_plugin_content() {
    let dir = data_folder("leak");
    let d = dir.to_str().unwrap();
    let marker = "ZZMARKERZZ";
    let world = SyntheticWorld::new(0xC00, marker).cell(0, 0, Heights::flat(8.0));
    let mut bytes = world_plugin(&[], &world, false);
    bytes.extend_from_slice(marker.as_bytes());
    std::fs::write(dir.join("Marked.esm"), bytes).unwrap();
    let base = ["--data", d, "--plugin", "Marked.esm"];
    let outputs = [
        go(&[&["plan"][..], &base, &["--json", "--blocks"]].concat()),
        go(&[&["generate"][..], &base, &["--json"]].concat()),
        go(&["plan", "--data", d, "--plugin", "Gone.esm", "--json"]),
    ];
    for (_, out, err) in outputs {
        for text in [&out, &err] {
            assert!(!text.contains(d), "data folder path leaked: {text}");
            assert!(!text.contains("/tmp"), "absolute path leaked: {text}");
        }
    }
    // The world name comes from the plugin by design (an editor id); the trailing marker bytes are never echoed.
    let (_, out, _) = go(&[&["generate"][..], &base, &["--json"]].concat());
    assert!(!out.contains(marker));
}
