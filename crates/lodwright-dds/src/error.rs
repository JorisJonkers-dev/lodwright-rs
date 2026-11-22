// SPDX-License-Identifier: GPL-3.0-only
//! Typed errors of the DDS crate.

use std::fmt;

/// What can go wrong building, writing or reading a DDS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DdsError {
    /// A width or height of zero or above the cap.
    BadDimensions {
        /// Width asked for.
        width: u32,
        /// Height asked for.
        height: u32,
    },
    /// The pixel buffer length is not `width * height * 4`.
    BadPixelBuffer,
    /// The input is shorter than the header, or than the surfaces the header describes.
    Truncated,
    /// The magic, the header size or the pixel format size is wrong.
    BadHeader,
    /// The file has a DX10 extension header where only a legacy header is accepted.
    Dx10Header,
    /// A DX10 extension header names a format other than BC7 (the `DXGI_FORMAT` value is given).
    UnsupportedDxgiFormat(u32),
    /// A pixel format other than DXT1, DXT3, DXT5 or ATI2 (BC1, BC2, BC3, BC5).
    UnsupportedFormat([u8; 4]),
    /// A mip count of zero or more than the sizes allow.
    BadMipCount(u32),
    /// The container library refused the write.
    Container(String),
}

impl fmt::Display for DdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadDimensions { width, height } => write!(f, "bad dimensions {width}x{height}"),
            Self::BadPixelBuffer => write!(f, "pixel buffer length does not match the dimensions"),
            Self::Truncated => write!(f, "input too short"),
            Self::BadHeader => write!(f, "not a valid DDS header"),
            Self::Dx10Header => write!(f, "DX10 extension headers are not supported"),
            Self::UnsupportedDxgiFormat(n) => {
                write!(f, "DX10 format {n} is not supported (only BC7 is read)")
            }
            Self::UnsupportedFormat(c) => write!(f, "unsupported pixel format {c:?}"),
            Self::BadMipCount(n) => write!(f, "bad mip count {n}"),
            Self::Container(m) => write!(f, "container error: {m}"),
        }
    }
}

impl std::error::Error for DdsError {}

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, DdsError>;
