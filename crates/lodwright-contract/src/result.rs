// SPDX-License-Identifier: GPL-3.0-only
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

/// Where a run spent its time and memory: one entry per step, and per world where the step runs once for each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepProfile {
    /// The step: `read_plugins`, `terrain`, `objects`, `trees`, `pack` or `manifest` (a finer phase of a step is
    /// named `<step>.<phase>`, for example `objects.models`).
    pub step: String,
    /// The world the step ran for; omitted for a step of the whole run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// Wall clock time in milliseconds.
    pub wall_ms: u64,
    /// CPU time of the whole process during the step in milliseconds, all threads together; omitted where the
    /// platform cannot say. More than `wall_ms` when the step used several threads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_ms: Option<u64>,
    /// Peak resident memory of the process during the step in MiB; omitted where the platform cannot say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_memory_mib: Option<u64>,
    /// Files the step wrote.
    pub files: u64,
    /// Bytes the step wrote.
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
    /// Time, CPU time and peak memory per step of a `generate` run, in the order the steps ran; omitted when the run
    /// did not generate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<StepProfile>,
    /// For `unsupported`: the capabilities that are missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_capabilities: Vec<String>,
    /// What was written.
    #[serde(default)]
    pub written: Vec<Written>,
    /// Things that did not go in and why, at most twenty lines (a missing texture, a model without LOD).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// How many warnings of each kind the run had, sorted by kind; the full list is in `warnings_file`. Omitted
    /// when the run had none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warning_counts: Vec<WarningCount>,
    /// Relative name of the file with every warning, one record each ([`WarningsFile`]), beside the manifest;
    /// omitted when the run had no warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warnings_file: Option<String>,
    /// Relative name of the manifest, when one was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
    /// Terrain blocks of a `generate` run: how many were built and how many were kept from the earlier run
    /// because their inputs did not change; omitted when no terrain was generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<BlockCounts>,
    /// Object faces left out by `objects.drop_under_terrain` and `objects.drop_under_water`; omitted when neither
    /// option is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dropped_faces: Option<DroppedFaces>,
    /// Diffuse tiles of a `generate` run: how many were left to the game's own far-land tiles and how many were
    /// composed; omitted when the run composes every tile (`terrain.compose_shipped_tiles`) or wrote no land tiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles: Option<TileCounts>,
}

/// The name of the file with every warning of a run, written beside the manifest.
pub const WARNINGS_FILE: &str = "lodwright-warnings.json";
/// The contract name of that file.
pub const WARNINGS_CONTRACT: &str = "lodwright-warnings";

/// How many warnings of one kind a run had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WarningCount {
    /// The kind, as in [`WarningRecord::kind`].
    pub kind: String,
    /// Warnings of that kind.
    pub count: u64,
}

/// One warning of a run: what was left out or drawn another way, which plugin it concerns and what a person can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WarningRecord {
    /// A stable name of the kind of warning, for example `far_model_without_lod_model`.
    pub kind: String,
    /// The world the warning concerns; omitted when it concerns no one world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// The plugin the warning concerns (for a far model: the plugin that marks the object visible from far); omitted
    /// when no plugin is known. lodwright reads plugins, not mods: a person finds the mod from the plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    /// The files, blocks or models the warning is about, sorted; complete (the short list in the result is not).
    pub subjects: Vec<String>,
    /// How many subjects (or objects, where the kind counts objects).
    pub count: u64,
    /// The text of the warning.
    pub message: String,
    /// What a person can do about it.
    pub advice: String,
}

impl WarningRecord {
    /// A record of `kind` with its text and the advice that goes with the kind; one subject per entry of `subjects`
    /// (sorted, without repeats) and `count` taken from them unless `count` is given.
    #[must_use]
    pub fn new(
        kind: &str,
        world: Option<&str>,
        plugin: Option<&str>,
        subjects: impl IntoIterator<Item = String>,
        count: Option<u64>,
        message: String,
    ) -> Self {
        let subjects: std::collections::BTreeSet<String> = subjects.into_iter().collect();
        let subjects: Vec<String> = subjects.into_iter().collect();
        Self {
            kind: kind.to_owned(),
            world: world.map(str::to_owned),
            plugin: plugin.map(str::to_owned),
            count: count.unwrap_or(subjects.len() as u64),
            subjects,
            message,
            advice: advice_for(kind).to_owned(),
        }
    }
}

/// The file of every warning of a run (`lodwright-warnings.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WarningsFile {
    /// The document version; 1.
    #[schemars(extend("const" = 1))]
    pub schema_version: u32,
    /// The contract name, `lodwright-warnings`.
    #[schemars(extend("const" = "lodwright-warnings"))]
    pub contract: String,
    /// The warnings per kind.
    pub counts: Vec<WarningCount>,
    /// Every warning, sorted by kind, plugin and subjects.
    pub warnings: Vec<WarningRecord>,
}

impl WarningsFile {
    /// The file for `records`: sorted, and counted per kind.
    #[must_use]
    pub fn new(mut records: Vec<WarningRecord>) -> Self {
        records.sort_by(|a, b| {
            (&a.kind, &a.world, &a.plugin, &a.subjects, &a.message).cmp(&(
                &b.kind,
                &b.world,
                &b.plugin,
                &b.subjects,
                &b.message,
            ))
        });
        records.dedup();
        Self {
            schema_version: 1,
            contract: WARNINGS_CONTRACT.to_owned(),
            counts: warning_counts(&records),
            warnings: records,
        }
    }
}

/// The records counted per kind, sorted by kind.
#[must_use]
pub fn warning_counts(records: &[WarningRecord]) -> Vec<WarningCount> {
    let mut by: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for r in records {
        *by.entry(&r.kind).or_default() += 1;
    }
    by.into_iter()
        .map(|(kind, count)| WarningCount {
            kind: kind.to_owned(),
            count,
        })
        .collect()
}

/// What a person can do about a warning of `kind`.
#[must_use]
pub fn advice_for(kind: &str) -> &'static str {
    match kind {
        "far_model_without_lod_model" => {
            "Nothing is wrong with the load order: no LOD model exists for these models in the game or any mod, so the \
             objects are left out of the far view (other LOD generators leave them out too). To see them from far, install a mod \
             that ships their LOD models, or ask the author of the plugin to stop marking them visible from far."
        }
        "far_model_not_found" => {
            "Neither the LOD model nor the full model was found. Check that the mod with the model is enabled and that \
             its files are in the data folder; if the plugin needs a master or mod you left out, enable it."
        }
        "far_model_unreadable" => {
            "The LOD model was found but cannot be read. Reinstall the mod that provides it, or remove its LOD \
             model so that the objects are left out."
        }
        "far_model_simplified" => {
            "Nothing is wrong: the request asked for far shapes from full models, and these models have no LOD model of \
             their own. Their far shapes are simplified copies. An authored LOD model, where a mod ships one, is used \
             instead."
        }
        "far_model_no_solid_shape" => {
            "Every shape of these models has an alpha property, so they may be effects or glass. No far shape is made \
             from them and the objects stay out of the far view. Install a mod that ships a LOD model if you want them \
             drawn."
        }
        "lod_shape_without_texture" => {
            "A shape of a LOD model names no diffuse texture. Lodwright takes the texture of the full model where it \
             can, else draws a flat colour; ask the author of the model's mod for a fixed LOD model."
        }
        "texture_not_found" => {
            "A texture a model or land type names is in no folder or archive of the load order. Install or enable the \
             mod that ships it; until then a flat colour or the neutral earth is used."
        }
        "land_texture_not_found" => {
            "A land type names a texture that no folder or archive of the load order holds, so the layers that use it \
             are left out of the far land. The game's own data \
             does this for BurntGround01: nothing to do. For a mod's land type, install or enable the mod that ships \
             the texture."
        }
        "texture_unreadable" => {
            "A texture was found but lodwright cannot read its format. Replace it with one in a common format (BC1, \
             BC3, BC7 or uncompressed); until then a flat colour is used."
        }
        "block_objects_left_out" => {
            "Single objects are too large for one block shape (more than 65535 vertices or triangles). Use a LOD \
             model with fewer triangles for them."
        }
        "block_split_into_shapes" => {
            "Nothing to do: a block with many objects is written as several shapes, as the game's own blocks are."
        }
        _ => "Read the message; no further advice is known for this kind of warning.",
    }
}

/// Diffuse tiles left to the game and composed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct TileCounts {
    /// Tiles not written because the game ships its own and no plugin outside the game's own masters changed the
    /// land under it.
    pub kept: u64,
    /// Tiles composed from the land textures.
    pub composed: u64,
}

/// Object faces left out, per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct DroppedFaces {
    /// Faces wholly under the ground.
    pub under_terrain: u64,
    /// Faces wholly under a cell's water (and not under the ground).
    pub under_water: u64,
}

/// Terrain blocks built and kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct BlockCounts {
    /// Blocks built in this run.
    pub generated: u64,
    /// Blocks whose files were kept from the earlier run.
    pub up_to_date: u64,
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
            warning_counts: Vec::new(),
            warnings_file: None,
            manifest: None,
            blocks: None,
            dropped_faces: None,
            tiles: None,
            steps: Vec::new(),
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
    /// The contracts: plan, result, request, manifest, verify, events, then the library API.
    pub contracts: Vec<ContractInfo>,
}

impl Handshake {
    /// This build.
    #[must_use]
    pub fn current() -> Self {
        let mut contracts: Vec<ContractInfo> = crate::DocumentKind::ALL
            .into_iter()
            .map(|kind| ContractInfo {
                name: kind.contract().to_owned(),
                versions: kind.versions().to_vec(),
            })
            .collect();
        contracts.push(ContractInfo {
            name: crate::API_CONTRACT.to_owned(),
            versions: vec![crate::API_VERSION],
        });
        Self { contracts }
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

/// Start a new peak: after this, [`peak_memory_mib`] reports the highest resident size since now (Linux only; a no-op
/// elsewhere). The peak of one step of a run is read after resetting it at the start of the step.
pub fn reset_peak_memory() {
    // "5" resets the peak resident size (`VmHWM`) to the current one; a file that cannot be written leaves the
    // peak as the whole run's
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

/// CPU time (user and system, every thread) the process has used so far in milliseconds, `None` where unavailable.
#[must_use]
pub fn process_cpu_ms() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    parse_cpu_ms(&stat)
}

fn parse_cpu_ms(stat: &str) -> Option<u64> {
    // the process name in parentheses may hold spaces: count the fields after the last parenthesis; utime and stime
    // are the 14th and 15th fields of the line, in clock ticks of 100 a second
    let after = &stat[stat.rfind(')')? + 1..];
    let mut fields = after.split_whitespace();
    let utime: u64 = fields.nth(11)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some((utime + stime) * 10)
}

fn parse_vm_hwm_kib(status: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{
        Handshake, ResultDoc, Status, VersionInfo, parse_cpu_ms, parse_vm_hwm_kib, peak_memory_mib,
    };

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
    fn the_version_info_and_the_handshake_list_every_contract() {
        let info = VersionInfo::current("1.2.3");
        assert_eq!(info.contracts, Handshake::current().contracts);
        assert_eq!(
            serde_json::to_string(&Handshake::current()).unwrap(),
            r#"{"contracts":[{"name":"lodwright-plan","versions":[1]},{"name":"lodwright-result","versions":[1]},{"name":"lodwright-request","versions":[1]},{"name":"lodwright-manifest","versions":[1]},{"name":"lodwright-verify","versions":[1]},{"name":"lodwright-events","versions":[1]},{"name":"lodwright-defaults","versions":[1]},{"name":"lodwright-bands","versions":[1]},{"name":"lodwright-api","versions":[1]}]}"#
        );
    }

    #[test]
    fn cpu_time_is_read_from_the_stat_line_whatever_the_process_name_holds() {
        // pid, name, state, then fields 4 to 13, then utime 250 and stime 50 ticks of 10 ms
        let line = "123 (a b) c) S 1 2 3 4 5 6 7 8 9 10 250 50 0 0 20 0 1 0 100 1000 100";
        assert_eq!(parse_cpu_ms(line), Some(3000));
        assert_eq!(parse_cpu_ms("garbage"), None);
    }
}
