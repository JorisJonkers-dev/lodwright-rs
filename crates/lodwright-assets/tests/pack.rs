// SPDX-License-Identifier: GPL-3.0-only
//! The BSA writer against a hex-text golden and the reader.

use std::fmt::Write as _;
use std::io::Cursor;
use std::path::PathBuf;

use lodwright_assets::{Archive, PackEntry, pack};
use lodwright_testkit::golden::assert_golden;

fn tiny() -> Vec<PackEntry> {
    vec![
        PackEntry {
            path: "Textures/Landscape/Lod/Tile.dds".into(),
            data: vec![0xAB; 64],
            compress: true,
        },
        PackEntry {
            path: "meshes\\landscape\\lod\\w\\blocks\\w.level4.x0.y0.nif".into(),
            data: b"mesh bytes".to_vec(),
            compress: false,
        },
        PackEntry {
            path: "lodsettings\\w.dlodsettings".into(),
            data: vec![1, 2, 3, 4],
            compress: false,
        },
    ]
}

fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for line in bytes.chunks(16) {
        for byte in line {
            write!(out, "{byte:02x}").unwrap();
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_tiny_archive_matches_its_hex_golden() {
    let bytes = pack::write(&tiny()).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/tiny.hex");
    assert_golden(&path, &hex_dump(&bytes));
}

#[test]
fn the_tiny_archive_reads_back_with_its_names_and_bytes() {
    let bytes = pack::write(&tiny()).unwrap();
    let mut cursor = Cursor::new(&bytes);
    let archive = Archive::open(&mut cursor).unwrap();
    let mut names: Vec<&str> = archive
        .entries()
        .iter()
        .map(lodwright_assets::Entry::path)
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "lodsettings\\w.dlodsettings",
            "meshes\\landscape\\lod\\w\\blocks\\w.level4.x0.y0.nif",
            "textures\\landscape\\lod\\tile.dds",
        ]
    );
    let tile = archive.find("textures/landscape/lod/tile.dds").unwrap();
    assert_eq!(
        archive.extract(&mut cursor, tile, 1 << 16).unwrap(),
        vec![0xAB; 64]
    );
}
