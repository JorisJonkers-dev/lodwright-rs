// SPDX-License-Identifier: GPL-3.0-only
//! Extract a whole archive into a folder, and into a temporary folder that removes itself.
//!
//! An archive path such as `meshes\landscape\lod\w\blocks\x.nif` becomes the folder path
//! `meshes/landscape/lod/w/blocks/x.nif` below the destination, so the result can be checked like loose output
//! (`verify` of the folder). A name that could leave the destination or that no file system accepts is refused.

use std::fmt;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Archive, Error};

/// What can go wrong extracting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    /// The archive could not be read.
    Archive(Error),
    /// An entry name would leave the destination or is not a usable file name.
    UnsafePath(String),
    /// Writing failed.
    Io(std::io::ErrorKind),
}

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archive(e) => write!(f, "{e}"),
            Self::UnsafePath(p) => write!(f, "the archive holds the unsafe path {p:?}"),
            Self::Io(kind) => write!(f, "write failed: {kind}"),
        }
    }
}

impl std::error::Error for ExtractError {}

impl From<Error> for ExtractError {
    fn from(error: Error) -> Self {
        Self::Archive(error)
    }
}

fn io(error: &std::io::Error) -> ExtractError {
    ExtractError::Io(error.kind())
}

/// The folder-relative path of an archive path, or `None` when it is not safe to create.
fn relative(path: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in path.split('\\') {
        let bad = part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"/|?*".contains(c));
        if bad {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

/// Extract every entry of `archive` (read through `reader`) below `dest`, creating folders as needed, and return
/// the extracted paths (`/` separated, relative to `dest`) in archive order. `cap` limits each entry's size.
///
/// # Errors
/// [`ExtractError::UnsafePath`] for a name that would leave `dest`, [`ExtractError::Archive`] when an entry cannot
/// be read, [`ExtractError::Io`] when a file cannot be written.
pub fn extract_all<R: Read + Seek>(
    archive: &Archive,
    reader: &mut R,
    dest: &Path,
    cap: u64,
) -> Result<Vec<String>, ExtractError> {
    let mut written = Vec::with_capacity(archive.entries().len());
    for entry in archive.entries() {
        let rel = relative(entry.path())
            .ok_or_else(|| ExtractError::UnsafePath(entry.path().to_owned()))?;
        let bytes = archive.extract(reader, entry, cap)?;
        let target = dest.join(&rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| io(&e))?;
        }
        fs::write(&target, bytes).map_err(|e| io(&e))?;
        written.push(entry.path().replace('\\', "/"));
    }
    Ok(written)
}

/// A temporary folder holding an extracted archive. It is removed when dropped.
#[derive(Debug)]
pub struct Extracted {
    root: PathBuf,
    files: Vec<String>,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl Extracted {
    /// The folder the archive was extracted into.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// The extracted files, `/` separated and relative to [`Extracted::path`], in archive order.
    #[must_use]
    pub fn files(&self) -> &[String] {
        &self.files
    }
}

impl Drop for Extracted {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Open the archive in `bytes` and extract it into a new folder below `parent` (the system temporary folder when
/// `None`). The folder is removed when the result is dropped, also after an error.
///
/// # Errors
/// As [`Archive::open`] and [`extract_all`]; [`ExtractError::Io`] when the folder cannot be created.
pub fn extract_to_temp(
    bytes: &[u8],
    parent: Option<&Path>,
    cap: u64,
) -> Result<Extracted, ExtractError> {
    let mut cursor = std::io::Cursor::new(bytes);
    let archive = Archive::open(&mut cursor)?;
    let base = parent.map_or_else(std::env::temp_dir, Path::to_path_buf);
    let root = base.join(format!(
        "lodwright-extract-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).map_err(|e| io(&e))?;
    // From here the guard removes the folder on every exit.
    let mut extracted = Extracted {
        root,
        files: Vec::new(),
    };
    extracted.files = extract_all(&archive, &mut cursor, &extracted.root, cap)?;
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::{ExtractError, extract_to_temp, relative};
    use crate::pack::{PackEntry, write};
    use lodwright_testkit::bsa::BsaBuilder;

    fn pack(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let entries: Vec<PackEntry> = files
            .iter()
            .map(|(path, data, compress)| PackEntry {
                path: (*path).to_owned(),
                data: data.to_vec(),
                compress: *compress,
            })
            .collect();
        write(&entries).unwrap()
    }

    #[test]
    fn every_file_lands_below_the_folder_with_its_bytes() {
        let bytes = pack(&[
            ("meshes\\landscape\\lod\\w\\blocks\\a.nif", b"mesh", false),
            ("textures\\landscape\\lod\\t.dds", &[9; 300], true),
        ]);
        let extracted = extract_to_temp(&bytes, None, 1 << 20).unwrap();
        assert_eq!(extracted.files().len(), 2);
        assert_eq!(
            std::fs::read(extracted.path().join("meshes/landscape/lod/w/blocks/a.nif")).unwrap(),
            b"mesh"
        );
        assert_eq!(
            std::fs::read(extracted.path().join("textures/landscape/lod/t.dds")).unwrap(),
            vec![9; 300]
        );
    }

    #[test]
    fn the_folder_is_removed_when_the_result_is_dropped() {
        let bytes = pack(&[("a\\b.txt", b"x", false)]);
        let extracted = extract_to_temp(&bytes, None, 1 << 20).unwrap();
        let path = extracted.path().to_path_buf();
        assert!(path.exists());
        drop(extracted);
        assert!(!path.exists());
    }

    #[test]
    fn a_bad_archive_leaves_no_folder_behind() {
        let parent =
            std::env::temp_dir().join(format!("lodwright-extract-parent-{}", std::process::id()));
        std::fs::create_dir_all(&parent).unwrap();
        let mut bytes = pack(&[("a\\b.txt", b"hello hello hello", true)]);
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        // The index is intact, so the folder is created before the entry fails to inflate.
        assert_eq!(
            extract_to_temp(&bytes, Some(&parent), 1 << 20).unwrap_err(),
            ExtractError::Archive(crate::Error::Decompress)
        );
        assert_eq!(std::fs::read_dir(&parent).unwrap().count(), 0);
        std::fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn a_name_that_leaves_the_folder_is_refused() {
        for bad in [
            "..\\x.txt",
            "a\\..\\..\\x.txt",
            "c:\\x.txt",
            "a\\\\b.txt",
            "a\\b.txt.",
            "a\\b|c.txt",
        ] {
            assert!(relative(bad).is_none(), "{bad}");
        }
        assert!(relative("a\\b\\c.txt").is_some());
        let bytes = BsaBuilder::new().file("..\\escape.txt", b"x").build();
        assert_eq!(
            extract_to_temp(&bytes, None, 1 << 20).unwrap_err(),
            ExtractError::UnsafePath("..\\escape.txt".into())
        );
    }

    #[test]
    fn errors_say_what_happened() {
        assert!(
            ExtractError::UnsafePath("p".into())
                .to_string()
                .contains("unsafe path")
        );
        assert!(
            ExtractError::Io(std::io::ErrorKind::NotFound)
                .to_string()
                .contains("write failed")
        );
        assert!(
            ExtractError::from(crate::Error::NotAnArchive)
                .to_string()
                .contains("not a BSA")
        );
    }
}
