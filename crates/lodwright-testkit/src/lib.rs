// SPDX-License-Identifier: MPL-2.0
//! Synthetic fixture builders: the only test data this repository allows.
//!
//! Everything is built in memory by code. No game file, no extract of one and no committed binary is ever used
//! (`docs/TESTING.md`, enforced by the data guard). The layouts are stand-ins for the real formats and are only
//! as accurate as their rows in `provenance/sources.toml`; each builder has a reader (or decoder) written here
//! so that round trips can be tested before the real readers exist.

pub mod dds;
pub mod golden;
pub mod land;
pub mod plugin;
pub mod rng;
pub mod world;
