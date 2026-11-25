// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic fixture builders: the only test data this repository allows.
//!
//! Everything is built in memory by code. No game file, no extract of one and no committed binary is ever used
//! (`docs/TESTING.md`, enforced by the data guard). The layouts are stand-ins for the real formats and are only
//! as accurate as their rows in `provenance/sources.toml`; each builder has a reader (or decoder) written here
//! so that round trips can be tested before the real readers exist.

pub mod bsa;
pub mod dds;
pub mod golden;
pub mod image;
pub mod land;
pub mod nif;
pub mod plugin;
pub mod probe_nif;
pub mod rng;
pub mod world;

// The feature catalogue's ids (catalogue/, docs/TESTING.md "Feature catalogue"): a test says which items it proves with
// `covers(&[ids::area::group::ITEM])`; an id the catalogue does not have does not compile.
include!(concat!(env!("OUT_DIR"), "/tally_ids.rs"));
