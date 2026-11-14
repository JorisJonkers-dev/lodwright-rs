// SPDX-License-Identifier: MPL-2.0
//! A clean-room generic reader and writer for the NIF container, version 20.2.0.7 (the version Fallout 3 and
//! New Vegas meshes use).
//!
//! The header, the block table, the string table and the footer are modelled in full. `NiNode`, `NiTriShape`,
//! `NiTriShapeData` and the common property blocks (`NiAlphaProperty`, `NiMaterialProperty`,
//! `NiZBufferProperty`) are typed values; every other block is carried as the exact bytes the header sizes it by,
//! so files that hold blocks this crate does not understand still round trip byte for byte. There is no block
//! profile for the New Vegas LOD meshes here: which blocks a terrain or object block mesh holds is a later,
//! separately recorded step.
//!
//! Layout facts come from the rows in `provenance/sources.toml` named in the `source:` comments.

mod block;
mod error;
mod file;
mod io;

pub use block::{
    AvObject, Body, GeometryData, NiAlphaProperty, NiMaterialProperty, NiNode, NiTriShape,
    NiTriShapeData, NiZBufferProperty, ObjectNet, Profile, Vec3, names,
};
pub use error::{NifError, Result};
pub use file::{Block, ExportInfo, Header, NifFile};
