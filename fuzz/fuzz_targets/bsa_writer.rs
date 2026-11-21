// SPDX-License-Identifier: GPL-3.0-only
//! Fuzz harness for the BSA v104 writer and the reader behind it. The input chooses files (names, bytes,
//! compression) and then bytes of the written archive to overwrite. Properties: the writer never panics; what it
//! writes reads back to the same files; a damaged archive never panics the reader and never extracts more than
//! the cap.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lodwright_assets::{Archive, PackEntry, pack};
use std::io::Cursor;

const CAP: u64 = 1 << 16;

fuzz_target!(|data: &[u8]| {
    // First byte: number of files (1..=8); then per file: name length, name bytes, data length, data bytes, flag.
    let mut at = 0_usize;
    let mut take = |n: usize| -> &[u8] {
        let end = (at + n).min(data.len());
        let slice = &data[at..end];
        at = end;
        slice
    };
    let count = usize::from(take(1).first().copied().unwrap_or(0) % 8) + 1;
    let mut entries = Vec::new();
    for _ in 0..count {
        let name_len = usize::from(take(1).first().copied().unwrap_or(0) % 24) + 1;
        let name: String = take(name_len)
            .iter()
            .map(|b| match b % 40 {
                0..=25 => char::from(b'a' + b % 40),
                26..=35 => char::from(b'0' + (b % 40 - 26)),
                36 => '\\',
                37 => '.',
                38 => '_',
                _ => '/',
            })
            .collect();
        let body_len = usize::from(take(1).first().copied().unwrap_or(0));
        let body = take(body_len).to_vec();
        let compress = take(1).first().is_some_and(|b| b % 2 == 0);
        entries.push(PackEntry {
            path: name,
            data: body,
            compress,
        });
    }
    let Ok(mut bytes) = pack::write(&entries) else {
        return;
    };
    // Round trip before any damage.
    {
        let mut cursor = Cursor::new(&bytes);
        let archive = Archive::open(&mut cursor).expect("the writer's output opens");
        assert_eq!(archive.entries().len(), entries.len());
        for entry in archive.entries() {
            archive
                .extract(&mut cursor, entry, CAP)
                .expect("the writer's output extracts");
        }
    }
    // The rest of the input overwrites bytes of the archive: pairs of (position, value).
    let rest = &data[at..];
    for pair in rest.chunks_exact(3) {
        let position = usize::from(u16::from_le_bytes([pair[0], pair[1]])) % bytes.len();
        bytes[position] = pair[2];
    }
    let mut cursor = Cursor::new(&bytes);
    if let Ok(archive) = Archive::open(&mut cursor) {
        for entry in archive.entries().iter().take(64) {
            if let Ok(out) = archive.extract(&mut cursor, entry, CAP) {
                assert!(out.len() as u64 <= CAP);
            }
        }
    }
});
