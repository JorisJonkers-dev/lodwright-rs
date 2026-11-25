// SPDX-License-Identifier: GPL-3.0-only
//! Clean-room core of lodwright.
//!
//! Format readers and the height-and-tile math of the terrain pipeline. Nothing here may be implemented from the
//! unlicensed 2016 generator C# source, decompiled binaries or leaked code: see `docs/clean-room.md`. Every format
//! fact the code relies on is a row in `provenance/sources.toml`.
//!
//! Pipeline so far: [`plugin`] reads plugins, [`worldspace`] assembles worlds across a load order, [`land`] decodes
//! heights (and, on request, [`layers`] and [`texture`] paths), [`raster`] puts them on a block-aligned grid, [`terrain`] plans the LOD blocks and meshes, [`layout`] names the
//! output files. [`probe`] holds the numbers-only readers behind `lodwright probe`. [`lod`] and [`dds`] are still
//! contracts only.

pub mod dds;
pub mod error;
pub mod land;
pub mod layers;
pub mod layout;
pub mod lights;
pub mod lod;
pub mod objects;
pub mod plugin;
pub mod probe;
pub mod raster;
pub mod shipped;
pub mod terrain;
pub mod texture;
pub mod tin;
pub mod trees;
pub mod worldspace;

pub use error::Error;
