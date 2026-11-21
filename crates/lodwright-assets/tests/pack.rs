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

#[test]
fn damaged_writer_output_never_panics_the_reader_nor_exceeds_the_cap() {
    // A seeded pseudo-random walk over what the fuzz target `bsa_writer` explores: byte overwrites of an
    // archive the writer made (stored and compressed files), then open and extract everything.
    let clean = pack::write(&tiny()).unwrap();
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let cap = 1_u64 << 16;
    let mut opened = 0;
    for _ in 0..4000 {
        let mut bytes = clean.clone();
        for _ in 0..=(next() % 4) {
            let at = usize::try_from(next() % bytes.len() as u64).unwrap();
            bytes[at] = u8::try_from(next() & 0xFF).unwrap();
        }
        let mut cursor = Cursor::new(&bytes);
        if let Ok(archive) = Archive::open(&mut cursor) {
            opened += 1;
            for entry in archive.entries() {
                if let Ok(out) = archive.extract(&mut cursor, entry, cap) {
                    assert!(out.len() as u64 <= cap);
                }
            }
        }
    }
    assert!(
        opened > 0,
        "some damaged archives still open, so the extract path ran"
    );
}
