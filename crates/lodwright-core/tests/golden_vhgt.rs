// SPDX-License-Identifier: GPL-3.0-only
//! Golden test: a fixed synthetic hill decoded from VHGT bytes equals the stored text rendering.

use lodwright_core::land::{SIDE, decode_vhgt};
use lodwright_testkit::golden::assert_golden;
use lodwright_testkit::land::{Heights, encode_vhgt};
use std::path::Path;

#[test]
fn a_synthetic_hill_decodes_to_the_stored_heights() {
    let hill = Heights::from_fn(|x, y| {
        let dx = f32::from(u8::try_from(x).unwrap()) - 16.0;
        let dy = f32::from(u8::try_from(y).unwrap()) - 16.0;
        8.0 * (40.0 - (dx * dx + dy * dy).sqrt().round() * 2.0).max(0.0)
    });
    let cell = decode_vhgt(&encode_vhgt(&hill)).unwrap();
    let mut text = String::new();
    for y in 0..SIDE {
        let row: Vec<String> = (0..SIDE).map(|x| format!("{}", cell.get(x, y))).collect();
        text.push_str(&row.join(" "));
        text.push('\n');
    }
    assert_golden(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/vhgt_hill.txt"),
        &text,
    );
}
