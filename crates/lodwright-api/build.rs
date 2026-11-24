// SPDX-License-Identifier: GPL-3.0-only
//! The generator's build identity (`LODWRIGHT_BUILD`): a hash over the sources of every crate that decides what a
//! run writes, this script and the dependency versions the build links, so two builds of one version that would
//! write different files tell each other apart (#422). Works without a git checkout.

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

/// The `Cargo.lock` of a workspace above `root` that holds lodwright as a member or path dependency, if any.
fn outer_lock(root: &Path) -> Option<PathBuf> {
    let mut dir = root.parent();
    while let Some(d) = dir {
        let lock = d.join("Cargo.lock");
        if lock.is_file() && d.join("Cargo.toml").is_file() {
            return Some(lock);
        }
        dir = d.parent();
    }
    None
}

/// The `(name, version)` of every package of a lock file, sorted.
fn packages(lock: &Path) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(lock).unwrap_or_default();
    let mut out = Vec::new();
    let mut name = None;
    for line in text.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            name = None;
        } else if let Some(v) = line.strip_prefix("name = ") {
            name = Some(v.trim_matches('"').to_owned());
        } else if let (Some(v), Some(n)) = (line.strip_prefix("version = "), name.take()) {
            out.push((n, v.trim_matches('"').to_owned()));
        }
    }
    out.sort();
    out.dedup();
    out
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
    // the build script itself decides the identity
    all.push(here.join("build.rs"));
    let own_lock = root.join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", own_lock.display());
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
    // The dependency versions this build links: from the lock of the workspace that builds it (an outer workspace
    // when lodwright is a path dependency or submodule, else its own), for the packages lodwright's own lock names.
    // Other packages of an outer lock do not reach lodwright, so their bumps do not change the identity.
    let used = outer_lock(root).unwrap_or_else(|| own_lock.clone());
    println!("cargo:rerun-if-changed={}", used.display());
    let ours: std::collections::BTreeSet<String> =
        packages(&own_lock).into_iter().map(|(n, _)| n).collect();
    for (name, version) in packages(&used) {
        if ours.contains(&name) && !name.starts_with("lodwright") {
            fnv(&mut hash, name.as_bytes());
            fnv(&mut hash, b" ");
            fnv(&mut hash, version.as_bytes());
            fnv(&mut hash, &[0]);
        }
    }
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
