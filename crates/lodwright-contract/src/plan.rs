// SPDX-License-Identifier: MPL-2.0
//! Plan v1: what `lodwright plan --json` prints.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The tool that produced a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolInfo {
    /// Always `lodwright`.
    pub name: String,
    /// The tool version (`SemVer`, see `VERSIONING.md`).
    pub version: String,
}

impl ToolInfo {
    /// This tool at `version`.
    #[must_use]
    pub fn new(version: &str) -> Self {
        Self {
            name: crate::TOOL_NAME.to_owned(),
            version: version.to_owned(),
        }
    }
}

/// What `generate` can do today.
#[allow(clippy::struct_excessive_bools)] // one flag per capability is the contract
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    /// Writes terrain meshes (`.nif`).
    pub terrain_mesh: bool,
    /// Writes terrain textures.
    pub terrain_texture: bool,
    /// Writes the LOD settings file.
    pub settings: bool,
    /// Writes object LOD (`.bto`).
    pub object_lod: bool,
    /// Writes tree LOD.
    pub tree_lod: bool,
}

impl Capabilities {
    /// This build: terrain meshes, terrain textures and the settings file are written; object and tree LOD are not.
    #[must_use]
    pub fn terrain() -> Self {
        Self {
            terrain_mesh: true,
            terrain_texture: true,
            settings: true,
            object_lod: false,
            tree_lod: false,
        }
    }

    /// The names of the capabilities that are false, in a fixed order.
    #[must_use]
    pub fn missing(&self) -> Vec<String> {
        [
            ("terrain_mesh", self.terrain_mesh),
            ("terrain_texture", self.terrain_texture),
            ("settings", self.settings),
            ("object_lod", self.object_lod),
            ("tree_lod", self.tree_lod),
        ]
        .into_iter()
        .filter(|(_, present)| !present)
        .map(|(name, _)| name.to_owned())
        .collect()
    }
}

/// Inclusive cell bounds of a world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Bounds {
    /// Smallest cell x.
    pub west: i32,
    /// Smallest cell y.
    pub south: i32,
    /// Largest cell x.
    pub east: i32,
    /// Largest cell y.
    pub north: i32,
}

/// Totals of one level of one world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelPlan {
    /// Cells per block edge.
    pub level: u32,
    /// Blocks at this level.
    pub blocks: u64,
    /// Vertices over all blocks.
    pub vertices: u64,
    /// Triangles over all blocks.
    pub triangles: u64,
    /// Largest height error over all blocks, in world units.
    pub max_error: f64,
}

/// One planned block: present only with `--blocks`. Names are relative to the output folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlannedBlock {
    /// Cells per block edge.
    pub level: u32,
    /// Cell x of the south-west cell.
    pub x: i32,
    /// Cell y of the south-west cell.
    pub y: i32,
    /// Sampling step in cells.
    pub step: u64,
    /// Vertices of the block mesh.
    pub vertices: u64,
    /// Triangles of the block mesh.
    pub triangles: u64,
    /// Height error of the block, in world units.
    pub max_error: f64,
    /// Relative path of the mesh file.
    pub mesh: String,
    /// Relative path of the diffuse texture.
    pub diffuse: String,
    /// Relative path of the normal texture.
    pub normal: String,
    /// Width and height of the diffuse texture.
    pub texture_size: [u32; 2],
    /// Mip levels of the textures.
    pub mips: u32,
}

/// The plan of one world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorldPlan {
    /// Editor id of the world.
    pub name: String,
    /// Absolute form id, eight upper-case hex digits.
    pub form_id: String,
    /// Cells with terrain.
    pub cells: u64,
    /// Cell bounds; omitted for a world without cells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// Largest height difference across a shared cell edge, in world units.
    pub max_seam_error: f64,
    /// Relative path of the LOD settings file.
    pub settings_path: String,
    /// One entry per level.
    pub levels: Vec<LevelPlan>,
    /// The blocks; present only when asked for with `--blocks`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<PlannedBlock>>,
}

/// Plan v1. It holds numbers and relative output names only: no game content, no absolute path from the data
/// folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    /// The document version; 1.
    #[schemars(extend("const" = 1))]
    pub schema_version: u32,
    /// The contract name, `lodwright-plan`: a reader refuses a document that names another.
    #[schemars(extend("const" = "lodwright-plan"))]
    pub contract: String,
    /// The tool that printed this plan.
    pub tool: ToolInfo,
    /// SHA-256 of the normalised arguments.
    pub arguments_digest: String,
    /// SHA-256 over the sizes and contents of the plugins, in load order (never the content itself).
    pub inputs_digest: String,
    /// What `generate` can do today.
    pub capabilities: Capabilities,
    /// One entry per selected world.
    pub worlds: Vec<WorldPlan>,
    /// Human-readable remarks.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Round an error in world units to three decimals, as the documents print it.
#[must_use]
pub fn round_error(value: f32) -> f64 {
    (f64::from(value) * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::{Capabilities, round_error};

    #[test]
    fn every_capability_is_missing_today() {
        assert_eq!(
            Capabilities::default().missing(),
            [
                "terrain_mesh",
                "terrain_texture",
                "settings",
                "object_lod",
                "tree_lod"
            ]
        );
    }

    #[test]
    fn a_present_capability_is_not_listed_as_missing() {
        let caps = Capabilities {
            settings: true,
            ..Capabilities::default()
        };
        assert!(!caps.missing().contains(&"settings".to_owned()));
    }

    #[test]
    fn errors_round_to_three_decimals() {
        assert!((round_error(1.234_56) - 1.235).abs() < 1e-12);
        assert!((round_error(0.0)).abs() < 1e-12);
    }
}
