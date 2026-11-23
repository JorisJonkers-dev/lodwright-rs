// SPDX-License-Identifier: GPL-3.0-only
//! DDS output for the LOD pipeline: an image buffer, box and Lanczos resampling, mip chains, BC1, BC3 and BC5
//! encoding through the permissively licensed `texpresso` crate, and the `ddsfile` crate for the container.
//!
//! Files are written with the 128-byte legacy header only (`DXT1`, `DXT5` and `ATI2` `FourCC`s); a `DX10` header is
//! never written and is refused when reading. Gamma and brightness/contrast are pure functions on [`Image`].
//! Everything is bounded by [`MAX_DIMENSION`].
//!
//! Layout facts come from the rows in `provenance/sources.toml` named in the `source:` comments (Microsoft's
//! public DDS documentation).

mod bc7;
mod dds;
mod error;
mod image;
mod resize;

pub use dds::{
    Decoded, EncodeOptions, Format, HEADER_LEN, Header, Quality, decode, decode_image, encode,
    encode_with_threads,
    psnr, read_header,
};
pub use error::{DdsError, Result};
pub use image::{Image, MAX_DIMENSION, adjust_brightness_contrast, apply_gamma, check_dimensions};
pub use resize::{Filter, mip_chain, mip_count, mip_size, resize};
