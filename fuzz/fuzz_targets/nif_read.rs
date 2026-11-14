// SPDX-License-Identifier: MPL-2.0
//! Fuzz harness for the NIF reader: arbitrary bytes never panic it, and whatever it accepts and can write again
//! reaches a fixed point (the second read-write gives the first write's bytes).

#![no_main]

use libfuzzer_sys::fuzz_target;
use lodwright_nif::NifFile;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = NifFile::read(data)
        && let Ok(bytes) = file.write()
    {
        let again = NifFile::read(&bytes).expect("a written file reads back");
        assert_eq!(again.write().expect("and writes again"), bytes);
    }
});
