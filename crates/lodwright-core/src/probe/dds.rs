// SPDX-License-Identifier: MPL-2.0
//! DDS probe: the header fields, and per-channel statistics of mip level 0 for uncompressed pixels and for
//! BC1 and BC3 (decoded). Pixel values enter the report only as the mean, minimum and maximum of each channel.

use super::reader::Reader;
use super::{Ident, ProbeError, Value};

const HEADER_BYTES: usize = 128;
const DX10_BYTES: usize = 20;
const MAX_STAT_PIXELS: u64 = 1 << 26;
const TRUNCATED: ProbeError = ProbeError::new("the DDS file ends inside its header");

/// Probe the DDS `data`.
///
/// # Errors
/// [`ProbeError`] when `data` is not a DDS with the magic and the header sizes, or ends inside the header or inside
/// the pixel data of mip level 0 (when statistics are taken).
pub fn probe(data: &[u8]) -> Result<Value, ProbeError> {
    // source: dds-header-layout, dds-pixel-format
    if data.len() < HEADER_BYTES {
        return Err(TRUNCATED);
    }
    if &data[..4] != b"DDS " {
        return Err(ProbeError::new("not a DDS file: bad magic"));
    }
    let mut r = Reader::new(&data[4..HEADER_BYTES]);
    let size = r.u32().ok_or(TRUNCATED)?;
    if size != 124 {
        return Err(ProbeError::new("not a DDS file: bad header size"));
    }
    let flags = r.u32().ok_or(TRUNCATED)?;
    let height = r.u32().ok_or(TRUNCATED)?;
    let width = r.u32().ok_or(TRUNCATED)?;
    let pitch_or_linear = r.u32().ok_or(TRUNCATED)?;
    let depth = r.u32().ok_or(TRUNCATED)?;
    let mips = r.u32().ok_or(TRUNCATED)?;
    r.skip(44).ok_or(TRUNCATED)?; // reserved
    // source: dds-pixel-format-masks
    let pf_size = r.u32().ok_or(TRUNCATED)?;
    if pf_size != 32 {
        return Err(ProbeError::new("not a DDS file: bad pixel format size"));
    }
    let pf = PixelFormat {
        flags: r.u32().ok_or(TRUNCATED)?,
        four_cc: r
            .take(4)
            .ok_or(TRUNCATED)?
            .try_into()
            .map_err(|_| TRUNCATED)?,
        bit_count: r.u32().ok_or(TRUNCATED)?,
        masks: [
            r.u32().ok_or(TRUNCATED)?,
            r.u32().ok_or(TRUNCATED)?,
            r.u32().ok_or(TRUNCATED)?,
            r.u32().ok_or(TRUNCATED)?,
        ],
    };

    let mut report = vec![
        ("file_bytes", Value::count(data.len())),
        ("width", Value::Uint(u64::from(width))),
        ("height", Value::Uint(u64::from(height))),
        ("depth", Value::Uint(u64::from(depth))),
        ("mip_count", Value::Uint(u64::from(mips))),
        ("header_flags", Value::Uint(u64::from(flags))),
        (
            "pitch_or_linear_size",
            Value::Uint(u64::from(pitch_or_linear)),
        ),
        ("pixel_format_flags", Value::Uint(u64::from(pf.flags))),
        (
            "four_cc_value",
            Value::Uint(u64::from(u32::from_le_bytes(pf.four_cc))),
        ),
    ];
    if pf.flags & 0x4 != 0 {
        report.push(("four_cc", pf.four_cc_value()));
    }
    report.push(("rgb_bit_count", Value::Uint(u64::from(pf.bit_count))));
    report.push((
        "masks",
        Value::Map(vec![
            ("red", Value::Uint(u64::from(pf.masks[0]))),
            ("green", Value::Uint(u64::from(pf.masks[1]))),
            ("blue", Value::Uint(u64::from(pf.masks[2]))),
            ("alpha", Value::Uint(u64::from(pf.masks[3]))),
        ]),
    ));

    let mut data_start = HEADER_BYTES;
    let mut format = Format::from_pixel_format(&pf);
    if pf.flags & 0x4 != 0 && &pf.four_cc == b"DX10" {
        // source: dds-header-dxt10
        let mut x = Reader::new(data.get(HEADER_BYTES..).unwrap_or_default());
        let dxgi = x
            .u32()
            .ok_or(ProbeError::new("the DDS file ends inside its DX10 header"))?;
        let dimension = x.u32().ok_or(TRUNCATED)?;
        x.skip(4).ok_or(TRUNCATED)?;
        let array_size = x.u32().ok_or(TRUNCATED)?;
        data_start += DX10_BYTES;
        format = Format::Skipped("dx10");
        report.push((
            "dx10",
            Value::Map(vec![
                ("dxgi_format", Value::Uint(u64::from(dxgi))),
                ("resource_dimension", Value::Uint(u64::from(dimension))),
                ("array_size", Value::Uint(u64::from(array_size))),
            ]),
        ));
    }
    report.push(("format", Value::Label(format.label())));
    if let Some(stats) = statistics(
        &format,
        width,
        height,
        data.get(data_start..).unwrap_or_default(),
    )? {
        report.push(("mip0", stats));
    } else if let Format::Skipped(why) = format {
        report.push(("mip0_skipped", Value::Label(why)));
    }
    Ok(Value::Map(report))
}

struct PixelFormat {
    flags: u32,
    four_cc: [u8; 4],
    bit_count: u32,
    masks: [u32; 4],
}

impl PixelFormat {
    /// The `FourCC` when its four bytes are ASCII letters, digits or spaces; a label otherwise.
    fn four_cc_value(&self) -> Value {
        let text: Vec<u8> = self
            .four_cc
            .iter()
            .copied()
            .filter(|b| *b != b' ')
            .collect();
        let safe = self
            .four_cc
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b' ');
        match (safe, Ident::checked(&text)) {
            (true, Some(ident)) => Value::Ident(ident),
            _ => Value::Label("other"),
        }
    }
}

enum Format {
    /// Block-compressed BC1 (DXT1): 8 bytes per 4 by 4 block.
    Bc1,
    /// Block-compressed BC3 (DXT4 and DXT5): 16 bytes per 4 by 4 block.
    Bc3,
    /// Uncompressed with contiguous channel masks.
    Rgb {
        bytes: usize,
        channels: Vec<Channel>,
    },
    /// Statistics are not taken, for this reason.
    Skipped(&'static str),
}

struct Channel {
    name: &'static str,
    shift: u32,
    bits: u32,
}

impl Format {
    // source: dds-pixel-format, dds-pixel-format-masks
    fn from_pixel_format(pf: &PixelFormat) -> Self {
        if pf.flags & 0x4 != 0 {
            return match &pf.four_cc {
                b"DXT1" => Self::Bc1,
                b"DXT4" | b"DXT5" => Self::Bc3,
                b"DX10" => Self::Skipped("dx10"),
                _ => Self::Skipped("four_cc"),
            };
        }
        if pf.flags & 0x40 == 0 || !matches!(pf.bit_count, 8 | 16 | 24 | 32) {
            return Self::Skipped("pixel_format");
        }
        let mut channels = vec![];
        let names = ["red", "green", "blue", "alpha"];
        for (index, name) in names.iter().enumerate() {
            let mask = pf.masks[index];
            if mask == 0 {
                if index < 3 {
                    return Self::Skipped("mask");
                }
                continue;
            }
            let shift = mask.trailing_zeros();
            let bits = (mask >> shift).trailing_ones();
            if bits + shift != 32 - mask.leading_zeros() || bits > 16 || shift + bits > pf.bit_count
            {
                return Self::Skipped("mask");
            }
            channels.push(Channel { name, shift, bits });
        }
        Self::Rgb {
            bytes: usize::try_from(pf.bit_count / 8).unwrap_or(4),
            channels,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Bc1 => "bc1",
            Self::Bc3 => "bc3",
            Self::Rgb { .. } => "uncompressed",
            Self::Skipped(_) => "not_decoded",
        }
    }
}

/// Running sum, minimum and maximum of one 8-bit channel.
#[derive(Clone, Copy)]
struct Stat {
    sum: u64,
    min: u8,
    max: u8,
}

impl Stat {
    const fn new() -> Self {
        Self {
            sum: 0,
            min: 255,
            max: 0,
        }
    }

    fn add(&mut self, v: u8) {
        self.sum += u64::from(v);
        self.min = self.min.min(v);
        self.max = self.max.max(v);
    }
}

fn channel_value(names: &[&'static str], stats: &[Stat], pixels: u64) -> Value {
    Value::Map(
        names
            .iter()
            .zip(stats)
            .map(|(name, s)| {
                #[allow(clippy::cast_precision_loss)]
                let mean = s.sum as f64 / pixels as f64;
                (
                    *name,
                    Value::Map(vec![
                        ("mean", Value::Float(mean)),
                        ("min", Value::Uint(u64::from(s.min))),
                        ("max", Value::Uint(u64::from(s.max))),
                    ]),
                )
            })
            .collect(),
    )
}

fn statistics(
    format: &Format,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<Option<Value>, ProbeError> {
    if matches!(format, Format::Skipped(_)) {
        return Ok(None);
    }
    let count = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 {
        return Err(ProbeError::new("the DDS has a zero width or height"));
    }
    if count > MAX_STAT_PIXELS {
        return Ok(Some(Value::Map(vec![(
            "skipped",
            Value::Label("too_large"),
        )])));
    }
    let (w, h) = (
        usize::try_from(width).unwrap_or(0),
        usize::try_from(height).unwrap_or(0),
    );
    let short = ProbeError::new("the DDS file ends inside the pixel data of mip level 0");
    let (names, stats): (Vec<&'static str>, Vec<Stat>) = match format {
        Format::Bc1 | Format::Bc3 => {
            let per_block = if matches!(format, Format::Bc1) { 8 } else { 16 };
            let (bx, by) = (w.div_ceil(4), h.div_ceil(4));
            let needed = bx.checked_mul(by).and_then(|b| b.checked_mul(per_block));
            let block_data = needed.and_then(|n| pixels.get(..n)).ok_or(short)?;
            let mut stats = [Stat::new(); 4];
            for (i, block) in block_data.chunks_exact(per_block).enumerate() {
                let rgba = decode_block(block, per_block == 16);
                let (x0, y0) = ((i % bx) * 4, (i / bx) * 4);
                for (t, texel) in rgba.iter().enumerate() {
                    if x0 + t % 4 < w && y0 + t / 4 < h {
                        for (c, stat) in stats.iter_mut().enumerate() {
                            stat.add(texel[c]);
                        }
                    }
                }
            }
            (vec!["red", "green", "blue", "alpha"], stats.to_vec())
        }
        Format::Rgb { bytes, channels } => {
            let needed = w.checked_mul(h).and_then(|n| n.checked_mul(*bytes));
            let data = needed.and_then(|n| pixels.get(..n)).ok_or(short)?;
            let mut stats = vec![Stat::new(); channels.len()];
            for px in data.chunks_exact(*bytes) {
                let mut raw = 0_u32;
                for (i, b) in px.iter().enumerate() {
                    raw |= u32::from(*b) << (8 * i);
                }
                for (channel, stat) in channels.iter().zip(&mut stats) {
                    let value = (raw >> channel.shift) & ((1_u32 << channel.bits) - 1);
                    let max = (1_u32 << channel.bits) - 1;
                    stat.add(u8::try_from((value * 255 + max / 2) / max).unwrap_or(255));
                }
            }
            (channels.iter().map(|c| c.name).collect(), stats)
        }
        Format::Skipped(_) => return Ok(None),
    };
    Ok(Some(channel_value(&names, &stats, count)))
}

fn expand5(v: u16) -> u8 {
    u8::try_from((v << 3) | (v >> 2)).unwrap_or(255)
}

fn expand6(v: u16) -> u8 {
    u8::try_from((v << 2) | (v >> 4)).unwrap_or(255)
}

// source: probe-bc1-block-layout
fn colour_table(block: &[u8], four_colour_only: bool) -> [[u8; 4]; 4] {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let rgb = |c: u16| [expand5(c >> 11), expand6((c >> 5) & 63), expand5(c & 31)];
    let (a, b) = (rgb(c0), rgb(c1));
    let mix = |wa: u16, wb: u16, div: u16, round: u16| -> [u8; 3] {
        std::array::from_fn(|i| {
            u8::try_from((wa * u16::from(a[i]) + wb * u16::from(b[i]) + round) / div).unwrap_or(255)
        })
    };
    let opaque = |rgb: [u8; 3]| [rgb[0], rgb[1], rgb[2], 255];
    let mut table = [opaque(a), opaque(b), [0; 4], [0; 4]];
    if c0 > c1 || four_colour_only {
        table[2] = opaque(mix(2, 1, 3, 1));
        table[3] = opaque(mix(1, 2, 3, 1));
    } else {
        table[2] = opaque(mix(1, 1, 2, 0));
        // code 11 is a transparent texel
    }
    table
}

fn decode_block(block: &[u8], with_alpha_block: bool) -> [[u8; 4]; 16] {
    let colour = if with_alpha_block { &block[8..] } else { block };
    let table = colour_table(colour, with_alpha_block);
    // source: probe-bc1-block-layout
    let bitmap = u32::from_le_bytes([colour[4], colour[5], colour[6], colour[7]]);
    let alphas = with_alpha_block.then(|| alpha_table(block));
    let alpha_bits = block[2..8]
        .iter()
        .rev()
        .fold(0_u64, |acc, b| (acc << 8) | u64::from(*b));
    let mut out = [[0; 4]; 16];
    for (t, texel) in out.iter_mut().enumerate() {
        *texel = table[((bitmap >> (2 * t)) & 3) as usize];
        if let Some(alphas) = &alphas {
            texel[3] = alphas[((alpha_bits >> (3 * t)) & 7) as usize];
        }
    }
    out
}

// source: probe-bc3-block-layout
fn alpha_table(block: &[u8]) -> [u8; 8] {
    let (a0, a1) = (u16::from(block[0]), u16::from(block[1]));
    let mut t = [0_u16; 8];
    t[0] = a0;
    t[1] = a1;
    if a0 > a1 {
        for k in 1..=6_u16 {
            t[usize::from(k) + 1] = ((7 - k) * a0 + k * a1 + 3) / 7;
        }
    } else {
        for k in 1..=4_u16 {
            t[usize::from(k) + 1] = ((5 - k) * a0 + k * a1 + 2) / 5;
        }
        t[6] = 0;
        t[7] = 255;
    }
    t.map(|v| u8::try_from(v).unwrap_or(255))
}

#[cfg(test)]
mod tests {
    use super::probe;
    use crate::probe::Value;
    use lodwright_testkit::dds::{Pixels, build_file};

    fn json(bytes: &[u8]) -> String {
        probe(bytes).unwrap().to_json()
    }

    #[test]
    fn header_fields_are_reported_for_a_bc1_file() {
        let bytes = build_file(8, 4, 1, &Pixels::Bc1Solid { colour: 0xF800 });
        let json = json(&bytes);
        assert!(json.contains(r#""width":8,"height":4"#), "{json}");
        assert!(json.contains(r#""mip_count":1"#), "{json}");
        assert!(json.contains(r#""four_cc":"DXT1""#), "{json}");
        assert!(json.contains(r#""format":"bc1""#), "{json}");
        // pure red: 5-bit 31 widens to 255
        assert!(
            json.contains(r#""red":{"mean":255.000,"min":255,"max":255}"#),
            "{json}"
        );
        assert!(
            json.contains(r#""green":{"mean":0.000,"min":0,"max":0}"#),
            "{json}"
        );
        assert!(
            json.contains(r#""alpha":{"mean":255.000,"min":255,"max":255}"#),
            "{json}"
        );
    }

    #[test]
    fn bc3_alpha_blocks_are_decoded() {
        let bytes = build_file(4, 4, 1, &Pixels::Bc3Gradient);
        let json = json(&bytes);
        assert!(json.contains(r#""format":"bc3""#), "{json}");
        // alpha_0 = 255, alpha_1 = 0: codes run 0..7 over 16 texels, two six-step ramps
        assert!(json.contains(r#""alpha":{"mean":"#), "{json}");
        assert!(json.contains(r#""min":0,"max":255}"#), "{json}");
    }

    #[test]
    fn bc1_three_colour_blocks_have_transparent_texels() {
        let bytes = build_file(4, 4, 1, &Pixels::Bc1Transparent);
        let json = json(&bytes);
        assert!(
            json.contains(r#""alpha":{"mean":0.000,"min":0,"max":0}"#),
            "{json}"
        );
    }

    #[test]
    fn uncompressed_masks_give_exact_channel_statistics() {
        let bytes = build_file(
            2,
            1,
            1,
            &Pixels::Argb32(vec![[10, 20, 30, 255], [30, 40, 50, 1]]),
        );
        let json = json(&bytes);
        assert!(json.contains(r#""format":"uncompressed""#), "{json}");
        assert!(
            json.contains(r#""red":{"mean":20.000,"min":10,"max":30}"#),
            "{json}"
        );
        assert!(
            json.contains(r#""green":{"mean":30.000,"min":20,"max":40}"#),
            "{json}"
        );
        assert!(
            json.contains(r#""blue":{"mean":40.000,"min":30,"max":50}"#),
            "{json}"
        );
        assert!(
            json.contains(r#""alpha":{"mean":128.000,"min":1,"max":255}"#),
            "{json}"
        );
        assert!(!json.contains(r#""four_cc""#), "no FourCC without the flag");
    }

    #[test]
    fn the_dx10_header_is_reported_and_pixels_are_not_decoded() {
        let bytes = build_file(4, 4, 1, &Pixels::Dx10 { dxgi_format: 71 });
        let json = json(&bytes);
        assert!(json.contains(r#""four_cc":"DX10""#), "{json}");
        assert!(json.contains(r#""dxgi_format":71"#), "{json}");
        assert!(json.contains(r#""mip0_skipped":"dx10""#), "{json}");
    }

    #[test]
    fn unknown_four_cc_text_is_not_echoed() {
        let mut bytes = build_file(4, 4, 1, &Pixels::Bc1Solid { colour: 0 });
        bytes[84..88].copy_from_slice(b"Q\"\n\x01");
        let json = json(&bytes);
        assert!(json.contains(r#""four_cc":"other""#), "{json}");
        assert!(json.contains(r#""mip0_skipped":"four_cc""#), "{json}");
        assert!(!json.contains('Q'));
    }

    #[test]
    fn truncated_and_garbage_input_is_an_error_at_every_length() {
        let bytes = build_file(8, 8, 1, &Pixels::Bc3Gradient);
        for cut in 0..bytes.len() {
            assert!(probe(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        assert!(probe(&bytes).is_ok());
        assert!(probe(&[0; 200]).is_err());
        assert!(probe(b"DDS ").is_err());
        let mut bad = bytes.clone();
        bad[4] = 100;
        assert!(probe(&bad).is_err());
        let mut bad = bytes.clone();
        bad[76] = 31;
        assert!(probe(&bad).is_err());
    }

    #[test]
    fn zero_sized_images_and_huge_images_do_not_panic() {
        let zero = build_file(0, 0, 1, &Pixels::Bc1Solid { colour: 0 });
        assert!(probe(&zero).is_err());
        let mut big = build_file(4, 4, 1, &Pixels::Bc1Solid { colour: 0 });
        big[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        big[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        let Value::Map(fields) = probe(&big).unwrap() else {
            panic!()
        };
        assert!(fields.iter().any(|(n, _)| *n == "mip0"));
        assert!(probe(&big).unwrap().to_json().contains("too_large"));
    }

    #[test]
    fn odd_masks_are_skipped_not_guessed() {
        let mut bytes = build_file(1, 1, 1, &Pixels::Argb32(vec![[1, 2, 3, 4]]));
        bytes[92..96].copy_from_slice(&0x00FF_00FF_u32.to_le_bytes()); // split red mask
        assert!(json(&bytes).contains(r#""mip0_skipped":"mask""#));
    }
}
