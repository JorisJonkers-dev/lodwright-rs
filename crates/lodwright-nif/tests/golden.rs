// SPDX-License-Identifier: MPL-2.0
//! Golden hex dump of the writer output for one fixed synthetic input.

mod common;

use lodwright_nif::NifFile;
use lodwright_testkit::golden::assert_golden;
use lodwright_testkit::nif::ShapeSpec;
use std::fmt::Write as _;
use std::path::Path;

fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let _ = write!(out, "{:08x} ", i * 16);
        for b in chunk {
            let _ = write!(out, " {b:02x}");
        }
        out.push('\n');
    }
    out
}

fn fixed_input() -> Vec<ShapeSpec> {
    vec![
        ShapeSpec {
            name: "tri".to_owned(),
            translation: [1.0, 2.0, 3.0],
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Some(vec![[0.0, 0.0, 1.0]; 3]),
            uvs: Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]),
            triangles: vec![[0, 1, 2]],
            alpha: Some((4844, 128)),
        },
        ShapeSpec {
            name: "bare".to_owned(),
            translation: [0.0; 3],
            vertices: vec![[0.5, 0.5, 0.5]; 3],
            normals: None,
            uvs: None,
            triangles: vec![],
            alpha: None,
        },
    ]
}

#[test]
fn the_writer_output_for_a_fixed_input_equals_the_stored_hex_dump() {
    let file = common::build(&fixed_input());
    let bytes = file.write().unwrap();
    assert_eq!(NifFile::read(&bytes).unwrap(), file);
    assert_golden(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/two_shapes.hex.txt"),
        &hex_dump(&bytes),
    );
}
