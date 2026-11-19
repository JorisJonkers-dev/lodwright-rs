// SPDX-License-Identifier: GPL-3.0-only
//! The documents `lodwright` prints for its callers, as types with serde and JSON Schema.
//!
//! The contract is ours (no format facts): plan v1, result v1, request v1, manifest v1, verify v1, defaults v1, the
//! progress event lines, the answer to `--version --json`, and the digests that tie a document to its arguments and inputs. nvmod vendors the schemas
//! published from this crate. Unknown fields are ignored on read; adding an optional field is not a version bump.
//! Optional numbers are omitted, never zero as a stand-in.

mod defaults;
mod digest;
mod event;
mod manifest;
mod plan;
mod request;
mod result;
mod schema;
mod verify;

pub use defaults::{Defaults, LevelDefault, OptionInfo, OptionKind};
pub use digest::{Sha256Stream, arguments_digest, sha256_digest};
pub use event::{BlockKind, Event};
pub use manifest::{MANIFEST_CONTRACT, MANIFEST_FILE, Manifest, ManifestBlock, ManifestFile};
pub use plan::{
    Bounds, Capabilities, LevelPlan, Plan, PlannedBlock, ToolInfo, WorldPlan, round_error,
};
pub use request::{
    Inclusion, LevelRequest, Limits, MAX_VERTICES_RANGE, ObjectsRequest, QUALITY_MIN, Request,
    TERRAIN_LEVELS, TEXTURE_SIZE_RANGE, THREADS_MIN, TerrainRequest, TreesRequest,
};
pub use result::{
    BlockCounts, ContractInfo, Handshake, ResultDoc, Status, VersionInfo, Written, peak_memory_mib,
};
pub use schema::{
    DocumentKind, SchemaError, check_manifest, check_plan, check_request, check_result,
    check_verify, schema_file_name, schema_json,
};
pub use verify::{ObjectBlock, ObjectReport, VerifyDoc, VerifyFinding, WaterBlock, WaterReport};

/// The plan document versions this build prints and reads, newest last.
pub const PLAN_VERSIONS: &[u32] = &[1];
/// The result document versions this build prints and reads, newest last.
pub const RESULT_VERSIONS: &[u32] = &[1];
/// The request document versions this build reads, newest last.
pub const REQUEST_VERSIONS: &[u32] = &[1];
/// The manifest document versions this build writes and reads, newest last.
pub const MANIFEST_VERSIONS: &[u32] = &[1];
/// The verify document versions this build prints, newest last.
pub const VERIFY_VERSIONS: &[u32] = &[1];
/// The progress event line versions this build prints, newest last.
pub const EVENTS_VERSIONS: &[u32] = &[1];
/// The contract name of the verify document (its `contract` field).
pub const VERIFY_CONTRACT: &str = "lodwright-verify";
/// The contract name of the progress event lines. The lines carry no `contract` field; the name is what the
/// handshake lists.
pub const EVENTS_CONTRACT: &str = "lodwright-events";
/// The versions of the defaults document this build speaks.
pub const DEFAULTS_VERSIONS: &[u32] = &[1];
/// The contract name of the defaults document (its `contract` field).
pub const DEFAULTS_CONTRACT: &str = "lodwright-defaults";
/// The contract name of the request document (its `contract` field).
pub const REQUEST_CONTRACT: &str = "lodwright-request";
/// The contract name of the plan document (its `contract` field).
pub const PLAN_CONTRACT: &str = "lodwright-plan";
/// The contract name of the result document (its `contract` field).
pub const RESULT_CONTRACT: &str = "lodwright-result";
/// The contract name of the in-process library API (`lodwright-api`), listed by `contracts --json`.
pub const API_CONTRACT: &str = "lodwright-api";
/// The version of the in-process library API. It changes when a call, a type or an error code of that API
/// changes in a way an existing caller would notice.
pub const API_VERSION: u32 = 1;
/// The name every document gives as its tool.
pub const TOOL_NAME: &str = "lodwright";
