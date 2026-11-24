// SPDX-License-Identifier: GPL-3.0-only
//! The manifest `generate` leaves in its output folder: what was written, from which request and inputs.
//!
//! A later run over the same request and the same plugin contents finds the manifest and the files it lists
//! and does nothing; a changed request or input replaces the files the old manifest lists.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ToolInfo;

/// One file of the output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManifestFile {
    /// Path relative to the output folder, forward slashes.
    pub path: String,
    /// Size in bytes.
    pub bytes: u64,
    /// `sha256:` of the contents.
    pub digest: String,
}

/// One terrain block of the output: what its files were built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManifestBlock {
    /// `terrain:<world>:<level>:<x>:<y>`, the world in lower case.
    pub key: String,
    /// `sha256:` over everything the block's files depend on.
    pub inputs: String,
    /// The block's files (paths in [`Manifest::files`]).
    pub files: Vec<String>,
}

/// The manifest, `manifest.json` in the output folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    /// The document version; 1.
    #[schemars(extend("const" = 1))]
    pub schema_version: u32,
    /// The contract name, `lodwright-manifest`.
    #[schemars(extend("const" = "lodwright-manifest"))]
    pub contract: String,
    /// The tool that wrote it.
    pub tool: ToolInfo,
    /// Digest of the request.
    pub request_digest: String,
    /// Digest of the plugins' contents.
    pub inputs_digest: String,
    /// Files, sorted by path.
    pub files: Vec<ManifestFile>,
    /// Terrain blocks, sorted by key; a later run keeps a block whose inputs did not change. Empty in manifests
    /// of earlier versions, which then rebuild every block.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<ManifestBlock>,
    /// Identity of the generator build that wrote the files: changes with any change to the code that decides what
    /// is written, also between two builds of one version. Absent in manifests of earlier builds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

/// The contract name of the manifest.
pub const MANIFEST_CONTRACT: &str = "lodwright-manifest";
/// The file name of the manifest in the output folder.
pub const MANIFEST_FILE: &str = "manifest.json";

impl Manifest {
    /// A manifest for `files` (sorted here).
    #[must_use]
    pub fn new(
        version: &str,
        request_digest: &str,
        inputs_digest: &str,
        mut files: Vec<ManifestFile>,
    ) -> Self {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Self {
            schema_version: 1,
            contract: MANIFEST_CONTRACT.to_owned(),
            tool: ToolInfo::new(version),
            request_digest: request_digest.to_owned(),
            inputs_digest: inputs_digest.to_owned(),
            files,
            blocks: Vec::new(),
            build: None,
        }
    }

    /// The same manifest naming the generator build that wrote it.
    #[must_use]
    pub fn with_build(mut self, build: &str) -> Self {
        self.build = Some(build.to_owned());
        self
    }

    /// The same manifest with `blocks` (sorted here).
    #[must_use]
    pub fn with_blocks(mut self, mut blocks: Vec<ManifestBlock>) -> Self {
        blocks.sort_by(|a, b| a.key.cmp(&b.key));
        self.blocks = blocks;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{Manifest, ManifestFile};

    #[test]
    fn files_are_sorted_and_the_document_round_trips() {
        let m = Manifest::new(
            "1",
            "sha256:a",
            "sha256:b",
            vec![
                ManifestFile {
                    path: "b".into(),
                    bytes: 1,
                    digest: "sha256:1".into(),
                },
                ManifestFile {
                    path: "a".into(),
                    bytes: 2,
                    digest: "sha256:2".into(),
                },
            ],
        );
        assert_eq!(m.files[0].path, "a");
        let back: Manifest = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }
}
