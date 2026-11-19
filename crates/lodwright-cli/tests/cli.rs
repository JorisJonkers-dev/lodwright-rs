// SPDX-License-Identifier: GPL-3.0-only
//! The binary itself: exit codes as a process sees them.

use std::process::Command;

fn lodwright(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lodwright"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn version_exits_zero() {
    let out = lodwright(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("lodwright ")
    );
}

#[test]
fn help_exits_zero() {
    assert_eq!(lodwright(&["--help"]).status.code(), Some(0));
}

#[test]
fn doctor_on_an_existing_folder_exits_zero() {
    let dir = env!("CARGO_TARGET_TMPDIR");
    assert_eq!(lodwright(&["doctor", dir]).status.code(), Some(0));
}

#[test]
fn doctor_on_a_missing_folder_exits_one() {
    let missing = format!("{}/missing", env!("CARGO_TARGET_TMPDIR"));
    assert_eq!(lodwright(&["doctor", &missing]).status.code(), Some(1));
}

#[test]
fn unknown_command_exits_two() {
    assert_eq!(lodwright(&["nope"]).status.code(), Some(2));
}

#[test]
fn contracts_json_lists_each_contract_with_its_versions() {
    let out = lodwright(&["contracts", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "{\"contracts\":[{\"name\":\"lodwright-plan\",\"versions\":[1]},{\"name\":\"lodwright-result\",\"versions\":[1]},{\"name\":\"lodwright-request\",\"versions\":[1]},{\"name\":\"lodwright-manifest\",\"versions\":[1]},{\"name\":\"lodwright-verify\",\"versions\":[1]},{\"name\":\"lodwright-events\",\"versions\":[1]},{\"name\":\"lodwright-api\",\"versions\":[1]}]}\n"
    );
}

#[test]
fn version_json_lists_the_same_contracts_as_the_handshake() {
    let version = lodwright(&["--version", "--json"]);
    let handshake = lodwright(&["contracts", "--json"]);
    let version: serde_json::Value = serde_json::from_slice(&version.stdout).unwrap();
    let handshake: serde_json::Value = serde_json::from_slice(&handshake.stdout).unwrap();
    assert_eq!(version["contracts"], handshake["contracts"]);
}

#[test]
fn schema_prints_the_committed_schema_of_every_listed_document() {
    for kind in ["plan", "result", "request", "manifest", "verify", "events"] {
        let out = lodwright(&["schema", kind]);
        assert_eq!(out.status.code(), Some(0), "{kind}");
        let committed = std::fs::read_to_string(format!(
            "{}/../../schemas/lodwright-{kind}.v1.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert_eq!(String::from_utf8(out.stdout).unwrap(), committed, "{kind}");
    }
}

#[test]
fn contracts_without_json_is_a_usage_error() {
    assert_eq!(lodwright(&["contracts"]).status.code(), Some(2));
}

/// A fresh empty folder outside any git working tree.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lodwright-cli-verify-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn path(p: &std::path::Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn verify_of_a_clean_folder_exits_zero() {
    let dir = scratch("clean");
    let out = lodwright(&["verify", path(&dir)]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn verify_with_an_error_in_the_folder_exits_one() {
    let dir = scratch("error");
    std::fs::create_dir_all(dir.join("lodsettings")).unwrap();
    std::fs::write(dir.join("lodsettings/w.dlodsettings"), b"short").unwrap();
    let out = lodwright(&["verify", path(&dir)]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn verify_with_a_difference_from_the_reference_exits_one() {
    let dir = scratch("differs");
    let reference = scratch("differs-reference");
    std::fs::write(reference.join("only-here.txt"), b"x").unwrap();
    let out = lodwright(&["verify", path(&dir), "--reference", path(&reference), "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["ok"], true, "no error in the folder itself");
    assert_eq!(report["differences"][0]["path"], "only-here.txt");
}

#[test]
fn verify_against_an_equal_reference_exits_zero() {
    let dir = scratch("equal");
    let reference = scratch("equal-reference");
    let out = lodwright(&["verify", path(&dir), "--reference", path(&reference)]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn verify_without_a_folder_or_with_an_unknown_option_exits_two() {
    assert_eq!(lodwright(&["verify"]).status.code(), Some(2));
    let dir = scratch("usage");
    assert_eq!(
        lodwright(&["verify", path(&dir), "--frobnicate"]).status.code(),
        Some(2)
    );
    assert_eq!(
        lodwright(&["verify", path(&dir), "--reference"]).status.code(),
        Some(2)
    );
}

#[test]
fn verify_of_a_missing_folder_exits_three() {
    let missing = scratch("missing").join("not-here");
    let out = lodwright(&["verify", path(&missing)]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(out.stdout, b"");
}

#[test]
fn verify_of_a_file_instead_of_a_folder_exits_three() {
    let file = scratch("file").join("a.txt");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(lodwright(&["verify", path(&file)]).status.code(), Some(3));
}

#[test]
fn verify_against_a_missing_reference_exits_three() {
    let dir = scratch("no-reference");
    let missing = dir.join("not-here");
    let out = lodwright(&["verify", path(&dir), "--reference", path(&missing)]);
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn help_documents_the_exit_codes_of_verify() {
    let out = String::from_utf8(lodwright(&["--help"]).stdout).unwrap();
    assert!(
        out.contains("verify exits 0 clean, 1 findings, 2 usage error, 3 unreadable folder"),
        "{out}"
    );
}
