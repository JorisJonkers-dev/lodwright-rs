// SPDX-License-Identifier: GPL-3.0-only
//! The packing step of a run: the generated files of the staging folder become one BSA archive and its stub plugin.
//!
//! It runs last, in the staging folder, after every world is finished and before the manifest is written. The loose
//! files it packed are removed from staging only after the archive and the stub are written, and staging replaces
//! the output folder only when the whole run succeeds (`docs/interrupted-runs.md`): a run killed here leaves the
//! previous output untouched, and its staging folder is pruned or rebuilt by the next run.

use std::path::Path;

use lodwright_assets::pack::{self, PackEntry, PackError};
use lodwright_assets::stub::{PackedNames, stub_plugin};
use lodwright_contract::ManifestFile;

use crate::Error;

/// The base name of the archive and its stub plugin: `Lodwright LOD.bsa` and `Lodwright LOD.esp`.
pub(crate) const ARCHIVE_BASE: &str = "Lodwright LOD";

/// Whether a manifest describes a packed output: it lists an archive in the output folder itself (a loose output
/// keeps every file in a folder below it).
pub(crate) fn is_packed(files: &[ManifestFile]) -> bool {
    files.iter().any(|f| {
        !f.path.contains('/')
            && Path::new(&f.path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("bsa"))
    })
}

/// Whether a file is stored compressed: an object or tree texture is (a performance choice from the guide that
/// `docs/spec/game-pickup.md` cites, not a format fact); terrain meshes and textures, object and tree block files
/// and the settings file are stored.
fn compress(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    Path::new(&lower)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dds"))
        && (lower.contains("/blocks/") || lower.contains("/trees/"))
}

fn failed(what: &str, detail: impl std::fmt::Display) -> Error {
    Error::new("output_failed", what).with_detail(detail.to_string())
}

fn pack_error(e: &PackError) -> Error {
    match e {
        PackError::ArchiveTooLarge { .. } | PackError::FileTooLarge { .. } => Error::new(
            "pack_too_large",
            "the generated LOD does not fit one archive; generate without packing",
        )
        .with_detail(e.to_string()),
        _ => Error::new("pack_failed", "the generated files could not be packed")
            .with_detail(e.to_string()),
    }
}

/// Pack `files` (paths relative to `stage`) into `<ARCHIVE_BASE>.bsa` with a stub plugin beside it, remove the loose
/// files and return the manifest entries of the archive and the stub. The stub names `master` (the first plugin of the
/// load order) with the size of its file, because the game opens a plugin's archives only when it has a master.
/// `progress` gets the number of files read.
///
/// # Errors
/// `output_failed` when a file cannot be read, written or removed; `pack_too_large` when the files do not fit one
/// archive; `pack_failed` for any other refusal of the writer.
pub(crate) fn pack_staging(
    stage: &Path,
    files: &[ManifestFile],
    master: (&str, u64),
    mut progress: impl FnMut(u64, u64),
) -> Result<Vec<ManifestFile>, Error> {
    let names = PackedNames::new(ARCHIVE_BASE).map_err(|e| failed("bad archive name", e))?;
    let total = files.len() as u64;
    let mut entries = Vec::with_capacity(files.len());
    for (i, file) in files.iter().enumerate() {
        let data = std::fs::read(stage.join(&file.path))
            .map_err(|e| failed(&format!("cannot read {} to pack it", file.path), e))?;
        entries.push(PackEntry {
            path: file.path.clone(),
            data,
            compress: compress(&file.path),
        });
        progress(i as u64 + 1, total);
    }
    let archive = pack::write(&entries).map_err(|e| pack_error(&e))?;
    drop(entries);
    let write = |name: &str, bytes: &[u8]| {
        lodwright_gen::write_output_file(stage, name, bytes)
            .map(|r| ManifestFile {
                path: r.path,
                bytes: r.bytes,
                digest: r.digest,
            })
            .map_err(|e| failed(&format!("cannot write {name}"), e))
    };
    let packed = vec![
        write(&names.archive, &archive)?,
        write(&names.plugin, &stub_plugin(master.0, master.1))?,
    ];
    // only now, with the archive on disk, do the loose files go
    for file in files {
        std::fs::remove_file(stage.join(&file.path))
            .map_err(|e| failed(&format!("cannot remove {} after packing it", file.path), e))?;
    }
    remove_empty_folders(stage);
    Ok(packed)
}

fn remove_empty_folders(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            remove_empty_folders(&entry.path());
            let _ = std::fs::remove_dir(entry.path()); // only succeeds when empty
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ARCHIVE_BASE, compress, is_packed, pack_staging};
    use lodwright_contract::ManifestFile;

    fn file(path: &str) -> ManifestFile {
        ManifestFile {
            path: path.into(),
            bytes: 1,
            digest: "sha256:x".into(),
        }
    }

    #[test]
    fn only_object_and_tree_textures_are_compressed() {
        for (path, expected) in [
            ("textures/landscape/lod/w/diffuse/a.dds", false),
            ("meshes/landscape/lod/w/blocks/a.nif", false),
            ("textures/landscape/lod/w/blocks/a.dds", true),
            ("textures/landscape/lod/w/trees/a.dds", true),
            ("lodsettings/w.dlodsettings", false),
        ] {
            assert_eq!(compress(path), expected, "{path}");
        }
    }

    #[test]
    fn a_manifest_is_packed_when_it_lists_an_archive_at_the_root() {
        assert!(is_packed(&[
            file("Lodwright LOD.bsa"),
            file("Lodwright LOD.esp")
        ]));
        assert!(!is_packed(&[file("meshes/a.nif"), file("textures/a.dds")]));
        assert!(!is_packed(&[file("textures/inner.bsa")]));
    }

    #[test]
    fn packing_replaces_the_loose_files_with_the_archive_and_the_stub() {
        let dir = std::env::temp_dir().join(format!("lodwright-pack-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("meshes/a")).unwrap();
        std::fs::write(dir.join("meshes/a/x.nif"), b"mesh").unwrap();
        std::fs::create_dir_all(dir.join("lodsettings")).unwrap();
        std::fs::write(dir.join("lodsettings/w.dlodsettings"), b"s").unwrap();
        let files = vec![file("meshes/a/x.nif"), file("lodsettings/w.dlodsettings")];
        let mut seen = Vec::new();
        let packed = pack_staging(&dir, &files, ("Island.esm", 5), |done, of| {
            seen.push((done, of));
        })
        .unwrap();
        assert_eq!(seen, [(1, 2), (2, 2)]);
        let names: Vec<&str> = packed.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            names,
            [format!("{ARCHIVE_BASE}.bsa"), format!("{ARCHIVE_BASE}.esp")]
        );
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["Lodwright LOD.bsa", "Lodwright LOD.esp"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_file_fails_before_anything_is_written() {
        let dir =
            std::env::temp_dir().join(format!("lodwright-pack-unit-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = pack_staging(
            &dir,
            &[file("meshes/gone.nif")],
            ("Island.esm", 5),
            |_, _| {},
        )
        .unwrap_err();
        assert_eq!(err.code, "output_failed");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
