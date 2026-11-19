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
