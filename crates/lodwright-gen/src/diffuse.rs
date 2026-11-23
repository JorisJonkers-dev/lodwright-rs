// SPDX-License-Identifier: GPL-3.0-only
//! Land texture colours for the terrain diffuse maps.
//!
//! Every land texture is reduced to a small tile once; the diffuse of a block samples the tile of the base texture
//! of each quadrant, blends the additional layers over it by their opacity points, and multiplies the vertex
//! colour. A texture that cannot be found is left out with a warning and the ground keeps the colour below it.

use std::collections::{BTreeSet, HashMap};

use lodwright_core::texture::TextureResolver;
use lodwright_core::worldspace::Worldspace;
use lodwright_dds::{Filter, Image, resize};

use crate::assets::{Assets, TextureFault, read_texture, texture_warning};
use crate::par::par_map;

/// Edge of a reduced tile in pixels.
const TILE: u32 = 32;

/// A land texture reduced to `TILE` by `TILE` colours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    rgb: Vec<[u8; 3]>,
    /// How often the texture repeats across a cell quadrant: 1 unless the request sets another.
    scale: Scale,
    /// The specular exponent of the land texture (`SNAM`), when its record has one.
    specular: Option<u8>,
    /// The mean up component of the texture's normal map (1 when it has none): near land is lit through the normal
    /// map, far land is not, so the far land of this texture is lit `1 / up` too bright (`LandTiles::lit`).
    up: Scale,
}

/// A tiling scale, kept as its bits so that a tile stays `Eq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Scale(u32);

impl Scale {
    const ONE: Self = Self(0x3F80_0000);

    fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

impl Tile {
    /// The mean colour of the tile.
    #[must_use]
    pub fn mean_color(&self) -> [u8; 3] {
        let n = self.rgb.len().max(1) as f32;
        let mut sum = [0.0_f32; 3];
        for p in &self.rgb {
            for k in 0..3 {
                sum[k] += f32::from(p[k]);
            }
        }
        sum.map(|v| (v / n).round() as u8)
    }

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
            scale: Scale::ONE,
            specular: None,
            up: Scale::ONE,
        }
    }

    /// How often the texture repeats across a cell quadrant (1 by default).
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale.get()
    }

    /// The same tile repeating `scale` times across a cell quadrant.
    #[must_use]
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = Scale(scale.to_bits());
        self
    }

    /// The same tile with the specular exponent of its land texture.
    #[must_use]
    pub fn with_specular(mut self, exponent: Option<u8>) -> Self {
        self.specular = exponent;
        self
    }

    /// The specular exponent of the land texture, when its record has one.
    #[must_use]
    pub fn specular(&self) -> Option<u8> {
        self.specular
    }

    /// The colour at (`u`, `v`) in 0..1 of a cell quadrant, repeating [`Tile::scale`] times, bilinear.
    #[must_use]
    pub fn sample(&self, u: f32, v: f32) -> [u8; 3] {
        let n = TILE as usize;
        let scale = self.scale.get();
        let (u, v) = (u * scale, v * scale);
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
    /// Lowest and highest specular exponent of the load order's land textures; `None` when no record has one.
    specular_range: Option<(u8, u8)>,
    /// The default land texture, which a quadrant without a base texture shows.
    default: Option<Tile>,
}

/// The default land texture the game uses when its ini does not say (`SDefaultLandDiffuseTexture` of the game's
/// `Fallout_default.ini`); a name without a folder is in `textures\landscape`.
pub const DEFAULT_LAND_TEXTURE: &str = "DirtWasteland01.dds";

/// The value of `SDefaultLandDiffuseTexture` in the `[Landscape]` section of an ini file's text, when it has one.
#[must_use]
pub fn default_land_texture_from_ini(text: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_section = name.eq_ignore_ascii_case("Landscape");
            continue;
        }
        let value = line
            .split_once('=')
            .filter(|(key, _)| {
                in_section
                    && key
                        .trim()
                        .eq_ignore_ascii_case("SDefaultLandDiffuseTexture")
            })
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty());
        if let Some(value) = value {
            return Some(value.to_owned());
        }
    }
    None
}

impl LandTiles {
    /// No textures: the ground falls back to vertex colours and a neutral earth.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Tiles given directly (for tests).
    #[cfg(test)]
    pub(crate) fn from_tiles(tiles: HashMap<u32, Tile>) -> Self {
        Self {
            tiles,
            specular_range: None,
            default: None,
        }
    }

    /// The same tiles with a default land tile (for tests).
    #[cfg(test)]
    pub(crate) fn with_default_tile(mut self, tile: Tile) -> Self {
        self.default = Some(tile);
        self
    }

    /// The same tiles with the tiling scale of each land texture in `scales` (absolute `FormID` to repeats per
    /// cell quadrant); a texture that is not listed keeps 1.
    #[must_use]
    pub fn with_scales(mut self, scales: &std::collections::BTreeMap<u32, f32>) -> Self {
        for (id, scale) in scales {
            if let Some(tile) = self.tiles.remove(id) {
                self.tiles.insert(*id, tile.with_scale(*scale));
            }
        }
        self
    }

    /// The same tiles with the exponent range of the load order, which the specular alpha is scaled over (for tests).
    #[cfg(test)]
    pub(crate) fn with_specular_range(mut self, range: Option<(u8, u8)>) -> Self {
        self.specular_range = range;
        self
    }

    /// Lowest and highest specular exponent of the load order's land textures; `None` when no record has one.
    #[must_use]
    pub fn specular_range(&self) -> Option<(u8, u8)> {
        self.specular_range
    }

    /// The land textures that have a tile but no specular exponent, sorted. A run that writes specular alpha warns
    /// about them: their specular counts as the lowest value.
    #[must_use]
    pub fn without_specular(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .tiles
            .iter()
            .filter(|(_, t)| t.specular.is_none())
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// The tile of the default land texture, when it was loaded.
    #[must_use]
    pub fn default_tile(&self) -> Option<&Tile> {
        self.default.as_ref()
    }

    /// The same tiles with the default land texture `path` (relative to `textures`, as the ini states it) read
    /// through `assets`; a texture that cannot be read is named in `warnings` and leaves the neutral earth.
    #[must_use]
    pub fn with_default_texture(
        mut self,
        path: &str,
        assets: &dyn Assets,
        warnings: &mut Vec<String>,
    ) -> Self {
        // the ini names the file relative to textures\landscape
        let relative = if path.contains(['\\', '/']) {
            path.to_owned()
        } else {
            format!("landscape/{path}")
        };
        let key = format!("textures/{}", relative.replace('\\', "/"));
        let image = read_texture(assets, &key);
        let fault = image.as_ref().err().cloned();
        // the normal map of a land texture is named after it: `dirt.dds` has `dirt_n.dds`
        let up = key
            .strip_suffix(".dds")
            .map(|stem| format!("{stem}_n.dds"))
            .and_then(|normal| read_texture(assets, &normal).ok())
            .and_then(|normal| mean_up(&normal))
            .unwrap_or(1.0);
        match image.ok().and_then(|i| reduce(&i)) {
            Some(rgb) => {
                self.default = Some(Tile {
                    rgb,
                    scale: Scale::ONE,
                    specular: None,
                    up: Scale(up.to_bits()),
                });
            }
            None => warnings.push(texture_warning(
                "default land texture",
                &key,
                &fault.unwrap_or(TextureFault::Unreadable("no pixels".to_owned())),
                "",
            )),
        }
        self
    }

    /// The same tiles with a flat default land colour, which replaces the default land texture: the game's own
    /// tiles of a world fill the quadrants without a base texture with one colour.
    #[must_use]
    pub fn with_default_fill(mut self, rgb: [u8; 3]) -> Self {
        self.default = Some(Tile::flat(rgb));
        self
    }

    /// The same tiles with the default land colour the game's own tiles use for `world`, when it is one whose
    /// tiles are not the default land texture: `TheStripWorldNew` is filled with one brown (67, 61, 49).
    // source: game-land-fill
    #[must_use]
    pub fn with_world_fill(self, world: &str) -> Self {
        if world.eq_ignore_ascii_case("TheStripWorldNew") {
            self.with_default_fill([67, 61, 49])
        } else {
            self
        }
    }

    /// The same tiles as the near land shows them: each texture's colours times the mean up component of its normal
    /// map, the default texture's too (own design, `docs/spec/terrain-lod.md`). Near land is lit through the
    /// normal map and far land is not, so far land composed from the textures alone is `1 / up` too bright (measured
    /// about 14 per cent in both worlds). A baked tile cannot hold the sun and the ambient part apart, so the whole
    /// colour is scaled: exact where the sun dominates, a little dark in shade. A flat fill is not scaled.
    #[must_use]
    pub fn lit(mut self) -> Self {
        for tile in self.tiles.values_mut().chain(self.default.as_mut()) {
            let up = f32::from_bits(tile.up.0);
            for p in &mut tile.rgb {
                for c in p.iter_mut() {
                    *c = (f32::from(*c) * up).round().clamp(0.0, 255.0) as u8;
                }
            }
            tile.up = Scale::ONE;
        }
        self
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
        Self::load_with_threads(world, resolver, assets, warnings, 1)
    }

    /// [`LandTiles::load`] with the textures decoded on up to `threads` threads; the tiles and the warnings (in the
    /// order of the texture ids) are the same for any thread count.
    pub fn load_with_threads(
        world: &Worldspace,
        resolver: &TextureResolver,
        assets: &dyn Assets,
        warnings: &mut Vec<String>,
        threads: usize,
    ) -> Self {
        let mut ids = BTreeSet::new();
        for ((x, y), _) in world.cells() {
            if let Some(layers) = world.layers(x, y) {
                ids.extend(layers.base.iter().flatten().copied());
                ids.extend(layers.alpha.iter().map(|a| a.texture));
            }
        }
        let ids: Vec<u32> = ids.into_iter().collect();
        let made = par_map(&ids, threads, |id| load_tile(*id, resolver, assets));
        let mut tiles = HashMap::new();
        for (id, made) in ids.into_iter().zip(made) {
            match made {
                Ok(tile) => {
                    if let Some(tile) = tile {
                        tiles.insert(id, tile);
                    }
                }
                Err(warning) => warnings.push(warning),
            }
        }
        Self {
            tiles,
            specular_range: resolver.specular_range(),
            default: None,
        }
    }
}

/// The tile of land texture `id`: `Err` with the warning when it names no texture or cannot be read, `Ok(None)` when
/// the picture cannot be reduced.
fn load_tile(
    id: u32,
    resolver: &TextureResolver,
    assets: &dyn Assets,
) -> Result<Option<Tile>, String> {
    let Some(path) = resolver.diffuse_path(id) else {
        return Err(format!("land texture {id:08X} names no texture"));
    };
    let key = format!("textures/{}", path.replace('\\', "/"));
    let image = read_texture(assets, &key)
        .map_err(|fault| texture_warning("land texture", &key, &fault, ""))?;
    Ok(reduce(&image).map(|rgb| {
        let up = resolver
            .normal_path(id)
            .map(|path| format!("textures/{}", path.replace('\\', "/")))
            .and_then(|key| read_texture(assets, &key).ok())
            .and_then(|normal| mean_up(&normal))
            .unwrap_or(1.0);
        Tile {
            rgb,
            scale: Scale::ONE,
            specular: resolver.specular_exponent(id),
            up: Scale(up.to_bits()),
        }
    }))
}

/// The mean up component of a tangent-space normal map: the blue channel of each normalised vector, over all pixels.
/// `None` for a map that is flat black or empty.
fn mean_up(normal: &Image) -> Option<f32> {
    let (mut sum, mut n) = (0.0_f64, 0_u64);
    for p in normal.rgba().as_chunks::<4>().0 {
        let v = [
            f64::from(p[0]) / 127.5 - 1.0,
            f64::from(p[1]) / 127.5 - 1.0,
            f64::from(p[2]) / 127.5 - 1.0,
        ];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if len > 0.0 {
            sum += (v[2] / len).max(0.0);
            n += 1;
        }
    }
    (n > 0)
        .then(|| (sum / n as f64) as f32)
        .filter(|u| *u > 0.0)
}

/// A land texture image reduced to `TILE` by `TILE` colours.
fn reduce(image: &Image) -> Option<Vec<[u8; 3]>> {
    let small = resize(image, TILE, TILE, Filter::Box).ok()?;
    Some(
        small
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2]])
            .collect(),
    )
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
    #[test]
    fn the_default_land_texture_is_read_from_the_landscape_section_of_an_ini() {
        let ini = "[Display]\nSDefaultLandDiffuseTexture=Wrong.dds\n[Landscape]\nfLandFriction=2.5\nsdefaultlanddiffusetexture = Landscape\\Mud01.dds \n[Other]\n";
        assert_eq!(
            super::default_land_texture_from_ini(ini).as_deref(),
            Some("Landscape\\Mud01.dds")
        );
        assert_eq!(super::default_land_texture_from_ini("[Landscape]\n"), None);
    }

    use super::{LandTiles, Scale, TILE, Tile, blend, mean_up};
    use lodwright_dds::Image;
    use std::collections::{BTreeMap, HashMap};

    /// A tile whose red channel rises from west to east.
    fn ramp() -> Tile {
        let n = TILE as usize;
        let rgb = (0..n * n)
            .map(|i| [u8::try_from((i % n) * 8).unwrap(), 0, 0])
            .collect();
        Tile {
            rgb,
            scale: Scale::ONE,
            specular: None,
            up: Scale::ONE,
        }
    }

    #[test]
    fn lit_scales_each_texture_by_the_up_component_of_its_normal_map_and_leaves_flat_fills() {
        // a normal map whose vectors lean: (0.2, 0, 0.8) normalises to up 0.97
        let leaning = [153_u8, 128, 230, 255].repeat(16);
        let normal = Image::new(4, 4, leaning).unwrap();
        let up = mean_up(&normal).unwrap();
        assert!((up - 0.97).abs() < 0.005, "{up}");
        let tile = Tile {
            up: Scale(up.to_bits()),
            ..Tile::flat([100, 200, 50])
        };
        let tiles =
            LandTiles::from_tiles(HashMap::from([(1, tile)])).with_default_fill([67, 61, 49]);
        let lit = tiles.lit();
        let [r, g, b] = lit.get(1).unwrap().mean_color();
        assert!(
            (i32::from(r) - 97).abs() <= 1
                && (i32::from(g) - 194).abs() <= 1
                && (i32::from(b) - 49).abs() <= 1
        );
        assert_eq!(lit.default_tile().unwrap().mean_color(), [67, 61, 49]);
        // a flat black normal map has no up component to scale by
        assert_eq!(mean_up(&Image::new(2, 2, vec![0; 16]).unwrap()), None);
    }

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

    #[test]
    fn a_tile_with_scale_two_repeats_twice_as_often_across_a_quadrant() {
        let once = ramp();
        let twice = ramp().with_scale(2.0);
        for u in [0.1_f32, 0.2, 0.3, 0.45] {
            assert_eq!(twice.sample(u, 0.5), once.sample(u * 2.0, 0.5), "{u}");
            // the second repeat starts where the first did
            assert_eq!(twice.sample(u + 0.5, 0.5), twice.sample(u, 0.5), "{u}");
        }
        assert_ne!(twice.sample(0.25, 0.5), once.sample(0.25, 0.5));
    }

    #[test]
    fn scale_one_samples_exactly_as_before_and_unlisted_textures_keep_it() {
        let plain = ramp();
        let scaled = ramp().with_scale(1.0);
        for u in [0.0_f32, 0.13, 0.5, 0.99] {
            assert_eq!(plain.sample(u, 0.3), scaled.sample(u, 0.3));
        }
        let tiles = LandTiles {
            tiles: HashMap::from([(1, ramp()), (2, ramp())]),
            specular_range: None,
            default: None,
        }
        .with_scales(&BTreeMap::from([(2, 4.0), (9, 3.0)]));
        assert_eq!(tiles.get(1).unwrap().scale(), 1.0);
        assert_eq!(tiles.get(2).unwrap().scale(), 4.0);
        assert!(
            tiles.get(9).is_none(),
            "a scale for a texture the world does not use adds nothing"
        );
    }
}
