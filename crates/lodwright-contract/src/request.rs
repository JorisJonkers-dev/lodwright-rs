// SPDX-License-Identifier: GPL-3.0-only
//! Request v1: what a caller asks `generate --request` to do, as one document.
//!
//! The request carries everything that decides the output, so its digest (`request_digest`) says whether an
//! existing output folder is up to date: the same request over the same plugin contents gives the same files.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::sha256_digest;

/// The terrain levels a request may list (cells per block edge).
pub const TERRAIN_LEVELS: [u32; 4] = [4, 8, 16, 32];
/// The vertex cap of a terrain block mesh: lowest and highest accepted.
pub const MAX_VERTICES_RANGE: (u32, u32) = (3, 32767);
/// The edge of a terrain texture in pixels: lowest and highest accepted (a power of two).
pub const TEXTURE_SIZE_RANGE: (u32, u32) = (4, 4096);
/// How often a land texture may repeat across a cell quadrant: lowest and highest accepted tiling scale.
pub const TEXTURE_SCALE_RANGE: (f32, f32) = (0.0625, 16.0);
/// The lowest accepted terrain quality.
pub const QUALITY_MIN: f32 = 0.0;
/// The fewest worker threads a request may ask for.
pub const THREADS_MIN: u32 = 1;

fn yes() -> bool {
    true
}

/// Settings of one terrain level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelRequest {
    /// Cells per block edge: 4, 8, 16 or 32.
    pub level: u32,
    /// Quality: the allowed vertical error is twice this many world units.
    pub quality: f32,
    /// Vertex cap per block mesh, 3 to 32767.
    pub max_vertices: u32,
    /// Edge of the diffuse map in pixels, a power of two from 4 to 4096.
    pub diffuse_size: u32,
    /// Edge of the normal map in pixels, a power of two from 4 to 4096.
    pub normal_size: u32,
    /// Carry specular in the alpha channel of the normal map, written as BC3 (own design, `docs/spec/textures.md`).
    /// Off by default and written only when set; off keeps the normal map opaque BC1.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub specular_alpha: bool,
    /// Close the gap between neighbouring blocks with a skirt under every border vertex, as the game's own blocks
    /// do (`docs/spec/terrain-lod.md`). Off by default and written only when set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skirts: bool,
}

/// Terrain settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TerrainRequest {
    /// Write terrain LOD.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Levels; empty means the defaults (4, 8, 16 and 32).
    #[serde(default)]
    pub levels: Vec<LevelRequest>,
    /// Blend the land textures into the diffuse maps (off: vertex colours only).
    #[serde(default = "yes")]
    pub land_textures: bool,
    /// Write water planes into the terrain blocks that have ground below water.
    #[serde(default = "yes")]
    pub water: bool,
    /// How often a land texture repeats across a cell quadrant in the diffuse maps, by the land texture's absolute
    /// `FormID` (eight hex digits); a texture not listed repeats once. Empty by default; written only when set.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub texture_scales: BTreeMap<String, f32>,
}

impl Default for TerrainRequest {
    fn default() -> Self {
        Self {
            enabled: true,
            levels: Vec::new(),
            land_textures: true,
            water: true,
            texture_scales: BTreeMap::new(),
        }
    }
}

/// Object LOD settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectsRequest {
    /// Write object LOD.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Leave out object faces that lie wholly under the ground (all three corners below the terrain at their
    /// position). Off by default; written only when on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drop_under_terrain: bool,
    /// Leave out object faces that lie wholly under the water of a cell that has water. Off by default; written
    /// only when on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drop_under_water: bool,
}

impl Default for ObjectsRequest {
    fn default() -> Self {
        Self {
            enabled: true,
            drop_under_terrain: false,
            drop_under_water: false,
        }
    }
}

/// Tree LOD settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TreesRequest {
    /// Write tree LOD: billboard atlas, type list and block files. Needs billboard textures that mods ship in
    /// the data folder (see docs/spec/object-lod.md); a world without any gets no tree LOD.
    #[serde(default = "yes")]
    pub enabled: bool,
}

impl Default for TreesRequest {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Which objects enter object LOD, beyond the game's own rule (visible when distant).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Inclusion {
    /// Leave out objects whose reference or base object comes from one of these plugins (file names).
    #[serde(default)]
    pub exclude_plugins: Vec<String>,
    /// When not empty, keep only objects whose reference or base object comes from one of these plugins.
    #[serde(default)]
    pub only_plugins: Vec<String>,
    /// Leave out these records: absolute `FormID`s of a reference or a base object, eight hex digits.
    #[serde(default)]
    pub exclude_records: Vec<String>,
    /// Leave out models whose path (relative to `meshes`, lower case, forward slashes) matches one of these
    /// patterns; `*` matches any run of characters.
    #[serde(default)]
    pub exclude_models: Vec<String>,
}

/// Resource limits.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Limits {
    /// Worker threads; omitted means the machine decides (at most four).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threads: Option<u32>,
    /// A memory budget in MiB, advisory: the run refuses early when the plugins alone are expected to need more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u64>,
}

/// Request v1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    /// The document version; 1.
    #[schemars(extend("const" = 1))]
    pub schema_version: u32,
    /// The contract name, `lodwright-request`.
    #[schemars(extend("const" = "lodwright-request"))]
    pub contract: String,
    /// The game's data folder (read only).
    pub data_folder: String,
    /// Plugin file names in load order, masters first.
    pub plugins: Vec<String>,
    /// Worlds to generate (editor ids); empty means every world with terrain.
    #[serde(default)]
    pub worlds: Vec<String>,
    /// The output folder: must not lie inside the data folder or a git working tree.
    pub output_dir: String,
    /// Terrain settings.
    #[serde(default)]
    pub terrain: TerrainRequest,
    /// Object LOD settings.
    #[serde(default)]
    pub objects: ObjectsRequest,
    /// Tree LOD settings.
    #[serde(default)]
    pub trees: TreesRequest,
    /// Inclusion rules.
    #[serde(default)]
    pub inclusion: Inclusion,
    /// Limits.
    #[serde(default)]
    pub limits: Limits,
}

impl Request {
    /// A request for `plugins` in `data_folder` writing to `output_dir`, everything else default.
    #[must_use]
    pub fn new(data_folder: &str, plugins: &[&str], output_dir: &str) -> Self {
        Self {
            schema_version: 1,
            contract: crate::REQUEST_CONTRACT.to_owned(),
            data_folder: data_folder.to_owned(),
            plugins: plugins.iter().map(|p| (*p).to_owned()).collect(),
            worlds: Vec::new(),
            output_dir: output_dir.to_owned(),
            terrain: TerrainRequest::default(),
            objects: ObjectsRequest::default(),
            trees: TreesRequest::default(),
            inclusion: Inclusion::default(),
            limits: Limits::default(),
        }
    }

    /// Check the values: what a generator would otherwise fail on halfway.
    ///
    /// # Errors
    /// A message naming the first field that is not acceptable.
    pub fn validate(&self) -> Result<(), String> {
        if self.plugins.is_empty() {
            return Err("plugins is empty".into());
        }
        if self
            .plugins
            .iter()
            .any(|p| p.is_empty() || p.contains(['/', '\\']))
        {
            return Err("plugins must be file names".into());
        }
        if self.output_dir.is_empty() || self.data_folder.is_empty() {
            return Err("data_folder and output_dir are required".into());
        }
        for l in &self.terrain.levels {
            if !TERRAIN_LEVELS.contains(&l.level) {
                return Err(format!("terrain level {} is not 4, 8, 16 or 32", l.level));
            }
            let (low, high) = MAX_VERTICES_RANGE;
            if !(low..=high).contains(&l.max_vertices) {
                return Err(format!(
                    "max_vertices {} is outside {low} to {high}",
                    l.max_vertices
                ));
            }
            if !l.quality.is_finite() || l.quality < QUALITY_MIN {
                return Err("quality must be a number of at least 0".into());
            }
            for (name, size) in [
                ("diffuse_size", l.diffuse_size),
                ("normal_size", l.normal_size),
            ] {
                let (low, high) = TEXTURE_SIZE_RANGE;
                if !size.is_power_of_two() || !(low..=high).contains(&size) {
                    return Err(format!(
                        "{name} {size} is not a power of two from {low} to {high}"
                    ));
                }
            }
        }
        let levels: Vec<u32> = self.terrain.levels.iter().map(|l| l.level).collect();
        if (1..levels.len()).any(|i| levels[..i].contains(&levels[i])) {
            return Err("a terrain level is listed twice".into());
        }
        for id in &self.inclusion.exclude_records {
            if id.len() != 8 || u32::from_str_radix(id, 16).is_err() {
                return Err(format!(
                    "exclude_records entry {id:?} is not eight hex digits"
                ));
            }
        }
        let (low, high) = TEXTURE_SCALE_RANGE;
        for (id, scale) in &self.terrain.texture_scales {
            if id.len() != 8 || u32::from_str_radix(id, 16).is_err() {
                return Err(format!("texture_scales key {id:?} is not eight hex digits"));
            }
            if !scale.is_finite() || !(low..=high).contains(scale) {
                return Err(format!(
                    "texture_scales {id} is {scale}, not from {low} to {high}"
                ));
            }
        }
        if self.limits.threads.is_some_and(|t| t < THREADS_MIN) {
            return Err("threads must be at least 1".into());
        }
        Ok(())
    }

    /// `sha256:` of the request in its canonical JSON form (fixed field order).
    ///
    /// # Panics
    /// Never: a request serialises.
    #[must_use]
    pub fn digest(&self) -> String {
        let json = serde_json::to_string(self).expect("a request serialises");
        sha256_digest(format!("lodwright-request-v1\n{json}").as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::{LevelRequest, Request};

    fn base() -> Request {
        Request::new("/data", &["FalloutNV.esm"], "/out")
    }

    #[test]
    fn a_minimal_request_is_valid_and_its_digest_is_stable() {
        let r = base();
        assert_eq!(r.validate(), Ok(()));
        assert_eq!(r.digest(), base().digest());
        assert!(r.digest().starts_with("sha256:"));
        let mut other = base();
        other.worlds.push("W".into());
        assert_ne!(other.digest(), r.digest());
    }

    #[test]
    fn omitted_fields_take_their_defaults_when_read() {
        let r: Request = serde_json::from_str(
            r#"{"schema_version":1,"contract":"lodwright-request","data_folder":"/d","plugins":["A.esm"],"output_dir":"/o"}"#,
        )
        .unwrap();
        assert!(
            r.terrain.enabled
                && r.objects.enabled
                && r.trees.enabled
                && r.terrain.land_textures
                && r.terrain.water
        );
        assert!(r.worlds.is_empty() && r.terrain.levels.is_empty());
    }

    #[test]
    fn each_bad_value_is_refused_with_its_name() {
        let level = |f: fn(&mut LevelRequest)| {
            let mut r = base();
            let mut l = LevelRequest {
                level: 4,
                quality: 10.0,
                max_vertices: 2000,
                diffuse_size: 256,
                normal_size: 256,
                specular_alpha: false,
                skirts: false,
            };
            f(&mut l);
            r.terrain.levels.push(l);
            r.validate()
        };
        assert!(level(|l| l.level = 5).unwrap_err().contains("level 5"));
        assert!(
            level(|l| l.max_vertices = 2)
                .unwrap_err()
                .contains("max_vertices")
        );
        assert!(
            level(|l| l.diffuse_size = 100)
                .unwrap_err()
                .contains("diffuse_size")
        );
        assert!(
            level(|l| l.quality = f32::NAN)
                .unwrap_err()
                .contains("quality")
        );
        let mut r = base();
        r.plugins.clear();
        assert!(r.validate().unwrap_err().contains("plugins"));
        let mut r = base();
        r.inclusion.exclude_records.push("xyz".into());
        assert!(r.validate().unwrap_err().contains("hex"));
        let mut r = base();
        r.limits.threads = Some(0);
        assert!(r.validate().unwrap_err().contains("threads"));
        let mut r = base();
        r.terrain.texture_scales.insert("0000ABCD".into(), 2.0);
        assert_eq!(r.validate(), Ok(()));
        r.terrain.texture_scales.insert("0000ABCE".into(), 0.0);
        assert!(
            r.validate()
                .unwrap_err()
                .contains("texture_scales 0000ABCE")
        );
        let mut r = base();
        r.terrain.texture_scales.insert("oak".into(), 2.0);
        assert!(r.validate().unwrap_err().contains("hex"));
    }
}
