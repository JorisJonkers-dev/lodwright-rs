// SPDX-License-Identifier: GPL-3.0-only
//! Per-block terrain images: normal maps from heights and diffuse maps from vertex colours.
//!
//! Orientation follows the block mesh's UVs (row `game-lod-terrain-block-nif`): image column `c` of `n` is at
//! block-local x = `size * (1 - (c + 0.5) / n)`, image row `r` at y = `size * (r + 0.5) / n`.
use lodwright_core::land::SIDE;
use lodwright_core::layers::{CellLayers, quadrant_origin};
use lodwright_core::layout::BlockId;
use lodwright_core::terrain::VERTEX_SPACING;
use lodwright_core::worldspace::Worldspace;
use lodwright_dds::Image;

use crate::diffuse::{LandTiles, Tile, blend};

const CELL_UNITS: f32 = 4096.0;
/// Colour of ground with no vertex colours (a neutral dry earth).
const FALLBACK: [u8; 3] = [96, 88, 72];
/// Colour of a cell without a LAND record in the game's own diffuse tiles (measured over 18 worlds).
const ABSENT_CELL: [u8; 3] = [125, 126, 125];

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
    normal_image_with_alpha(samples, span, block_units, size, None)
}

/// [`normal_image`] with `alpha`, one byte per pixel in the pixel order of the map, in place of the opaque alpha.
///
/// # Panics
/// Never for a power-of-two `size` in range; an `alpha` shorter than the map reads as opaque past its end.
#[must_use]
pub fn normal_image_with_alpha(
    samples: &[f32],
    span: usize,
    block_units: f32,
    size: u32,
    alpha: Option<&[u8]>,
) -> Image {
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
            let a = alpha.and_then(|a| a.get(r * n + c)).copied().unwrap_or(255);
            rgba.extend_from_slice(&[enc(-slope_x), enc(-slope_y), enc(1.0), a]);
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

/// The land textures at vertex position (`vx`, `vy`) of a cell's layers, as the diffuse blend and the specular alpha
/// both read them: the base texture of the quadrant, the additional layers over it with their interpolated opacity
/// (in layer order), and the position in the quadrant (0..1).
struct Stack<'a> {
    base: Option<&'a Tile>,
    over: Vec<(&'a Tile, f32)>,
    u: f32,
    v: f32,
}

fn stack<'a>(layers: &CellLayers, vx: f32, vy: f32, tiles: &'a LandTiles) -> Stack<'a> {
    let max = (SIDE - 1) as f32 - 1e-3;
    let (vx, vy) = (vx.clamp(0.0, max), vy.clamp(0.0, max));
    let quadrant = u8::from(vx >= 16.0) | (u8::from(vy >= 16.0) << 1);
    let (ox, oy) = quadrant_origin(quadrant);
    let (qx, qy) = (vx - ox as f32, vy - oy as f32);
    // source: game-default-land-texture
    // a quadrant without a base texture shows the default land texture
    let base = match layers.base[usize::from(quadrant)] {
        Some(id) => tiles.get(id),
        None => tiles.default_tile(),
    };
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
        over.push((tile, opacity));
    }
    Stack {
        base,
        over,
        u: qx / 16.0,
        v: qy / 16.0,
    }
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
        // source: game-land-fill
        // a cell with a CELL record and no LAND is the game's flat default land colour; a cell with no record at
        // all is a flat grey
        if !world.has_cell_record(cell.0, cell.1) {
            return ABSENT_CELL;
        }
        return tiles.default_tile().map_or(FALLBACK, Tile::mean_color);
    };
    let s = stack(layers, vx, vy, tiles);
    let base = s.base.map_or(FALLBACK, |t| t.sample(s.u, s.v));
    let over: Vec<([u8; 3], f32)> = s
        .over
        .iter()
        .map(|(tile, opacity)| (tile.sample(s.u, s.v), *opacity))
        .collect();
    let ground = blend(base, &over);
    tint.map_or(ground, |t| {
        [0, 1, 2].map(|k| ((u16::from(ground[k]) * u16::from(t[k])) / 255) as u8)
    })
}

/// The specular exponent at a position, scaled to 0..255 over `range`: the base texture's, the additional layers'
/// blended over it by the same opacities as the diffuse. A texture without an exponent, and ground without a
/// texture, count as the lowest value. A range of one value gives 255 everywhere.
fn ground_specular(
    world: &Worldspace,
    cell: (i32, i32),
    vx: f32,
    vy: f32,
    tiles: &LandTiles,
    range: (u8, u8),
) -> u8 {
    let scale = |tile: Option<&Tile>| {
        let Some(e) = tile.and_then(Tile::specular) else {
            return 0.0;
        };
        if range.1 == range.0 {
            return 255.0;
        }
        f32::from(e.saturating_sub(range.0)) / f32::from(range.1 - range.0) * 255.0
    };
    let Some(layers) = world.layers(cell.0, cell.1) else {
        return 0;
    };
    let s = stack(layers, vx, vy, tiles);
    let mut value = scale(s.base);
    for (tile, opacity) in &s.over {
        let t = opacity.clamp(0.0, 1.0);
        value = value * (1.0 - t) + scale(Some(tile)) * t;
    }
    value.round().clamp(0.0, 255.0) as u8
}

/// The alpha channel of the normal map of block `id`, `size * size` bytes in the pixel order of the maps: the
/// specular of the land textures under each pixel (see [`ground_specular`]), over the exponent range of the load
/// order. Opaque everywhere when no land texture of the load order has an exponent.
#[must_use]
pub fn specular_alpha(world: &Worldspace, id: BlockId, size: u32, tiles: &LandTiles) -> Vec<u8> {
    let n = size as usize;
    let Some(range) = tiles.specular_range() else {
        return vec![255; n * n];
    };
    let units = id.level as f32 * CELL_UNITS;
    let mut alpha = Vec::with_capacity(n * n);
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
            alpha.push(ground_specular(world, cell, local.0, local.1, tiles, range));
        }
    }
    alpha
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
    use crate::diffuse::{LandTiles, Tile};
    use lodwright_core::land::CellHeights;
    use lodwright_core::layout::BlockId;
    use lodwright_core::worldspace::Worldspace;

    fn world() -> Worldspace {
        // the four by four cells of block (0, 0) at level 4, so every pixel of the block lies in a cell
        Worldspace::from_cells(
            1,
            "W",
            (0..4).flat_map(|y| (0..4).map(move |x| ((x, y), CellHeights::flat(10.0)))),
        )
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
    fn a_cell_without_layers_shows_the_default_land_texture_not_the_flat_earth() {
        let id = BlockId {
            level: 4,
            x: 0,
            y: 0,
        };
        let tiles = LandTiles::empty().with_default_tile(Tile::flat([10, 20, 30]));
        let img = diffuse_image(&world(), id, 4, &tiles);
        assert_eq!(img.pixel(1, 1).unwrap(), [10, 20, 30, 255]);
    }

    #[test]
    fn a_cell_with_no_record_at_all_is_the_games_flat_grey() {
        let id = BlockId {
            level: 4,
            x: 0,
            y: 0,
        };
        let tiles = LandTiles::empty().with_default_tile(Tile::flat([10, 20, 30]));
        // the world has no cell at (4..8, 0..4), so a block there is all cells without a record
        let img = diffuse_image(&world(), BlockId { x: 40, ..id }, 4, &tiles);
        assert_eq!(img.pixel(1, 1).unwrap(), [125, 126, 125, 255]);
    }

    #[test]
    fn the_strip_world_is_filled_with_its_own_brown() {
        let id = BlockId {
            level: 4,
            x: 0,
            y: 0,
        };
        let tiles = LandTiles::empty()
            .with_default_tile(Tile::flat([10, 20, 30]))
            .with_world_fill("TheStripWorldNew");
        let img = diffuse_image(&world(), id, 4, &tiles);
        assert_eq!(img.pixel(1, 1).unwrap(), [67, 61, 49, 255]);
        let other = LandTiles::empty()
            .with_default_tile(Tile::flat([10, 20, 30]))
            .with_world_fill("WastelandNV");
        assert_eq!(
            diffuse_image(&world(), id, 4, &other).pixel(1, 1).unwrap(),
            [10, 20, 30, 255]
        );
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
