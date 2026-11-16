// SPDX-License-Identifier: MPL-2.0
//! Library entry point of lodwright: turns a worldspace into LOD files in an output folder.
//!
//! This is what a mod manager links against; the command line is a thin shell over it. Nothing here prints.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel, sample and cell indices are small and the maths is in f32"
)]

mod error;
mod images;
mod terrain;

pub use error::GenError;
pub use images::{diffuse_image, normal_image};
pub use terrain::{FileRecord, Options, Report, generate_terrain};
