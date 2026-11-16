// SPDX-License-Identifier: GPL-3.0-only
//! Game assets for lodwright: a reader for BSA v104 archives and the resolver that finds a path the way the game
//! does (loose files first, then archives).
//!
//! Clean-room: the archive layout comes from the public UESP pages recorded in `provenance/sources.toml`
//! (`bsa-*` rows) and the pickup rules from `fnv-archive-loading`. No game archive is ever committed; tests build
//! theirs with `lodwright-testkit`.

pub mod bsa;
pub mod error;
pub mod path;
pub mod resolve;

pub use bsa::{Archive, Entry};
pub use error::Error;
pub use resolve::{ArchiveListing, ConflictRule, Listing, Location, Resolver};
