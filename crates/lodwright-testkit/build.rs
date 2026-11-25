// SPDX-License-Identifier: GPL-3.0-only
//! Writes the catalogue's ids module (catalogue/, read by tally) with tally's generator: a copy that `tally check`
//! keeps equal to the pinned tally's (`tally ids --generator catalogue/tally_ids_gen.rs`).

// tally's generator is a copy of another repository's file: it is held to that file's lints, not this workspace's.
#![allow(missing_docs, clippy::all, clippy::pedantic)]

include!("../../catalogue/tally_ids_gen.rs");

fn main() {
    tally_ids_gen::build("../../catalogue");
}
