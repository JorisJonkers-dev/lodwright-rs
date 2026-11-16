// SPDX-License-Identifier: MPL-2.0
//! Result v1: what `generate --json` prints at the end, also for exit 1 and 3, and the answer of
//! `--version --json`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ToolInfo;

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Finished; exit code 0.
    Ok,
    /// A capability is not built yet; exit code 3.
    Unsupported,
    /// Failed; exit code 1.
    Failed,
}

/// What was written for one world and kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Written {
    /// World editor id.
    pub world: String,
    /// `terrain_mesh`, `terrain_texture`, `settings`, `object_lod` or `tree_lod`.
    pub kind: String,
    /// Files written.
    pub files: u64,
    /// Bytes written.
    pub bytes: u64,
}

/// Result v1. Result and error documents go to stdout; stderr carries events and human text only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResultDoc {
    /// The document version; 1.
    #[schemars(extend("const" = 1))]
    pub schema_version: u32,
    /// The contract name, `lodwright-result`: a reader refuses a document that names another.
    #[schemars(extend("const" = "lodwright-result"))]
    pub contract: String,
    /// The tool that printed this document.
    pub tool: ToolInfo,
    /// How the run ended.
    pub status: Status,
    /// A stable machine-readable string that callers match on.
    pub code: String,
    /// Prose for people; never matched on.
    pub reason: String,
    /// SHA-256 of the normalised arguments (`request_digest` once a request document exists); omitted when no
    /// arguments were given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments_digest: Option<String>,
    /// SHA-256 over the plugins' sizes and contents; omitted when they were not read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs_digest: Option<String>,
    /// Peak resident memory in MiB; omitted when the platform cannot say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_memory_mib: Option<u64>,
    /// For `unsupported`: the capabilities that are missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_capabilities: Vec<String>,
    /// What was written.
    #[serde(default)]
    pub written: Vec<Written>,
    /// Things that did not go in and why, at most twenty lines (a missing texture, a model without LOD).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Relative name of the manifest, when one was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
}

impl ResultDoc {
    /// A document with a status, a code and a reason; the optional parts are empty.
    #[must_use]
    pub fn new(version: &str, status: Status, code: &str, reason: &str) -> Self {
        Self {
            schema_version: 1,
            contract: crate::RESULT_CONTRACT.to_owned(),
            tool: ToolInfo::new(version),
            status,
            code: code.to_owned(),
            reason: reason.to_owned(),
            arguments_digest: None,
            inputs_digest: None,
            peak_memory_mib: None,
            missing_capabilities: Vec::new(),
            written: Vec::new(),
            warnings: Vec::new(),
            manifest: None,
        }
    }

    /// The process exit code that goes with the status: 0, 3 or 1.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self.status {
            Status::Ok => 0,
            Status::Failed => 1,
            Status::Unsupported => 3,
        }
    }
}

/// One contract this build speaks and the schema versions it can print.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContractInfo {
    /// The contract name, for example `lodwright-plan`.
    pub name: String,
    /// The schema versions of that contract, oldest first.
    pub versions: Vec<u32>,
}

/// The answer of `lodwright contracts --json` (ADR-0006): every contract this build speaks and its versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Handshake {
    /// The contracts, plan first.
    pub contracts: Vec<ContractInfo>,
}

impl Handshake {
    /// This build.
    #[must_use]
    pub fn current() -> Self {
        Self {
            contracts: vec![
                ContractInfo {
                    name: crate::PLAN_CONTRACT.to_owned(),
                    versions: crate::PLAN_VERSIONS.to_vec(),
                },
                ContractInfo {
                    name: crate::RESULT_CONTRACT.to_owned(),
                    versions: crate::RESULT_VERSIONS.to_vec(),
                },
            ],
        }
    }
}

/// The answer of `lodwright --version --json`: the tool, its version and the contracts it speaks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VersionInfo {
    /// The tool name.
    pub tool: String,
    /// The tool version.
    pub version: String,
    /// The contracts spoken, as `lodwright contracts --json` lists them.
    pub contracts: Vec<ContractInfo>,
}

impl VersionInfo {
    /// This build.
    #[must_use]
    pub fn current(version: &str) -> Self {
        Self {
            tool: crate::TOOL_NAME.to_owned(),
            version: version.to_owned(),
            contracts: Handshake::current().contracts,
        }
    }
}

/// Peak resident size of this process in MiB (`VmHWM` of `/proc/self/status`), `None` where unavailable.
#[must_use]
pub fn peak_memory_mib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vm_hwm_kib(&status).map(|kib| kib.div_ceil(1024))
}

fn parse_vm_hwm_kib(status: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{Handshake, ResultDoc, Status, VersionInfo, parse_vm_hwm_kib, peak_memory_mib};

    #[test]
    fn exit_codes_follow_the_status() {
        let code = |s| ResultDoc::new("0.0.0", s, "c", "r").exit_code();
        assert_eq!(code(Status::Ok), 0);
        assert_eq!(code(Status::Failed), 1);
        assert_eq!(code(Status::Unsupported), 3);
    }

    #[test]
    fn optional_parts_are_omitted_not_zero() {
        let text =
            serde_json::to_string(&ResultDoc::new("0.0.0", Status::Failed, "c", "r")).unwrap();
        assert!(!text.contains("peak_memory_mib"));
        assert!(!text.contains("digest"));
        assert!(!text.contains("manifest"));
    }

    #[test]
    fn the_peak_is_read_from_a_status_text() {
        assert_eq!(
            parse_vm_hwm_kib("Name:\tx\nVmHWM:\t  2049 kB\nVmRSS:\t1 kB\n"),
            Some(2049)
        );
        assert_eq!(parse_vm_hwm_kib("Name:\tx\n"), None);
    }

    #[test]
    fn the_peak_of_this_process_is_positive_where_proc_exists() {
        if std::path::Path::new("/proc/self/status").exists() {
            assert!(peak_memory_mib().is_some_and(|mib| mib > 0));
        }
    }

    #[test]
    fn the_version_info_and_the_handshake_list_both_contracts() {
        let info = VersionInfo::current("1.2.3");
        assert_eq!(info.contracts, Handshake::current().contracts);
        assert_eq!(
            serde_json::to_string(&Handshake::current()).unwrap(),
            r#"{"contracts":[{"name":"lodwright-plan","versions":[1]},{"name":"lodwright-result","versions":[1]}]}"#
        );
    }
}
