// SPDX-License-Identifier: GPL-3.0-only
//! BSA version 104 writer: name hashes, record order, per-file zlib compression and the limits of the format.
//!
//! The archive is built in memory from files that are already generated ([`PackEntry`], [`write`]), or streamed to
//! a writer with one file in memory at a time ([`PackSource`], [`write_streamed`]); the result reads back through
//! [`crate::Archive`]. Output is a pure function of the set of entries: it does not depend on the order
//! the entries are given in, nor on how many threads produced them.
//!
//! Derived from: the UESP archive-format pages (rows `bsa-v104-header`, `bsa-v104-folder-records`,
//! `bsa-v104-file-records`, `bsa-v104-data-blocks`, `bsa-path-form`, `bsa-name-hash`), the UESP page "Hash
//! Calculation" (its prose rules only, row `bsa-name-hash-rules`), the record order (`bsa-record-order`) and the
//! header values measured as numbers on the owner's own archives (row `bsa-writer-header-values`). Never from
//! decompiled or unlicensed code. A round trip through our own reader proves only that writer and reader agree;
//! the hash and the order are checked against numbers measured on game archives (see the tests).

use std::collections::BTreeMap;
use std::fmt;

use crate::path::normalise;

// source: bsa-v104-header
const HEADER_LEN: u32 = 36;
// source: bsa-v104-header
const VERSION: u32 = 104;
// source: bsa-v104-header
const FLAG_DIR_NAMES: u32 = 0x1;
// source: bsa-v104-header
const FLAG_FILE_NAMES: u32 = 0x2;
// source: bsa-v104-header
const FLAG_COMPRESSED: u32 = 0x4;
// source: bsa-v104-file-records
const SIZE_INVERT: u32 = 0x4000_0000;
/// The largest stored size of one file: the top two bits of the size field are flags.
// source: bsa-v104-file-records
pub const MAX_FILE_SIZE: u64 = 0x3FFF_FFFF;
/// The largest archive: data offsets are u32.
// source: bsa-v104-file-records
pub const MAX_ARCHIVE_SIZE: u64 = 0xFFFF_FFFF;
/// The longest folder or file name in bytes: the folder name's length byte counts the NUL.
// source: bsa-v104-folder-records
pub const MAX_NAME_LEN: usize = 254;
/// The zlib level of every compressed file. Pinned: a different level or a `miniz_oxide` change changes bytes.
const ZLIB_LEVEL: u8 = 6;
// source: bsa-writer-header-values
const CONTENT_MESHES: u16 = 0x1;
// source: bsa-writer-header-values
const CONTENT_TEXTURES: u16 = 0x2;
// source: bsa-writer-header-values
const CONTENT_MISC: u16 = 0x100;

/// One file to pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackEntry {
    /// Relative path, `folder\name` or `folder/name`, any case. Normalised to lower case with backslashes.
    pub path: String,
    /// The file's bytes.
    pub data: Vec<u8>,
    /// Whether to store the file zlib-compressed (a performance choice of the caller, not a format fact).
    pub compress: bool,
}

/// Why an archive was not written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// No entry at all: an empty archive is not written.
    Empty,
    /// A path the format cannot hold (no folder part, empty or over-long name, not ASCII).
    BadPath {
        /// The offending path as given.
        path: String,
        /// Why it was refused.
        reason: &'static str,
    },
    /// Two paths that are the same after lower-casing and separator folding.
    CaseCollision {
        /// The first path as given.
        first: String,
        /// The second path as given.
        second: String,
    },
    /// Two different names with the same hash in the same place (folder names, or file names in one folder).
    HashCollision {
        /// The first name (lower case, with its folder for a file).
        first: String,
        /// The second name.
        second: String,
    },
    /// One file's stored size does not fit the 30-bit size field.
    FileTooLarge {
        /// The file's path as given.
        path: String,
        /// Its stored size in bytes.
        size: u64,
    },
    /// The archive would pass the limits of the format (u32 offsets and counts). It has to be split.
    ArchiveTooLarge {
        /// Bytes the archive would need.
        size: u64,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("there is no file to pack"),
            Self::BadPath { path, reason } => write!(f, "cannot pack {path:?}: {reason}"),
            Self::CaseCollision { first, second } => {
                write!(
                    f,
                    "{first:?} and {second:?} are the same path in an archive"
                )
            }
            Self::HashCollision { first, second } => {
                write!(f, "{first:?} and {second:?} have the same name hash")
            }
            Self::FileTooLarge { path, size } => write!(
                f,
                "{path:?} needs {size} bytes, more than the {MAX_FILE_SIZE} one stored file can hold"
            ),
            Self::ArchiveTooLarge { size } => write!(
                f,
                "the archive would be {size} bytes, more than the {MAX_ARCHIVE_SIZE} the format can address; split it"
            ),
        }
    }
}

impl std::error::Error for PackError {}

/// Fold the hash recurrence over `bytes` (wrapping 32-bit).
// source: bsa-name-hash
fn fold(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0_u32, |h, c| {
        h.wrapping_mul(0x0001_003F).wrapping_add(u32::from(*c))
    })
}

/// The u64 name hash of a folder or file name that is already lower case with backslashes.
///
/// `is_file` splits the name at its last dot; a folder name is never split (a folder may contain a dot).
#[must_use]
pub fn name_hash(name: &str, is_file: bool) -> u64 {
    // source: bsa-name-hash
    let bytes = name.as_bytes();
    let (stem, ext) = match bytes.iter().rposition(|b| *b == b'.') {
        Some(dot) if is_file => bytes.split_at(dot),
        _ => (bytes, &bytes[bytes.len()..]),
    };
    let mut low = 0_u32;
    if let Some(last) = stem.last() {
        low |= u32::from(*last);
    }
    if stem.len() > 2 {
        low |= u32::from(stem[stem.len() - 2]) << 8;
    }
    // The stem length is one byte and wraps at 256.
    low |= u32::from(u8::try_from(stem.len() & 0xFF).unwrap_or(0)) << 16;
    if let Some(first) = stem.first() {
        low |= u32::from(*first) << 24;
    }
    low |= match ext {
        b".kf" => 0x80,
        b".nif" => 0x8000,
        b".dds" => 0x8080,
        b".wav" => 0x8000_0000,
        _ => 0,
    };
    let middle = if stem.len() > 3 {
        &stem[1..stem.len() - 2]
    } else {
        &[]
    };
    let high = fold(middle).wrapping_add(fold(ext));
    (u64::from(high) << 32) | u64::from(low)
}

/// The content-type bits of the header's file flags for a set of lower-case paths.
// source: bsa-writer-header-values
fn content_flags<'a>(paths: impl Iterator<Item = &'a str>) -> u16 {
    let mut flags = 0;
    for path in paths {
        let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
        flags |= match ext {
            "nif" | "btr" | "bto" => CONTENT_MESHES,
            "dds" => CONTENT_TEXTURES,
            _ => CONTENT_MISC,
        };
    }
    flags
}

/// One file to pack whose bytes are read only when the archive needs them ([`write_streamed`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSource {
    /// Relative path, `folder\name` or `folder/name`, any case. Normalised to lower case with backslashes.
    pub path: String,
    /// The file's length in bytes; the bytes read later must have exactly this length.
    pub len: u64,
    /// Whether to store the file zlib-compressed (a performance choice of the caller, not a format fact).
    pub compress: bool,
}

/// Why a streamed archive was not written.
#[derive(Debug)]
pub enum StreamError {
    /// The writer refused the set of files (see [`PackError`]).
    Pack(PackError),
    /// A file could not be read, had another length than it was given with, or the spool or the archive could not
    /// be written.
    Io {
        /// What was being done.
        what: String,
        /// The cause.
        source: std::io::Error,
    },
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pack(e) => e.fmt(f),
            Self::Io { what, source } => write!(f, "{what}: {source}"),
        }
    }
}

impl std::error::Error for StreamError {}

impl From<PackError> for StreamError {
    fn from(e: PackError) -> Self {
        Self::Pack(e)
    }
}

/// A validated file: normalised folder and file name, hash, and where it came from.
struct Placed<'a> {
    folder: String,
    name: String,
    file_hash: u64,
    /// Its place in the caller's list.
    at: usize,
    path: &'a str,
    compress: bool,
}

fn validate<'a>(
    entries: impl ExactSizeIterator<Item = (&'a str, bool)>,
) -> Result<Vec<Placed<'a>>, PackError> {
    if entries.len() == 0 {
        return Err(PackError::Empty);
    }
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let mut placed = Vec::with_capacity(entries.len());
    for (at, (path, compress)) in entries.enumerate() {
        let bad = |reason| PackError::BadPath {
            path: path.to_owned(),
            reason,
        };
        if !path.is_ascii() {
            return Err(bad("the name is not ASCII"));
        }
        let full = normalise(path);
        let (folder, name) = full
            .rsplit_once('\\')
            .ok_or_else(|| bad("the path has no folder part"))?;
        if folder.is_empty() || folder.split('\\').any(str::is_empty) {
            return Err(bad("a folder name is empty"));
        }
        if name.is_empty() {
            return Err(bad("the file name is empty"));
        }
        if folder.len() > MAX_NAME_LEN || name.len() > MAX_NAME_LEN {
            return Err(bad("a name is longer than 254 bytes"));
        }
        if let Some(first) = seen.insert(full.clone(), path) {
            return Err(PackError::CaseCollision {
                first: first.to_owned(),
                second: path.to_owned(),
            });
        }
        placed.push(Placed {
            folder: folder.to_owned(),
            name: name.to_owned(),
            file_hash: name_hash(name, true),
            at,
            path,
            compress,
        });
    }
    Ok(placed)
}

/// The sizes the limits are checked against, so that they can be tested without gigabytes of data.
fn check_limits(
    files: u64,
    folders: u64,
    index_len: u64,
    stored: impl Iterator<Item = (usize, u64)>,
    paths: &[&str],
) -> Result<(), PackError> {
    if files > u64::from(u32::MAX) || folders > u64::from(u32::MAX) {
        return Err(PackError::ArchiveTooLarge { size: index_len });
    }
    let mut total = index_len;
    for (at, size) in stored {
        if size > MAX_FILE_SIZE {
            return Err(PackError::FileTooLarge {
                path: paths[at].to_owned(),
                size,
            });
        }
        total += size;
    }
    if total > MAX_ARCHIVE_SIZE {
        return Err(PackError::ArchiveTooLarge { size: total });
    }
    Ok(())
}

type Folders<'a> = BTreeMap<u64, (String, Vec<&'a Placed<'a>>)>;

/// Group the files by folder hash, refuse equal hashes of different names, and order the files of each folder.
fn group<'a>(placed: &'a [Placed<'a>]) -> Result<Folders<'a>, PackError> {
    let mut folders: Folders<'a> = BTreeMap::new();
    for file in placed {
        let hash = name_hash(&file.folder, false);
        let slot = folders
            .entry(hash)
            .or_insert_with(|| (file.folder.clone(), Vec::new()));
        if slot.0 != file.folder {
            return Err(PackError::HashCollision {
                first: slot.0.clone(),
                second: file.folder.clone(),
            });
        }
        slot.1.push(file);
    }
    for (folder, files) in folders.values_mut() {
        files.sort_by(|a, b| {
            a.file_hash
                .cmp(&b.file_hash)
                .then_with(|| a.name.cmp(&b.name))
        });
        if let Some(pair) = files.windows(2).find(|w| w[0].file_hash == w[1].file_hash) {
            return Err(PackError::HashCollision {
                first: format!("{folder}\\{}", pair[0].name),
                second: format!("{folder}\\{}", pair[1].name),
            });
        }
    }
    Ok(folders)
}

/// The header, folder and file records and the file names of an archive whose files, in record order, have these
/// stored lengths.
fn index_bytes(
    folders: &Folders<'_>,
    ordered: &[&Placed<'_>],
    stored_len: &[u64],
    default_compressed: bool,
    paths: &[&str],
) -> Result<Vec<u8>, PackError> {
    let folder_names_len: u64 = folders.values().map(|(n, _)| n.len() as u64 + 1).sum();
    let file_names_len: u64 = ordered.iter().map(|p| p.name.len() as u64 + 1).sum();
    let index_len = u64::from(HEADER_LEN)
        + 16 * folders.len() as u64
        + folders.len() as u64
        + folder_names_len
        + 16 * ordered.len() as u64
        + file_names_len;
    check_limits(
        ordered.len() as u64,
        folders.len() as u64,
        index_len,
        stored_len.iter().copied().enumerate(),
        paths,
    )?;
    // Every limit above is checked, so these conversions hold.
    let u32_of = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);

    let mut flags = FLAG_DIR_NAMES | FLAG_FILE_NAMES;
    if default_compressed {
        flags |= FLAG_COMPRESSED;
    }
    let content = content_flags(ordered.iter().map(|p| p.name.as_str()));
    let mut index = Vec::with_capacity(usize::try_from(index_len).unwrap_or(0));
    index.extend_from_slice(b"BSA\0");
    index.extend_from_slice(&VERSION.to_le_bytes());
    index.extend_from_slice(&HEADER_LEN.to_le_bytes());
    index.extend_from_slice(&flags.to_le_bytes());
    index.extend_from_slice(&u32_of(folders.len() as u64).to_le_bytes());
    index.extend_from_slice(&u32_of(ordered.len() as u64).to_le_bytes());
    index.extend_from_slice(&u32_of(folder_names_len).to_le_bytes());
    index.extend_from_slice(&u32_of(file_names_len).to_le_bytes());
    index.extend_from_slice(&content.to_le_bytes());
    index.extend_from_slice(&0_u16.to_le_bytes());

    // source: bsa-v104-folder-records
    let mut block_at = u64::from(HEADER_LEN) + 16 * folders.len() as u64;
    for (hash, (name, files)) in folders {
        index.extend_from_slice(&hash.to_le_bytes());
        index.extend_from_slice(&u32_of(files.len() as u64).to_le_bytes());
        index.extend_from_slice(&u32_of(block_at + file_names_len).to_le_bytes());
        block_at += 1 + name.len() as u64 + 1 + 16 * files.len() as u64;
    }
    let mut data_at = index_len;
    let mut next = 0_usize;
    for (name, files) in folders.values() {
        index.push(u8::try_from(name.len() + 1).unwrap_or(u8::MAX));
        index.extend_from_slice(name.as_bytes());
        index.push(0);
        for file in files {
            // source: bsa-v104-file-records
            let len = stored_len[next];
            let mut size = u32_of(len);
            if file.compress != default_compressed {
                size |= SIZE_INVERT;
            }
            index.extend_from_slice(&file.file_hash.to_le_bytes());
            index.extend_from_slice(&size.to_le_bytes());
            index.extend_from_slice(&u32_of(data_at).to_le_bytes());
            data_at += len;
            next += 1;
        }
    }
    for file in ordered {
        index.extend_from_slice(file.name.as_bytes());
        index.push(0);
    }
    Ok(index)
}

/// Write a v104 archive holding `entries`, in memory.
///
/// Folders ascend by folder hash and the files of a folder by file hash (unsigned), so the result does not
/// depend on the order of `entries`. Each file is compressed (zlib, level 6) or stored as asked. The archive is
/// marked compressed by default when more than half of the files are compressed; the others carry the invert bit.
/// Flag 0x100 (embedded names) is never set. The bytes are those [`write_streamed`] writes for the same files.
///
/// # Errors
/// [`PackError`]: no entry, a path the format cannot hold, two paths that are one after case folding, equal
/// hashes of different names, a file over [`MAX_FILE_SIZE`], or an archive over [`MAX_ARCHIVE_SIZE`].
pub fn write(entries: &[PackEntry]) -> Result<Vec<u8>, PackError> {
    let sources: Vec<PackSource> = entries
        .iter()
        .map(|e| PackSource {
            path: e.path.clone(),
            len: e.data.len() as u64,
            compress: e.compress,
        })
        .collect();
    let mut out = Vec::new();
    let mut spool = std::io::Cursor::new(Vec::new());
    match write_streamed(
        &sources,
        &mut |at| Ok(entries[at].data.clone()),
        &mut spool,
        &mut out,
    ) {
        Ok(_) => Ok(out),
        Err(StreamError::Pack(e)) => Err(e),
        // memory cannot fail to read or write, and every length is the length of its bytes
        Err(StreamError::Io { .. }) => Err(PackError::Empty),
    }
}

/// Write a v104 archive holding `sources` to `out`, the same bytes as [`write`], holding at most one file and its
/// compressed copy in memory at a time.
///
/// `read(i)` returns the bytes of `sources[i]`; it is called once for each compressed file (in the first pass, which
/// writes the compressed copies to `spool`) and once for each stored file (while its bytes are written). `spool` is
/// scratch space the caller provides (a temporary file); it may be left with data in it. Returns the archive's
/// length.
///
/// # Errors
/// [`StreamError::Pack`] for what [`write`] refuses; [`StreamError::Io`] when `read` fails, returns another length
/// than the source gave, or `spool` or `out` cannot be written.
pub fn write_streamed<S, W>(
    sources: &[PackSource],
    read: &mut dyn FnMut(usize) -> std::io::Result<Vec<u8>>,
    spool: &mut S,
    out: &mut W,
) -> Result<u64, StreamError>
where
    S: std::io::Read + std::io::Write + std::io::Seek,
    W: std::io::Write,
{
    use std::io::{Read as _, SeekFrom};
    let io = |what: String| move |source| StreamError::Io { what, source };
    let placed = validate(sources.iter().map(|s| (s.path.as_str(), s.compress)))?;
    let folders = group(&placed)?;

    // Files in record order.
    let ordered: Vec<&Placed<'_>> = folders
        .values()
        .flat_map(|(_, f)| f.iter().copied())
        .collect();
    let compressed_count = ordered.iter().filter(|p| p.compress).count();
    let default_compressed = compressed_count * 2 > ordered.len();
    let read_checked = |read: &mut dyn FnMut(usize) -> std::io::Result<Vec<u8>>,
                        p: &Placed<'_>|
     -> Result<Vec<u8>, StreamError> {
        let data = read(p.at).map_err(io(format!("cannot read {}", p.path)))?;
        if data.len() as u64 != sources[p.at].len {
            return Err(StreamError::Io {
                what: format!("cannot pack {}", p.path),
                source: std::io::Error::other(format!(
                    "it has {} bytes, not the {} it was listed with",
                    data.len(),
                    sources[p.at].len
                )),
            });
        }
        Ok(data)
    };

    // First pass: the stored length of every file; compressed copies go to the spool, in record order.
    spool
        .seek(SeekFrom::Start(0))
        .map_err(io("cannot use the spool".to_owned()))?;
    let mut stored_len: Vec<u64> = Vec::with_capacity(ordered.len());
    for p in &ordered {
        if p.compress {
            let data = read_checked(read, p)?;
            let packed = miniz_oxide::deflate::compress_to_vec_zlib(&data, ZLIB_LEVEL);
            drop(data);
            let raw = u32::try_from(sources[p.at].len).unwrap_or(u32::MAX);
            spool
                .write_all(&raw.to_le_bytes())
                .and_then(|()| spool.write_all(&packed))
                .map_err(io(format!("cannot spool {}", p.path)))?;
            stored_len.push(4 + packed.len() as u64);
        } else {
            stored_len.push(sources[p.at].len);
        }
    }

    let paths: Vec<&str> = ordered.iter().map(|p| p.path).collect();
    let index = index_bytes(&folders, &ordered, &stored_len, default_compressed, &paths)?;
    out.write_all(&index)
        .map_err(io("cannot write the archive".to_owned()))?;
    let index_len = index.len() as u64;
    drop(index);

    // Second pass: the data blocks in record order, stored files from their source, compressed ones from the spool.
    spool
        .seek(SeekFrom::Start(0))
        .map_err(io("cannot use the spool".to_owned()))?;
    for (p, len) in ordered.iter().zip(&stored_len) {
        if p.compress {
            let copied = std::io::copy(&mut std::io::Read::by_ref(spool).take(*len), out)
                .map_err(io(format!("cannot copy {} into the archive", p.path)))?;
            if copied != *len {
                return Err(StreamError::Io {
                    what: format!("cannot copy {} into the archive", p.path),
                    source: std::io::ErrorKind::UnexpectedEof.into(),
                });
            }
        } else {
            let data = read_checked(read, p)?;
            out.write_all(&data)
                .map_err(io(format!("cannot write {} into the archive", p.path)))?;
        }
    }
    Ok(index_len + stored_len.iter().sum::<u64>())
}

#[cfg(test)]
mod tests {
    use super::{PackEntry, PackError, check_limits, name_hash, write};
    use crate::Archive;
    use std::io::Cursor;

    fn entry(path: &str, data: &[u8], compress: bool) -> PackEntry {
        PackEntry {
            path: path.to_owned(),
            data: data.to_vec(),
            compress,
        }
    }

    // Name and hash pairs read from the record tables of the owner's own game archives (names and numbers only,
    // row `bsa-name-hash`, oracle note); every one of the 43,000 records of five archives agreed.
    const VECTORS: &[(&str, bool, u64)] = &[
        (
            "meshes\\landscape\\lod\\wastelandnvmini\\blocks",
            false,
            0x1c18_9546_6d2b_6b73,
        ),
        (
            "wastelandnvmini.level4.x-4.y-4.nif",
            true,
            0x19fa_f20a_771e_ad34,
        ),
        (
            "wastelandnvmini.level4.x-4.y-8.nif",
            true,
            0x19fa_f20a_771e_ad38,
        ),
        ("nv_deathclawsign.nif", true, 0xbaea_89bd_6e10_e76e),
        ("snowglobenelisafb_d.dds", true, 0x039e_cb9a_7313_dfe4),
        ("snowglobenelisafb_n.dds", true, 0x039e_cb9a_7313_dfee),
        ("mtfastforward.kf", true, 0x0ef9_8d0c_6d0d_72e4),
        (
            "npc_mirelurkking_sonic_equip.wav",
            true,
            0x345c_7d09_ee1c_6970,
        ),
        ("falloutdict.txt", true, 0x04f8_5d54_660b_6374),
    ];

    #[test]
    fn name_hashes_match_the_numbers_measured_on_game_archives() {
        for (name, is_file, expected) in VECTORS {
            assert_eq!(
                name_hash(name, *is_file),
                *expected,
                "{name} hashed to {:#x}",
                name_hash(name, *is_file)
            );
        }
    }

    #[test]
    fn a_folder_name_with_a_dot_is_not_split() {
        assert_ne!(
            name_hash("textures\\a.dds", false),
            name_hash("textures\\a.dds", true)
        );
    }

    #[test]
    fn a_name_without_a_dot_has_no_extension_part() {
        // The stem is the whole name: low word from the stem alone, high word from the middle only.
        let hash = name_hash("abcde", true);
        assert_eq!(hash & 0xFFFF_FFFF, 0x6105_6465);
    }

    #[test]
    fn round_trip_gives_the_same_names_and_bytes_stored_and_compressed() {
        let a = vec![7_u8; 3000];
        let b = b"terrain".to_vec();
        let c = (0..=255_u8).cycle().take(5000).collect::<Vec<_>>();
        let bytes = write(&[
            entry("Textures/Landscape/Lod/a.dds", &a, true),
            entry("meshes\\landscape\\lod\\w\\blocks\\b.nif", &b, false),
            entry("meshes\\landscape\\lod\\w\\blocks\\c.nif", &c, true),
        ])
        .unwrap();
        let mut cursor = Cursor::new(&bytes);
        let archive = Archive::open(&mut cursor).unwrap();
        assert_eq!(archive.entries().len(), 3);
        for (path, data, compressed) in [
            ("textures\\landscape\\lod\\a.dds", &a, true),
            ("meshes\\landscape\\lod\\w\\blocks\\b.nif", &b, false),
            ("meshes\\landscape\\lod\\w\\blocks\\c.nif", &c, true),
        ] {
            let found = archive.find(path).expect(path);
            assert_eq!(found.is_compressed(), compressed, "{path}");
            assert_eq!(
                &archive.extract(&mut cursor, found, 1 << 20).unwrap(),
                data,
                "{path}"
            );
        }
    }

    /// Read the hashes back out of the written bytes: folder records, then file records of each folder.
    fn record_hashes(bytes: &[u8]) -> (Vec<u64>, Vec<Vec<u64>>) {
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let folders = u32_at(16) as usize;
        let all_names = u32_at(28) as usize;
        let mut folder_hashes = Vec::new();
        let mut file_hashes = Vec::new();
        for i in 0..folders {
            let at = 36 + 16 * i;
            folder_hashes.push(u64_at(at));
            let count = u32_at(at + 8) as usize;
            let block = u32_at(at + 12) as usize - all_names;
            let name_len = bytes[block] as usize;
            let first = block + 1 + name_len;
            file_hashes.push((0..count).map(|j| u64_at(first + 16 * j)).collect());
        }
        (folder_hashes, file_hashes)
    }

    #[test]
    fn records_hold_the_hashes_of_the_names_in_ascending_order() {
        let entries: Vec<PackEntry> = (0..40)
            .map(|i| {
                entry(
                    &format!(
                        "meshes\\landscape\\lod\\w{}\\blocks\\w.level4.x{i}.y{}.nif",
                        i % 5,
                        i * 4
                    ),
                    b"x",
                    false,
                )
            })
            .collect();
        let bytes = write(&entries).unwrap();
        let (folders, files) = record_hashes(&bytes);
        assert!(folders.windows(2).all(|w| w[0] < w[1]), "folders ascend");
        assert_eq!(folders.len(), 5);
        for hashes in &files {
            assert!(hashes.windows(2).all(|w| w[0] < w[1]), "files ascend");
        }
        let mut expected: Vec<u64> = (0..5)
            .map(|i| name_hash(&format!("meshes\\landscape\\lod\\w{i}\\blocks"), false))
            .collect();
        expected.sort_unstable();
        assert_eq!(folders, expected);
    }

    #[test]
    fn output_does_not_depend_on_the_order_of_the_input() {
        let entries: Vec<PackEntry> = (0..30)
            .map(|i| {
                entry(
                    &format!("textures\\d{}\\t{i}.dds", i % 4),
                    &[u8::try_from(i).unwrap(); 600],
                    i % 3 == 0,
                )
            })
            .collect();
        let mut reversed = entries.clone();
        reversed.reverse();
        let mut rotated = entries.clone();
        rotated.rotate_left(11);
        let reference = write(&entries).unwrap();
        assert_eq!(write(&reversed).unwrap(), reference);
        assert_eq!(write(&rotated).unwrap(), reference);
    }

    #[test]
    fn output_is_identical_when_written_from_many_threads() {
        let entries: Vec<PackEntry> = (0..20)
            .map(|i| {
                entry(
                    &format!("meshes\\m{}\\f{i}.nif", i % 3),
                    &[u8::try_from(i).unwrap(); 900],
                    true,
                )
            })
            .collect();
        let reference = write(&entries).unwrap();
        let results: Vec<Vec<u8>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..8)
                .map(|_| s.spawn(|| write(&entries).unwrap()))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(results.iter().all(|r| *r == reference));
    }

    #[test]
    fn the_default_compression_follows_the_majority_and_never_the_order() {
        let majority = write(&[
            entry("a\\1.dds", b"aaaaaaaa", true),
            entry("a\\2.dds", b"bbbbbbbb", true),
            entry("a\\3.dds", b"cc", false),
        ])
        .unwrap();
        assert_eq!(
            u32::from_le_bytes(majority[12..16].try_into().unwrap()) & 4,
            4
        );
        let tie = write(&[
            entry("a\\1.dds", b"aaaa", true),
            entry("a\\2.dds", b"bbbb", false),
        ])
        .unwrap();
        assert_eq!(u32::from_le_bytes(tie[12..16].try_into().unwrap()) & 4, 0);
    }

    #[test]
    fn the_header_never_sets_embedded_names_and_carries_content_flags() {
        let bytes = write(&[
            entry("meshes\\a\\x.nif", b"1", false),
            entry("textures\\a\\x.dds", b"2", true),
            entry("lodsettings\\w.dlodsettings", b"3", false),
        ])
        .unwrap();
        let flags = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        assert_eq!(flags & 0x100, 0);
        assert_eq!(flags & 3, 3);
        assert_eq!(
            u16::from_le_bytes(bytes[32..34].try_into().unwrap()),
            0x1 | 0x2 | 0x100
        );
    }

    #[test]
    fn two_paths_that_differ_only_in_case_are_refused() {
        let err = write(&[
            entry("Meshes/A.nif", b"1", false),
            entry("meshes\\a.NIF", b"2", false),
        ])
        .unwrap_err();
        assert_eq!(
            err,
            PackError::CaseCollision {
                first: "Meshes/A.nif".into(),
                second: "meshes\\a.NIF".into()
            }
        );
    }

    #[test]
    fn two_different_names_with_one_hash_are_refused() {
        let names = colliding_names(true, ".txt");
        let err = write(&[
            entry(&format!("f\\{}", names.0), b"1", false),
            entry(&format!("f\\{}", names.1), b"2", false),
        ])
        .unwrap_err();
        assert_eq!(
            err,
            PackError::HashCollision {
                first: format!("f\\{}", names.0.clone().min(names.1.clone())),
                second: format!("f\\{}", names.0.max(names.1))
            }
        );
    }

    /// Two different names with the same hash. The middle letters enter the high word through the recurrence
    /// `h = h * 0x1003F + c` (wrapping 32-bit), so the differences (-1, -3, -1, 5, 5, -1, -3, -1) between eight
    /// middle letters cancel: found by a meet-in-the-middle search and checked here. `suffix` is appended to both.
    fn colliding_names(is_file: bool, suffix: &str) -> (String, String) {
        let a = format!("xppppppppyz{suffix}");
        let b = format!("xomouuomoyz{suffix}");
        assert_eq!(name_hash(&a, is_file), name_hash(&b, is_file));
        (a, b)
    }

    #[test]
    fn two_folders_with_one_hash_are_refused() {
        let names = colliding_names(false, "");
        let err = write(&[
            entry(&format!("{}\\a.txt", names.0), b"1", false),
            entry(&format!("{}\\a.txt", names.1), b"2", false),
        ])
        .unwrap_err();
        assert!(matches!(err, PackError::HashCollision { .. }), "{err:?}");
    }

    #[test]
    fn paths_the_format_cannot_hold_are_refused() {
        let long = "a".repeat(255);
        for (path, reason) in [
            ("file.nif", "the path has no folder part"),
            ("a\\", "the path has no folder part"),
            ("\\a.nif", "the path has no folder part"),
            ("a\\\\b\\c.nif", "a folder name is empty"),
            ("caf\u{e9}\\a.nif", "the name is not ASCII"),
        ] {
            assert_eq!(
                write(&[entry(path, b"1", false)]).unwrap_err(),
                PackError::BadPath {
                    path: path.to_owned(),
                    reason
                },
                "{path:?}"
            );
        }
        assert!(matches!(
            write(&[entry(&format!("{long}\\a.nif"), b"1", false)]).unwrap_err(),
            PackError::BadPath { .. }
        ));
        assert!(matches!(
            write(&[entry(&format!("a\\{long}"), b"1", false)]).unwrap_err(),
            PackError::BadPath { .. }
        ));
        assert!(write(&[entry(&format!("a\\{}", "b".repeat(254)), b"1", false)]).is_ok());
    }

    #[test]
    fn no_entry_is_refused() {
        assert_eq!(write(&[]).unwrap_err(), PackError::Empty);
    }

    #[test]
    fn a_file_over_one_gibibyte_is_refused_with_its_path() {
        let err = check_limits(
            1,
            1,
            100,
            [(0_usize, 0x4000_0000_u64)].into_iter(),
            &["a\\big.dds"],
        )
        .unwrap_err();
        assert_eq!(
            err,
            PackError::FileTooLarge {
                path: "a\\big.dds".into(),
                size: 0x4000_0000
            }
        );
        assert!(
            check_limits(
                1,
                1,
                100,
                [(0_usize, 0x3FFF_FFFF_u64)].into_iter(),
                &["a\\b"]
            )
            .is_ok()
        );
    }

    #[test]
    fn an_archive_over_four_gibibytes_is_refused_and_says_to_split() {
        let sizes = (0..5).map(|i| (i, 0x3FFF_0000_u64));
        let paths = ["a"; 5];
        let err = check_limits(5, 1, 100, sizes, &paths).unwrap_err();
        assert!(matches!(err, PackError::ArchiveTooLarge { size } if size > 0xFFFF_FFFF));
        assert!(err.to_string().contains("split"));
    }

    #[test]
    fn more_files_than_a_u32_counts_is_refused() {
        let err =
            check_limits(u64::from(u32::MAX) + 1, 1, 100, std::iter::empty(), &[]).unwrap_err();
        assert!(matches!(err, PackError::ArchiveTooLarge { .. }));
    }

    #[test]
    fn error_messages_name_the_problem() {
        assert!(PackError::Empty.to_string().contains("no file"));
        assert!(
            PackError::BadPath {
                path: "p".into(),
                reason: "r"
            }
            .to_string()
            .contains("\"p\"")
        );
        assert!(
            PackError::HashCollision {
                first: "a".into(),
                second: "b".into()
            }
            .to_string()
            .contains("same name hash")
        );
        assert!(
            PackError::FileTooLarge {
                path: "p".into(),
                size: 9
            }
            .to_string()
            .contains('9')
        );
    }
}
