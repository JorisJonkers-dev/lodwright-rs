// SPDX-License-Identifier: GPL-3.0-only
//! Reading game assets by path: loose files over archives, as the game finds them.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use lodwright_assets::{Archive, ArchiveListing, ConflictRule, Listing, Location, Resolver};

/// Something that can hand over a file by its game path (relative to the data folder).
pub trait Assets: Sync {
    /// The bytes of `path` (case-insensitive, either separator), or `None` when no source has it.
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

/// Why a texture could not be loaded: no source has the file, or a source has it and it cannot be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureFault {
    /// No loose file or archive holds the path.
    Missing,
    /// The file exists, the decoder refused it (the text says why).
    Unreadable(String),
}

/// Read and decode the largest level of the texture at `path` (block-compressed, or uncompressed 16, 24 or 32 bit).
///
/// # Errors
/// [`TextureFault::Missing`] when no source has the file, [`TextureFault::Unreadable`] when it does not decode.
pub fn read_texture(assets: &dyn Assets, path: &str) -> Result<lodwright_dds::Image, TextureFault> {
    let bytes = assets.read(path).ok_or(TextureFault::Missing)?;
    lodwright_dds::decode_image(&bytes).map_err(|e| TextureFault::Unreadable(e.to_string()))
}

/// The warning for a texture that could not be loaded; `what` names its use, `tail` what happens instead.
#[must_use]
pub fn texture_warning(what: &str, path: &str, fault: &TextureFault, tail: &str) -> String {
    match fault {
        TextureFault::Missing => format!("{what} {path} not found{tail}"),
        TextureFault::Unreadable(why) => {
            format!("{what} {path} found but not readable ({why}){tail}")
        }
    }
}

/// Largest single asset read: a texture or model larger than this is treated as missing.
const MAX_ASSET: u64 = 256 * 1024 * 1024;

/// The data folder: loose files under `meshes` and `textures`, and the archives next to the plugins.
pub struct FolderAssets {
    root: PathBuf,
    resolver: Resolver,
    archives: Vec<(PathBuf, Archive)>,
    loose: HashMap<String, PathBuf>,
}

impl FolderAssets {
    /// Index `data`: every `.bsa` in it and every file below its `meshes` and `textures` folders. `plugins` are
    /// the plugin names in load order (archives load with the plugin that shares their name).
    ///
    /// # Errors
    /// A message when the folder cannot be read.
    pub fn open(data: &Path, plugins: &[String]) -> Result<Self, String> {
        let mut listing = Listing::default();
        let mut archives = Vec::new();
        let mut names: Vec<_> = std::fs::read_dir(data)
            .map_err(|e| format!("cannot read the data folder: {e}"))?
            .flatten()
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("bsa"))
            })
            .collect();
        names.sort_by_key(std::fs::DirEntry::file_name);
        for entry in names {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(file) = File::open(&path) else {
                continue;
            };
            let Ok(archive) = Archive::open(&mut BufReader::new(file)) else {
                continue;
            };
            let marker = path.with_extension("bsa.override").exists();
            listing
                .archives
                .push(ArchiveListing::from_archive(&name, &archive, marker));
            archives.push((path, archive));
        }
        let mut loose = HashMap::new();
        for top in ["meshes", "textures"] {
            let Some(dir) = find_dir(data, top) else {
                continue;
            };
            walk(&dir, &format!("{top}/"), &mut listing.loose, &mut loose);
        }
        let resolver = Resolver::new(&listing, plugins, ConflictRule::LaterWins);
        Ok(Self {
            root: data.to_path_buf(),
            resolver,
            archives,
            loose,
        })
    }
}

/// Where the file the game would read for a path comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Winner {
    /// No loose file and no archive has it.
    Missing,
    /// A loose file in the data folder (it beats every archive).
    Loose,
    /// A file inside the archive of this file name.
    Archive(String),
}

impl FolderAssets {
    /// Which source wins `path` (case-insensitive, either separator) as the game resolves files: loose files over
    /// archives, later archives over earlier ones.
    #[must_use]
    pub fn winner(&self, path: &str) -> Winner {
        match self.resolver.resolve(path) {
            None => Winner::Missing,
            Some(Location::Loose(_)) => Winner::Loose,
            Some(Location::Archive { archive, .. }) => self
                .archives
                .get(*archive)
                .and_then(|(file, _)| file.file_name())
                .map_or(Winner::Missing, |n| {
                    Winner::Archive(n.to_string_lossy().into_owned())
                }),
        }
    }
}

impl FolderAssets {
    /// Whether an archive that `own` does not skip holds `path`: the files the game ships, as opposed to what a mod
    /// put loose in the data folder or in an archive of its own. `own` is given each archive's file name.
    #[must_use]
    pub fn shipped(&self, path: &str, skip: &dyn Fn(&str) -> bool) -> bool {
        self.shipped_entry(path, skip).is_some()
    }

    /// The bytes of `path` from an archive that `skip` does not name (see [`FolderAssets::shipped`]); the last
    /// archive in name order that has it.
    #[must_use]
    pub fn shipped_read(&self, path: &str, skip: &dyn Fn(&str) -> bool) -> Option<Vec<u8>> {
        let (file, parsed, entry) = self.shipped_entry(path, skip)?;
        let mut reader = BufReader::new(File::open(file).ok()?);
        parsed.extract(&mut reader, entry, MAX_ASSET).ok()
    }

    fn shipped_entry(
        &self,
        path: &str,
        skip: &dyn Fn(&str) -> bool,
    ) -> Option<(&Path, &Archive, &lodwright_assets::Entry)> {
        self.archives.iter().rev().find_map(|(file, archive)| {
            let name = file.file_name()?.to_string_lossy();
            if skip(&name) {
                return None;
            }
            Some((file.as_path(), archive, archive.find(path)?))
        })
    }
}

fn find_dir(parent: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(parent).ok()?.flatten().find_map(|e| {
        (e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.path().is_dir())
            .then(|| e.path())
    })
}

fn walk(dir: &Path, prefix: &str, listing: &mut Vec<String>, files: &mut HashMap<String, PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let rel = format!("{prefix}{name}");
        if e.path().is_dir() {
            walk(&e.path(), &format!("{rel}/"), listing, files);
        } else {
            files.insert(rel.clone(), e.path());
            listing.push(rel);
        }
    }
}

impl Assets for FolderAssets {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self.resolver.resolve(path)? {
            Location::Loose(listed) => {
                let file = self.loose.get(listed)?;
                debug_assert!(file.starts_with(&self.root));
                if std::fs::metadata(file).ok()?.len() > MAX_ASSET {
                    return None;
                }
                std::fs::read(file).ok()
            }
            Location::Archive { archive, path } => {
                let (file, parsed) = self.archives.get(*archive)?;
                let entry = parsed.find(path)?;
                let mut reader = BufReader::new(File::open(file).ok()?);
                parsed.extract(&mut reader, entry, MAX_ASSET).ok()
            }
        }
    }
}
