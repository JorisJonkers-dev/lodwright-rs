// SPDX-License-Identifier: GPL-3.0-only
//! The streamed BSA writer holds one file at a time: packing 128 MiB of files from disk keeps the peak resident
//! size of this test process far below the size of the files (the in-memory writer needs the files and the whole
//! archive at once, more than 256 MiB here). One test in its own file, so its process measures only this.

#![cfg(target_os = "linux")]

use std::io::Write as _;

use lodwright_assets::pack::{self, PackSource};

const FILES: usize = 128;
const FILE_LEN: usize = 1 << 20;
const LIMIT_MIB: u64 = 64;

fn peak_mib() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let line = status.lines().find(|l| l.starts_with("VmHWM:")).unwrap();
    let kib: u64 = line
        .split_whitespace()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap();
    kib / 1024
}

#[test]
fn packing_streams_the_files_and_stays_far_below_their_size() {
    let dir = std::env::temp_dir().join(format!("lodwright-pack-memory-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Half the files are compressed (texture-like), half stored; the bytes differ per file so nothing is shared.
    let mut buf = vec![0_u8; FILE_LEN];
    let mut sources = Vec::with_capacity(FILES);
    for i in 0..FILES {
        let mut state = 0x9E37_79B9_u32.wrapping_add(u32::try_from(i).unwrap());
        for b in &mut buf {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *b = state.to_be_bytes()[0];
        }
        let name = format!("f{i:03}.dds");
        std::fs::write(dir.join(&name), &buf).unwrap();
        sources.push(PackSource {
            path: format!("textures/landscape/lod/w/blocks/{name}"),
            len: FILE_LEN as u64,
            compress: i % 2 == 0,
        });
    }
    drop(buf);
    let before = peak_mib();
    let mut spool = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(dir.join("spool"))
        .unwrap();
    let mut out = std::io::BufWriter::new(std::fs::File::create(dir.join("out.bsa")).unwrap());
    let mut read = |at: usize| std::fs::read(dir.join(format!("f{at:03}.dds")));
    let len = pack::write_streamed(&sources, &mut read, &mut spool, &mut out).unwrap();
    out.flush().unwrap();
    drop(out);
    let after = peak_mib();
    assert_eq!(std::fs::metadata(dir.join("out.bsa")).unwrap().len(), len);
    assert!(len > (FILES * FILE_LEN) as u64 / 2, "{len}");
    std::fs::remove_dir_all(&dir).unwrap();
    eprintln!("peak resident size: {before} MiB before packing, {after} MiB after");
    assert!(
        after < LIMIT_MIB,
        "packing {} MiB of files raised the peak to {after} MiB (limit {LIMIT_MIB})",
        (FILES * FILE_LEN) >> 20
    );
}
