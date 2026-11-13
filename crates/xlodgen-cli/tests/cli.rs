// SPDX-License-Identifier: MPL-2.0
//! The binary itself: exit codes as a process sees them.

use std::process::Command;

fn xlodgen(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xlodgen"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn version_exits_zero() {
    let out = xlodgen(&["--version"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("xlodgen ")
    );
}

#[test]
fn help_exits_zero() {
    assert_eq!(xlodgen(&["--help"]).status.code(), Some(0));
}

#[test]
fn doctor_on_an_existing_folder_exits_zero() {
    let dir = env!("CARGO_TARGET_TMPDIR");
    assert_eq!(xlodgen(&["doctor", dir]).status.code(), Some(0));
}

#[test]
fn doctor_on_a_missing_folder_exits_one() {
    let missing = format!("{}/missing", env!("CARGO_TARGET_TMPDIR"));
    assert_eq!(xlodgen(&["doctor", &missing]).status.code(), Some(1));
}

#[test]
fn unknown_command_exits_two() {
    assert_eq!(xlodgen(&["nope"]).status.code(), Some(2));
}
