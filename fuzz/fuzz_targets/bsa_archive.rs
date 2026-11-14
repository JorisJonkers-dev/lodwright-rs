// SPDX-License-Identifier: MPL-2.0
//! Fuzz harness for the BSA v104 reader. The only property: arbitrary bytes never panic, and an extracted entry
//! never exceeds the cap.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

const CAP: u64 = 1 << 16;

fuzz_target!(|data: &[u8]| {
    let mut cursor = Cursor::new(data);
    if let Ok(archive) = lodwright_assets::Archive::open(&mut cursor) {
        for entry in archive.entries().iter().take(64) {
            if let Ok(bytes) = archive.extract(&mut cursor, entry, CAP) {
                assert!(bytes.len() as u64 <= CAP);
            }
        }
    }
});
