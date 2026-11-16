// SPDX-License-Identifier: GPL-3.0-only
//! `lodwright probe`: small readers that turn a NIF, DDS, settings or `.dtl` file into numbers and structure, so
//! the owner can take measurements without copying content.
//!
//! Every reader returns a [`Value`] tree. The tree can hold numbers, fixed labels and a few sanitised identifiers
//! (block type names, a `FourCC`); it cannot hold arbitrary input bytes, so no report can echo content. A reader
//! never panics on any input and never allocates by a count the input claims without checking it against what is
//! left of the file.

pub mod dds;
pub mod flat;
pub mod nif;
mod reader;
mod value;

pub use value::{Ident, ProbeError, Value};

/// The largest file the NIF and DDS readers read into memory.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
