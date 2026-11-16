// SPDX-License-Identifier: GPL-3.0-only
//! Fuzz harness for `lodwright probe dds`. The only property: arbitrary bytes never panic, and a report never holds
//! a byte of the input beyond the documented numbers (the report type cannot carry free text).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(report) = lodwright_core::probe::dds::probe(data) {
        let _ = report.to_json();
        let _ = report.to_text();
    }
});
