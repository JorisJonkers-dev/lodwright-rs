// SPDX-License-Identifier: GPL-3.0-only
//! Landscape textures: `LTEX` records and the `TXST` texture sets they point to, with load-order overrides, and a
//! resolver from an `LTEX` `FormID` to a texture path.
//!
//! Derived from: xEdit's FNV definitions and the UESP LTEX page (`provenance/sources.toml`: `ltex-texture-set`,
//! `txst-texture-paths`, `plugin-record-override`, `plugin-formid-masters`).

use std::collections::HashMap;

use crate::Error;
use crate::plugin::Plugin;

/// A `LTEX` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandTexture {
    /// `FormID` of the record.
    pub form_id: u32,
    /// Editor id.
    pub editor_id: Option<String>,
    /// The `ICON` texture path, as stored.
    pub icon: Option<String>,
    /// The `TNAM` texture-set `FormID`.
    pub texture_set: Option<u32>,
    /// The `SNAM` specular exponent (its first byte), when the record has one.
    pub specular_exponent: Option<u8>,
}

impl LandTexture {
    /// A record with no subrecords.
    #[must_use]
    pub fn new(form_id: u32) -> Self {
        Self {
            form_id,
            editor_id: None,
            icon: None,
            texture_set: None,
            specular_exponent: None,
        }
    }
}

/// A `TXST` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureSet {
    /// `FormID` of the record.
    pub form_id: u32,
    /// Editor id.
    pub editor_id: Option<String>,
    /// `TX00`, the base image, as stored.
    pub diffuse: Option<String>,
    /// `TX01`, the normal map, as stored.
    pub normal: Option<String>,
}

impl TextureSet {
    /// A record with no subrecords.
    #[must_use]
    pub fn new(form_id: u32) -> Self {
        Self {
            form_id,
            editor_id: None,
            diffuse: None,
            normal: None,
        }
    }
}

/// The textures of a load order after overrides: a `FormID` to texture path resolver.
#[derive(Debug, Clone, Default)]
pub struct TextureResolver {
    land: HashMap<u32, LandTexture>,
    sets: HashMap<u32, TextureSet>,
}

impl TextureResolver {
    /// Resolve the textures of a load order (plugins in load order). A later plugin's `LTEX` or `TXST` replaces
    /// an earlier one with the same absolute `FormID` as a whole; the `TNAM` of the winning record is made
    /// absolute through its own plugin's masters.
    ///
    /// # Errors
    /// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
    pub fn from_load_order(plugins: &[(String, Plugin)]) -> Result<Self, Error> {
        let mut resolver = Self::default();
        for (index, (_, plugin)) in plugins.iter().enumerate() {
            let table = crate::worldspace::master_table(plugins, plugin)?;
            let own = u32::try_from(index).map_err(|_| Error::Malformed("too many plugins"))?;
            // source: plugin-record-override
            for texture in &plugin.land_textures {
                let id = crate::worldspace::absolute(texture.form_id, &table, own);
                let mut texture = texture.clone();
                texture.form_id = id;
                texture.texture_set = texture
                    .texture_set
                    .filter(|set| *set != 0)
                    .map(|set| crate::worldspace::absolute(set, &table, own));
                resolver.land.insert(id, texture);
            }
            for set in &plugin.texture_sets {
                let id = crate::worldspace::absolute(set.form_id, &table, own);
                let mut set = set.clone();
                set.form_id = id;
                resolver.sets.insert(id, set);
            }
        }
        Ok(resolver)
    }

    /// The winning `LTEX` record for an absolute `FormID`.
    #[must_use]
    pub fn land_texture(&self, form_id: u32) -> Option<&LandTexture> {
        self.land.get(&form_id)
    }

    /// The winning `TXST` record for an absolute `FormID`.
    #[must_use]
    pub fn texture_set(&self, form_id: u32) -> Option<&TextureSet> {
        self.sets.get(&form_id)
    }

    /// The diffuse texture path of an `LTEX` (absolute `FormID`): the `TX00` of its texture set, else its
    /// `ICON`; `None` when neither names a path. The path is as stored in the plugin.
    #[must_use]
    pub fn diffuse_path(&self, form_id: u32) -> Option<&str> {
        // source: ltex-texture-set
        let texture = self.land.get(&form_id)?;
        texture
            .texture_set
            .and_then(|set| self.sets.get(&set))
            .and_then(|set| set.diffuse.as_deref())
            .or(texture.icon.as_deref())
            .filter(|path| !path.is_empty())
    }

    /// The `SNAM` specular exponent of an `LTEX`, when the winning record has one.
    #[must_use]
    pub fn specular_exponent(&self, form_id: u32) -> Option<u8> {
        // source: ltex-texture-set
        self.land.get(&form_id)?.specular_exponent
    }

    /// The lowest and highest specular exponent of any `LTEX` of the load order; `None` when none has one.
    #[must_use]
    pub fn specular_range(&self) -> Option<(u8, u8)> {
        let mut all = self.land.values().filter_map(|t| t.specular_exponent);
        let first = all.next()?;
        Some(all.fold((first, first), |(lo, hi), e| (lo.min(e), hi.max(e))))
    }

    /// The normal-map path of an `LTEX`: the `TX01` of its texture set.
    #[must_use]
    pub fn normal_path(&self, form_id: u32) -> Option<&str> {
        let texture = self.land.get(&form_id)?;
        texture
            .texture_set
            .and_then(|set| self.sets.get(&set))
            .and_then(|set| set.normal.as_deref())
            .filter(|path| !path.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::TextureResolver;
    use crate::Error;
    use crate::plugin::{Plugin, read};
    use lodwright_testkit::plugin::PluginBuilder;
    use lodwright_testkit::world::{ltex_record, record_group, txst_record};
    use lodwright_testkit::{covers, ids};

    fn load(items: &[(&str, Vec<u8>)]) -> Vec<(String, Plugin)> {
        items
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), read(bytes).unwrap()))
            .collect()
    }

    fn base_plugin() -> Vec<u8> {
        PluginBuilder::new()
            .group(record_group(
                *b"TXST",
                vec![
                    txst_record(
                        0x0A00,
                        "DirtSet",
                        Some("landscape\\dirt.dds"),
                        Some("landscape\\dirt_n.dds"),
                    ),
                    txst_record(0x0A01, "NoPathSet", None, None),
                ],
            ))
            .group(record_group(
                *b"LTEX",
                vec![
                    ltex_record(0x0B00, "Dirt", Some("old\\dirt.dds"), Some(0x0A00)),
                    ltex_record(0x0B01, "IconOnly", Some("old\\rock.dds"), None),
                    ltex_record(0x0B02, "Empty", None, Some(0x0A01)),
                    ltex_record(0x0B03, "DanglingSet", None, Some(0x0FFF)),
                ],
            ))
            .build()
    }

    #[test]
    fn a_texture_resolves_through_its_texture_set() {
        covers(&[ids::terrain::textures::TEXTURE_RESOLUTION]);
        let resolver =
            TextureResolver::from_load_order(&load(&[("Base.esm", base_plugin())])).unwrap();
        assert_eq!(resolver.diffuse_path(0x0B00), Some("landscape\\dirt.dds"));
        assert_eq!(resolver.normal_path(0x0B00), Some("landscape\\dirt_n.dds"));
        assert_eq!(
            resolver.land_texture(0x0B00).unwrap().editor_id.as_deref(),
            Some("Dirt")
        );
        assert_eq!(
            resolver.texture_set(0x0A00).unwrap().editor_id.as_deref(),
            Some("DirtSet")
        );
    }

    #[test]
    fn the_specular_exponent_is_read_and_the_range_spans_the_load_order() {
        covers(&[ids::terrain::textures::SPECULAR_ALPHA]);
        let bytes = PluginBuilder::new()
            .group(record_group(
                *b"LTEX",
                vec![
                    ltex_record(0x0B00, "Dull", None, None).with_subrecord(*b"SNAM", vec![10]),
                    ltex_record(0x0B01, "Shiny", None, None).with_subrecord(*b"SNAM", vec![200]),
                    ltex_record(0x0B02, "None", None, None),
                ],
            ))
            .build();
        let resolver = TextureResolver::from_load_order(&load(&[("Base.esm", bytes)])).unwrap();
        assert_eq!(resolver.specular_exponent(0x0B00), Some(10));
        assert_eq!(resolver.specular_exponent(0x0B01), Some(200));
        assert_eq!(resolver.specular_exponent(0x0B02), None);
        assert_eq!(resolver.specular_range(), Some((10, 200)));
        let none = TextureResolver::from_load_order(&load(&[("Base.esm", base_plugin())])).unwrap();
        assert_eq!(none.specular_range(), None);
    }

    #[test]
    fn a_texture_without_a_set_falls_back_to_its_icon() {
        covers(&[ids::terrain::textures::TEXTURE_RESOLUTION]);
        let resolver =
            TextureResolver::from_load_order(&load(&[("Base.esm", base_plugin())])).unwrap();
        assert_eq!(resolver.diffuse_path(0x0B01), Some("old\\rock.dds"));
        assert_eq!(resolver.normal_path(0x0B01), None);
    }

    #[test]
    fn a_texture_that_names_no_path_resolves_to_nothing() {
        let resolver =
            TextureResolver::from_load_order(&load(&[("Base.esm", base_plugin())])).unwrap();
        assert_eq!(resolver.diffuse_path(0x0B02), None);
        assert_eq!(resolver.diffuse_path(0x0B03), None);
        assert_eq!(resolver.diffuse_path(0x9999), None);
    }

    #[test]
    fn a_later_plugin_overriding_a_texture_changes_the_resolved_path() {
        covers(&[ids::terrain::textures::TEXTURE_RESOLUTION]);
        let patch = PluginBuilder::new()
            .master("Base.esm")
            .group(record_group(
                *b"TXST",
                vec![txst_record(
                    0x0000_0A00,
                    "DirtSet",
                    Some("patch\\dirt.dds"),
                    None,
                )],
            ))
            .build();
        let resolver = TextureResolver::from_load_order(&load(&[
            ("Base.esm", base_plugin()),
            ("Patch.esp", patch),
        ]))
        .unwrap();
        // the TXST of the patch replaces the base one as a whole: new path, and the normal map is gone
        assert_eq!(resolver.diffuse_path(0x0B00), Some("patch\\dirt.dds"));
        assert_eq!(resolver.normal_path(0x0B00), None);
    }

    #[test]
    fn a_later_ltex_pointing_at_another_set_changes_the_resolved_path() {
        covers(&[ids::terrain::textures::TEXTURE_RESOLUTION]);
        let patch = PluginBuilder::new()
            .master("Base.esm")
            .group(record_group(
                *b"TXST",
                vec![txst_record(
                    0x0100_0C00,
                    "NewSet",
                    Some("mod\\grass.dds"),
                    None,
                )],
            ))
            .group(record_group(
                *b"LTEX",
                vec![ltex_record(0x0000_0B00, "Dirt", None, Some(0x0100_0C00))],
            ))
            .build();
        let resolver = TextureResolver::from_load_order(&load(&[
            ("Base.esm", base_plugin()),
            ("Patch.esp", patch),
        ]))
        .unwrap();
        assert_eq!(resolver.diffuse_path(0x0B00), Some("mod\\grass.dds"));
        // the patch's own FormID (high byte 1 = itself) becomes absolute index 1
        assert_eq!(
            resolver.land_texture(0x0B00).unwrap().texture_set,
            Some(0x0100_0C00)
        );
        assert_eq!(resolver.diffuse_path(0x0B01), Some("old\\rock.dds"));
    }

    #[test]
    fn a_missing_master_is_an_error() {
        let patch = PluginBuilder::new().master("Absent.esm").build();
        assert_eq!(
            TextureResolver::from_load_order(&load(&[("Patch.esp", patch)])).unwrap_err(),
            Error::MissingMaster("Absent.esm".into())
        );
    }
}
