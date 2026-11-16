// SPDX-License-Identifier: MPL-2.0
//! The documents `lodwright` prints for its callers, as types with serde and JSON Schema.
//!
//! The contract is ours (no format facts): plan v1 and result v1, the progress event lines, the answer to
//! `--version --json`, and the digests that tie a document to its arguments and inputs. nvmod vendors the schemas
//! published from this crate. Unknown fields are ignored on read; adding an optional field is not a version bump.
//! Optional numbers are omitted, never zero as a stand-in.

mod digest;
mod event;
mod manifest;
mod plan;
mod request;
mod result;
mod schema;

pub use digest::{Sha256Stream, arguments_digest, sha256_digest};
pub use event::Event;
pub use manifest::{MANIFEST_CONTRACT, MANIFEST_FILE, Manifest, ManifestFile};
pub use plan::{
    Bounds, Capabilities, LevelPlan, Plan, PlannedBlock, ToolInfo, WorldPlan, round_error,
};
pub use request::{
    Inclusion, LevelRequest, Limits, ObjectsRequest, Request, TerrainRequest, TreesRequest,
};
pub use result::{
    ContractInfo, Handshake, ResultDoc, Status, VersionInfo, Written, peak_memory_mib,
};
pub use schema::{
    DocumentKind, SchemaError, check_plan, check_request, check_result, schema_file_name,
    schema_json,
};

/// The plan document versions this build prints and reads, newest last.
pub const PLAN_VERSIONS: &[u32] = &[1];
/// The result document versions this build prints and reads, newest last.
pub const RESULT_VERSIONS: &[u32] = &[1];
/// The request document versions this build reads, newest last.
pub const REQUEST_VERSIONS: &[u32] = &[1];
/// The contract name of the request document (its `contract` field).
pub const REQUEST_CONTRACT: &str = "lodwright-request";
/// The contract name of the plan document (its `contract` field).
pub const PLAN_CONTRACT: &str = "lodwright-plan";
/// The contract name of the result document (its `contract` field).
pub const RESULT_CONTRACT: &str = "lodwright-result";
/// The name every document gives as its tool.
pub const TOOL_NAME: &str = "lodwright";
