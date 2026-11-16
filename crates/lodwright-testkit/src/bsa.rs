// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic BSA v104 archives, built in memory.
//!
//! Layout used: the 36-byte header, 16-byte folder records, one block per folder (length-prefixed folder name, then
//! 16-byte file records), the file-name block, then the file data. Hashes are stand-ins (the real hash function has
//! no provenance row, and the reader ignores hashes). Compression is zlib, as in v104.

use miniz_oxide::deflate::compress_to_vec_zlib;

// source: bsa-v104-header
const HEADER_LEN: u32 = 36;
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
// source: bsa-v104-file-records
const SIZE_INVERT: u32 = 0x4000_0000;

/// One file to store.
#[derive(Debug, Clone)]
struct File {
    path: String,
    data: Vec<u8>,
    invert: bool,
}

/// Builder for a synthetic archive.
#[derive(Debug, Clone, Default)]
pub struct BsaBuilder {
    files: Vec<File>,
    compressed: bool,
    embed_names: bool,
    big_endian_flag: bool,
    version: Option<u32>,
}

impl BsaBuilder {
    /// An empty archive: version 104, not compressed by default.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compress files by default (archive flag 0x4).
    #[must_use]
    pub fn compressed(mut self) -> Self {
        self.compressed = true;
        self
    }

    /// Embed each file's full path at the start of its data block (archive flag 0x100).
    #[must_use]
    pub fn embed_names(mut self) -> Self {
        self.embed_names = true;
        self
    }

    /// Set the big-endian flag (0x40) without changing the bytes, to test that the reader refuses it.
    #[must_use]
    pub fn big_endian_flag(mut self) -> Self {
        self.big_endian_flag = true;
        self
    }

    /// Write another version number, to test that the reader refuses it.
    #[must_use]
    pub fn version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    /// Add a file by its `folder\name` path (written lower case, with backslashes).
    ///
    /// # Panics
    /// If the path has no folder part or a part is over 254 bytes.
    #[must_use]
    pub fn file(self, path: &str, data: &[u8]) -> Self {
        self.push(path, data, false)
    }

    /// Add a file whose compression is the opposite of the archive default (size bit 30).
    ///
    /// # Panics
    /// As [`BsaBuilder::file`].
    #[must_use]
    pub fn file_inverted(self, path: &str, data: &[u8]) -> Self {
        self.push(path, data, true)
    }

    fn push(mut self, path: &str, data: &[u8], invert: bool) -> Self {
        let path = path.to_ascii_lowercase().replace('/', "\\");
        let (folder, name) = path.rsplit_once('\\').expect("a path needs a folder");
        assert!(folder.len() < 255 && name.len() < 255, "name too long");
        self.files.push(File {
            path,
            data: data.to_vec(),
            invert,
        });
        self
    }

    /// The archive bytes.
    ///
    /// # Panics
    /// If a size does not fit 32 bits.
    #[must_use]
    pub fn build(&self) -> Vec<u8> {
        // folders in first-seen order, files in the order added
        let mut folders: Vec<(String, Vec<&File>)> = Vec::new();
        for file in &self.files {
            let folder = file.path.rsplit_once('\\').unwrap().0;
            match folders.iter_mut().find(|(name, _)| name == folder) {
                Some((_, files)) => files.push(file),
                None => folders.push((folder.to_owned(), vec![file])),
            }
        }
        let size32 = |n: usize| u32::try_from(n).expect("fits 32 bits");
        let folder_names: usize = folders.iter().map(|(n, _)| n.len() + 1).sum();
        let file_names: usize = folders
            .iter()
            .flat_map(|(_, f)| f)
            .map(|f| f.path.rsplit_once('\\').unwrap().1.len() + 1)
            .sum();
        let folder_records = 16 * folders.len();
        let blocks_len: usize = folders
            .iter()
            .map(|(n, files)| 1 + n.len() + 1 + 16 * files.len())
            .sum();
        let data_start = HEADER_LEN as usize + folder_records + blocks_len + file_names;

        // data blocks, with their offsets and stored sizes
        let mut data = Vec::new();
        let mut placed: Vec<(u32, u32)> = Vec::new();
        for file in folders.iter().flat_map(|(_, f)| f) {
            let compress = self.compressed != file.invert;
            let mut block = Vec::new();
            if self.embed_names {
                block.push(u8::try_from(file.path.len()).expect("short path"));
                block.extend_from_slice(file.path.as_bytes());
            }
            if compress {
                block.extend_from_slice(&size32(file.data.len()).to_le_bytes());
                block.extend_from_slice(&compress_to_vec_zlib(&file.data, 6));
            } else {
                block.extend_from_slice(&file.data);
            }
            let mut size = size32(block.len());
            if file.invert {
                size |= SIZE_INVERT;
            }
            placed.push((size32(data_start + data.len()), size));
            data.extend_from_slice(&block);
        }

        let mut flags = FLAG_DIR_NAMES | FLAG_FILE_NAMES;
        if self.compressed {
            flags |= FLAG_COMPRESSED;
        }
        if self.embed_names {
            flags |= FLAG_EMBED_NAMES;
        }
        if self.big_endian_flag {
            flags |= FLAG_BIG_ENDIAN;
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"BSA\0");
        out.extend_from_slice(&self.version.unwrap_or(104).to_le_bytes());
        out.extend_from_slice(&HEADER_LEN.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&size32(folders.len()).to_le_bytes());
        out.extend_from_slice(&size32(self.files.len()).to_le_bytes());
        out.extend_from_slice(&size32(folder_names).to_le_bytes());
        out.extend_from_slice(&size32(file_names).to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());

        // source: bsa-v104-folder-records
        let mut block_at = HEADER_LEN as usize + folder_records;
        for (index, (name, files)) in folders.iter().enumerate() {
            out.extend_from_slice(&(index as u64 + 1).to_le_bytes());
            out.extend_from_slice(&size32(files.len()).to_le_bytes());
            out.extend_from_slice(&size32(block_at + file_names).to_le_bytes());
            block_at += 1 + name.len() + 1 + 16 * files.len();
        }
        let mut file_index = 0_usize;
        for (name, files) in &folders {
            out.push(u8::try_from(name.len() + 1).expect("short name"));
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            for _ in files {
                let (offset, size) = placed[file_index];
                out.extend_from_slice(&(1000 + file_index as u64).to_le_bytes());
                out.extend_from_slice(&size.to_le_bytes());
                out.extend_from_slice(&offset.to_le_bytes());
                file_index += 1;
            }
        }
        // source: bsa-v104-file-records
        for file in folders.iter().flat_map(|(_, f)| f) {
            out.extend_from_slice(file.path.rsplit_once('\\').unwrap().1.as_bytes());
            out.push(0);
        }
        out.extend_from_slice(&data);
        out
    }
}
