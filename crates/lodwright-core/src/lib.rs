// SPDX-License-Identifier: MPL-2.0
//! Clean-room core of lodwright.
//!
//! Format readers and the height-and-tile math of the terrain pipeline. Nothing here may be implemented from the
//! unlicensed 2016 generator C# source, decompiled binaries or leaked code: see `docs/clean-room.md`. Every format
//! fact the code relies on is a row in `provenance/sources.toml`.
//!
//! Pipeline so far: [`plugin`] reads plugins, [`worldspace`] assembles worlds across a load order, [`land`] decodes
//! heights, [`raster`] puts them on a block-aligned grid, [`terrain`] plans the LOD blocks and meshes, [`layout`] names the
//! output files. [`lod`] and [`dds`] are still contracts only.

pub mod dds;
pub mod error;
pub mod land;
pub mod layout;
pub mod lod;
pub mod plugin;
pub mod raster;
pub mod terrain;
pub mod worldspace;

pub use error::Error;
