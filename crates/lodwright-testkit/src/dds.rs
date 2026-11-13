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
