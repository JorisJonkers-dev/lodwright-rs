// SPDX-License-Identifier: GPL-3.0-only
//! Land texture colours for the terrain diffuse maps.
//!
//! Every land texture is reduced to a small tile once; the diffuse of a block samples the tile of the base texture
//! of each quadrant, blends the additional layers over it by their opacity points, and multiplies the vertex
//! colour. A texture that cannot be found is left out with a warning and the ground keeps the colour below it.

use std::collections::{BTreeSet, HashMap};

use lodwright_core::texture::TextureResolver;
use lodwright_core::worldspace::Worldspace;
use lodwright_dds::{Filter, resize};

use crate::assets::Assets;

/// Edge of a reduced tile in pixels.
const TILE: u32 = 32;

/// A land texture reduced to `TILE` by `TILE` colours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    rgb: Vec<[u8; 3]>,
}

impl Tile {
    /// The pixels, row-major.
    #[must_use]
    pub fn pixels(&self) -> &[[u8; 3]] {
        &self.rgb
    }

    /// A tile of one colour.
    #[must_use]
    pub fn flat(rgb: [u8; 3]) -> Self {
        Self {
            rgb: vec![rgb; (TILE * TILE) as usize],
        }
    }

    /// The colour at (`u`, `v`) in 0..1, repeating, bilinear.
    #[must_use]
    pub fn sample(&self, u: f32, v: f32) -> [u8; 3] {
        let n = TILE as usize;
        let (fu, fv) = (
            u.rem_euclid(1.0) * n as f32 - 0.5,
            v.rem_euclid(1.0) * n as f32 - 0.5,
        );
        let (x0, y0) = (fu.floor(), fv.floor());
        let (tx, ty) = (fu - x0, fv - y0);
        let wrap = |c: f32| (c as i64).rem_euclid(n as i64) as usize;
        let (x0, y0) = (wrap(x0), wrap(y0));
        let (x1, y1) = ((x0 + 1) % n, (y0 + 1) % n);
        let mut out = [0_u8; 3];
        for (k, o) in out.iter_mut().enumerate() {
            let at = |x: usize, y: usize| f32::from(self.rgb[y * n + x][k]);
            let lo = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
            let hi = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
            *o = (lo * (1.0 - ty) + hi * ty).round().clamp(0.0, 255.0) as u8;
        }
        out
    }
}

/// The tiles of every land texture a world uses.
#[derive(Debug, Clone, Default)]
pub struct LandTiles {
    tiles: HashMap<u32, Tile>,
}

impl LandTiles {
    /// No textures: the ground falls back to vertex colours and a neutral earth.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// The tile of land texture `id` (absolute `FormID`).
    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Tile> {
        self.tiles.get(&id)
    }

    /// Load and reduce every land texture `world` uses. Warnings name what could not be read.
    pub fn load(
        world: &Worldspace,
        resolver: &TextureResolver,
        assets: &dyn Assets,
        warnings: &mut Vec<String>,
    ) -> Self {
        let mut ids = BTreeSet::new();
        for ((x, y), _) in world.cells() {
            if let Some(layers) = world.layers(x, y) {
                ids.extend(layers.base.iter().flatten().copied());
                ids.extend(layers.alpha.iter().map(|a| a.texture));
            }
        }
        let mut tiles = HashMap::new();
        for id in ids {
            let Some(path) = resolver.diffuse_path(id) else {
                warnings.push(format!("land texture {id:08X} names no texture"));
                continue;
            };
            let key = format!("textures/{}", path.replace('\\', "/"));
            let image = assets
                .read(&key)
                .and_then(|b| lodwright_dds::decode(&b).ok())
                .and_then(|d| d.levels.into_iter().next());
            let Some(image) = image else {
                warnings.push(format!("land texture {key} not found"));
                continue;
            };
            if let Ok(small) = resize(&image, TILE, TILE, Filter::Box) {
                let rgb = small
                    .rgba()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| [p[0], p[1], p[2]])
                    .collect();
                tiles.insert(id, Tile { rgb });
            }
        }
        Self { tiles }
    }
}

/// Blend `layers` (colour and opacity, in layer order) over `base`.
#[must_use]
pub fn blend(base: [u8; 3], layers: &[([u8; 3], f32)]) -> [u8; 3] {
    let mut c = base.map(f32::from);
    for (color, opacity) in layers {
        let t = opacity.clamp(0.0, 1.0);
        for k in 0..3 {
            c[k] = c[k] * (1.0 - t) + f32::from(color[k]) * t;
        }
    }
    c.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::{Tile, blend};

    #[test]
    fn layers_blend_over_the_base_by_opacity_in_order() {
        assert_eq!(blend([0, 0, 0], &[]), [0, 0, 0]);
        assert_eq!(blend([0, 0, 0], &[([200, 100, 50], 0.5)]), [100, 50, 25]);
        assert_eq!(
            blend([0, 0, 0], &[([100, 100, 100], 1.0), ([200, 200, 200], 0.5)]),
            [150, 150, 150]
        );
        assert_eq!(
            blend([10, 10, 10], &[([250, 250, 250], 7.0)]),
            [250, 250, 250]
        );
    }

    #[test]
    fn a_flat_tile_samples_to_its_colour_everywhere_and_wraps() {
        let t = Tile::flat([10, 20, 30]);
        for (u, v) in [(0.0, 0.0), (0.37, 0.91), (1.5, -0.25), (123.4, 0.0)] {
            assert_eq!(t.sample(u, v), [10, 20, 30]);
        }
    }
}
