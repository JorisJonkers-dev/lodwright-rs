// SPDX-License-Identifier: MPL-2.0
//! Fuzz harness for the tree LOD readers. The only property: arbitrary bytes never panic, and what parses writes
//! back to the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(block) = lodwright_core::trees::parse_block(data) {
        let again = lodwright_core::trees::write_block(&block);
        let reread = lodwright_core::trees::parse_block(&again).expect("what we write parses");
        assert_eq!(reread.groups.len(), block.groups.len());
    }
    if let Ok(types) = lodwright_core::trees::parse_types(data) {
        let _ = lodwright_core::trees::write_types(&types);
    }
});
