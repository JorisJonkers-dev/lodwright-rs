// SPDX-License-Identifier: GPL-3.0-only
//! DDS files: legacy-header writing, strict header reading, and decoding for tests.
//!
//! Only DXT1 (BC1), DXT5 (BC3) and ATI2 (BC5) with the 128-byte legacy header are handled. A DX10 extension
//! header is never written and is refused by [`decode`]: the target engine's Direct3D 9 renderer cannot read it.
//! [`decode_image`] reads a BC7 file with a DX10 header to RGBA, for the mods that ship their far textures that way.

use crate::error::{DdsError, Result};
use crate::image::{Image, check_dimensions};
use crate::resize::{Filter, mip_chain, mip_count, mip_size};
use ddsfile::{D3DFormat, Dds, FourCC, NewD3dParams};
use std::io::Cursor;
use texpresso::{Algorithm, Params};

/// Bytes of the magic plus the header.
pub const HEADER_LEN: usize = 128;

// source: dds-fourcc-formats
const FOURCC_DXT1: [u8; 4] = *b"DXT1";
// source: dds-fourcc-formats
const FOURCC_DXT5: [u8; 4] = *b"DXT5";
// source: bc2-block-layout
const FOURCC_DXT3: [u8; 4] = *b"DXT3";
// source: dds-fourcc-formats
const FOURCC_ATI2: [u8; 4] = *b"ATI2";

/// A block-compressed format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// BC1, written as DXT1: colour with optional 1-bit alpha.
    Bc1,
    /// BC2, written as DXT3: colour with 4-bit explicit alpha.
    Bc2,
    /// BC3, written as DXT5: colour with an interpolated 8-bit alpha block.
    Bc3,
    /// BC5, written as ATI2: two interpolated 8-bit channels (red and green).
    Bc5,
}

impl Format {
    /// The `FourCC` in the pixel format.
    #[must_use]
    pub fn four_cc(self) -> [u8; 4] {
        match self {
            Self::Bc1 => FOURCC_DXT1,
            Self::Bc2 => FOURCC_DXT3,
            Self::Bc3 => FOURCC_DXT5,
            Self::Bc5 => FOURCC_ATI2,
        }
    }

    /// Bytes per 4 by 4 block.
    // source: dds-linear-size-pitch, bc1-block-layout, bc3-block-layout, bc5-block-layout
    #[must_use]
    pub fn block_bytes(self) -> usize {
        match self {
            Self::Bc1 => 8,
            Self::Bc2 | Self::Bc3 | Self::Bc5 => 16,
        }
    }

    /// Bytes of one surface of `width` by `height` pixels, with partial blocks rounded up.
    // source: dds-linear-size-pitch, bc-block-padding
    #[must_use]
    pub fn surface_bytes(self, width: u32, height: u32) -> usize {
        let blocks = |n: u32| (n as usize).div_ceil(4).max(1);
        blocks(width) * blocks(height) * self.block_bytes()
    }

    fn texpresso(self) -> texpresso::Format {
        match self {
            Self::Bc1 => texpresso::Format::Bc1,
            Self::Bc2 => texpresso::Format::Bc2,
            Self::Bc3 => texpresso::Format::Bc3,
            Self::Bc5 => texpresso::Format::Bc5,
        }
    }

    fn from_four_cc(code: [u8; 4]) -> Result<Self> {
        match &code {
            b"DXT1" => Ok(Self::Bc1),
            b"DXT3" => Ok(Self::Bc2),
            b"DXT5" => Ok(Self::Bc3),
            b"ATI2" => Ok(Self::Bc5),
            b"DX10" => Err(DdsError::Dx10Header),
            _ => Err(DdsError::UnsupportedFormat(code)),
        }
    }
}

/// The fields of a DDS header this crate reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Number of mip levels (at least 1).
    pub mip_count: u32,
    /// Pixel format.
    pub format: Format,
}

impl Header {
    /// Bytes of all surfaces the header describes.
    #[must_use]
    pub fn data_bytes(&self) -> usize {
        (0..self.mip_count)
            .map(|l| {
                let (w, h) = mip_size(self.width, self.height, l);
                self.format.surface_bytes(w, h)
            })
            .sum()
    }
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Read and validate the 128-byte header.
///
/// A reader must not rely on the optional flags, so only the magic, the two structure sizes, the dimensions and
/// the pixel format `FourCC` are checked; the mip count is taken from the header field when it is non-zero.
/// The data after the header must be long enough for every surface the header describes.
///
/// # Errors
/// [`DdsError::Truncated`], [`DdsError::BadHeader`], [`DdsError::Dx10Header`],
/// [`DdsError::UnsupportedFormat`], [`DdsError::BadDimensions`] or [`DdsError::BadMipCount`].
// source: dds-header-layout, dds-pixel-format, dds-reader-validation
pub fn read_header(bytes: &[u8]) -> Result<Header> {
    if bytes.len() < HEADER_LEN {
        return Err(DdsError::Truncated);
    }
    if &bytes[0..4] != b"DDS " || le32(bytes, 4) != 124 || le32(bytes, 76) != 32 {
        return Err(DdsError::BadHeader);
    }
    let height = le32(bytes, 12);
    let width = le32(bytes, 16);
    check_dimensions(width, height)?;
    let pixel_flags = le32(bytes, 80);
    if pixel_flags & 0x4 == 0 {
        return Err(DdsError::UnsupportedFormat([0; 4]));
    }
    let format = Format::from_four_cc([bytes[84], bytes[85], bytes[86], bytes[87]])?;
    let declared = le32(bytes, 28);
    let mips = declared.max(1);
    if mips > mip_count(width, height) {
        return Err(DdsError::BadMipCount(declared));
    }
    let header = Header {
        width,
        height,
        mip_count: mips,
        format,
    };
    if bytes.len() - HEADER_LEN < header.data_bytes() {
        return Err(DdsError::Truncated);
    }
    Ok(header)
}

/// Options for [`encode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeOptions {
    /// The block format.
    pub format: Format,
    /// Write a full mip chain down to 1 by 1 (otherwise only the base level).
    pub mips: bool,
    /// The filter for the mip chain.
    pub filter: Filter,
    /// Spend more time for better blocks.
    pub quality: Quality,
}

/// Encoder effort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Range fit: fast, lower quality.
    Fast,
    /// Cluster fit: slower, better.
    Normal,
}

fn compress(level: &Image, format: Format, quality: Quality) -> Vec<u8> {
    let params = Params {
        algorithm: match quality {
            Quality::Fast => Algorithm::RangeFit,
            Quality::Normal => Algorithm::ClusterFit,
        },
        ..Params::default()
    };
    let (w, h) = (level.width() as usize, level.height() as usize);
    // BC1 here stores opaque colour: the encoder would otherwise read alpha below 128 as the transparent index
    // and flatten those pixels to black.
    let opaque: Vec<u8>;
    let pixels = if format == Format::Bc1 {
        opaque = level
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        &opaque[..]
    } else {
        level.rgba()
    };
    let mut out = vec![0_u8; format.texpresso().compressed_size(w, h)];
    format.texpresso().compress(pixels, w, h, params, &mut out);
    out
}

/// Encode `image` as a complete DDS file with a legacy header.
///
/// # Errors
/// [`DdsError::Container`] if the container library refuses the sizes (cannot happen for an [`Image`]).
// source: dds-header-layout, dds-pixel-format, dds-fourcc-formats, dds-header-flags-caps, dds-linear-size-pitch
pub fn encode(image: &Image, options: &EncodeOptions) -> Result<Vec<u8>> {
    let chain = if options.mips {
        mip_chain(image, options.filter)?
    } else {
        vec![image.clone()]
    };
    let levels = u32::try_from(chain.len()).map_err(|_| DdsError::BadMipCount(0))?;
    // ATI2 has no entry in the container library's format list; DXT5 has the same 16-byte block layout and
    // the same header fields, so the container is built as DXT5 and the FourCC replaced.
    let mut dds = Dds::new_d3d(NewD3dParams {
        height: image.height(),
        width: image.width(),
        depth: None,
        format: match options.format {
            Format::Bc1 => D3DFormat::DXT1,
            Format::Bc2 => D3DFormat::DXT3,
            Format::Bc3 | Format::Bc5 => D3DFormat::DXT5,
        },
        mipmap_levels: (levels > 1).then_some(levels),
        caps2: None,
    })
    .map_err(|e| DdsError::Container(e.to_string()))?;
    if options.format == Format::Bc5 {
        dds.header.spf.fourcc = Some(FourCC(u32::from_le_bytes(FOURCC_ATI2)));
    }
    let mut data = Vec::with_capacity(dds.data.len());
    for level in &chain {
        data.extend_from_slice(&compress(level, options.format, options.quality));
    }
    if data.len() != dds.data.len() {
        return Err(DdsError::Container(
            "surface sizes disagree with the container".to_owned(),
        ));
    }
    dds.data = data;
    let mut out = Vec::with_capacity(HEADER_LEN + dds.data.len());
    dds.write(&mut Cursor::new(&mut out))
        .map_err(|e| DdsError::Container(e.to_string()))?;
    Ok(out)
}

/// A decoded DDS: its format and every mip level as RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// The block format.
    pub format: Format,
    /// The levels, largest first.
    pub levels: Vec<Image>,
}

/// Decode a DDS written with a legacy header into RGBA levels (BC5 gives red and green, blue zero, alpha opaque).
///
/// The data length is validated against the header before anything is allocated, so the work is bounded by the
/// input size.
///
/// # Errors
/// Anything [`read_header`] refuses.
pub fn decode(bytes: &[u8]) -> Result<Decoded> {
    let header = read_header(bytes)?;
    let mut at = HEADER_LEN;
    let mut levels = Vec::with_capacity(header.mip_count as usize);
    for level in 0..header.mip_count {
        let (w, h) = mip_size(header.width, header.height, level);
        let size = header.format.surface_bytes(w, h);
        let mut rgba = vec![0_u8; w as usize * h as usize * 4];
        header.format.texpresso().decompress(
            &bytes[at..at + size],
            w as usize,
            h as usize,
            &mut rgba,
        );
        at += size;
        levels.push(Image::new(w, h, rgba)?);
    }
    Ok(Decoded {
        format: header.format,
        levels,
    })
}

// source: dds-header-layout, dds-pixel-format, dds-pixel-format-masks
fn decode_masked(bytes: &[u8]) -> Result<Image> {
    if bytes.len() < HEADER_LEN
        || &bytes[0..4] != b"DDS "
        || le32(bytes, 4) != 124
        || le32(bytes, 76) != 32
    {
        return Err(DdsError::BadHeader);
    }
    let (height, width) = (le32(bytes, 12), le32(bytes, 16));
    check_dimensions(width, height)?;
    let flags = le32(bytes, 80);
    let bits = le32(bytes, 88);
    if flags & 0x40 == 0 || !matches!(bits, 16 | 24 | 32) {
        return Err(DdsError::UnsupportedFormat([0; 4]));
    }
    let masks = [
        le32(bytes, 92),
        le32(bytes, 96),
        le32(bytes, 100),
        if flags & 0x1 != 0 {
            le32(bytes, 104)
        } else {
            0
        },
    ];
    let step = (bits / 8) as usize;
    let count = width as usize * height as usize;
    if bytes.len() - HEADER_LEN < count * step {
        return Err(DdsError::Truncated);
    }
    let channel = |px: u32, mask: u32, otherwise: u8| -> u8 {
        if mask == 0 {
            return otherwise;
        }
        let shift = mask.trailing_zeros();
        let max = mask >> shift;
        u8::try_from(((px & mask) >> shift) * 255 / max).unwrap_or(255)
    };
    let mut rgba = Vec::with_capacity(count * 4);
    for i in 0..count {
        let at = HEADER_LEN + i * step;
        let mut px = 0_u32;
        for (k, b) in bytes[at..at + step].iter().enumerate() {
            px |= u32::from(*b) << (8 * k);
        }
        rgba.extend_from_slice(&[
            channel(px, masks[0], 0),
            channel(px, masks[1], 0),
            channel(px, masks[2], 0),
            channel(px, masks[3], 255),
        ]);
    }
    Image::new(width, height, rgba)
}

// source: dds-dx10-header
/// Bytes of the magic, the header and the 20-byte DX10 extension header: the data starts here.
const DX10_DATA_AT: usize = HEADER_LEN + 20;

// source: dds-dx10-header
/// The `DXGI_FORMAT` values of BC7 (`UNORM` and `UNORM_SRGB`), the one DX10 format the far-object resource mods ship.
const DXGI_BC7: [u32; 2] = [98, 99];

/// Decode the largest level of a file with a DX10 extension header. Only BC7 is read; the game's own renderer cannot
/// read such a file, so this is for reading a mod's texture (it is re-encoded into the atlas), never for writing.
fn decode_dx10(bytes: &[u8]) -> Result<Image> {
    if bytes.len() < DX10_DATA_AT {
        return Err(DdsError::Truncated);
    }
    if &bytes[0..4] != b"DDS " || le32(bytes, 4) != 124 {
        return Err(DdsError::BadHeader);
    }
    let (height, width) = (le32(bytes, 12), le32(bytes, 16));
    check_dimensions(width, height)?;
    let format = le32(bytes, HEADER_LEN);
    if !DXGI_BC7.contains(&format) {
        return Err(DdsError::UnsupportedDxgiFormat(format));
    }
    let blocks = |n: u32| (n as usize).div_ceil(4).max(1);
    let need = blocks(width) * blocks(height) * 16;
    if bytes.len() - DX10_DATA_AT < need {
        return Err(DdsError::Truncated);
    }
    let rgba = crate::bc7::decode_surface(
        &bytes[DX10_DATA_AT..DX10_DATA_AT + need],
        width as usize,
        height as usize,
    );
    Image::new(width, height, rgba)
}

/// Decode the largest level of a DDS to RGBA: a block-compressed file (as [`decode`]), an uncompressed 16, 24 or
/// 32 bit one described by its colour masks, or a BC7 file with a DX10 extension header.
///
/// # Errors
/// Anything [`read_header`] refuses, a mask format that is not 16, 24 or 32 bit RGB, or a DX10 header that names a
/// format other than BC7 ([`DdsError::UnsupportedDxgiFormat`]).
pub fn decode_image(bytes: &[u8]) -> Result<Image> {
    if bytes.len() >= 84 && le32(bytes, 80) & 0x4 == 0 {
        return decode_masked(bytes);
    }
    if bytes.len() >= 88 && bytes[84..88] == *b"DX10" {
        return decode_dx10(bytes);
    }
    decode(bytes)?
        .levels
        .into_iter()
        .next()
        .ok_or(DdsError::Truncated)
}

/// The mean squared error to peak signal-to-noise ratio, in decibels, over the channels in `channels` (indices
/// 0 to 3). Identical images give infinity.
///
/// # Panics
/// If the images differ in size.
#[must_use]
pub fn psnr(a: &Image, b: &Image, channels: &[usize]) -> f64 {
    assert_eq!(
        (a.width(), a.height()),
        (b.width(), b.height()),
        "images differ in size"
    );
    let mut squared = 0.0_f64;
    let mut n = 0.0_f64;
    for (p, q) in a
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba().as_chunks::<4>().0)
    {
        for &c in channels {
            let d = f64::from(p[c]) - f64::from(q[c]);
            squared += d * d;
            n += 1.0;
        }
    }
    if squared == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255.0_f64 * 255.0 / (squared / n)).log10()
}
