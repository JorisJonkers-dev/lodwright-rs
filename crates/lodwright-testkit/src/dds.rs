// SPDX-License-Identifier: MPL-2.0
//! Synthetic DDS headers: building the 128 bytes (magic and header) and reading the fields back.

/// Bytes of the magic plus the header.
pub const HEADER_LEN: usize = 128;

/// The fields a LOD texture check cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Number of mip levels (at least 1).
    pub mip_count: u32,
    /// Pixel-format `FourCC`, for example `*b"DXT1"`.
    pub four_cc: [u8; 4],
}

// source: dds-header-layout, dds-pixel-format
/// Build a legacy (non-DX10) header for `header`.
#[must_use]
pub fn build_header(header: &Header) -> Vec<u8> {
    let mut b = vec![0_u8; HEADER_LEN];
    b[0..4].copy_from_slice(b"DDS ");
    put(&mut b, 4, 124); // header size
    put(&mut b, 8, 0x0002_1007); // CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT
    put(&mut b, 12, header.height);
    put(&mut b, 16, header.width);
    put(&mut b, 28, header.mip_count);
    put(&mut b, 76, 32); // pixel format size
    put(&mut b, 80, 0x4); // FOURCC flag
    b[84..88].copy_from_slice(&header.four_cc);
    put(&mut b, 108, 0x0040_1008); // COMPLEX | TEXTURE | MIPMAP
    b
}

/// Pixel data for [`build_file`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pixels {
    /// BC1 (DXT1) with every texel set to colour 0 (RGB 5:6:5) of a four-colour block (colour 1 is 0).
    Bc1Solid {
        /// The 5:6:5 colour; it must be greater than 0 for the block to be four-colour.
        colour: u16,
    },
    /// BC1 three-colour blocks whose every texel is the transparent code.
    Bc1Transparent,
    /// BC3 (DXT5): white colour blocks, alpha 255 and 0 as references and the codes 0 to 7 repeating over the texels.
    Bc3Gradient,
    /// Uncompressed 32-bit pixels, each `[red, green, blue, alpha]`, masks `00FF0000 0000FF00 000000FF FF000000`.
    Argb32(Vec<[u8; 4]>),
    /// A DX10 header with this DXGI format and eight bytes of zero data.
    Dx10 {
        /// The DXGI format number.
        dxgi_format: u32,
    },
}

// source: probe-bc1-block-layout, probe-bc3-block-layout, dds-pixel-format-masks, dds-header-dxt10
/// A whole DDS file: header for `width`, `height` and `mips`, and the mip level 0 data for `pixels` (no other mips).
///
/// # Panics
/// Never for test-sized input.
#[must_use]
pub fn build_file(width: u32, height: u32, mips: u32, pixels: &Pixels) -> Vec<u8> {
    let four_cc = match pixels {
        Pixels::Bc1Solid { .. } | Pixels::Bc1Transparent => *b"DXT1",
        Pixels::Bc3Gradient => *b"DXT5",
        Pixels::Dx10 { .. } => *b"DX10",
        Pixels::Argb32(_) => *b"\0\0\0\0",
    };
    let mut b = build_header(&Header {
        width,
        height,
        mip_count: mips,
        four_cc,
    });
    let blocks = (width.div_ceil(4) * height.div_ceil(4)) as usize;
    match pixels {
        Pixels::Bc1Solid { colour } => {
            for _ in 0..blocks {
                b.extend_from_slice(&colour.to_le_bytes());
                b.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
            }
        }
        Pixels::Bc1Transparent => {
            for _ in 0..blocks {
                b.extend_from_slice(&[0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
            }
        }
        Pixels::Bc3Gradient => {
            let codes: u64 = (0..16_u64).fold(0, |acc, t| acc | ((t % 8) << (3 * t)));
            for _ in 0..blocks {
                b.extend_from_slice(&[255, 0]);
                b.extend_from_slice(&codes.to_le_bytes()[..6]);
                b.extend_from_slice(&[0xFF, 0xFF, 0, 0, 0, 0, 0, 0]);
            }
        }
        Pixels::Argb32(texels) => {
            put(&mut b, 80, 0x41); // RGB | ALPHAPIXELS
            put(&mut b, 88, 32);
            put(&mut b, 92, 0x00FF_0000);
            put(&mut b, 96, 0x0000_FF00);
            put(&mut b, 100, 0x0000_00FF);
            put(&mut b, 104, 0xFF00_0000);
            b[84..88].copy_from_slice(&[0; 4]);
            for [r, g, bl, a] in texels {
                let value =
                    u32::from(*a) << 24 | u32::from(*r) << 16 | u32::from(*g) << 8 | u32::from(*bl);
                b.extend_from_slice(&value.to_le_bytes());
            }
        }
        Pixels::Dx10 { dxgi_format } => {
            b.extend_from_slice(&dxgi_format.to_le_bytes());
            b.extend_from_slice(&3_u32.to_le_bytes()); // resource dimension
            b.extend_from_slice(&0_u32.to_le_bytes());
            b.extend_from_slice(&1_u32.to_le_bytes()); // array size
            b.extend_from_slice(&0_u32.to_le_bytes());
            b.extend_from_slice(&[0; 8]);
        }
    }
    b
}

// source: dds-header-layout
/// Read the fields back. `None` if the magic, the sizes or the length are wrong.
#[must_use]
pub fn parse_header(bytes: &[u8]) -> Option<Header> {
    if bytes.len() < HEADER_LEN
        || &bytes[0..4] != b"DDS "
        || get(bytes, 4) != 124
        || get(bytes, 76) != 32
    {
        return None;
    }
    Some(Header {
        width: get(bytes, 16),
        height: get(bytes, 12),
        mip_count: get(bytes, 28),
        four_cc: bytes[84..88].try_into().ok()?,
    })
}

fn put(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn get(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[cfg(test)]
mod tests {
    use super::{HEADER_LEN, Header, build_header, parse_header};

    #[test]
    fn header_round_trips() {
        let h = Header {
            width: 512,
            height: 256,
            mip_count: 7,
            four_cc: *b"DXT5",
        };
        let bytes = build_header(&h);
        assert_eq!(bytes.len(), HEADER_LEN);
        assert_eq!(parse_header(&bytes), Some(h));
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = build_header(&Header {
            width: 4,
            height: 4,
            mip_count: 1,
            four_cc: *b"DXT1",
        });
        bytes[0] = b'X';
        assert_eq!(parse_header(&bytes), None);
    }

    #[test]
    fn a_short_buffer_is_rejected() {
        assert_eq!(parse_header(&[0; 64]), None);
    }
}
