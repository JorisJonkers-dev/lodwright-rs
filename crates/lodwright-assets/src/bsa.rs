// SPDX-License-Identifier: GPL-3.0-only
//! BSA version 104 reader: header, folder and file records, names and optional zlib compression.
//!
//! Opening an archive reads only the index (header, records, names), never the file data, so a multi-gigabyte
//! archive costs memory in proportion to its file count. [`Archive::extract`] then reads one entry, refusing to
//! allocate more than a caller-given cap.
//!
//! Derived from: the UESP archive-format pages (`provenance/sources.toml`: `bsa-v104-header`,
//! `bsa-v104-folder-records`, `bsa-v104-file-records`, `bsa-v104-data-blocks`, `bsa-path-form`). Never from
//! decompiled or unlicensed code.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};

use crate::Error;
use crate::path::normalise;

// source: bsa-v104-header
const HEADER_LEN: usize = 36;
// source: bsa-v104-header
const VERSION: u32 = 104;
// source: bsa-v104-header
const FLAG_DIR_NAMES: u32 = 0x1;
// source: bsa-v104-header
const FLAG_FILE_NAMES: u32 = 0x2;
// source: bsa-v104-header
const FLAG_COMPRESSED: u32 = 0x4;
// source: bsa-v104-header
const FLAG_BIG_ENDIAN: u32 = 0x40;
// source: bsa-v104-header
const FLAG_EMBED_NAMES: u32 = 0x100;
// source: bsa-v104-header
const FLAG_XMEM: u32 = 0x200;
// source: bsa-v104-folder-records
const FOLDER_RECORD_LEN: usize = 16;
// source: bsa-v104-file-records
const FILE_RECORD_LEN: usize = 16;
// source: bsa-v104-file-records
const SIZE_INVERT: u32 = 0x4000_0000;
// source: bsa-v104-file-records
const SIZE_MASK: u32 = 0x3FFF_FFFF;

/// One file in an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    path: String,
    offset: u32,
    stored_size: u32,
    compressed: bool,
}

impl Entry {
    /// The full lower-case path, `folder\name`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Whether the data is stored zlib-compressed.
    #[must_use]
    pub fn is_compressed(&self) -> bool {
        self.compressed
    }

    /// Bytes the entry occupies in the archive from its offset: with archive flag 0x100 the embedded path (one
    /// length byte plus the path) is counted, then, for a compressed file, the length word and the zlib stream
    /// (measured on 34355 files of the game's own archives, row `bsa-v104-data-blocks`).
    #[must_use]
    pub fn stored_size(&self) -> u32 {
        self.stored_size
    }
}

/// The index of an opened archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archive {
    entries: Vec<Entry>,
    lookup: HashMap<String, usize>,
    embedded_names: bool,
    length: u64,
}

fn read_exact<R: Read>(reader: &mut R, len: usize, what: &'static str) -> Result<Vec<u8>, Error> {
    let mut buffer = vec![0_u8; len];
    reader
        .read_exact(&mut buffer)
        .map_err(|e| Error::from_io(&e, what))?;
    Ok(buffer)
}

fn le_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn seek<R: Seek>(reader: &mut R, to: u64, what: &'static str) -> Result<(), Error> {
    reader
        .seek(SeekFrom::Start(to))
        .map(|_| ())
        .map_err(|e| Error::from_io(&e, what))
}

/// The counts and flags of a header.
struct Header {
    flags: u32,
    folder_count: u64,
    file_count: u64,
    folder_names_len: u64,
    file_names_len: u64,
}

fn parse_header(header: &[u8]) -> Result<Header, Error> {
    // source: bsa-v104-header
    if &header[0..4] != b"BSA\0" {
        return Err(Error::NotAnArchive);
    }
    let version = le_u32(header, 4);
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    if usize::try_from(le_u32(header, 8)) != Ok(HEADER_LEN) {
        return Err(Error::Malformed("folder records do not follow the header"));
    }
    let flags = le_u32(header, 12);
    if flags & (FLAG_BIG_ENDIAN | FLAG_XMEM) != 0 {
        return Err(Error::Unsupported("big-endian or XMem archive"));
    }
    if flags & (FLAG_DIR_NAMES | FLAG_FILE_NAMES) != (FLAG_DIR_NAMES | FLAG_FILE_NAMES) {
        return Err(Error::Unsupported("archive without folder or file names"));
    }
    Ok(Header {
        flags,
        folder_count: u64::from(le_u32(header, 16)),
        file_count: u64::from(le_u32(header, 20)),
        folder_names_len: u64::from(le_u32(header, 24)),
        file_names_len: u64::from(le_u32(header, 28)),
    })
}

impl Archive {
    /// Read the index of the archive in `reader`.
    ///
    /// # Errors
    /// [`Error::NotAnArchive`], [`Error::UnsupportedVersion`], [`Error::Unsupported`] (big-endian or `XMem` archives,
    /// or an archive without folder or file names), [`Error::Truncated`] when the archive ends early and
    /// [`Error::Malformed`] when its counts, offsets or names contradict each other or the archive length.
    pub fn open<R: Read + Seek>(reader: &mut R) -> Result<Self, Error> {
        let length = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| Error::from_io(&e, "the archive"))?;
        seek(reader, 0, "the header")?;
        let header = read_exact(reader, HEADER_LEN, "the header")?;
        let Header {
            flags,
            folder_count,
            file_count,
            folder_names_len,
            file_names_len,
        } = parse_header(&header)?;

        // Every count and length must fit in the archive before anything is allocated for it.
        let blocks = folder_names_len
            .checked_add(folder_count)
            .and_then(|n| n.checked_add(FILE_RECORD_LEN as u64 * file_count))
            .ok_or(Error::Malformed("counts overflow"))?;
        let index_len =
            HEADER_LEN as u64 + FOLDER_RECORD_LEN as u64 * folder_count + blocks + file_names_len;
        if index_len > length {
            return Err(Error::Truncated { what: "the index" });
        }

        // source: bsa-v104-folder-records
        let folder_records = read_exact(
            reader,
            usize::try_from(folder_count)
                .ok()
                .and_then(|n| n.checked_mul(FOLDER_RECORD_LEN))
                .ok_or(Error::Malformed("too many folders"))?,
            "the folder records",
        )?;
        let mut folders = Vec::new();
        let mut files_claimed = 0_u64;
        for record in folder_records.as_chunks::<FOLDER_RECORD_LEN>().0 {
            let count = u64::from(le_u32(record, 8));
            let offset = u64::from(le_u32(record, 12));
            files_claimed += count;
            let at = offset
                .checked_sub(file_names_len)
                .ok_or(Error::Malformed("a folder offset is inside the header"))?;
            folders.push((count, at));
        }
        if files_claimed != file_count {
            return Err(Error::Malformed("folder file counts do not add up"));
        }

        let mut pending: Vec<(String, u32, u32)> = Vec::new();
        for (count, at) in folders {
            seek(reader, at, "a folder block")?;
            let name_len = read_exact(reader, 1, "a folder name")?[0];
            let raw = read_exact(reader, usize::from(name_len), "a folder name")?;
            let folder = std::str::from_utf8(raw.strip_suffix(&[0]).unwrap_or(&raw))
                .map_err(|_| Error::Malformed("a folder name is not text"))?
                .to_owned();
            // source: bsa-v104-file-records
            let records = read_exact(
                reader,
                usize::try_from(count)
                    .ok()
                    .and_then(|n| n.checked_mul(FILE_RECORD_LEN))
                    .ok_or(Error::Malformed("too many files"))?,
                "the file records",
            )?;
            for record in records.as_chunks::<FILE_RECORD_LEN>().0 {
                pending.push((folder.clone(), le_u32(record, 8), le_u32(record, 12)));
            }
        }

        // The name block follows the last folder block (blocks are stored one after another).
        let names_at = HEADER_LEN as u64 + FOLDER_RECORD_LEN as u64 * folder_count + blocks;
        seek(reader, names_at, "the file names")?;
        let name_block = read_exact(
            reader,
            usize::try_from(file_names_len).map_err(|_| Error::Malformed("name block"))?,
            "the file names",
        )?;
        let mut names = name_block.split(|b| *b == 0);
        let default_compressed = flags & FLAG_COMPRESSED != 0;
        let mut entries = Vec::with_capacity(pending.len());
        let mut lookup = HashMap::with_capacity(pending.len());
        for (folder, size, offset) in pending {
            let name = names
                .next()
                .ok_or(Error::Malformed("fewer file names than files"))?;
            let name = std::str::from_utf8(name)
                .map_err(|_| Error::Malformed("a file name is not text"))?;
            let path = normalise(&format!("{folder}\\{name}"));
            let entry = Entry {
                path: path.clone(),
                offset,
                stored_size: size & SIZE_MASK,
                compressed: default_compressed != (size & SIZE_INVERT != 0),
            };
            // The first entry of a path wins if an archive repeats one.
            lookup.entry(path).or_insert(entries.len());
            entries.push(entry);
        }
        Ok(Self {
            entries,
            lookup,
            embedded_names: flags & FLAG_EMBED_NAMES != 0,
            length,
        })
    }

    /// Every file, in archive order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry for `path` (case-insensitive, either separator).
    #[must_use]
    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.lookup.get(&normalise(path)).map(|i| &self.entries[*i])
    }

    /// Read one entry's bytes, inflating them when stored compressed. Never allocates more than about `cap`
    /// bytes for the result and the stored data.
    ///
    /// # Errors
    /// [`Error::TooLarge`] when the entry claims more than `cap` bytes (decompressed, or stored plus slack),
    /// [`Error::Truncated`] when the data lies outside the archive, [`Error::Decompress`] when a compressed
    /// stream does not inflate to exactly the size it claims.
    pub fn extract<R: Read + Seek>(
        &self,
        reader: &mut R,
        entry: &Entry,
        cap: u64,
    ) -> Result<Vec<u8>, Error> {
        let stored = u64::from(entry.stored_size);
        u64::from(entry.offset)
            .checked_add(stored)
            .filter(|end| *end <= self.length)
            .ok_or(Error::Truncated { what: "an entry" })?;
        // An incompressible file compresses to slightly more than itself; allow for that and the length word.
        let stored_cap = cap.saturating_add(cap / 8).saturating_add(4096);
        if stored > stored_cap {
            return Err(Error::TooLarge {
                claimed: stored,
                cap,
            });
        }
        seek(reader, u64::from(entry.offset), "an entry")?;
        let mut block = read_exact(
            reader,
            usize::try_from(stored).map_err(|_| Error::Malformed("entry size"))?,
            "an entry",
        )?;
        // source: bsa-v104-data-blocks
        if self.embedded_names {
            let name_len = usize::from(*block.first().ok_or(Error::Truncated {
                what: "an embedded path",
            })?);
            if 1 + name_len > block.len() {
                return Err(Error::Truncated {
                    what: "an embedded path",
                });
            }
            block.drain(..=name_len);
        }
        if !entry.compressed {
            return if block.len() as u64 > cap {
                Err(Error::TooLarge {
                    claimed: block.len() as u64,
                    cap,
                })
            } else {
                Ok(block)
            };
        }
        let (size_bytes, stream) = block.split_first_chunk::<4>().ok_or(Error::Truncated {
            what: "a compressed block",
        })?;
        let claimed = u64::from(u32::from_le_bytes(*size_bytes));
        if claimed > cap {
            return Err(Error::TooLarge { claimed, cap });
        }
        let claimed = usize::try_from(claimed).map_err(|_| Error::Decompress)?;
        let plain = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stream, claimed)
            .map_err(|_| Error::Decompress)?;
        if plain.len() != claimed {
            return Err(Error::Decompress);
        }
        Ok(plain)
    }
}

#[cfg(test)]
mod tests {
    use super::Archive;
    use crate::Error;
    use lodwright_testkit::bsa::BsaBuilder;
    use lodwright_testkit::rng::Rng;
    use std::io::Cursor;

    const CAP: u64 = 1 << 20;

    fn files() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("textures\\landscape\\dirt.dds", vec![7; 300]),
            ("textures\\landscape\\sand.dds", (0..=255).collect()),
            ("meshes\\rock.nif", b"rock".to_vec()),
            ("meshes\\empty.nif", Vec::new()),
        ]
    }

    fn build(builder: BsaBuilder) -> Vec<u8> {
        files()
            .iter()
            .fold(builder, |b, (path, data)| b.file(path, data))
            .build()
    }

    fn read_back(bytes: &[u8]) {
        let mut cursor = Cursor::new(bytes);
        let archive = Archive::open(&mut cursor).unwrap();
        assert_eq!(archive.entries().len(), files().len());
        for (path, data) in files() {
            let entry = archive.find(path).unwrap();
            assert_eq!(archive.extract(&mut cursor, entry, CAP).unwrap(), data);
        }
    }

    #[test]
    fn an_uncompressed_archive_reads_back_exactly() {
        read_back(&build(BsaBuilder::new()));
    }

    #[test]
    fn a_compressed_archive_reads_back_exactly() {
        let bytes = build(BsaBuilder::new().compressed());
        read_back(&bytes);
        let archive = Archive::open(&mut Cursor::new(&bytes)).unwrap();
        assert!(archive.entries().iter().all(super::Entry::is_compressed));
    }

    #[test]
    fn embedded_names_are_skipped_in_both_modes() {
        read_back(&build(BsaBuilder::new().embed_names()));
        read_back(&build(BsaBuilder::new().compressed().embed_names()));
    }

    #[test]
    fn the_stored_size_counts_the_embedded_path() {
        // The measured rule: with flag 0x100 the stored size is the path (one length byte and the path), plus the
        // length word and the zlib stream when compressed. The expected sizes are computed here, not by the builder.
        let path = "a\\b.bin";
        let data = [9_u8; 40];
        let plain = BsaBuilder::new().embed_names().file(path, &data).build();
        let archive = Archive::open(&mut Cursor::new(&plain)).unwrap();
        let entry = archive.find(path).unwrap();
        assert_eq!(entry.stored_size() as usize, 1 + path.len() + data.len());
        let zipped = BsaBuilder::new()
            .compressed()
            .embed_names()
            .file(path, &data)
            .build();
        let archive = Archive::open(&mut Cursor::new(&zipped)).unwrap();
        let entry = archive.find(path).unwrap();
        // The independent check of the measurement: the last block ends where the archive ends, and the stream
        // consumes everything after the length word.
        let body = entry.stored_size() as usize - (1 + path.len() + 4);
        assert_eq!(
            &zipped[zipped.len() - body..zipped.len() - body + 2],
            &[0x78, 0x9c][..],
            "a zlib header follows the path and the length word"
        );
        // The last block ends at the end of the archive: no byte after the counted size.
        assert_eq!(
            u64::from(entry.offset) + u64::from(entry.stored_size()),
            zipped.len() as u64
        );
    }

    #[test]
    fn an_embedded_path_longer_than_its_block_is_truncated() {
        let mut bytes = BsaBuilder::new()
            .embed_names()
            .file("a\\b.bin", &[1; 8])
            .build();
        // Point the length byte of the one block past the end of the archive.
        let archive = Archive::open(&mut Cursor::new(&bytes)).unwrap();
        let at = archive.entries()[0].offset as usize;
        bytes[at] = 255;
        let mut cursor = Cursor::new(&bytes);
        let archive = Archive::open(&mut cursor).unwrap();
        let entry = &archive.entries()[0];
        assert!(matches!(
            archive.extract(&mut cursor, entry, CAP),
            Err(Error::Truncated { .. })
        ));
    }

    #[test]
    fn a_file_can_invert_the_archive_default() {
        for compressed in [false, true] {
            let mut builder = BsaBuilder::new();
            if compressed {
                builder = builder.compressed();
            }
            let bytes = builder
                .file("a\\plain.bin", &[1; 64])
                .file_inverted("a\\flipped.bin", &[2; 64])
                .build();
            let mut cursor = Cursor::new(&bytes);
            let archive = Archive::open(&mut cursor).unwrap();
            assert_eq!(
                archive.find("a\\plain.bin").unwrap().is_compressed(),
                compressed
            );
            let flipped = archive.find("a\\flipped.bin").unwrap();
            assert_eq!(flipped.is_compressed(), !compressed);
            assert_eq!(
                archive.extract(&mut cursor, flipped, CAP).unwrap(),
                vec![2; 64]
            );
        }
    }

    #[test]
    fn lookup_ignores_case_and_separator_style() {
        let bytes = build(BsaBuilder::new());
        let archive = Archive::open(&mut Cursor::new(&bytes)).unwrap();
        assert!(archive.find("Textures/Landscape/DIRT.dds").is_some());
        assert!(archive.find("textures\\landscape\\missing.dds").is_none());
    }

    #[test]
    fn an_empty_archive_opens_with_no_entries() {
        let bytes = BsaBuilder::new().build();
        let archive = Archive::open(&mut Cursor::new(&bytes)).unwrap();
        assert_eq!(archive.entries(), []);
    }

    #[test]
    fn the_wrong_magic_is_not_an_archive() {
        let mut bytes = build(BsaBuilder::new());
        bytes[0] = b'X';
        assert_eq!(
            Archive::open(&mut Cursor::new(&bytes)),
            Err(Error::NotAnArchive)
        );
    }

    #[test]
    fn other_versions_are_refused_by_number() {
        for version in [103, 105] {
            let bytes = build(BsaBuilder::new().version(version));
            assert_eq!(
                Archive::open(&mut Cursor::new(&bytes)),
                Err(Error::UnsupportedVersion(version))
            );
        }
    }

    #[test]
    fn a_big_endian_archive_is_unsupported() {
        let bytes = build(BsaBuilder::new().big_endian_flag());
        assert!(matches!(
            Archive::open(&mut Cursor::new(&bytes)),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn every_truncation_of_an_archive_is_a_typed_error_or_a_readable_archive() {
        let bytes = build(BsaBuilder::new().compressed());
        for len in 0..bytes.len() {
            let mut cursor = Cursor::new(&bytes[..len]);
            match Archive::open(&mut cursor) {
                Ok(archive) => {
                    for entry in archive.entries() {
                        let _ = archive.extract(&mut cursor, entry, CAP);
                    }
                }
                Err(error) => assert!(
                    matches!(
                        error,
                        Error::Truncated { .. } | Error::Malformed(_) | Error::NotAnArchive
                    ),
                    "unexpected {error:?} at {len}"
                ),
            }
        }
    }

    #[test]
    fn cutting_the_data_gives_a_truncated_entry() {
        let bytes = build(BsaBuilder::new());
        let full = Archive::open(&mut Cursor::new(&bytes)).unwrap();
        let last = full
            .entries()
            .iter()
            .max_by_key(|e| e.offset)
            .unwrap()
            .clone();
        let short = &bytes[..bytes.len() - 1];
        let mut cursor = Cursor::new(short);
        let archive = Archive::open(&mut cursor).unwrap();
        let result = archive.extract(&mut cursor, &last, CAP);
        // the last stored entry (or an empty one) either still fits or is reported truncated
        assert!(result.is_ok() || result == Err(Error::Truncated { what: "an entry" }));
        if last.stored_size() > 0 {
            assert_eq!(result, Err(Error::Truncated { what: "an entry" }));
        }
    }

    #[test]
    fn random_corruption_never_panics() {
        let original = build(BsaBuilder::new().compressed().embed_names());
        let mut rng = Rng::new(0x00B5_A104);
        for _ in 0..400 {
            let mut bytes = original.clone();
            for _ in 0..=rng.below(4) {
                let at = usize::try_from(rng.below(u32::try_from(bytes.len()).unwrap())).unwrap();
                bytes[at] = u8::try_from(rng.below(256)).unwrap();
            }
            let mut cursor = Cursor::new(&bytes);
            if let Ok(archive) = Archive::open(&mut cursor) {
                for entry in archive.entries() {
                    let _ = archive.extract(&mut cursor, entry, CAP);
                }
            }
        }
    }

    #[test]
    fn an_entry_claiming_more_than_the_cap_is_refused_before_inflating() {
        let bytes = BsaBuilder::new()
            .compressed()
            .file("a\\zeros.bin", &vec![0_u8; 100_000])
            .build();
        let mut cursor = Cursor::new(&bytes);
        let archive = Archive::open(&mut cursor).unwrap();
        let entry = archive.find("a\\zeros.bin").unwrap();
        assert_eq!(
            archive.extract(&mut cursor, entry, 1000),
            Err(Error::TooLarge {
                claimed: 100_000,
                cap: 1000
            })
        );
        assert_eq!(
            archive.extract(&mut cursor, entry, 100_000).unwrap().len(),
            100_000
        );
    }

    #[test]
    fn a_stream_that_inflates_past_its_claimed_size_is_a_decompression_error() {
        // a bomb: the length word says 16 bytes, the stream holds 100 000
        let mut bytes = BsaBuilder::new()
            .compressed()
            .file("a\\bomb.bin", &vec![0_u8; 100_000])
            .build();
        let archive = Archive::open(&mut Cursor::new(&bytes)).unwrap();
        let at = usize::try_from(archive.entries()[0].offset).unwrap();
        bytes[at..at + 4].copy_from_slice(&16_u32.to_le_bytes());
        let mut cursor = Cursor::new(&bytes);
        let archive = Archive::open(&mut cursor).unwrap();
        assert_eq!(
            archive.extract(&mut cursor, &archive.entries()[0], CAP),
            Err(Error::Decompress)
        );
    }

    #[test]
    fn a_huge_claimed_size_never_allocates() {
        let mut bytes = BsaBuilder::new()
            .compressed()
            .file("a\\x.bin", &[1; 10])
            .build();
        let archive = Archive::open(&mut Cursor::new(&bytes)).unwrap();
        let at = usize::try_from(archive.entries()[0].offset).unwrap();
        bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut cursor = Cursor::new(&bytes);
        let archive = Archive::open(&mut cursor).unwrap();
        assert_eq!(
            archive.extract(&mut cursor, &archive.entries()[0], CAP),
            Err(Error::TooLarge {
                claimed: u64::from(u32::MAX),
                cap: CAP
            })
        );
    }

    #[test]
    fn an_oversized_stored_entry_is_refused_by_its_stored_size() {
        let bytes = BsaBuilder::new()
            .file("a\\big.bin", &vec![9; 50_000])
            .build();
        let mut cursor = Cursor::new(&bytes);
        let archive = Archive::open(&mut cursor).unwrap();
        let entry = &archive.entries()[0];
        assert!(matches!(
            archive.extract(&mut cursor, entry, 100),
            Err(Error::TooLarge { .. })
        ));
        // within the slack for incompressible data, but over the cap after the path is dropped
        let bytes = BsaBuilder::new().file("a\\mid.bin", &[9; 600]).build();
        let mut cursor = Cursor::new(&bytes);
        let archive = Archive::open(&mut cursor).unwrap();
        assert!(matches!(
            archive.extract(&mut cursor, &archive.entries()[0], 500),
            Err(Error::TooLarge {
                claimed: 600,
                cap: 500
            })
        ));
    }

    #[test]
    fn counts_that_exceed_the_archive_are_refused_without_allocating() {
        let mut bytes = build(BsaBuilder::new());
        bytes[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            Archive::open(&mut Cursor::new(&bytes)),
            Err(Error::Truncated { what: "the index" })
        );
    }
}
