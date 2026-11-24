// SPDX-License-Identifier: GPL-3.0-only
//! Writing into the output folder without following a link that sits inside it.
//!
//! The output folder itself may be a link (a caller may point it anywhere outside the data folder). Below it,
//! every folder of a file's path must be a real folder: a link planted there would make a write, a replace or a
//! removal land outside the output folder.

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use lodwright_contract::sha256_digest;

use crate::GenError;
use crate::terrain::FileRecord;

/// Whether `rel` is a plain relative path: not empty, not absolute, no `..`, no leading `.` and no drive or root.
#[must_use]
pub fn is_plain_relative(rel: &str) -> bool {
    !rel.is_empty()
        && Path::new(rel)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// The first folder of `rel` below `out` that is a link, if any (folders that do not exist yet are not links).
#[must_use]
pub fn linked_folder(out: &Path, rel: &str) -> Option<PathBuf> {
    let mut dir = out.to_path_buf();
    for c in Path::new(rel).parent()?.components() {
        dir.push(c);
        match std::fs::symlink_metadata(&dir) {
            Ok(meta) if meta.file_type().is_symlink() => return Some(dir),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}

/// Write `bytes` to `rel` under `out`: the folders are created one by one and none may be a link, the content goes
/// to a fresh `.tmp` file next to the target (an entry already at that path is removed first, never followed) and is
/// then renamed over the target (which replaces a link at the target instead of writing through it).
///
/// # Errors
/// [`GenError::LinkInOutput`] for a path that is not plain or that passes a link, [`GenError::Io`] for a failed write.
pub fn write_output_file(out: &Path, rel: &str, bytes: &[u8]) -> Result<FileRecord, GenError> {
    write_output_with(out, rel, |file| file.write_all(bytes))?;
    Ok(FileRecord {
        path: rel.to_owned(),
        bytes: bytes.len() as u64,
        digest: sha256_digest(bytes),
    })
}

/// Write `rel` under `out` as [`write_output_file`] does, with the content from `write_content`, which writes it to the fresh
/// file; nothing is held in memory here. The record's digest is read back from the written file, a part at a time,
/// and equals [`sha256_digest`] of the whole content.
///
/// # Errors
/// As [`write_output_file`]; [`GenError::Io`] also when `write_content` fails (the `.tmp` file is then removed and the target
/// left as it was).
pub fn write_output_stream(
    out: &Path,
    rel: &str,
    write_content: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> Result<FileRecord, GenError> {
    let target = write_output_with(out, rel, write_content)?;
    let io = |source| GenError::Io {
        what: rel.to_owned(),
        source,
    };
    // a file whose digest cannot be read back is not left behind as if it were written
    let digest = std::fs::File::open(&target).and_then(|file| {
        let len = file.metadata()?.len();
        Ok((len, lodwright_contract::sha256_digest_of_reader(len, file)?))
    });
    let (len, digest) = match digest {
        Ok(d) => d,
        Err(e) => {
            let _ = std::fs::remove_file(&target);
            return Err(io(e));
        }
    };
    Ok(FileRecord {
        path: rel.to_owned(),
        bytes: len,
        digest,
    })
}

fn write_output_with(
    out: &Path,
    rel: &str,
    write_content: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> Result<PathBuf, GenError> {
    let refused = || GenError::LinkInOutput {
        what: rel.to_owned(),
    };
    if !is_plain_relative(rel) {
        return Err(refused());
    }
    let io = |source| GenError::Io {
        what: rel.to_owned(),
        source,
    };
    let target: PathBuf = out.join(rel);
    // the output folder itself may be a link; only what lies below it is checked
    std::fs::create_dir_all(out).map_err(io)?;
    let mut dir = out.to_path_buf();
    if let Some(parent) = Path::new(rel).parent() {
        for c in parent.components() {
            dir.push(c);
            match std::fs::symlink_metadata(&dir) {
                Ok(meta) if meta.file_type().is_symlink() => return Err(refused()),
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    match std::fs::create_dir(&dir) {
                        Ok(()) => {}
                        // another worker made it in the meantime: check it like an existing one
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                            if std::fs::symlink_metadata(&dir)
                                .map_err(io)?
                                .file_type()
                                .is_symlink()
                            {
                                return Err(refused());
                            }
                        }
                        Err(e) => return Err(io(e)),
                    }
                }
                Err(e) => return Err(io(e)),
            }
        }
    }
    let temp = target.with_extension("tmp");
    if std::fs::symlink_metadata(&temp).is_ok() {
        std::fs::remove_file(&temp).map_err(io)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(io)?;
    if let Err(e) = write_content(&mut file).and_then(|()| file.flush()) {
        drop(file);
        let _ = std::fs::remove_file(&temp);
        return Err(io(e));
    }
    drop(file);
    // a file that cannot take its name is not left behind under the temporary one
    if let Err(e) = std::fs::rename(&temp, &target) {
        let _ = std::fs::remove_file(&temp);
        return Err(io(e));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::{is_plain_relative, linked_folder, write_output_file, write_output_stream};
    use crate::GenError;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lodwright-output-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_write_whose_rename_fails_leaves_no_temporary_file() {
        let out = temp("rename-fails");
        // a folder that holds a file stands where the output should go, so the rename is refused
        std::fs::create_dir_all(out.join("a.bin")).unwrap();
        std::fs::write(out.join("a.bin").join("keep"), b"x").unwrap();
        assert!(write_output_file(&out, "a.bin", b"abc").is_err());
        assert!(
            !out.join("a.tmp").exists(),
            "the temporary file was left behind"
        );
        assert!(out.join("a.bin").join("keep").exists());
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn only_plain_relative_paths_are_accepted() {
        for good in ["a.nif", "meshes/a/b.nif"] {
            assert!(is_plain_relative(good), "{good}");
        }
        for bad in ["", "/etc/x", "../x", "a/../../x", "./a"] {
            assert!(!is_plain_relative(bad), "{bad}");
        }
    }

    #[test]
    fn a_file_is_written_with_its_folders_and_its_record() {
        let out = temp("plain");
        let record = write_output_file(&out, "a/b/c.txt", b"abc").unwrap();
        assert_eq!(std::fs::read(out.join("a/b/c.txt")).unwrap(), b"abc");
        assert_eq!((record.path.as_str(), record.bytes), ("a/b/c.txt", 3));
        assert!(!out.join("a/b/c.tmp").exists());
        assert_eq!(linked_folder(&out, "a/b/c.txt"), None);
    }

    #[test]
    fn a_streamed_file_has_the_record_of_the_same_bytes_written_at_once() {
        let out = temp("stream");
        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let streamed =
            write_output_stream(&out, "s/a.bin", |f| std::io::Write::write_all(f, &bytes)).unwrap();
        let whole = write_output_file(&out, "w/a.bin", &bytes).unwrap();
        assert_eq!(
            (streamed.bytes, &streamed.digest),
            (whole.bytes, &whole.digest)
        );
        assert_eq!(std::fs::read(out.join("s/a.bin")).unwrap(), bytes);
    }

    #[test]
    fn a_failed_stream_leaves_the_previous_file_and_no_temp_file() {
        let out = temp("stream-fail");
        write_output_file(&out, "a.bsa", b"previous").unwrap();
        let err = write_output_stream(&out, "a.bsa", |f| {
            std::io::Write::write_all(f, b"half")?;
            Err(std::io::Error::other("stopped"))
        })
        .unwrap_err();
        assert!(matches!(err, GenError::Io { .. }), "{err}");
        assert_eq!(std::fs::read(out.join("a.bsa")).unwrap(), b"previous");
        assert!(!out.join("a.tmp").exists());
    }

    #[test]
    fn a_path_that_climbs_out_is_refused_and_nothing_is_written() {
        let out = temp("climb");
        let err = write_output_file(&out.join("inner"), "../x.txt", b"x").unwrap_err();
        assert!(matches!(err, GenError::LinkInOutput { .. }), "{err}");
        assert!(!out.join("x.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_on_the_path_is_refused_and_found() {
        let root = temp("linked");
        let (out, outside) = (root.join("out"), root.join("outside"));
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, out.join("a")).unwrap();
        let err = write_output_file(&out, "a/b.txt", b"x").unwrap_err();
        assert!(matches!(err, GenError::LinkInOutput { .. }), "{err}");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        assert_eq!(linked_folder(&out, "a/b.txt"), Some(out.join("a")));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_at_the_temporary_path_is_removed_not_followed() {
        let root = temp("tmp-link");
        let (out, victim) = (root.join("out"), root.join("victim.txt"));
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(&victim, b"keep").unwrap();
        std::os::unix::fs::symlink(&victim, out.join("f.tmp")).unwrap();
        write_output_file(&out, "f.dds", b"new").unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
        assert_eq!(std::fs::read(out.join("f.dds")).unwrap(), b"new");
    }
}
