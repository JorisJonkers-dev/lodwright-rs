// SPDX-License-Identifier: GPL-3.0-only
//! Per-block terrain images: normal maps from heights and diffuse maps from vertex colours.
//!
//! Orientation follows the block mesh's UVs (row `game-lod-terrain-block-nif`): image column `c` of `n` is at
//! block-local x = `size * (1 - (c + 0.5) / n)`, image row `r` at y = `size * (r + 0.5) / n`.
use lodwright_core::land::SIDE;
use lodwright_core::layers::quadrant_origin;
use lodwright_core::layout::BlockId;
use lodwright_core::terrain::VERTEX_SPACING;
use lodwright_core::worldspace::Worldspace;
use lodwright_dds::Image;

use crate::diffuse::{LandTiles, blend};

const CELL_UNITS: f32 = 4096.0;
/// Colour of ground with no vertex colours (a neutral dry earth).
const FALLBACK: [u8; 3] = [96, 88, 72];

fn fill_gaps(samples: &[f32]) -> Vec<f32> {
    let valid: Vec<f32> = samples.iter().copied().filter(|v| !v.is_nan()).collect();
    let mean = if valid.is_empty() {
        0.0
    } else {
        valid.iter().sum::<f32>() / valid.len() as f32
    };
    samples
        .iter()
        .map(|v| if v.is_nan() { mean } else { *v })
        .collect()
}

fn height_at(h: &[f32], span: usize, x: f32, y: f32) -> f32 {
    let max = (span - 1) as f32;
    let (x, y) = (x.clamp(0.0, max), y.clamp(0.0, max));
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(span - 1), (y0 + 1).min(span - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |cx: usize, cy: usize| h[cy * span + cx];
    let south = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
    let north = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
    south * (1.0 - fy) + north * fy
}

/// The normal map of a block from its `span * span` height samples (`NaN` is no data), `size` pixels square, for a
/// block `block_units` wide. Red is the x component, green y, blue z, each mapped from -1..1 to 0..255; flat ground
/// is (128, 128, 255). Alpha is opaque.
///
/// # Panics
/// Never for a power-of-two `size` in range; the image constructor only fails for sizes the DDS crate refuses.
#[must_use]
pub fn normal_image(samples: &[f32], span: usize, block_units: f32, size: u32) -> Image {
    let h = fill_gaps(samples);
    let n = size as usize;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for r in 0..n {
        for c in 0..n {
            let x = block_units * (1.0 - (c as f32 + 0.5) / n as f32);
            let y = block_units * ((r as f32 + 0.5) / n as f32);
            let (sx, sy) = (x / VERTEX_SPACING, y / VERTEX_SPACING);
            let slope_x = (height_at(&h, span, sx + 1.0, sy) - height_at(&h, span, sx - 1.0, sy))
                / (2.0 * VERTEX_SPACING);
            let slope_y = (height_at(&h, span, sx, sy + 1.0) - height_at(&h, span, sx, sy - 1.0))
                / (2.0 * VERTEX_SPACING);
            let len = (slope_x * slope_x + slope_y * slope_y + 1.0).sqrt();
            let enc = |v: f32| ((v / len) * 127.5 + 127.5).round().clamp(0.0, 255.0) as u8;
            rgba.extend_from_slice(&[enc(-slope_x), enc(-slope_y), enc(1.0), 255]);
        }
    }
    Image::new(size, size, rgba).expect("a square power-of-two image")
}

fn vertex_color(world: &Worldspace, cell: (i32, i32), vx: f32, vy: f32) -> Option<[u8; 3]> {
    let grid = world
        .layers(cell.0, cell.1)
        .and_then(|l| l.colors.as_ref())?;
    let max = (SIDE - 1) as f32;
    let (vx, vy) = (vx.clamp(0.0, max), vy.clamp(0.0, max));
    let (x0, y0) = (vx.floor() as usize, vy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(SIDE - 1), (y0 + 1).min(SIDE - 1));
    let (fx, fy) = (vx - x0 as f32, vy - y0 as f32);
    let mut out = [0_u8; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let at = |x: usize, y: usize| f32::from(grid.get(x, y)[k]);
        let south = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
        let north = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
        *o = (south * (1.0 - fy) + north * fy).round().clamp(0.0, 255.0) as u8;
    }
    Some(out)
}

/// The ground colour at vertex position (`vx`, `vy`) of `cell`: the base land texture of the quadrant, the
/// additional layers blended over it, times the vertex colour.
fn ground_color(
    world: &Worldspace,
    cell: (i32, i32),
    vx: f32,
    vy: f32,
    tiles: &LandTiles,
) -> [u8; 3] {
    let tint = vertex_color(world, cell, vx, vy);
    let Some(layers) = world.layers(cell.0, cell.1) else {
        return tint.unwrap_or(FALLBACK);
    };
    let max = (SIDE - 1) as f32 - 1e-3;
    let (vx, vy) = (vx.clamp(0.0, max), vy.clamp(0.0, max));
    let quadrant = u8::from(vx >= 16.0) | (u8::from(vy >= 16.0) << 1);
    let (ox, oy) = quadrant_origin(quadrant);
    let (qx, qy) = (vx - ox as f32, vy - oy as f32);
    let (u, v) = (qx / 16.0, qy / 16.0);
    let base = layers.base[usize::from(quadrant)]
        .and_then(|id| tiles.get(id))
        .map_or(FALLBACK, |t| t.sample(u, v));
    let mut over = Vec::new();
    for layer in layers.alpha.iter().filter(|l| l.quadrant == quadrant) {
        let Some(tile) = tiles.get(layer.texture) else {
            continue;
        };
        let (c0, r0) = (qx.floor() as usize, qy.floor() as usize);
        let (c1, r1) = ((c0 + 1).min(16), (r0 + 1).min(16));
        let (fx, fy) = (qx - c0 as f32, qy - r0 as f32);
        let o = |c: usize, r: usize| layer.point(c, r);
        let opacity = (o(c0, r0) * (1.0 - fx) + o(c1, r0) * fx) * (1.0 - fy)
            + (o(c0, r1) * (1.0 - fx) + o(c1, r1) * fx) * fy;
        over.push((tile.sample(u, v), opacity));
    }
    let ground = blend(base, &over);
    tint.map_or(ground, |t| {
        [0, 1, 2].map(|k| ((u16::from(ground[k]) * u16::from(t[k])) / 255) as u8)
    })
}

/// The diffuse map of block `id` of `world`, `size` pixels square: the land textures of the LAND records the pixel
/// falls in, blended by their layers and tinted by the vertex colours, and a neutral earth colour where there are
/// none. Alpha is opaque.
///
/// # Panics
/// Never for a size the DDS crate accepts.
#[must_use]
pub fn diffuse_image(world: &Worldspace, id: BlockId, size: u32, tiles: &LandTiles) -> Image {
    let n = size as usize;
    let units = id.level as f32 * CELL_UNITS;
    let mut rgba = Vec::with_capacity(n * n * 4);
    for r in 0..n {
        for c in 0..n {
            let x = units * (1.0 - (c as f32 + 0.5) / n as f32);
            let y = units * ((r as f32 + 0.5) / n as f32);
            let cell = (
                id.x + (x / CELL_UNITS).floor() as i32,
                id.y + (y / CELL_UNITS).floor() as i32,
            );
            let local = (
                (x % CELL_UNITS) / VERTEX_SPACING,
                (y % CELL_UNITS) / VERTEX_SPACING,
            );
            let [red, green, blue] = ground_color(world, cell, local.0, local.1, tiles);
            rgba.extend_from_slice(&[red, green, blue, 255]);
        }
    }
    Image::new(size, size, rgba).expect("a square power-of-two image")
}

#[cfg(test)]
mod tests {
    use super::{FALLBACK, diffuse_image, normal_image};
    use crate::diffuse::LandTiles;
    use lodwright_core::land::CellHeights;
    use lodwright_core::layout::BlockId;
    use lodwright_core::worldspace::Worldspace;

    fn world() -> Worldspace {
        Worldspace::from_cells(1, "W", [((0, 0), CellHeights::flat(10.0))])
    }

    #[test]
    fn flat_ground_gives_the_flat_normal() {
        let samples = vec![50.0; 9 * 9];
        let img = normal_image(&samples, 9, 1024.0, 4);
        for p in img.rgba().chunks(4) {
            assert_eq!(p, [128, 128, 255, 255]);
        }
    }

    #[test]
    fn a_constant_slope_gives_one_constant_normal() {
        // height rises with x: 2 units per sample (128 units)
        let span = 9;
        let samples: Vec<f32> = (0..span * span).map(|i| (i % span) as f32 * 2.0).collect();
        let img = normal_image(&samples, span, 1024.0, 8);
        let first = img.pixel(0, 0).unwrap();
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(img.pixel(x, y).unwrap(), first);
            }
        }
        assert_ne!(first[0], 128, "the slope tilts the normal in x");
        assert_eq!(first[1], 128, "and not in y");
    }

    #[test]
    fn gaps_do_not_poison_the_map() {
        let mut samples = vec![0.0; 9 * 9];
        samples[40] = f32::NAN;
        let img = normal_image(&samples, 9, 1024.0, 4);
        assert!(img.rgba().chunks(4).all(|p| p[2] > 200));
    }

    #[test]
    fn the_diffuse_falls_back_to_earth_without_vertex_colours() {
        let id = BlockId {
            level: 4,
            x: 0,
            y: 0,
        };
        let img = diffuse_image(&world(), id, 4, &LandTiles::empty());
        assert_eq!(
            img.pixel(1, 1).unwrap(),
            [FALLBACK[0], FALLBACK[1], FALLBACK[2], 255]
        );
    }
}
