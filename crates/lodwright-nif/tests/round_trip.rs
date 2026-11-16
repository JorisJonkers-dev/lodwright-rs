// SPDX-License-Identifier: GPL-3.0-only
//! Round trips: `read(write(x)) == x` over generated shape sets, and byte-exact carriage of unknown blocks.

mod common;

use common::build;
use lodwright_nif::{Block, Body, NifError, NifFile};
use lodwright_testkit::nif::shape_set;
use lodwright_testkit::rng::Rng;

#[test]
fn reading_what_was_written_gives_the_same_file_for_generated_shape_sets() {
    let mut rng = Rng::new(0x5EED);
    for _ in 0..300 {
        let file = build(&shape_set(&mut rng, 6));
        let bytes = file.write().unwrap();
        assert_eq!(NifFile::read(&bytes).unwrap(), file);
    }
}

#[test]
fn writing_what_was_read_gives_the_same_bytes_for_generated_shape_sets() {
    let mut rng = Rng::new(0xB17E5);
    for _ in 0..100 {
        let bytes = build(&shape_set(&mut rng, 6)).write().unwrap();
        assert_eq!(NifFile::read(&bytes).unwrap().write().unwrap(), bytes);
    }
}

#[test]
fn unknown_blocks_round_trip_byte_for_byte() {
    let mut rng = Rng::new(11);
    for round in 0..50_u32 {
        let mut file = build(&shape_set(&mut rng, 3));
        let blob: Vec<u8> = (0..rng.below(40))
            .map(|_| u8::try_from(rng.below(256)).unwrap())
            .collect();
        file.header
            .block_types
            .push(format!("SyntheticBlock{round}").into_bytes());
        let type_index = u16::try_from(file.header.block_types.len() - 1).unwrap();
        file.blocks.push(Block {
            type_index,
            body: Body::Opaque(blob.clone()),
        });
        let bytes = file.write().unwrap();
        let back = NifFile::read(&bytes).unwrap();
        assert_eq!(back.blocks.last().unwrap().body, Body::Opaque(blob));
        assert_eq!(back.write().unwrap(), bytes);
    }
}

#[test]
fn a_block_whose_typed_layout_is_shorter_than_its_declared_size_is_refused() {
    let mut rng = Rng::new(5);
    let file = build(&shape_set(&mut rng, 2));
    // Append a stray byte to the last typed block by writing it as opaque under a typed name is not possible
    // through the API, so patch the bytes: grow the final block's size and insert a byte after its body.
    let mut bytes = file.write().unwrap();
    let footer = 4 + 4 * file.roots.len();
    let at = bytes.len() - footer;
    bytes.insert(at, 0);
    let last = file.blocks.len() - 1;
    let sizes_at = find_sizes(&file, &bytes);
    let p = sizes_at + 4 * last;
    let old = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
    bytes[p..p + 4].copy_from_slice(&(old + 1).to_le_bytes());
    assert!(matches!(
        NifFile::read(&bytes),
        Err(NifError::BlockSizeMismatch { .. })
    ));
}

/// Offset of the block size array in bytes written for `file` (header layout of the fixed synthetic header).
fn find_sizes(file: &NifFile, bytes: &[u8]) -> usize {
    let h = &file.header;
    let mut at = 38 + 1 + 4 + 1 + 4 + 4; // magic line, version, endian, user version, block count
    at += 4 + 1 + h.bethesda.as_ref().unwrap().1.author.len(); // user version 2, author
    at += 2 + 2; // process script and export script (length byte and one NUL each)
    at += 2 + h.block_types.iter().map(|t| 4 + t.len()).sum::<usize>();
    at += 2 * file.blocks.len();
    assert!(at < bytes.len());
    at
}
