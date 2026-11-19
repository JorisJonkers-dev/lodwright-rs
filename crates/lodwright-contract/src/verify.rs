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
