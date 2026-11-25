// SPDX-License-Identifier: GPL-3.0-only
//! Hostile and malformed input: typed errors, never a panic.

mod common;

use common::build;
use lodwright_nif::{Block, Body, NifError, NifFile, names};
use lodwright_testkit::nif::shape_set;
use lodwright_testkit::rng::Rng;
use lodwright_testkit::{covers, ids};

fn sample() -> (NifFile, Vec<u8>) {
    let mut rng = Rng::new(21);
    let file = loop {
        let specs = shape_set(&mut rng, 3);
        if specs.len() >= 2 {
            break build(&specs);
        }
    };
    let bytes = file.write().unwrap();
    (file, bytes)
}

#[test]
fn every_truncation_of_a_valid_file_is_an_error_and_never_a_panic() {
    covers(&[ids::formats::meshes_and_images::NIF_HOSTILE_INPUT]);
    let (_, bytes) = sample();
    for len in 0..bytes.len() {
        assert!(
            NifFile::read(&bytes[..len]).is_err(),
            "prefix of {len} bytes was accepted"
        );
    }
}

#[test]
fn a_file_with_bytes_after_the_footer_is_refused() {
    let (_, mut bytes) = sample();
    bytes.push(0);
    assert_eq!(NifFile::read(&bytes), Err(NifError::TrailingBytes(1)));
}

#[test]
fn a_reader_that_ignores_the_tail_reads_a_file_with_bytes_after_the_footer_as_the_file_without_them()
 {
    let (file, mut bytes) = sample();
    bytes.extend_from_slice(&[7; 2696]);
    assert_eq!(NifFile::read_ignoring_trailing_bytes(&bytes), Ok(file));
    // truncation is still an error
    assert!(NifFile::read_ignoring_trailing_bytes(&bytes[..40]).is_err());
}

#[test]
fn another_header_string_is_refused() {
    let (_, mut bytes) = sample();
    bytes[0] = b'g';
    assert_eq!(NifFile::read(&bytes), Err(NifError::BadMagic));
    assert_eq!(NifFile::read(b""), Err(NifError::BadMagic));
    assert_eq!(NifFile::read(&[b'x'; 200]), Err(NifError::BadMagic));
}

#[test]
fn another_version_is_refused_with_its_number() {
    covers(&[ids::formats::meshes_and_images::NIF_HOSTILE_INPUT]);
    let (_, mut bytes) = sample();
    bytes[39..43].copy_from_slice(&0x1401_0003_u32.to_le_bytes());
    assert_eq!(
        NifFile::read(&bytes),
        Err(NifError::UnsupportedVersion(0x1401_0003))
    );
}

#[test]
fn a_big_endian_marker_is_refused() {
    let (_, mut bytes) = sample();
    bytes[43] = 0;
    assert_eq!(NifFile::read(&bytes), Err(NifError::UnsupportedEndian(0)));
}

#[test]
fn a_block_count_larger_than_the_file_is_a_bad_count() {
    let (_, mut bytes) = sample();
    bytes[48..52].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        NifFile::read(&bytes),
        Err(NifError::BadCount { what: "blocks", .. })
    ));
}

#[test]
fn a_block_type_index_past_the_table_is_refused() {
    let (file, mut bytes) = sample();
    // type indices follow the block type table; find the first one by writing the table length back out.
    let table: usize = file.header.block_types.iter().map(|t| 4 + t.len()).sum();
    let author = file.header.bethesda.as_ref().unwrap().1.author.len();
    let at = 52 + 4 + 1 + author + 4 + 2 + table;
    bytes[at..at + 2].copy_from_slice(&200_u16.to_le_bytes());
    assert_eq!(
        NifFile::read(&bytes),
        Err(NifError::BadBlockType {
            block: 0,
            index: 200
        })
    );
}

#[test]
fn block_sizes_running_past_the_file_are_refused() {
    let (file, mut bytes) = sample();
    let table: usize = file.header.block_types.iter().map(|t| 4 + t.len()).sum();
    let author = file.header.bethesda.as_ref().unwrap().1.author.len();
    let at = 52 + 4 + 1 + author + 4 + 2 + table + 2 * file.blocks.len();
    bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(NifFile::read(&bytes), Err(NifError::BlocksOverrun));
}

#[test]
fn a_huge_string_count_does_not_allocate_and_is_a_bad_count() {
    covers(&[ids::formats::meshes_and_images::NIF_HOSTILE_INPUT]);
    let (file, mut bytes) = sample();
    let table: usize = file.header.block_types.iter().map(|t| 4 + t.len()).sum();
    let author = file.header.bethesda.as_ref().unwrap().1.author.len();
    let at = 52 + 4 + 1 + author + 4 + 2 + table + 6 * file.blocks.len();
    bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        NifFile::read(&bytes),
        Err(NifError::BadCount {
            what: "strings",
            ..
        })
    ));
}

#[test]
fn random_bytes_never_panic_the_reader() {
    covers(&[ids::formats::meshes_and_images::NIF_HOSTILE_INPUT]);
    let mut rng = Rng::new(99);
    for _ in 0..2000 {
        let n = rng.below(300) as usize;
        let junk: Vec<u8> = (0..n)
            .map(|_| u8::try_from(rng.below(256)).unwrap())
            .collect();
        let _ = NifFile::read(&junk);
    }
}

#[test]
fn single_byte_flips_of_a_valid_file_never_panic_the_reader() {
    covers(&[ids::formats::meshes_and_images::NIF_HOSTILE_INPUT]);
    let (_, bytes) = sample();
    for i in 0..bytes.len() {
        for bit in [0x01_u8, 0x80] {
            let mut m = bytes.clone();
            m[i] ^= bit;
            let _ = NifFile::read(&m);
        }
    }
}

#[test]
fn a_type_index_outside_the_table_cannot_be_written() {
    let (mut file, _) = sample();
    file.blocks[0].type_index = 99;
    assert!(matches!(file.write(), Err(NifError::Inconsistent(_))));
}

#[test]
fn a_typed_block_under_another_type_name_cannot_be_written() {
    let (mut file, _) = sample();
    file.header.block_types[usize::from(file.blocks[0].type_index)] = b"NiSomethingElse".to_vec();
    assert!(matches!(file.write(), Err(NifError::Inconsistent(_))));
}

#[test]
fn a_vertex_count_that_disagrees_with_the_vertices_cannot_be_written() {
    covers(&[ids::formats::meshes_and_images::NIF_HOSTILE_INPUT]);
    let mut file = build(&shape_set(&mut Rng::new(2), 1));
    for b in &mut file.blocks {
        if let Body::TriShapeData(d) = &mut b.body {
            d.geometry.num_vertices += 1;
        }
    }
    if file.blocks.len() > 1 {
        assert!(matches!(file.write(), Err(NifError::Inconsistent(_))));
    }
}

#[test]
fn a_mismatched_triangle_point_count_survives_as_the_value_given() {
    // The count is carried, not derived: a consumer decides whether three times the triangles is enforced.
    let mut file = build(&shape_set(&mut Rng::new(4), 2));
    for b in &mut file.blocks {
        if let Body::TriShapeData(d) = &mut b.body {
            d.num_triangle_points = 7;
        }
    }
    let bytes = file.write().unwrap();
    assert_eq!(NifFile::read(&bytes).unwrap(), file);
}

#[test]
fn an_empty_block_table_with_no_blocks_round_trips() {
    let mut file = build(&[]);
    file.blocks.clear();
    file.roots.clear();
    file.header.block_types.clear();
    let bytes = file.write().unwrap();
    assert_eq!(NifFile::read(&bytes).unwrap(), file);
}

#[test]
fn a_block_of_a_known_type_with_too_few_bytes_is_truncated() {
    let mut file = build(&[]);
    file.header.block_types.push(names::TRI_SHAPE_DATA.to_vec());
    let type_index = u16::try_from(file.header.block_types.len() - 1).unwrap();
    file.blocks.push(Block {
        type_index,
        body: Body::Opaque(vec![0; 3]),
    });
    // Written under the typed name it is just bytes; reading decodes it by name and finds it short.
    let bytes = file.write().unwrap();
    assert!(matches!(
        NifFile::read(&bytes),
        Err(NifError::Truncated { .. })
    ));
}
