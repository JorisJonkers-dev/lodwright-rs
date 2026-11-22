// SPDX-License-Identifier: GPL-3.0-only
//! BC7 block decoding, written from the public BPTC format description (the Khronos Data Format Specification,
//! section "BC7", and Microsoft's BC7 format page): eight modes, the two- and three-subset partition tables, anchor
//! texels, p-bits, rotation and the index selection bit. Only decoding: lodwright never writes BC7.

// source: bptc-format
/// The two-subset partitions: bit `i` is the subset of texel `i` (y-major), one mask per partition number.
const PARTITIONS_2: [u16; 64] = [
    0xcccc, 0x8888, 0xeeee, 0xecc8, 0xc880, 0xfeec, 0xfec8, 0xec80, 0xc800, 0xffec, 0xfe80, 0xe800,
    0xffe8, 0xff00, 0xfff0, 0xf000, 0xf710, 0x008e, 0x7100, 0x08ce, 0x008c, 0x7310, 0x3100, 0x8cce,
    0x088c, 0x3110, 0x6666, 0x366c, 0x17e8, 0x0ff0, 0x718e, 0x399c, 0xaaaa, 0xf0f0, 0x5a5a, 0x33cc,
    0x3c3c, 0x55aa, 0x9696, 0xa55a, 0x73ce, 0x13c8, 0x324c, 0x3bdc, 0x6996, 0xc33c, 0x9966, 0x0660,
    0x0272, 0x04e4, 0x4e40, 0x2720, 0xc936, 0x936c, 0x39c6, 0x639c, 0x9336, 0x9cc6, 0x817e, 0xe718,
    0xccf0, 0x0fcc, 0x7744, 0xee22,
];

// source: bptc-format
/// The three-subset partitions: bits `2i..2i+2` are the subset of texel `i` (y-major), one word per partition number.
const PARTITIONS_3: [u32; 64] = [
    0xaa68_5050,
    0x6a5a_5040,
    0x5a5a_4200,
    0x5450_a0a8,
    0xa5a5_0000,
    0xa0a0_5050,
    0x5555_a0a0,
    0x5a5a_5050,
    0xaa55_0000,
    0xaa55_5500,
    0xaaaa_5500,
    0x9090_9090,
    0x9494_9494,
    0xa4a4_a4a4,
    0xa9a5_9450,
    0x2a0a_4250,
    0xa594_5040,
    0x0a42_5054,
    0xa5a5_a500,
    0x55a0_a0a0,
    0xa8a8_5454,
    0x6a6a_4040,
    0xa4a4_5000,
    0x1a1a_0500,
    0x0050_a4a4,
    0xaaa5_9090,
    0x1469_6914,
    0x6969_1400,
    0xa085_85a0,
    0xaa82_1414,
    0x50a4_a450,
    0x6a5a_0200,
    0xa9a5_8000,
    0x5090_a0a8,
    0xa8a0_9050,
    0x2424_2424,
    0x00aa_5500,
    0x2492_4924,
    0x2449_9224,
    0x50a5_0a50,
    0x500a_a550,
    0xaaaa_4444,
    0x6666_0000,
    0xa5a0_a5a0,
    0x50a0_50a0,
    0x6928_6928,
    0x44aa_aa44,
    0x6666_6600,
    0xaa44_4444,
    0x54a8_54a8,
    0x9580_9580,
    0x9696_9600,
    0xa854_54a8,
    0x8095_9580,
    0xaa14_1414,
    0x9696_0000,
    0xaaaa_1414,
    0xa050_50a0,
    0xa0a5_a5a0,
    0x9600_0000,
    0x4080_4080,
    0xa9a8_a9a8,
    0xaaaa_aa44,
    0x2a4a_5254,
];

// source: bptc-format
/// The anchor texel of the second subset of two, per partition number.
const ANCHOR_2_OF_2: [u8; 64] = [
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 2, 8, 2, 2, 8, 8, 15, 2, 8,
    2, 2, 8, 8, 2, 2, 15, 15, 6, 8, 2, 8, 15, 15, 2, 8, 2, 2, 2, 15, 15, 6, 6, 2, 6, 8, 15, 15, 2,
    2, 15, 15, 15, 15, 15, 2, 2, 15,
];

// source: bptc-format
/// The anchor texel of the second subset of three, per partition number.
const ANCHOR_2_OF_3: [u8; 64] = [
    3, 3, 15, 15, 8, 3, 15, 15, 8, 8, 6, 6, 6, 5, 3, 3, 3, 3, 8, 15, 3, 3, 6, 10, 5, 8, 8, 6, 8, 5,
    15, 15, 8, 15, 3, 5, 6, 10, 8, 15, 15, 3, 15, 5, 15, 15, 15, 15, 3, 15, 5, 5, 5, 8, 5, 10, 5,
    10, 8, 13, 15, 12, 3, 3,
];

// source: bptc-format
/// The anchor texel of the third subset of three, per partition number.
const ANCHOR_3_OF_3: [u8; 64] = [
    15, 8, 8, 3, 15, 15, 3, 8, 15, 15, 15, 15, 15, 15, 15, 8, 15, 8, 15, 3, 15, 8, 15, 8, 3, 15, 6,
    10, 15, 15, 10, 8, 15, 3, 15, 10, 10, 8, 9, 10, 6, 15, 8, 15, 3, 6, 6, 8, 15, 3, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 3, 15, 15, 8,
];

// source: bptc-format
/// Per mode: subsets, partition bits, rotation bits, index selection bits, colour bits, alpha bits, p-bits per
/// endpoint, shared p-bits per subset, index bits, secondary index bits.
#[derive(Clone, Copy)]
struct Mode {
    subsets: usize,
    partition_bits: u32,
    rotation_bits: u32,
    selection_bits: u32,
    colour_bits: u32,
    alpha_bits: u32,
    endpoint_pbits: bool,
    shared_pbits: bool,
    index_bits: u32,
    secondary_bits: u32,
}

#[allow(clippy::too_many_arguments)] // one row of the format's mode table
const fn mode(
    subsets: usize,
    partition_bits: u32,
    rotation_bits: u32,
    selection_bits: u32,
    colour_bits: u32,
    alpha_bits: u32,
    pbits: (bool, bool),
    index_bits: (u32, u32),
) -> Mode {
    Mode {
        subsets,
        partition_bits,
        rotation_bits,
        selection_bits,
        colour_bits,
        alpha_bits,
        endpoint_pbits: pbits.0,
        shared_pbits: pbits.1,
        index_bits: index_bits.0,
        secondary_bits: index_bits.1,
    }
}

// source: bptc-format
const MODES: [Mode; 8] = [
    mode(3, 4, 0, 0, 4, 0, (true, false), (3, 0)),
    mode(2, 6, 0, 0, 6, 0, (false, true), (3, 0)),
    mode(3, 6, 0, 0, 5, 0, (false, false), (2, 0)),
    mode(2, 6, 0, 0, 7, 0, (true, false), (2, 0)),
    mode(1, 0, 2, 1, 5, 6, (false, false), (2, 3)),
    mode(1, 0, 2, 0, 7, 8, (false, false), (2, 2)),
    mode(1, 0, 0, 0, 7, 7, (true, false), (4, 0)),
    mode(2, 6, 0, 0, 5, 5, (true, false), (2, 0)),
];

// source: bptc-format
/// The 6-bit interpolation weights for 2-, 3- and 4-bit indices.
const WEIGHTS_2: [u32; 4] = [0, 21, 43, 64];
const WEIGHTS_3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHTS_4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

/// Reads bits from the least significant end of a 128-bit block.
struct Bits {
    block: u128,
    at: u32,
}

impl Bits {
    fn take(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = (self.block >> self.at) & ((1_u128 << n) - 1);
        self.at += n;
        // n is at most 8 here, so the value fits
        u32::try_from(v).unwrap_or(0)
    }
}

fn weight(bits: u32, index: u32) -> u32 {
    let i = index as usize;
    match bits {
        2 => WEIGHTS_2[i],
        3 => WEIGHTS_3[i],
        _ => WEIGHTS_4[i],
    }
}

fn interpolate(e0: u32, e1: u32, w: u32) -> u8 {
    u8::try_from(((64 - w) * e0 + w * e1 + 32) >> 6).unwrap_or(u8::MAX)
}

/// An endpoint value of `n` bits (4 to 8, p-bit included) widened to 8 bits by repeating its top bits.
fn widen(v: u32, n: u32) -> u32 {
    if n >= 8 {
        v
    } else {
        (v << (8 - n)) | (v >> (2 * n - 8))
    }
}

/// The subset texel `texel` belongs to, and whether it is the anchor of that subset.
fn subset_of(subsets: usize, partition: usize, texel: usize) -> (usize, bool) {
    match subsets {
        2 => {
            let s = usize::from((PARTITIONS_2[partition] >> texel) & 1);
            let anchor = if s == 0 {
                0
            } else {
                usize::from(ANCHOR_2_OF_2[partition])
            };
            (s, texel == anchor)
        }
        3 => {
            let s = ((PARTITIONS_3[partition] >> (2 * texel)) & 3) as usize;
            let anchor = match s {
                0 => 0,
                1 => usize::from(ANCHOR_2_OF_3[partition]),
                _ => usize::from(ANCHOR_3_OF_3[partition]),
            };
            (s, texel == anchor)
        }
        _ => (0, texel == 0),
    }
}

/// Decode one 16-byte BC7 block into 16 RGBA texels, y-major. A block with no mode bit set (a reserved encoding)
/// decodes to transparent black, as the format description asks.
#[must_use]
pub fn decode_block(bytes: &[u8; 16]) -> [[u8; 4]; 16] {
    let block = u128::from_le_bytes(*bytes);
    let Some(number) = (0..8).find(|m| block >> m & 1 == 1) else {
        return [[0; 4]; 16];
    };
    let m = MODES[number];
    let mut bits = Bits {
        block,
        at: u32::try_from(number).unwrap_or(0) + 1,
    };
    let partition = bits.take(m.partition_bits) as usize;
    let rotation = bits.take(m.rotation_bits);
    let selection = bits.take(m.selection_bits);
    // endpoints[subset * 2 + end][channel]
    let ends = m.subsets * 2;
    let mut endpoints = [[0_u32; 4]; 6];
    for channel in 0..3 {
        for e in endpoints.iter_mut().take(ends) {
            e[channel] = bits.take(m.colour_bits);
        }
    }
    for e in endpoints.iter_mut().take(ends) {
        e[3] = bits.take(m.alpha_bits);
    }
    let (mut colour_bits, mut alpha_bits) = (m.colour_bits, m.alpha_bits);
    if m.endpoint_pbits || m.shared_pbits {
        let mut pbits = [0_u32; 6];
        if m.endpoint_pbits {
            for p in pbits.iter_mut().take(ends) {
                *p = bits.take(1);
            }
        } else {
            for subset in 0..m.subsets {
                let p = bits.take(1);
                pbits[subset * 2] = p;
                pbits[subset * 2 + 1] = p;
            }
        }
        for (e, p) in endpoints.iter_mut().zip(pbits).take(ends) {
            for c in e.iter_mut() {
                *c = (*c << 1) | p;
            }
        }
        colour_bits += 1;
        if alpha_bits > 0 {
            alpha_bits += 1;
        }
    }
    for e in endpoints.iter_mut().take(ends) {
        for c in &mut e[..3] {
            *c = widen(*c, colour_bits);
        }
        e[3] = if alpha_bits == 0 {
            255
        } else {
            widen(e[3], alpha_bits)
        };
    }
    let mut primary = [0_u32; 16];
    for (texel, index) in primary.iter_mut().enumerate() {
        let (_, anchor) = subset_of(m.subsets, partition, texel);
        *index = bits.take(m.index_bits - u32::from(anchor));
    }
    let mut secondary = [0_u32; 16];
    if m.secondary_bits > 0 {
        for (texel, index) in secondary.iter_mut().enumerate() {
            *index = bits.take(m.secondary_bits - u32::from(texel == 0));
        }
    }
    let mut out = [[0_u8; 4]; 16];
    for (texel, px) in out.iter_mut().enumerate() {
        let (subset, _) = subset_of(m.subsets, partition, texel);
        let (e0, e1) = (endpoints[subset * 2], endpoints[subset * 2 + 1]);
        // the index selection bit swaps which index set the colour and the alpha use
        let ((cb, ci), (ab, ai)) = if m.secondary_bits == 0 {
            (
                (m.index_bits, primary[texel]),
                (m.index_bits, primary[texel]),
            )
        } else if selection == 1 {
            (
                (m.secondary_bits, secondary[texel]),
                (m.index_bits, primary[texel]),
            )
        } else {
            (
                (m.index_bits, primary[texel]),
                (m.secondary_bits, secondary[texel]),
            )
        };
        let wc = weight(cb, ci);
        for c in 0..3 {
            px[c] = interpolate(e0[c], e1[c], wc);
        }
        px[3] = interpolate(e0[3], e1[3], weight(ab, ai));
        match rotation {
            1 => px.swap(3, 0),
            2 => px.swap(3, 1),
            3 => px.swap(3, 2),
            _ => {}
        }
    }
    out
}

/// Decode a `width` by `height` BC7 surface (blocks in rows, 16 bytes each) to RGBA. `data` must hold every block.
#[must_use]
pub fn decode_surface(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let across = width.div_ceil(4);
    let mut rgba = vec![0_u8; width * height * 4];
    for (k, block) in data.as_chunks::<16>().0.iter().enumerate() {
        let (bx, by) = (k % across * 4, k / across * 4);
        if by >= height {
            break;
        }
        for (texel, px) in decode_block(block).iter().enumerate() {
            let (x, y) = (bx + texel % 4, by + texel / 4);
            if x < width && y < height {
                let at = (y * width + x) * 4;
                rgba[at..at + 4].copy_from_slice(px);
            }
        }
    }
    rgba
}
