// SPDX-License-Identifier: MPL-2.0
//! Clean-room core of lodwright.
//!
//! Every module here is a contract for code that does not exist yet. Each states what it will do and the open
//! source its format facts will be derived from. Nothing may be implemented from the unlicensed 2016 generator C#
//! source, decompiled binaries or leaked code: see `docs/clean-room.md`. Every format fact the code relies on is
//! a row in `provenance/sources.toml`.
//!
//! The first slice is: `LAND`/`VHGT` heightfield reader ([`land`]), `.lod` writer ([`lod`]) and a block-aligned
//! raster ([`raster`]), verified by synthetic round trips (`docs/TESTING.md`).

pub mod dds;
pub mod error;
pub mod land;
pub mod lod;
pub mod plugin;
pub mod raster;

pub use error::Error;
