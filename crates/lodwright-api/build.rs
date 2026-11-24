// SPDX-License-Identifier: GPL-3.0-only
//! The generator's build identity (`LODWRIGHT_BUILD`): a hash over the sources of every crate that decides what a
//! run writes and over the lock file, so two builds of one version that would write different files tell each
//! other apart (#422). Works without a git checkout.

use std::path::{Path, PathBuf};

/// The crates whose code reaches the output; the command line and the test kit do not.
const CRATES: &[&str] = &[
    "lodwright-api",
    "lodwright-assets",
    "lodwright-contract",
    "lodwright-core",
    "lodwright-dds",
    "lodwright-gen",
    "lodwright-nif",
    "lodwright-plugin-meta",
];

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// FNV-1a, 64 bits: an identity, not a security check.
fn fnv(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash ^= u64::from(*b);
        *hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
}

fn main() {
    let here = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let crates = here.parent().expect("the crate sits in crates/");
    let root = crates.parent().expect("crates/ sits in the workspace");
    let mut all = Vec::new();
    for name in CRATES {
        let dir = crates.join(name);
        println!("cargo:rerun-if-changed={}", dir.join("src").display());
        println!(
            "cargo:rerun-if-changed={}",
            dir.join("Cargo.toml").display()
        );
        files(&dir.join("src"), &mut all);
        all.push(dir.join("Cargo.toml"));
    }
    let lock = root.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    all.push(lock);
    let mut keyed: Vec<(String, PathBuf)> = all
        .into_iter()
        .filter(|p| p.is_file())
        .map(|p| {
            let key = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            (key, p)
        })
        .collect();
    keyed.sort();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for (key, path) in &keyed {
        fnv(&mut hash, key.as_bytes());
        fnv(&mut hash, &[0]);
        // line endings are not part of the identity: a checkout on Windows gives the same build
        let text = std::fs::read(path).unwrap_or_default();
        let text: Vec<u8> = text.into_iter().filter(|b| *b != b'\r').collect();
        fnv(&mut hash, &text);
        fnv(&mut hash, &[0]);
    }
    println!(
        "cargo:rustc-env=LODWRIGHT_BUILD={}+{hash:016x}",
        std::env::var("CARGO_PKG_VERSION").unwrap_or_default()
    );
}
