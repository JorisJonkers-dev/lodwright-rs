// SPDX-License-Identifier: MPL-2.0
//! Fuzz harness for the plugin (TES4 header and records) reader.
//!
//! The reader does not exist in `lodwright-core` yet, so this runs the testkit's own reference reader (or nothing
//! where the testkit has none). When the real reader lands, replace the call below with it; the target already
//! compiles and runs in the weekly fuzz workflow. The only property: arbitrary bytes never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = lodwright_testkit::plugin::parse_records(data);
});
