// SPDX-License-Identifier: GPL-3.0-only
//! Library entry point of lodwright: turns a worldspace into LOD files in an output folder.
//!
//! This is what a mod manager links against; the command line is a thin shell over it. Nothing here prints.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::type_complexity,
    reason = "pixel, sample and cell indices are small, the maths is in f32 and the geometry code names axes and vertices by letter"
)]

mod assets;
mod atlas;
mod correlation;
mod diffuse;
mod error;
mod filter;
mod images;
mod lodmodel;
mod objects;
mod outline;
mod output;
mod terrain;
mod trees;
mod verify;
mod water;

pub use assets::{Assets, FolderAssets, Winner};
pub use atlas::{Atlas, Rect, pack};
pub use correlation::{TextureCorrelation, channel_correlation, compare_textures};
pub use diffuse::{DEFAULT_LAND_TEXTURE, LandTiles, Tile, blend, default_land_texture_from_ini};
pub use error::GenError;
pub use filter::{filter_instances, glob};
pub use images::{
    ShippedPatch, diffuse_image, normal_image, normal_image_with_alpha, patch_image, specular_alpha,
};
pub use lodmodel::{Part, parse_lod_model};
pub use objects::{
    BLOCK_CELLS, HeightAt, ObjectReport, Surfaces, generate_objects, generate_objects_with_progress,
};
pub use output::{is_plain_relative, linked_folder, write_output_file};
pub use terrain::{BlockRecord, FileRecord, Options, Report, generate_terrain};
pub use trees::{TreeReport, billboard_path, generate_trees, generate_trees_with_progress};
pub use verify::{
    BlockComparison, Finding, OBJECT_TOLERANCE, ObjectAgreement, TerrainHeightDifference,
    Verification, WaterOverlap, WaterProfile, compare_blocks, compare_folders, verify_folder,
};
