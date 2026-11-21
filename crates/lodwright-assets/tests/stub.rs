// SPDX-License-Identifier: GPL-3.0-only
//! The stub plugin against a hex-text golden.

use std::fmt::Write as _;
use std::path::PathBuf;

use lodwright_assets::stub::stub_plugin;
use lodwright_testkit::golden::assert_golden;

#[test]
fn the_stub_plugin_matches_its_hex_golden() {
    let mut text = String::new();
    for line in stub_plugin().chunks(16) {
        for byte in line {
            write!(text, "{byte:02x}").unwrap();
        }
        text.push('\n');
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/stub.hex");
    assert_golden(&path, &text);
}
