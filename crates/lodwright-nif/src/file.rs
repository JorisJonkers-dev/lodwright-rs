// SPDX-License-Identifier: GPL-3.0-only
//! The NIF container: header, block table, blocks and footer.
//!
//! Derived from: `Core/wbDataFormatNif.pas` and `Core/wbDataFormatNifTypes.pas` in TES5Edit/TES5Edit at revision
//! `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b` (MPL-2.0).
//!
//! Only version 20.2.0.7, little endian, is read and written. Everything the header derives from the blocks
//! (block count, block sizes, the longest string, the group count) is recomputed on write, so a file whose header
//! says something else about those still reads, but writes back with the derived values.

use crate::block::{Body, Profile};
use crate::error::{NifError, Result};
use crate::io::{
    Reader, len16, len32, put_i32, put_short_string, put_sized_string, put_u16, put_u32,
};

// source: nif-header-magic-version
const MAGIC: &[u8] = b"Gamebryo File Format, Version 20.2.0.7";
// source: nif-header-magic-version
const VERSION: u32 = 0x1402_0007;
const MAX_MAGIC_LINE: usize = 80;

/// The export information that follows the user versions in Bethesda streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportInfo {
    /// Author, raw bytes as stored.
    pub author: Vec<u8>,
    /// An unused `u32` (present when user version 2 is above 130).
    pub unknown_int: u32,
    /// Process script, raw bytes (present when user version 2 is below 131).
    pub process_script: Vec<u8>,
    /// Export script, raw bytes.
    pub export_script: Vec<u8>,
    /// Max file path, raw bytes (present when user version 2 is 103 or more).
    pub max_filepath: Vec<u8>,
}

/// The header fields that are not derived from the blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// User version.
    pub user_version: u32,
    /// User version 2 and the export info, present together in Bethesda streams (user version 3 or more).
    pub bethesda: Option<(u32, ExportInfo)>,
    /// Block type names in table order, raw bytes.
    pub block_types: Vec<Vec<u8>>,
    /// The string table, raw bytes of each entry.
    pub strings: Vec<Vec<u8>>,
    /// The group sizes list.
    pub groups: Vec<u32>,
}

impl Header {
    /// The numbers that select optional block fields.
    #[must_use]
    pub fn profile(&self) -> Profile {
        Profile {
            user_version: self.user_version,
            user_version2: self.bethesda.as_ref().map_or(0, |b| b.0),
        }
    }
}

/// One block: its type table index and its content.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Index into [`Header::block_types`].
    pub type_index: u16,
    /// The decoded or opaque content.
    pub body: Body,
}

/// A whole NIF file.
#[derive(Debug, Clone, PartialEq)]
pub struct NifFile {
    /// The header.
    pub header: Header,
    /// The blocks in file order.
    pub blocks: Vec<Block>,
    /// Root block links from the footer.
    pub roots: Vec<i32>,
}

impl NifFile {
    /// Read a file.
    ///
    /// Never panics: truncation, impossible counts, unknown versions and block sizes that do not match the
    /// typed layouts all come back as a [`NifError`].
    ///
    /// # Errors
    /// See [`NifError`].
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let (header, type_indices, sizes) = read_header(&mut r)?;
        read_blocks(&mut r, header, &type_indices, &sizes, false)
    }

    /// [`NifFile::read`], but bytes after the footer are ignored, as the game's own reader ignores them (some mods
    /// ship models with the tail of an earlier, longer export after the footer). For reading, not for a rewrite:
    /// the bytes are dropped.
    ///
    /// # Errors
    /// See [`NifError`]; never [`NifError::TrailingBytes`].
    pub fn read_ignoring_trailing_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let (header, type_indices, sizes) = read_header(&mut r)?;
        read_blocks(&mut r, header, &type_indices, &sizes, true)
    }

    /// Write the file.
    ///
    /// # Errors
    /// [`NifError::Inconsistent`] when a block's type index is out of range, a typed block sits under another
    /// type's name, or a typed block's own counts disagree with its arrays.
    pub fn write(&self) -> Result<Vec<u8>> {
        // source: nif-header-magic-version, nif-header-fields, nif-block-table, nif-footer-roots
        let h = &self.header;
        let profile = h.profile();
        let mut bodies = Vec::with_capacity(self.blocks.len());
        for b in &self.blocks {
            let name = h
                .block_types
                .get(usize::from(b.type_index))
                .ok_or(NifError::Inconsistent("block type index out of range"))?;
            if let Some(expect) = b.body.type_name()
                && expect != name.as_slice()
            {
                return Err(NifError::Inconsistent(
                    "typed block under a different type name",
                ));
            }
            bodies.push(b.body.write(profile)?);
        }

        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(0x0A);
        put_u32(&mut out, VERSION);
        out.push(1); // little endian
        put_u32(&mut out, h.user_version);
        put_u32(&mut out, len32(self.blocks.len(), "blocks")?);
        if let Some((uv2, info)) = &h.bethesda {
            put_u32(&mut out, *uv2);
            put_short_string(&mut out, &info.author, "author")?;
            if *uv2 > 130 {
                put_u32(&mut out, info.unknown_int);
            }
            if *uv2 < 131 {
                put_short_string(&mut out, &info.process_script, "process script")?;
            }
            put_short_string(&mut out, &info.export_script, "export script")?;
            if *uv2 >= 103 {
                put_short_string(&mut out, &info.max_filepath, "max file path")?;
            }
        }
        put_u16(&mut out, len16(h.block_types.len(), "block types")?);
        for t in &h.block_types {
            put_sized_string(&mut out, t, "block type name")?;
        }
        for b in &self.blocks {
            put_u16(&mut out, b.type_index);
        }
        for body in &bodies {
            put_u32(&mut out, len32(body.len(), "block size")?);
        }
        put_u32(&mut out, len32(h.strings.len(), "strings")?);
        let longest = h.strings.iter().map(Vec::len).max().unwrap_or(0);
        put_u32(&mut out, len32(longest, "string length")?);
        for s in &h.strings {
            put_sized_string(&mut out, s, "string")?;
        }
        put_u32(&mut out, len32(h.groups.len(), "groups")?);
        for g in &h.groups {
            put_u32(&mut out, *g);
        }
        for body in &bodies {
            out.extend_from_slice(body);
        }
        put_u32(&mut out, len32(self.roots.len(), "roots")?);
        for root in &self.roots {
            put_i32(&mut out, *root);
        }
        Ok(out)
    }
}

// source: nif-header-magic-version, nif-header-fields
fn read_header(r: &mut Reader<'_>) -> Result<(Header, Vec<u16>, Vec<u32>)> {
    let line_len = (0..=MAX_MAGIC_LINE)
        .find(|i| r.peek(*i) == Some(0x0A))
        .ok_or(NifError::BadMagic)?;
    if r.take(line_len)? != MAGIC {
        return Err(NifError::BadMagic);
    }
    r.take(1)?;
    let version = r.u32()?;
    if version != VERSION {
        return Err(NifError::UnsupportedVersion(version));
    }
    let endian = r.u8()?;
    if endian != 1 {
        return Err(NifError::UnsupportedEndian(endian));
    }
    let user_version = r.u32()?;
    let num_blocks = r.u32()?;
    // The count is checked against the bytes the type indices and sizes need further on (6 per block).
    let num_blocks = r.check_count("blocks", num_blocks, 6)?;
    let bethesda = if user_version >= 3 {
        let uv2 = r.u32()?;
        let author = r.short_string()?;
        let unknown_int = if uv2 > 130 { r.u32()? } else { 0 };
        let process_script = if uv2 < 131 {
            r.short_string()?
        } else {
            Vec::new()
        };
        let export_script = r.short_string()?;
        let max_filepath = if uv2 >= 103 {
            r.short_string()?
        } else {
            Vec::new()
        };
        Some((
            uv2,
            ExportInfo {
                author,
                unknown_int,
                process_script,
                export_script,
                max_filepath,
            },
        ))
    } else {
        None
    };
    let n_types = r.count16("block types", 4)?;
    let block_types = (0..n_types)
        .map(|_| r.sized_string("block type name"))
        .collect::<Result<Vec<_>>>()?;
    let mut type_indices = Vec::with_capacity(num_blocks);
    for block in 0..num_blocks {
        let index = r.u16()?;
        if usize::from(index) >= block_types.len() {
            return Err(NifError::BadBlockType { block, index });
        }
        type_indices.push(index);
    }
    let sizes = (0..num_blocks)
        .map(|_| r.u32())
        .collect::<Result<Vec<_>>>()?;
    let n_strings = r.count32("strings", 4)?;
    let _max_string_length = r.u32()?;
    let strings = (0..n_strings)
        .map(|_| r.sized_string("string"))
        .collect::<Result<Vec<_>>>()?;
    let n_groups = r.count32("groups", 4)?;
    let groups = (0..n_groups).map(|_| r.u32()).collect::<Result<Vec<_>>>()?;
    Ok((
        Header {
            user_version,
            bethesda,
            block_types,
            strings,
            groups,
        },
        type_indices,
        sizes,
    ))
}

// source: nif-block-table, nif-footer-roots
fn read_blocks(
    r: &mut Reader<'_>,
    header: Header,
    type_indices: &[u16],
    sizes: &[u32],
    ignore_trailing: bool,
) -> Result<NifFile> {
    let profile = header.profile();
    let mut blocks = Vec::with_capacity(type_indices.len());
    for (block, (&type_index, &size)) in type_indices.iter().zip(sizes).enumerate() {
        let declared = usize::try_from(size).map_err(|_| NifError::BlocksOverrun)?;
        if declared > r.remaining() {
            return Err(NifError::BlocksOverrun);
        }
        let bytes = r.take(declared)?;
        let name = &header.block_types[usize::from(type_index)];
        let (body, used) = Body::parse(name, bytes, profile)?;
        if used != declared {
            return Err(NifError::BlockSizeMismatch {
                block,
                declared: size,
                used,
            });
        }
        blocks.push(Block { type_index, body });
    }
    let n_roots = r.count32("roots", 4)?;
    let roots = r.i32s(n_roots)?;
    if r.remaining() != 0 && !ignore_trailing {
        return Err(NifError::TrailingBytes(r.remaining()));
    }
    Ok(NifFile {
        header,
        blocks,
        roots,
    })
}
