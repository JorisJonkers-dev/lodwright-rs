// SPDX-License-Identifier: MPL-2.0
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
