// SPDX-License-Identifier: GPL-3.0-only
//! BC7 blocks of every mode, built here field by field from the public bit layout, decoded through `decode_image`.

use lodwright_dds::decode_image;
use lodwright_testkit::{covers, ids};

/// Per mode: subsets, partition bits, rotation bits, index selection bits, colour bits, alpha bits, p-bit kind
/// (0 none, 1 per endpoint, 2 shared per subset), index bits, secondary index bits.
type Row = (usize, u32, u32, u32, u32, u32, u8, u32, u32);

const LAYOUT: [Row; 8] = [
    (3, 4, 0, 0, 4, 0, 1, 3, 0),
    (2, 6, 0, 0, 6, 0, 2, 3, 0),
    (3, 6, 0, 0, 5, 0, 0, 2, 0),
    (2, 6, 0, 0, 7, 0, 1, 2, 0),
    (1, 0, 2, 1, 5, 6, 0, 2, 3),
    (1, 0, 2, 0, 7, 8, 0, 2, 2),
    (1, 0, 0, 0, 7, 7, 1, 4, 0),
    (2, 6, 0, 0, 5, 5, 1, 2, 0),
];

/// Two-subset partition 17 (subset per texel, y-major) and its second anchor; three-subset partition 1 and its anchors.
const P2_17: [usize; 16] = [0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0];
const P2_17_ANCHOR: usize = 2;
const P3_1: [usize; 16] = [0, 0, 0, 1, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2, 2, 1];
const P3_1_ANCHORS: [usize; 2] = [3, 8];

struct Writer {
    bits: u128,
    at: u32,
}

impl Writer {
    fn put(&mut self, value: u32, n: u32) {
        assert!(n == 0 || value < (1 << n), "{value} does not fit {n} bits");
        self.bits |= u128::from(value) << self.at;
        self.at += n;
    }
}

/// What a block holds: endpoints as stored (without p-bits) per subset and end, RGBA; p-bits as stored; indices.
struct Block {
    mode: usize,
    partition: u32,
    rotation: u32,
    selection: u32,
    endpoints: Vec<[u32; 4]>,
    pbits: Vec<u32>,
    primary: [u32; 16],
    secondary: [u32; 16],
}

fn subsets_of(mode: usize) -> [usize; 16] {
    match LAYOUT[mode].0 {
        2 => P2_17,
        3 => P3_1,
        _ => [0; 16],
    }
}

fn is_anchor(mode: usize, texel: usize) -> bool {
    texel == 0
        || match LAYOUT[mode].0 {
            2 => texel == P2_17_ANCHOR,
            3 => P3_1_ANCHORS.contains(&texel),
            _ => false,
        }
}

fn encode(b: &Block) -> [u8; 16] {
    let (_, pb, rb, isb, cb, ab, pk, ib, ib2) = LAYOUT[b.mode];
    let mut w = Writer { bits: 0, at: 0 };
    w.put(1 << b.mode, u32::try_from(b.mode).unwrap() + 1);
    w.put(b.partition, pb);
    w.put(b.rotation, rb);
    w.put(b.selection, isb);
    for c in 0..4 {
        for e in &b.endpoints {
            w.put(e[c], if c == 3 { ab } else { cb });
        }
    }
    if pk != 0 {
        for p in &b.pbits {
            w.put(*p, 1);
        }
    }
    for (t, i) in b.primary.iter().enumerate() {
        w.put(*i, ib - u32::from(is_anchor(b.mode, t)));
    }
    if ib2 > 0 {
        for (t, i) in b.secondary.iter().enumerate() {
            w.put(*i, ib2 - u32::from(t == 0));
        }
    }
    assert_eq!(w.at, 128, "mode {} fills the block", b.mode);
    w.bits.to_le_bytes()
}

fn dx10_file(block: &[u8; 16]) -> Vec<u8> {
    let mut f = vec![0_u8; 148];
    f[0..4].copy_from_slice(b"DDS ");
    f[4..8].copy_from_slice(&124_u32.to_le_bytes());
    f[8..12].copy_from_slice(&0x1007_u32.to_le_bytes());
    f[12..16].copy_from_slice(&4_u32.to_le_bytes());
    f[16..20].copy_from_slice(&4_u32.to_le_bytes());
    f[76..80].copy_from_slice(&32_u32.to_le_bytes());
    f[80..84].copy_from_slice(&4_u32.to_le_bytes());
    f[84..88].copy_from_slice(b"DX10");
    f[128..132].copy_from_slice(&98_u32.to_le_bytes());
    f[132..136].copy_from_slice(&3_u32.to_le_bytes());
    f[140..144].copy_from_slice(&1_u32.to_le_bytes());
    f.extend_from_slice(block);
    f
}

fn texels(block: &[u8; 16]) -> Vec<[u8; 4]> {
    let image = decode_image(&dx10_file(block)).unwrap();
    (0..16)
        .map(|t| image.pixel(t % 4, t / 4).unwrap())
        .collect()
}

/// A stored value of `n` bits widened to 8 by repeating its top bits.
fn widen(v: u32, n: u32) -> u8 {
    u8::try_from(if n == 8 {
        v
    } else {
        (v << (8 - n)) | (v >> (2 * n - 8))
    })
    .unwrap()
}

fn lerp(a: u8, b: u8, w: u32) -> u8 {
    u8::try_from(((64 - w) * u32::from(a) + w * u32::from(b) + 32) >> 6).unwrap()
}

const W2: [u32; 4] = [0, 21, 43, 64];
const W3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const W4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

#[test]
fn every_mode_draws_each_subset_in_its_own_colour_where_the_partition_puts_it() {
    covers(&[ids::formats::meshes_and_images::DDS_READER]);
    for mode in 0..8 {
        let (ns, _, _, _, cb, ab, pk, _, _) = LAYOUT[mode];
        let top = (1 << cb) - 1;
        let alpha_top = if ab == 0 { 0 } else { (1 << ab) - 1 };
        // subset colours: (top, 0, top), (0, top, 0), (top, top, 0); alpha full, then half
        let colours = [
            [top, 0, top, alpha_top],
            [0, top, 0, alpha_top / 2],
            [top, top, 0, alpha_top],
        ];
        let endpoints: Vec<[u32; 4]> = (0..ns).flat_map(|s| [colours[s], colours[s]]).collect();
        let pbits = match pk {
            1 => vec![1; ns * 2],
            2 => vec![1; ns],
            _ => vec![],
        };
        let partition = match ns {
            2 => 17,
            3 => 1,
            _ => 0,
        };
        let block = Block {
            mode,
            partition,
            rotation: 0,
            selection: 0,
            endpoints,
            pbits,
            primary: [0; 16],
            secondary: [0; 16],
        };
        let got = texels(&encode(&block));
        let p = u32::from(pk != 0);
        let expect = |v: u32, bits: u32| widen((v << p) | p, bits + p);
        for (t, px) in got.iter().enumerate() {
            let c = colours[subsets_of(mode)[t]];
            let alpha = if ab == 0 { 255 } else { expect(c[3], ab) };
            assert_eq!(
                *px,
                [expect(c[0], cb), expect(c[1], cb), expect(c[2], cb), alpha],
                "mode {mode} texel {t}"
            );
        }
    }
}

/// Indices 0, 1, 2, ... per texel (within each anchor's smaller range) over black to white endpoints.
fn ramp(mode: usize) -> [u32; 16] {
    let ib = LAYOUT[mode].7;
    let mut idx = [0; 16];
    for (t, i) in idx.iter_mut().enumerate() {
        let bits = ib - u32::from(is_anchor(mode, t));
        *i = u32::try_from(t).unwrap() % (1 << bits);
    }
    idx
}

#[test]
fn indices_with_anchor_texels_interpolate_by_the_weights_in_every_multi_subset_mode() {
    for mode in [0, 1, 2, 3, 7] {
        let (ns, _, _, _, cb, ab, pk, ib, _) = LAYOUT[mode];
        let top = (1 << cb) - 1;
        let a = if ab == 0 { 0 } else { (1 << ab) - 1 };
        let endpoints: Vec<[u32; 4]> = (0..ns)
            .flat_map(|_| [[0, 0, 0, a], [top, top, top, a]])
            .collect();
        // p-bit 0 on the black end, 1 on the white end, so the ends widen to 0 and 255
        let pbits = match pk {
            1 => (0..ns).flat_map(|_| [0, 1]).collect(),
            2 => vec![0; ns],
            _ => vec![],
        };
        let partition = if ns == 2 { 17 } else { 1 };
        let primary = ramp(mode);
        let block = Block {
            mode,
            partition,
            rotation: 0,
            selection: 0,
            endpoints,
            pbits,
            primary,
            secondary: [0; 16],
        };
        let got = texels(&encode(&block));
        // the white end: shared p-bits of 0 (mode 1) leave its lowest stored bit 0
        let white = if pk == 2 {
            widen(top << 1, cb + 1)
        } else {
            255
        };
        for (t, px) in got.iter().enumerate() {
            let w = match ib {
                2 => W2[primary[t] as usize],
                3 => W3[primary[t] as usize],
                _ => W4[primary[t] as usize],
            };
            let v = lerp(0, white, w);
            assert_eq!(&px[..3], &[v, v, v], "mode {mode} texel {t}");
        }
    }
}

#[test]
fn mode_6_interpolates_sixteen_steps_from_black_to_white() {
    covers(&[ids::formats::meshes_and_images::DDS_READER]);
    let primary = ramp(6);
    let block = Block {
        mode: 6,
        partition: 0,
        rotation: 0,
        selection: 0,
        endpoints: vec![[0, 0, 0, 127], [127, 127, 127, 127]],
        pbits: vec![0, 1],
        primary,
        secondary: [0; 16],
    };
    let got = texels(&encode(&block));
    for (t, px) in got.iter().enumerate() {
        let v = lerp(0, 255, W4[primary[t] as usize]);
        // alpha: 127 with p-bit 0 is 254, with 1 is 255
        assert_eq!(
            *px,
            [v, v, v, lerp(254, 255, W4[primary[t] as usize])],
            "texel {t}"
        );
    }
    // the last texel has index 15: white
    assert_eq!(got[15], [255, 255, 255, 255]);
}

#[test]
fn modes_4_and_5_take_colour_and_alpha_from_separate_indices_with_selection_and_rotation() {
    covers(&[ids::formats::meshes_and_images::DDS_READER]);
    let primary = ramp(4);
    let secondary = {
        let mut s = [0; 16];
        for (t, i) in s.iter_mut().enumerate() {
            *i = (15 - u32::try_from(t).unwrap()) % if t == 0 { 4 } else { 8 };
        }
        s
    };
    for selection in 0..2 {
        for rotation in 0..4 {
            let block = Block {
                mode: 4,
                partition: 0,
                rotation,
                selection,
                endpoints: vec![[0, 0, 0, 0], [31, 31, 31, 63]],
                pbits: vec![],
                primary,
                secondary,
            };
            let got = texels(&encode(&block));
            for (t, px) in got.iter().enumerate() {
                let (c, a) = if selection == 0 {
                    (W2[primary[t] as usize], W3[secondary[t] as usize])
                } else {
                    (W3[secondary[t] as usize], W2[primary[t] as usize])
                };
                let mut want = [
                    lerp(0, 255, c),
                    lerp(0, 255, c),
                    lerp(0, 255, c),
                    lerp(0, 255, a),
                ];
                if rotation > 0 {
                    want.swap(3, rotation as usize - 1);
                }
                assert_eq!(
                    *px, want,
                    "selection {selection} rotation {rotation} texel {t}"
                );
            }
        }
    }
    // mode 5: two-bit colour and two-bit alpha indices, eight alpha bits
    let block = Block {
        mode: 5,
        partition: 0,
        rotation: 0,
        selection: 0,
        endpoints: vec![[0, 0, 0, 255], [127, 127, 127, 0]],
        pbits: vec![],
        primary: ramp(5),
        secondary: ramp(5),
    };
    let got = texels(&encode(&block));
    for (t, px) in got.iter().enumerate() {
        let w = W2[ramp(5)[t] as usize];
        assert_eq!(
            *px,
            [
                lerp(0, 255, w),
                lerp(0, 255, w),
                lerp(0, 255, w),
                lerp(255, 0, w)
            ],
            "texel {t}"
        );
    }
}

#[test]
fn a_block_without_a_mode_bit_is_transparent_black() {
    assert!(texels(&[0; 16]).iter().all(|px| *px == [0, 0, 0, 0]));
}

#[test]
fn any_block_decodes_without_a_panic() {
    // a cheap sweep over many bit patterns; the fuzz target goes further
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..20_000 {
        let mut block = [0_u8; 16];
        for b in &mut block {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *b = state.to_le_bytes()[0];
        }
        let _ = texels(&block);
    }
}
