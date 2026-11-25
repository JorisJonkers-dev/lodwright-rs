// SPDX-License-Identifier: GPL-3.0-only
//! The stub plugin against a hex-text golden.

use lodwright_testkit::{covers, ids};
use std::fmt::Write as _;
use std::path::PathBuf;

use lodwright_assets::stub::stub_plugin;
use lodwright_testkit::golden::assert_golden;

#[test]
fn the_stub_plugin_matches_its_hex_golden() {
    covers(&[ids::formats::plugins::STUB_PLUGIN]);
    let mut text = String::new();
    for line in stub_plugin("FalloutNV.esm", 14_000_000).chunks(16) {
        for byte in line {
            write!(text, "{byte:02x}").unwrap();
        }
        text.push('\n');
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/stub.hex");
    assert_golden(&path, &text);
}
