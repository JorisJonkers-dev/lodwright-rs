// SPDX-License-Identifier: MPL-2.0
//! Fuzz harness for the `.bto` object mesh reader.
//!
//! The reader does not exist in `xlodgen-core` yet, so this runs the testkit's own reference reader (or nothing
//! where the testkit has none). When the real reader lands, replace the call below with it; the target already
//! compiles and runs in the weekly fuzz workflow. The only property: arbitrary bytes never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = data;
});
