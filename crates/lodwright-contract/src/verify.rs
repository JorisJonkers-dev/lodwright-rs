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
    /// With a reference: every object block present on both sides, compared with the reference's. Omitted without
    /// a reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objects: Option<ObjectReport>,
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
            objects: None,
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
