// SPDX-License-Identifier: GPL-3.0-only
//! Fuzz harness for `lodwright probe settings` and `lodwright probe dtl`, which share one reader. The only property:
//! arbitrary bytes and sizes never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let size = u64::try_from(data.len()).unwrap_or(u64::MAX);
    if let Ok(report) = lodwright_core::probe::flat::probe(size, data) {
        let _ = report.to_json();
        let _ = report.to_text();
    }
});
