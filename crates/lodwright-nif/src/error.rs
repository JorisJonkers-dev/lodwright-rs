// SPDX-License-Identifier: MPL-2.0
//! Typed errors of the NIF reader and writer.

use std::fmt;

/// What can go wrong reading or writing a NIF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NifError {
    /// The input ended while `needed` more bytes were wanted at offset `at`.
    Truncated {
        /// Offset where the read started.
        at: usize,
        /// Bytes wanted.
        needed: usize,
    },
    /// The first line is not the Gamebryo header string for a supported version.
    BadMagic,
    /// A version other than 20.2.0.7 (carried as its `u32`).
    UnsupportedVersion(u32),
    /// The file is big endian, which this reader does not handle.
    UnsupportedEndian(u8),
    /// A count that cannot fit in the bytes that remain, or that is otherwise out of bounds.
    BadCount {
        /// Which count.
        what: &'static str,
        /// Its value.
        count: u32,
    },
    /// A block names a type index that is not in the block type table.
    BadBlockType {
        /// Block index.
        block: usize,
        /// The offending index.
        index: u16,
    },
    /// A typed block did not use exactly the bytes the header gives it.
    BlockSizeMismatch {
        /// Block index.
        block: usize,
        /// Size from the header.
        declared: u32,
        /// Bytes the typed layout consumed.
        used: usize,
    },
    /// The block sizes run past the end of the file.
    BlocksOverrun,
    /// Bytes remain after the footer.
    TrailingBytes(usize),
    /// A value to be written is inconsistent with the rest (a count that disagrees with its array, a type
    /// name that does not match the block, a number that does not fit its field).
    Inconsistent(&'static str),
}

impl fmt::Display for NifError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { at, needed } => {
                write!(f, "truncated: {needed} bytes wanted at offset {at}")
            }
            Self::BadMagic => write!(f, "not a supported Gamebryo file header"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported version {v:#010x}"),
            Self::UnsupportedEndian(e) => write!(f, "unsupported endian marker {e}"),
            Self::BadCount { what, count } => write!(f, "bad count {count} for {what}"),
            Self::BadBlockType { block, index } => {
                write!(f, "block {block} uses unknown type index {index}")
            }
            Self::BlockSizeMismatch {
                block,
                declared,
                used,
            } => {
                write!(
                    f,
                    "block {block} declares {declared} bytes but its layout used {used}"
                )
            }
            Self::BlocksOverrun => write!(f, "block sizes run past the end of the file"),
            Self::TrailingBytes(n) => write!(f, "{n} bytes after the footer"),
            Self::Inconsistent(what) => write!(f, "inconsistent value: {what}"),
        }
    }
}

impl std::error::Error for NifError {}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, NifError>;
