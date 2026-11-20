// SPDX-License-Identifier: GPL-3.0-only
//! The staging folder of a [`generate`](crate::generate) run: where a run builds, how it is resumed and how it is
//! swapped into place. The contract is in `docs/interrupted-runs.md` and `docs/contracts.md`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lodwright_contract::{MANIFEST_FILE, Manifest, ManifestBlock, ManifestFile};

use crate::Error;

/// The checkpoint a run keeps in its staging folder; it is never part of the output.
pub(crate) const CHECKPOINT_FILE: &str = "checkpoint.json";

/// `<output>` with `suffix` appended to its last component: a sibling on the same file system.
pub(crate) fn sibling(out: &Path, suffix: &str) -> PathBuf {
    let mut name = out
        .file_name()
        .map_or_default(std::ffi::OsStr::to_os_string);
    name.push(suffix);
    out.with_file_name(name)
}

/// The staging folder of `out`.
pub(crate) fn staging_of(out: &Path) -> PathBuf {
    sibling(out, ".lodwright-staging")
}

/// The folder the previous output is moved to while the swap runs.
fn old_of(out: &Path) -> PathBuf {
    sibling(out, ".lodwright-old")
}

fn failed(what: &str, e: &std::io::Error) -> Error {
    Error::new("output_failed", what).with_detail(e.to_string())
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

/// Finish what a killed swap left: with no output folder but the previous one set aside, put it back (the next run
/// builds again or resumes staging); with both, the set-aside copy is a leftover of a finished swap.
pub(crate) fn recover_swap(out: &Path) -> Result<(), Error> {
    let old = old_of(out);
    if !exists(&old) {
        return Ok(());
    }
    if exists(out) {
        carry_over(&old, out);
        std::fs::remove_dir_all(&old)
            .map_err(|e| failed("cannot remove the previous output folder", &e))
    } else {
        std::fs::rename(&old, out)
            .map_err(|e| failed("cannot restore the previous output folder", &e))
    }
}

/// The manifest of `dir` named `file`, when it parses and lists only plain relative paths.
pub(crate) fn read_manifest_file(dir: &Path, file: &str) -> Option<Manifest> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join(file)).ok()?).ok()?;
    let plain = manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(
            manifest
                .blocks
                .iter()
                .flat_map(|b| b.files.iter().map(String::as_str)),
        )
        .all(lodwright_gen::is_plain_relative);
    plain.then_some(manifest)
}

/// The checkpoint in `stage` when it was written for this request and these inputs; any other staging folder (no
/// checkpoint, another request) is removed so the run starts clean.
pub(crate) fn resume_or_clear(
    stage: &Path,
    request_digest: &str,
    inputs_digest: &str,
) -> Result<Option<Manifest>, Error> {
    if !exists(stage) {
        return Ok(None);
    }
    let checkpoint = read_manifest_file(stage, CHECKPOINT_FILE)
        .filter(|c| c.request_digest == request_digest && c.inputs_digest == inputs_digest);
    if checkpoint.is_none() {
        std::fs::remove_dir_all(stage)
            .map_err(|e| failed("cannot remove the staging folder", &e))?;
    }
    Ok(checkpoint)
}

/// Write the checkpoint of a run: every file and block finished so far, with the request's own digest.
pub(crate) fn write_checkpoint(
    stage: &Path,
    version: &str,
    request_digest: &str,
    inputs_digest: &str,
    files: &[ManifestFile],
    blocks: &[ManifestBlock],
) -> Result<(), Error> {
    let mut files = files.to_vec();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    let mut blocks = blocks.to_vec();
    blocks.sort_by(|a, b| a.key.cmp(&b.key));
    blocks.dedup_by(|a, b| a.key == b.key);
    let manifest = Manifest::new(version, request_digest, inputs_digest, files).with_blocks(blocks);
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| {
        Error::new("output_failed", "cannot write the checkpoint").with_detail(e.to_string())
    })? + "\n";
    lodwright_gen::write_output_file(stage, CHECKPOINT_FILE, text.as_bytes())
        .map(|_| ())
        .map_err(|e| {
            Error::new("output_failed", "cannot write the checkpoint").with_detail(e.to_string())
        })
}

/// Make `rel` of `from` appear in `to`: a hard link, or a copy where links are not possible. `None` when neither works.
pub(crate) fn link_or_copy(from: &Path, to: &Path, rel: &str) -> Option<()> {
    if !lodwright_gen::is_plain_relative(rel) || lodwright_gen::linked_folder(to, rel).is_some() {
        return None;
    }
    let (src, dst) = (from.join(rel), to.join(rel));
    std::fs::create_dir_all(dst.parent()?).ok()?;
    if exists(&dst) {
        std::fs::remove_file(&dst).ok()?;
    }
    std::fs::hard_link(&src, &dst)
        .or_else(|_| std::fs::copy(&src, &dst).map(|_| ()))
        .ok()
}

fn walk(root: &Path, dir: &Path, keep: &BTreeSet<String>, remove_unlisted: bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(root, &path, keep, remove_unlisted);
            if remove_unlisted {
                let _ = std::fs::remove_dir(&path); // only when empty
            }
        } else if remove_unlisted {
            let rel = path
                .strip_prefix(root)
                .map_or_default(|p| p.to_string_lossy().replace('\\', "/"));
            if !keep.contains(&rel) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// Remove every file of `stage` that `listed` does not name (a file written after the last checkpoint, a file an
/// earlier run left) and the folders that become empty.
pub(crate) fn prune(stage: &Path, listed: &[ManifestFile]) {
    let keep: BTreeSet<String> = listed.iter().map(|f| f.path.clone()).collect();
    walk(stage, stage, &keep, true);
}

/// Refuse a run whose staging copy cannot fit: `need` bytes against `free`.
///
/// # Errors
/// `output_no_space` with a plain line.
pub(crate) fn check_room(need: u64, free: u64) -> Result<(), Error> {
    if need <= free {
        return Ok(());
    }
    Err(Error::new(
        "output_no_space",
        format!(
            "the output disk has {} MiB free and the run needs about {} MiB for its staging copy: free space and run again",
            free / (1024 * 1024),
            need.div_ceil(1024 * 1024)
        ),
    ))
}

/// The free bytes of the file system that holds `out`'s parent, `None` where the platform does not tell.
pub(crate) fn free_bytes(out: &Path) -> Option<u64> {
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs4::available_space(parent).ok()
}

/// Move the top-level entries of `old` that `new` does not have (a file the user keeps in the output folder) into `new`.
fn carry_over(old: &Path, new: &Path) {
    if let Ok(entries) = std::fs::read_dir(old) {
        for entry in entries.flatten() {
            let target = new.join(entry.file_name());
            if !exists(&target) {
                let _ = std::fs::rename(entry.path(), target);
            }
        }
    }
}

/// Replace `out` by `stage`: the previous output is moved aside, the staging folder takes its name, top-level entries
/// of the previous output that the new one does not have are carried over and the previous one is removed. A failed
/// rename puts the previous output back. The checkpoint is removed last, so a kill between the renames leaves staging
/// resumable.
pub(crate) fn swap(out: &Path, stage: &Path) -> Result<(), Error> {
    let old = old_of(out);
    if exists(&old) {
        std::fs::remove_dir_all(&old)
            .map_err(|e| failed("cannot remove the previous output folder", &e))?;
    }
    let had_old = exists(out);
    if had_old {
        std::fs::rename(out, &old)
            .map_err(|e| failed("cannot set the previous output folder aside", &e))?;
    }
    if let Err(e) = std::fs::rename(stage, out) {
        if had_old {
            let _ = std::fs::rename(&old, out);
        }
        return Err(failed("cannot move the staging folder into place", &e));
    }
    let _ = std::fs::remove_file(out.join(CHECKPOINT_FILE));
    if had_old {
        carry_over(&old, out);
        let _ = std::fs::remove_dir_all(&old);
    }
    Ok(())
}

/// Write the final manifest into the staging folder.
pub(crate) fn finish_manifest(stage: &Path, manifest: &Manifest) -> Result<(), Error> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| {
        Error::new("output_failed", "cannot write the manifest").with_detail(e.to_string())
    })? + "\n";
    lodwright_gen::write_output_file(stage, MANIFEST_FILE, text.as_bytes())
        .map(|_| ())
        .map_err(|e| {
            Error::new("output_failed", "cannot write the manifest").with_detail(e.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::{
        CHECKPOINT_FILE, check_room, prune, recover_swap, resume_or_clear, sibling, swap,
        write_checkpoint,
    };
    use lodwright_contract::ManifestFile;
    use std::path::{Path, PathBuf};

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lodwright-staging-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(path: &str) -> ManifestFile {
        ManifestFile {
            path: path.to_owned(),
            bytes: 1,
            digest: String::new(),
        }
    }

    #[test]
    fn a_swap_puts_staging_in_place_keeps_the_users_top_level_files_and_drops_the_checkpoint() {
        let root = temp("swap");
        let (out, stage) = (root.join("out"), root.join("out.lodwright-staging"));
        std::fs::create_dir_all(out.join("meshes")).unwrap();
        std::fs::write(out.join("meshes/old.nif"), b"old").unwrap();
        std::fs::write(out.join("notes.txt"), b"mine").unwrap();
        std::fs::create_dir_all(stage.join("meshes")).unwrap();
        std::fs::write(stage.join("meshes/new.nif"), b"new").unwrap();
        std::fs::write(stage.join(CHECKPOINT_FILE), b"{}").unwrap();
        swap(&out, &stage).unwrap();
        assert!(out.join("meshes/new.nif").is_file());
        assert!(!out.join("meshes/old.nif").exists());
        assert_eq!(std::fs::read(out.join("notes.txt")).unwrap(), b"mine");
        assert!(!out.join(CHECKPOINT_FILE).exists());
        assert!(!stage.exists() && !root.join("out.lodwright-old").exists());
    }

    #[test]
    fn a_swap_with_no_previous_output_is_one_rename() {
        let root = temp("swap-first");
        let (out, stage) = (root.join("out"), root.join("out.lodwright-staging"));
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("a.txt"), b"a").unwrap();
        swap(&out, &stage).unwrap();
        assert!(out.join("a.txt").is_file() && !stage.exists());
    }

    #[test]
    fn a_swap_killed_after_the_first_rename_is_put_back_and_one_killed_after_the_second_is_cleaned_up()
     {
        let root = temp("recover");
        let out = root.join("out");
        let old = root.join("out.lodwright-old");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("a.txt"), b"a").unwrap();
        recover_swap(&out).unwrap();
        assert!(out.join("a.txt").is_file() && !old.exists());
        // both exist: the swap had finished; the user's file in the old folder is not lost
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("keep.txt"), b"k").unwrap();
        recover_swap(&out).unwrap();
        assert!(out.join("keep.txt").is_file() && !old.exists());
    }

    #[test]
    fn staging_of_another_request_or_without_a_checkpoint_is_removed_and_the_same_one_is_kept() {
        let root = temp("resume");
        let stage = root.join("s");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(stage.join("x.txt"), b"x").unwrap();
        assert!(resume_or_clear(&stage, "r", "i").unwrap().is_none());
        assert!(!stage.exists(), "no checkpoint: removed");
        std::fs::create_dir_all(&stage).unwrap();
        write_checkpoint(&stage, "0", "r", "i", &[file("x.txt")], &[]).unwrap();
        assert!(resume_or_clear(&stage, "r", "other").unwrap().is_none());
        assert!(!stage.exists(), "other inputs: removed");
        std::fs::create_dir_all(&stage).unwrap();
        write_checkpoint(&stage, "0", "r", "i", &[file("x.txt")], &[]).unwrap();
        assert_eq!(
            resume_or_clear(&stage, "r", "i")
                .unwrap()
                .unwrap()
                .files
                .len(),
            1
        );
        assert!(stage.exists());
    }

    #[test]
    fn prune_removes_unlisted_files_and_the_folders_they_leave_empty() {
        let root = temp("prune");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::create_dir_all(root.join("keep")).unwrap();
        std::fs::write(root.join("a/b/stray.txt"), b"s").unwrap();
        std::fs::write(root.join("keep/f.txt"), b"f").unwrap();
        prune(&root, &[file("keep/f.txt")]);
        assert!(root.join("keep/f.txt").is_file());
        assert!(!root.join("a").exists());
    }

    #[test]
    fn a_sibling_sits_next_to_the_folder() {
        assert_eq!(
            sibling(Path::new("/mods/lod"), ".lodwright-staging"),
            Path::new("/mods/lod.lodwright-staging")
        );
    }

    #[test]
    fn a_run_that_cannot_fit_is_refused_with_a_plain_line() {
        assert!(check_room(10, 10).is_ok());
        let err = check_room(5 * 1024 * 1024, 1024 * 1024).unwrap_err();
        assert_eq!(err.code, "output_no_space");
        assert!(
            err.summary.contains("1 MiB free") && err.summary.contains("5 MiB"),
            "{}",
            err.summary
        );
    }
}
