// SPDX-License-Identifier: MPL-2.0
//! Library entry point of lodwright: turns a worldspace into LOD files in an output folder.
//!
//! This is what a mod manager links against; the command line is a thin shell over it. Nothing here prints.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::type_complexity,
    reason = "pixel, sample and cell indices are small, the maths is in f32 and the geometry code names axes and vertices by letter"
)]

mod assets;
mod atlas;
mod error;
mod images;
mod lodmodel;
mod objects;
mod terrain;
mod verify;

pub use assets::{Assets, FolderAssets};
pub use atlas::{Atlas, Rect, pack};
pub use error::GenError;
pub use images::{diffuse_image, normal_image};
pub use lodmodel::{Part, parse_lod_model};
pub use objects::{BLOCK_CELLS, ObjectReport, generate_objects};
pub use terrain::{FileRecord, Options, Report, generate_terrain};
pub use verify::{Finding, Verification, compare_folders, verify_folder};
