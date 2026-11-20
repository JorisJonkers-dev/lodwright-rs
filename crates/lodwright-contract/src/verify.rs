// SPDX-License-Identifier: GPL-3.0-only
//! Verify v1: what `lodwright verify --json` prints about a generated LOD folder.
//!
//! The report carries file names (relative to the folder) and numbers only; nothing read from a file is copied
//! into it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One finding: a file and what is wrong with it, or how it differs from the reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VerifyFinding {
    /// The file, relative to the folder, forward slashes.
    pub path: String,
    /// What is wrong, as prose for people; never matched on.
    pub problem: String,
}

/// Verify v1, the report of `lodwright verify --json`.
// Not `Eq`: optional keys may carry measurements as floating-point numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerifyDoc {
    /// The document version; 1.
    #[schemars(extend("const" = 1))]
    pub schema_version: u32,
    /// The contract name, `lodwright-verify`: a reader refuses a document that names another.
    #[schemars(extend("const" = "lodwright-verify"))]
    pub contract: String,
    /// True when the folder has no errors.
    pub ok: bool,
    /// Terrain block meshes checked.
    pub terrain_blocks: u64,
    /// Object block meshes checked.
    pub object_blocks: u64,
    /// Tree block files checked.
    pub tree_blocks: u64,
    /// Textures checked.
    pub textures: u64,
    /// Settings files checked.
    pub settings: u64,
    /// What is wrong with the folder, sorted by path.
    pub errors: Vec<VerifyFinding>,
    /// Differences from the reference folder; empty when no reference was given.
    #[serde(default)]
    pub differences: Vec<VerifyFinding>,
    /// With a reference: the water of every terrain block that has water on either side, compared with the
    /// reference's. Omitted without a reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water: Option<WaterReport>,
    /// With a reference: the terrain heights of every block present on both sides, compared with the reference's.
    /// Omitted without a reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terrain: Option<TerrainReport>,
    /// With a reference: every object block present on both sides, compared with the reference's. Omitted without
    /// a reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objects: Option<ObjectReport>,
    /// With a reference: the per-channel correlation of the terrain diffuse (and normal map) of every block present
    /// on both sides with the reference's. Omitted without a reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation: Option<CorrelationReport>,
    /// With `--known-differences`: the differences the list names, with its reason and release. They are reported
    /// and do not make the run fail. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known: Vec<KnownDifference>,
    /// With `--known-differences`: listed paths that no longer differ, so the list can lose them. Omitted when
    /// empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<String>,
}

/// A difference the known-differences list names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct KnownDifference {
    /// The file, relative to the folder, forward slashes.
    pub path: String,
    /// What differs, as prose for people; never matched on.
    pub problem: String,
    /// Why the difference is accepted, from the list.
    pub reason: String,
    /// The release expected to remove it, from the list.
    pub release: String,
}

/// The terrain heights of one level compared with the reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TerrainLevel {
    /// Cells per block edge.
    pub level: u32,
    /// Blocks of this level that were compared.
    pub blocks: u64,
    /// Of those, the blocks whose largest difference is over the tolerance.
    pub over_tolerance: u64,
    /// The largest difference of the level, in game units.
    pub max: f64,
    /// The mean of the blocks' mean differences, in game units.
    pub mean: f64,
}

/// The terrain heights of the blocks compared with the reference: the ground of both meshes sampled on a 33 by 33
/// grid over the bounds of the block, at the points both cover, as the absolute difference of the heights in game
/// units. Numbers are rounded to three decimals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TerrainReport {
    /// Terrain blocks compared (present on both sides and sharing sampled ground).
    pub blocks: u64,
    /// The largest difference over which a block counts in `over_tolerance`: `--height-tolerance`, or 1 without it.
    pub tolerance: f64,
    /// Blocks whose largest difference is over `tolerance`.
    pub over_tolerance: u64,
    /// The largest difference of any block; omitted when no block was compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The mean of the blocks' mean differences; omitted when no block was compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    /// Every compared block, sorted by path.
    pub per_block: Vec<TerrainBlock>,
    /// The same numbers per terrain level, smallest level first. Omitted when no block was compared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_level: Vec<TerrainLevel>,
}

/// The height difference of one terrain block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TerrainBlock {
    /// The block mesh, relative to the folder, lower case.
    pub path: String,
    /// The largest absolute height difference, in game units.
    pub max: f64,
    /// The mean absolute height difference, in game units.
    pub mean: f64,
}

/// The water of one terrain level compared with the reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WaterLevel {
    /// Cells per block edge.
    pub level: u32,
    /// Blocks of this level with water on either side.
    pub blocks: u64,
    /// Of those, the blocks under the threshold.
    pub under_threshold: u64,
    /// The lowest `IoU` of the level.
    pub lowest: f64,
    /// The mean `IoU` of the level.
    pub mean: f64,
}

/// The water of the terrain blocks compared with the reference: the `IoU` (intersection over union) of the ground the
/// two water planes cover, seen from above, sampled at the centres of a 400 by 400 grid over the bounds of both
/// planes. Numbers are rounded to three decimals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WaterReport {
    /// Terrain blocks with water on either side (and present on both sides).
    pub blocks: u64,
    /// The `IoU` under which a block counts in `under_threshold`: `--water-iou`, or 0.95 without it.
    pub threshold: f64,
    /// Blocks whose `IoU` is under `threshold`.
    pub under_threshold: u64,
    /// The lowest `IoU`; omitted when no block has water.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lowest: Option<f64>,
    /// The mean `IoU`; omitted when no block has water.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    /// Every block with water, sorted by path.
    pub per_block: Vec<WaterBlock>,
    /// The same numbers per terrain level, smallest level first: what the release notes list. Omitted when no
    /// block has water.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_level: Vec<WaterLevel>,
}

/// The water `IoU` of one terrain block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WaterBlock {
    /// The block mesh, relative to the folder, lower case.
    pub path: String,
    /// 1 for the same covered ground, 0 for none in common.
    pub iou: f64,
}

/// The object blocks compared with the reference. A block is exact when the vertex and triangle counts are equal and
/// every vertex lies within 0.05 units of a vertex of the reference's block, in any order. Not being exact is reported,
/// not a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectReport {
    /// Object blocks present on both sides.
    pub blocks: u64,
    /// Of those, the exact ones.
    pub exact: u64,
    /// Every block present on both sides, sorted by path.
    pub per_block: Vec<ObjectBlock>,
}

/// One object block compared with the reference's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectBlock {
    /// The block mesh, relative to the folder, lower case.
    pub path: String,
    /// Vertices of the block (all its shapes).
    pub vertices: u64,
    /// Vertices of the reference's block.
    pub reference_vertices: u64,
    /// Triangles of the block.
    pub triangles: u64,
    /// Triangles of the reference's block.
    pub reference_triangles: u64,
    /// Vertices of the block with no vertex of the reference's block within 0.05 units.
    pub unmatched_vertices: u64,
    /// Equal counts and no unmatched vertex.
    pub exact: bool,
}

/// A Pearson correlation per colour channel, from -1 to 1, rounded to three decimals.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Channels {
    /// The red channel.
    pub r: f64,
    /// The green channel.
    pub g: f64,
    /// The blue channel.
    pub b: f64,
}

/// The correlations of a set of blocks: the mean and the lowest per channel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorrelationSummary {
    /// Blocks in the set.
    pub blocks: u64,
    /// The mean of the blocks' correlations, per channel.
    pub mean: Channels,
    /// The lowest correlation of any block, per channel.
    pub lowest: Channels,
}

/// The texture correlation of one terrain level.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorrelationLevel {
    /// Cells per block edge.
    pub level: u32,
    /// The diffuse textures of this level.
    pub diffuse: CorrelationSummary,
    /// The normal maps of this level; omitted when none could be compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normals: Option<CorrelationSummary>,
}

/// The Pearson correlation per colour channel between the terrain textures of the blocks present on both sides and
/// the reference's, over the pixels of the decoded images (alpha is not compared). Two flat channels correlate as 1
/// when they hold the same value and 0 otherwise; a flat channel against a varying one is 0. A pair whose size
/// differs, or that cannot be decoded, is left out. Numbers are rounded to three decimals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorrelationReport {
    /// Blocks whose diffuse textures were compared.
    pub blocks: u64,
    /// The summary of all compared diffuse textures; omitted when none was compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diffuse: Option<CorrelationSummary>,
    /// The summary of all compared normal maps; omitted when none was compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normals: Option<CorrelationSummary>,
    /// Every compared block, sorted by path.
    pub per_block: Vec<CorrelationBlock>,
    /// The same summaries per terrain level, smallest level first. Omitted when no block was compared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_level: Vec<CorrelationLevel>,
}

/// The texture correlation of one terrain block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorrelationBlock {
    /// The diffuse texture, relative to the folder, lower case.
    pub path: String,
    /// The correlation of the diffuse texture.
    pub diffuse: Channels,
    /// The correlation of the normal map; omitted when it is missing on a side or cannot be compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normals: Option<Channels>,
}

impl VerifyDoc {
    /// An empty report of a clean folder: no files counted, no findings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            contract: crate::VERIFY_CONTRACT.to_owned(),
            ok: true,
            terrain_blocks: 0,
            object_blocks: 0,
            tree_blocks: 0,
            textures: 0,
            settings: 0,
            errors: Vec::new(),
            differences: Vec::new(),
            water: None,
            terrain: None,
            objects: None,
            correlation: None,
            known: Vec::new(),
            stale: Vec::new(),
        }
    }
}

impl Default for VerifyDoc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{VerifyDoc, VerifyFinding};

    #[test]
    fn a_new_report_names_its_contract_and_version() {
        let doc = VerifyDoc::new();
        assert_eq!(
            (doc.schema_version, doc.contract.as_str()),
            (1, "lodwright-verify")
        );
        assert!(doc.ok);
    }

    #[test]
    fn a_report_without_differences_reads_back() {
        let mut doc = VerifyDoc::new();
        doc.errors.push(VerifyFinding {
            path: "a.nif".into(),
            problem: "p".into(),
        });
        let mut value = serde_json::to_value(&doc).unwrap();
        value.as_object_mut().unwrap().remove("differences");
        let back: VerifyDoc = serde_json::from_value(value).unwrap();
        assert_eq!(back, doc);
    }
}
