// SPDX-License-Identifier: GPL-3.0-only
//! `cargo xtask release-assets <out-dir>`: build the release assets of the Linux `x86_64` binary into one directory.
//!
//! - `lodwright-x86_64-unknown-linux-gnu`: the binary;
//! - `lodwright-<document>.v<N>.json`: the contract schemas, copied from `schemas/` after checking that the binary
//!   prints exactly the same text;
//! - `SHA256SUMS`: the SHA-256 of every file above, in the `sha256sum` format, sorted by name.
//!
//! The attestation is made by the release workflow over this directory.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// Name of the binary in the release.
pub const BINARY_ASSET: &str = "lodwright-x86_64-unknown-linux-gnu";

/// A committed schema file name split into its document kind and version: `lodwright-request.v1.json` gives
/// (`request`, `1`).
#[must_use]
pub fn schema_kind_and_version(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("lodwright-")?.strip_suffix(".json")?;
    let (kind, version) = rest.split_once(".v")?;
    if kind.is_empty() || version.is_empty() || kind.contains('.') {
        return None;
    }
    Some((kind, version))
}

/// The `SHA256SUMS` text for the files in `dir`, sorted by name, as `sha256sum` writes it.
///
/// # Errors
/// A message when the directory or a file cannot be read.
pub fn sums(dir: &Path) -> Result<String, String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot list {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    let mut out = String::new();
    for name in names {
        let bytes =
            std::fs::read(dir.join(&name)).map_err(|e| format!("cannot read {name}: {e}"))?;
        let digest = Sha256::digest(&bytes);
        let hex = digest.iter().fold(String::new(), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        });
        let _ = writeln!(out, "{hex}  {name}");
    }
    Ok(out)
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// The macOS asset (arm64 and x86_64 in one file), built on macOS and passed in as a directory.
pub const MACOS_ASSET: &str = "lodwright-universal-apple-darwin";

/// Copy the macOS binary from `dir` into `out` as an executable release asset.
///
/// # Errors
/// A message when the file is missing or cannot be copied.
pub fn add_macos(out: &Path, dir: &Path) -> Result<(), String> {
    let source = dir.join(MACOS_ASSET);
    if !source.is_file() {
        return Err(format!("release-assets: {} is missing", source.display()));
    }
    let target = out.join(MACOS_ASSET);
    std::fs::copy(&source, &target)
        .map_err(|e| format!("release-assets: cannot copy the macOS binary: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("release-assets: cannot make the macOS binary executable: {e}"))?;
    }
    Ok(())
}

/// Build the assets into `out`, which must be missing or empty.
///
/// # Errors
/// A message naming the step that failed; nothing is published.
pub fn build(out: &Path, macos: Option<&Path>) -> Result<(), String> {
    if out.exists()
        && std::fs::read_dir(out)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err(format!(
            "release-assets: {} exists and is not empty",
            out.display()
        ));
    }
    std::fs::create_dir_all(out)
        .map_err(|e| format!("release-assets: cannot create {}: {e}", out.display()))?;
    let root = root();
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(&root)
        .args(["build", "--release", "--locked", "-p", "lodwright-cli"])
        .status()
        .map_err(|e| format!("release-assets: cannot run cargo: {e}"))?;
    if !status.success() {
        return Err("release-assets: cargo build failed".into());
    }
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let binary = target.join("release").join("lodwright");
    std::fs::copy(&binary, out.join(BINARY_ASSET))
        .map_err(|e| format!("release-assets: cannot copy the binary: {e}"))?;

    let mut schemas: Vec<PathBuf> = std::fs::read_dir(root.join("schemas"))
        .map_err(|e| format!("release-assets: cannot list schemas/: {e}"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .and_then(schema_kind_and_version)
                .is_some()
        })
        .collect();
    schemas.sort();
    if schemas.is_empty() {
        return Err("release-assets: no schema under schemas/".into());
    }
    for file in schemas {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let (kind, version) = schema_kind_and_version(&name).unwrap_or_default();
        let printed = Command::new(&binary)
            .args(["schema", kind, "--schema-version", version])
            .output()
            .map_err(|e| format!("release-assets: cannot run the binary: {e}"))?;
        let committed =
            std::fs::read(&file).map_err(|e| format!("release-assets: cannot read {name}: {e}"))?;
        if !printed.status.success() || printed.stdout != committed {
            return Err(format!(
                "release-assets: schemas/{name} differs from 'lodwright schema {kind} --schema-version {version}'"
            ));
        }
        std::fs::write(out.join(&name), committed)
            .map_err(|e| format!("release-assets: cannot write {name}: {e}"))?;
    }
    if let Some(dir) = macos {
        add_macos(out, dir)?;
    }
    let text = sums(out)?;
    std::fs::write(out.join("SHA256SUMS"), text)
        .map_err(|e| format!("release-assets: cannot write SHA256SUMS: {e}"))?;
    for name in std::fs::read_dir(out)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
    {
        println!("{}", name.file_name().to_string_lossy());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{schema_kind_and_version, sums};

    #[test]
    fn schema_names_give_their_kind_and_version() {
        assert_eq!(
            schema_kind_and_version("lodwright-request.v1.json"),
            Some(("request", "1"))
        );
        assert_eq!(
            schema_kind_and_version("lodwright-plan.v12.json"),
            Some(("plan", "12"))
        );
        assert_eq!(schema_kind_and_version("other.v1.json"), None);
        assert_eq!(schema_kind_and_version("lodwright-request.json"), None);
    }

    #[test]
    fn the_macos_binary_is_copied_and_a_missing_one_is_an_error() {
        let base = std::env::temp_dir().join(format!("xtask-macos-{}", std::process::id()));
        let (out, mac) = (base.join("out"), base.join("mac"));
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(&mac).unwrap();
        let error = add_macos(&out, &mac).unwrap_err();
        assert!(error.contains("is missing"), "{error}");
        std::fs::write(mac.join(MACOS_ASSET), b"binary").unwrap();
        add_macos(&out, &mac).unwrap();
        assert_eq!(std::fs::read(out.join(MACOS_ASSET)).unwrap(), b"binary");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn sums_are_sorted_and_in_the_sha256sum_format() {
        let dir = std::env::temp_dir().join(format!("xtask-sums-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b"), b"").unwrap();
        std::fs::write(dir.join("a"), b"abc").unwrap();
        let text = sums(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            text,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  a\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  b\n"
        );
    }
}
