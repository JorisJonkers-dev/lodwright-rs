// SPDX-License-Identifier: GPL-3.0-only
//! Fuzz harness for the DDS header reader and decoder: arbitrary bytes never panic either, a header the reader
//! accepts always describes a data run that fits in the input, and decoding does no more work than that.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = lodwright_testkit::dds::parse_header(data);
    if let Ok(header) = lodwright_dds::read_header(data) {
        assert!(data.len() >= lodwright_dds::HEADER_LEN + header.data_bytes());
        let decoded = lodwright_dds::decode(data).expect("a header the reader accepts decodes");
        assert_eq!(decoded.levels.len(), header.mip_count as usize);
    }
});
