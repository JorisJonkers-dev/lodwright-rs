// SPDX-License-Identifier: GPL-3.0-only
//! [`lodwright_api::generate`] called in process: the result, progress, cancel and resume, and the errors.

use std::path::PathBuf;

use lodwright_api::{
    CancelToken, GenerateOutcome, GenerateRequest, NoProgress, ProgressEvent, Step, generate,
};
use lodwright_contract::{MANIFEST_FILE, Manifest, Request};
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-api-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A data folder with one world two level-4 blocks wide, and a terrain-only request on one thread for it.
fn job(name: &str) -> (GenerateRequest, PathBuf) {
    let data = temp(&format!("{name}-data"));
    std::fs::create_dir_all(&data).unwrap();
    let world = SyntheticWorld::new(0xB00, "Island")
        .cell(0, 0, Heights::flat(16.0))
        .cell(5, 0, Heights::flat(24.0));
    std::fs::write(data.join("Island.esm"), world_plugin(&[], &world, false)).unwrap();
    let out = temp(&format!("{name}-out"));
    let mut request = Request::new(
        data.to_str().unwrap(),
        &["Island.esm"],
        out.to_str().unwrap(),
    );
    request.objects.enabled = false;
    request.trees.enabled = false;
    request.terrain.land_textures = false;
    request.limits.threads = Some(1);
    (GenerateRequest::new(request), out)
}

fn manifest(out: &std::path::Path) -> Manifest {
    serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap()
}

/// Every file in `out`, as a path relative to it with forward slashes (the manifest's form).
fn files_on_disk(out: &std::path::Path) -> Vec<String> {
    fn walk(out: &std::path::Path, dir: &std::path::Path, into: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(out, &path, into);
            } else if path.is_file() {
                let rel = path.strip_prefix(out).unwrap();
                let parts: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                into.push(parts.join("/"));
            }
        }
    }
    let mut files = Vec::new();
    walk(out, out, &mut files);
    files.sort();
    files
}

fn done(outcome: GenerateOutcome) -> lodwright_contract::ResultDoc {
    match outcome {
        GenerateOutcome::Done { result } => *result,
        GenerateOutcome::Cancelled => panic!("the call was cancelled"),
    }
}

#[test]
fn generate_writes_the_lod_and_a_manifest_and_answers_generated() {
    let (job, out) = job("done");
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated");
    assert_eq!(result.manifest.as_deref(), Some(MANIFEST_FILE));
    let m = manifest(&out);
    assert_eq!(m.request_digest, job.request.digest());
    assert!(m.files.iter().all(|f| out.join(&f.path).is_file()));
    assert!(m.files.iter().any(|f| f.path.ends_with(".dlodsettings")));
}

#[test]
fn the_result_profiles_every_step_with_its_time_and_the_files_it_wrote() {
    let (job, out) = job("profile");
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    let names: Vec<(&str, Option<&str>)> = result
        .steps
        .iter()
        .map(|s| (s.step.as_str(), s.world.as_deref()))
        .collect();
    assert_eq!(
        names,
        [
            ("read_plugins", None),
            ("setup", None),
            ("assets_index", None),
            ("terrain.land_tiles", Some("Island")),
            ("terrain.shipped_plan", Some("Island")),
            ("terrain.blocks", Some("Island")),
            ("manifest", None),
        ]
    );
    // the files of the terrain step are the manifest's terrain files, with their bytes
    let m = manifest(&out);
    let blocks = result
        .steps
        .iter()
        .find(|s| s.step == "terrain.blocks")
        .unwrap();
    assert_eq!(blocks.files, u64::try_from(m.files.len()).unwrap());
    assert_eq!(blocks.bytes, m.files.iter().map(|f| f.bytes).sum::<u64>());
    if cfg!(target_os = "linux") {
        assert!(blocks.cpu_ms.is_some() && blocks.peak_memory_mib.is_some());
        // the run's peak is the highest of its steps
        assert_eq!(
            result.peak_memory_mib,
            result.steps.iter().filter_map(|s| s.peak_memory_mib).max()
        );
    }
}

#[test]
fn a_second_call_with_the_same_request_is_up_to_date() {
    let (job, _) = job("again");
    generate(&job, &mut NoProgress, &CancelToken::new()).unwrap();
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "up_to_date");
}

#[test]
fn an_output_written_by_another_version_is_rebuilt_not_reported_up_to_date() {
    let (job, out) = job("other-version");
    generate(&job, &mut NoProgress, &CancelToken::new()).unwrap();
    let mut m = manifest(&out);
    m.tool.version = "0.0.1".to_owned();
    std::fs::write(out.join(MANIFEST_FILE), serde_json::to_vec(&m).unwrap()).unwrap();
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated");
    assert_eq!(manifest(&out).tool.version, result.tool.version);
}

#[test]
fn progress_starts_each_step_and_counts_every_terrain_block() {
    let (job, _) = job("progress");
    let mut events: Vec<ProgressEvent> = Vec::new();
    let mut progress = |e: &ProgressEvent| events.push(e.clone());
    generate(&job, &mut progress, &CancelToken::new()).unwrap();
    let steps: Vec<Step> = events.iter().map(|e| e.step).collect();
    assert_eq!(steps.first(), Some(&Step::ReadPlugins));
    assert_eq!(steps.last(), Some(&Step::Manifest));
    let blocks: Vec<&ProgressEvent> = events.iter().filter(|e| e.done.is_some()).collect();
    assert_ne!(blocks.len(), 0);
    assert!(blocks.iter().all(|e| {
        e.step == Step::Terrain
            && e.world.as_deref() == Some("Island")
            && e.level.is_some()
            && e.done <= e.total
    }));
    // every level reaches its total
    let level4: Vec<_> = blocks.iter().filter(|e| e.level == Some(4)).collect();
    assert_eq!(level4.len(), 2);
    assert_eq!(level4.last().unwrap().done, Some(2));
    // every counted event has its events-file line, so the file and the callback tell the same story
    assert!(blocks.iter().all(|e| e.block_line().is_some()));
}

fn staging(out: &std::path::Path) -> PathBuf {
    let mut name = out.file_name().unwrap().to_os_string();
    name.push(".lodwright-staging");
    out.with_file_name(name)
}

fn checkpoint(out: &std::path::Path) -> Manifest {
    serde_json::from_slice(&std::fs::read(staging(out).join("checkpoint.json")).unwrap()).unwrap()
}

#[test]
fn every_event_of_a_run_carries_the_run_numbers_and_the_run_total_never_goes_back() {
    let (job, _) = job("run-progress");
    let mut events: Vec<ProgressEvent> = Vec::new();
    let mut progress = |e: &ProgressEvent| events.push(e.clone());
    generate(&job, &mut progress, &CancelToken::new()).unwrap();
    // read plugins, terrain (start + blocks), manifest
    assert!(events.len() >= 4);
    let totals: Vec<_> = events.iter().map(|e| e.run_total).collect();
    assert!(totals.iter().all(|t| *t == Some(10_000)));
    let done: Vec<u64> = events.iter().map(|e| e.run_done.unwrap()).collect();
    assert!(done.windows(2).all(|w| w[0] <= w[1]), "{done:?}");
    assert_eq!(done[0], 0);
    assert!(done.last().unwrap() < &10_000 && *done.last().unwrap() > done[1]);
    // the first event does not know the phase count yet; every later one does: read, terrain, manifest
    assert_eq!(events[0].phase_index, Some(1));
    assert_eq!(events[0].phase_count, None);
    assert!(events[1..].iter().all(|e| e.phase_count == Some(3)));
    let index: Vec<u32> = events.iter().map(|e| e.phase_index.unwrap()).collect();
    assert!(index.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(index.last(), Some(&3));
    // inside a phase the number is never above its total, and the last terrain block ends the terrain slice
    for e in &events {
        assert!(e.phase_done <= e.phase_total);
    }
    let last_block = events.iter().rfind(|e| e.done.is_some()).unwrap();
    assert!(last_block.phase_done > Some(0));
    // the manifest phase starts exactly where the terrain slice ends
    let terrain_end = events
        .iter()
        .find(|e| e.step == Step::Terrain)
        .map(|e| e.run_done.unwrap() + e.phase_total.unwrap())
        .unwrap();
    assert_eq!(*done.last().unwrap(), terrain_end);
}

/// Cancel the call at the first finished terrain block.
fn cancel_at_first_block() -> (CancelToken, impl FnMut(&ProgressEvent)) {
    let cancel = CancelToken::new();
    let stopper = cancel.clone();
    (cancel, move |e: &ProgressEvent| {
        if e.done.is_some() {
            stopper.cancel();
        }
    })
}

#[test]
fn a_token_cancelled_before_the_call_writes_nothing() {
    let (job, out) = job("cancel-first");
    let cancel = CancelToken::new();
    cancel.cancel();
    let outcome = generate(&job, &mut NoProgress, &cancel).unwrap();
    assert_eq!(outcome, GenerateOutcome::Cancelled);
    assert!(!out.exists());
    assert!(!staging(&out).exists());
}

#[test]
fn a_cancel_after_the_first_block_keeps_the_staging_folder_the_next_call_resumes_from() {
    let (job, out) = job("resume");
    let (cancel, mut progress) = cancel_at_first_block();
    let outcome = generate(&job, &mut progress, &cancel).unwrap();
    assert_eq!(outcome, GenerateOutcome::Cancelled);
    assert!(
        !out.exists(),
        "the output folder is only written on success"
    );
    let partial = checkpoint(&out);
    assert_eq!(partial.blocks.len(), 1, "only the first block finished");
    assert_eq!(partial.request_digest, job.request.digest());
    assert!(
        !partial
            .files
            .iter()
            .any(|f| f.path.ends_with(".dlodsettings"))
    );
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated");
    let counts = result.blocks.unwrap();
    assert_eq!(counts.up_to_date, 1, "the finished block is kept");
    assert!(counts.generated > 0);
    assert_eq!(manifest(&out).request_digest, job.request.digest());
    assert!(!staging(&out).exists(), "staging is swapped into place");
    assert!(!out.join("checkpoint.json").exists());
}

#[test]
fn a_resumed_run_writes_the_files_an_uninterrupted_run_writes() {
    let (job, out) = job("resume-bytes");
    let (cancel, mut progress) = cancel_at_first_block();
    generate(&job, &mut progress, &cancel).unwrap();
    done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    let (clean, clean_out) = self::job("resume-bytes-clean");
    done(generate(&clean, &mut NoProgress, &CancelToken::new()).unwrap());
    let (a, b) = (manifest(&out), manifest(&clean_out));
    assert_eq!(a.files, b.files, "same files with the same digests");
    assert_eq!(a.blocks, b.blocks);
    assert_eq!(files_on_disk(&out), files_on_disk(&clean_out));
}

#[test]
fn a_cancel_during_a_second_run_leaves_the_previous_output_untouched() {
    let (mut job, out) = job("changed-resume");
    job.request.terrain.water = false;
    let first = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(first.code, "generated");
    let first_manifest = manifest(&out);
    let before = files_on_disk(&out);
    // water on: every terrain block is built again; cancelled after the first block
    job.request.terrain.water = true;
    let (cancel, mut progress) = cancel_at_first_block();
    let outcome = generate(&job, &mut progress, &cancel).unwrap();
    assert_eq!(outcome, GenerateOutcome::Cancelled);
    assert_eq!(
        manifest(&out),
        first_manifest,
        "the previous manifest stands"
    );
    assert_eq!(files_on_disk(&out), before, "and so do its files");
    assert_eq!(checkpoint(&out).blocks.len(), 1);
    // the next run without a cancel finishes and leaves nothing the manifest does not list
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated");
    let final_manifest = manifest(&out);
    assert_eq!(final_manifest.request_digest, job.request.digest());
    let listed: Vec<String> = final_manifest
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    let mut on_disk = files_on_disk(&out);
    on_disk.retain(|p| p != MANIFEST_FILE);
    assert_eq!(listed, on_disk, "no stray file is left");
    assert!(!staging(&out).exists());
    let verification = lodwright_gen::verify_folder(&out);
    assert!(verification.errors.is_empty(), "{:?}", verification.errors);
    let (mut clean_job, clean_out) = self::job("changed-resume-clean");
    clean_job.request.terrain.water = true;
    done(generate(&clean_job, &mut NoProgress, &CancelToken::new()).unwrap());
    for file in &final_manifest.files {
        assert_eq!(
            std::fs::read(out.join(&file.path)).unwrap(),
            std::fs::read(clean_out.join(&file.path)).unwrap(),
            "{} is the file a clean run writes",
            file.path
        );
    }
    assert_eq!(final_manifest.files.len(), manifest(&clean_out).files.len());
}

#[test]
fn a_changed_request_discards_the_staging_folder_of_the_interrupted_one() {
    let (mut job, out) = job("changed-request");
    let (cancel, mut progress) = cancel_at_first_block();
    generate(&job, &mut progress, &cancel).unwrap();
    let stale: Vec<String> = checkpoint(&out)
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    assert_ne!(stale.len(), 0);
    job.request.terrain.water = !job.request.terrain.water;
    let (cancel, mut progress) = cancel_at_first_block();
    generate(&job, &mut progress, &cancel).unwrap();
    let now = checkpoint(&out);
    assert_eq!(
        now.request_digest,
        job.request.digest(),
        "a clean start for the new request"
    );
    assert_eq!(now.blocks.len(), 1);
}

#[test]
fn a_cancel_between_the_terrain_step_and_a_later_step_is_cancelled() {
    let (mut job, out) = job("cancel-after-terrain");
    // the terrain step starts and then the cancel is seen before the first block; objects would follow it
    job.request.objects.enabled = true;
    let cancel = CancelToken::new();
    let stopper = cancel.clone();
    let mut progress = |e: &ProgressEvent| {
        if e.step == Step::Terrain && e.done.is_none() {
            stopper.cancel();
        }
    };
    let outcome = generate(&job, &mut progress, &cancel).unwrap();
    assert_eq!(outcome, GenerateOutcome::Cancelled);
    // the cancel is checked before each block, so no block finished
    let partial = checkpoint(&out);
    assert_eq!(partial.blocks.len(), 0);
    assert!(partial.files.is_empty(), "{:?}", partial.files);
    assert!(!out.exists());
}

#[test]
fn a_cancel_on_the_last_event_of_the_last_step_is_cancelled() {
    let (first, _) = job("cancel-last");
    let mut events = 0;
    let mut count = |_: &ProgressEvent| events += 1;
    generate(&first, &mut count, &CancelToken::new()).unwrap();
    // terrain only: the last event before the manifest step is the last terrain block
    let last_before_manifest = events - 1;
    let (second, out) = job("cancel-last-run");
    let cancel = CancelToken::new();
    let stopper = cancel.clone();
    let mut seen = 0;
    let mut progress = |_: &ProgressEvent| {
        seen += 1;
        if seen == last_before_manifest {
            stopper.cancel();
        }
    };
    let outcome = generate(&second, &mut progress, &cancel).unwrap();
    assert_eq!(outcome, GenerateOutcome::Cancelled);
    assert!(
        !out.exists(),
        "no manifest and no output: the run did not succeed"
    );
    assert!(staging(&out).join("checkpoint.json").is_file());
}

#[test]
fn a_checkpoint_carries_no_block_of_a_run_with_other_plugins() {
    let (job, out) = job("changed-inputs");
    done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert!(
        manifest(&out).blocks.len() > 1,
        "several blocks to carry over"
    );
    // the same request, but the plugin file is not the one the first run read
    let world = SyntheticWorld::new(0xB00, "Island")
        .cell(0, 0, Heights::flat(16.0))
        .cell(5, 0, Heights::flat(40.0));
    let plugin = std::path::Path::new(&job.request.data_folder).join("Island.esm");
    std::fs::write(plugin, world_plugin(&[], &world, false)).unwrap();
    let (cancel, mut progress) = cancel_at_first_block();
    let outcome = generate(&job, &mut progress, &cancel).unwrap();
    assert_eq!(outcome, GenerateOutcome::Cancelled);
    assert_eq!(
        checkpoint(&out).blocks.len(),
        1,
        "only the block this run built; the first run's other block was built from other plugins"
    );
}

#[test]
fn a_swap_killed_between_its_renames_is_restored_by_the_next_call() {
    let (job, out) = job("swap-recover");
    done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    let before = manifest(&out);
    // the state a kill between "set the output aside" and "move staging in" leaves
    let mut old = out.file_name().unwrap().to_os_string();
    old.push(".lodwright-old");
    std::fs::rename(&out, out.with_file_name(old)).unwrap();
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "up_to_date");
    assert_eq!(manifest(&out), before);
}

#[test]
fn a_file_the_user_keeps_at_the_top_of_the_output_survives_a_regenerate() {
    let (mut job, out) = job("carry-over");
    done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    std::fs::write(out.join("notes.txt"), b"mine").unwrap();
    job.request.terrain.water = !job.request.terrain.water;
    done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(std::fs::read(out.join("notes.txt")).unwrap(), b"mine");
}

#[test]
fn a_missing_plugin_is_a_typed_error_whose_copy_text_has_no_command_line() {
    let (mut job, _) = job("missing");
    job.request.plugins = vec!["Gone.esm".into()];
    let err = generate(&job, &mut NoProgress, &CancelToken::new()).unwrap_err();
    assert_eq!(err.code, "plugin_not_found");
    assert!(err.summary.contains("Gone.esm"), "{}", err.summary);
    assert!(!err.copy_text().contains("--"), "{}", err.copy_text());
}

#[test]
fn an_output_folder_inside_the_data_folder_is_refused() {
    let (mut job, _) = job("in-data");
    job.request.output_dir = format!("{}/lod", job.request.data_folder);
    let err = generate(&job, &mut NoProgress, &CancelToken::new()).unwrap_err();
    assert_eq!(err.code, "output_in_data");
    assert!(!err.copy_text().contains("--"));
}

#[test]
fn an_output_folder_that_is_a_working_tree_root_is_refused() {
    let (job, out) = job("git-root");
    std::fs::create_dir_all(out.join(".git")).unwrap();
    let err = generate(&job, &mut NoProgress, &CancelToken::new()).unwrap_err();
    assert_eq!(err.code, "output_in_repo");
}

#[test]
fn a_full_output_folder_without_a_manifest_is_refused_unless_overwrite_is_set() {
    let (mut job, out) = job("full");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("other.txt"), b"x").unwrap();
    let err = generate(&job, &mut NoProgress, &CancelToken::new()).unwrap_err();
    assert_eq!(err.code, "output_not_empty");
    job.overwrite = true;
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated");
}

#[test]
fn the_request_round_trips_through_json() {
    let (job, _) = job("json");
    let text = serde_json::to_string(&job).unwrap();
    let back: GenerateRequest = serde_json::from_str(&text).unwrap();
    assert_eq!(back, job);
}

// ---- far models without a LOD model: one warning per plugin, the full list in a file (#345, #346) ----

const VISIBLE_WHEN_DISTANT: u32 = 0x8000;

fn stat(id: u32, model: &str, flags: u32) -> lodwright_testkit::plugin::Record {
    let mut r = lodwright_testkit::plugin::Record::new(*b"STAT", id)
        .with_subrecord(*b"MODL", format!("{model}\0").into_bytes());
    r.flags = flags;
    r
}

fn refr(id: u32, base: u32, x: f32) -> lodwright_testkit::plugin::Record {
    let mut data = Vec::new();
    for v in [x, 0.0, 0.0, 0.0, 0.0, 0.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    lodwright_testkit::plugin::Record::new(*b"REFR", id)
        .with_subrecord(*b"NAME", base.to_le_bytes().to_vec())
        .with_subrecord(*b"DATA", data)
}

/// Two plugins: `Base.esm` (the world, one base flagged visible from far whose full model exists and has no LOD model)
/// and `Mod.esp` (flags two more bases, one with a full model and one with none, and places references to all).
fn far_model_job(name: &str) -> (GenerateRequest, PathBuf) {
    use lodwright_testkit::plugin::{Group, PluginBuilder};
    use lodwright_testkit::world::{record_group, world_group};
    let data = temp(&format!("{name}-data"));
    std::fs::create_dir_all(data.join("meshes/arch")).unwrap();
    // the full models exist, as files whose content is never read (no LOD model exists for any of them)
    for m in ["parking.nif", "kit.nif"] {
        std::fs::write(data.join("meshes/arch").join(m), b"not a model").unwrap();
    }
    let world = SyntheticWorld::new(0xB00, "Island").cell(0, 0, Heights::flat(16.0));
    let base_refs = Group::new(1, 0xB00_u32.to_le_bytes())
        .with(refr(0xC00, 0x100, 10.0))
        .with(refr(0xC01, 0x100, 20.0));
    let base = PluginBuilder::new()
        .group(record_group(
            *b"STAT",
            vec![stat(0x100, "arch\\parking.nif", VISIBLE_WHEN_DISTANT)],
        ))
        .group(world_group(&world, false))
        .group(base_refs)
        .build();
    std::fs::write(data.join("Base.esm"), base).unwrap();
    let mod_refs = Group::new(1, 0xB00_u32.to_le_bytes())
        .with(refr(0x0100_0C00, 0x0100_0100, 30.0))
        .with(refr(0x0100_0C01, 0x0100_0101, 40.0))
        .with(refr(0x0100_0C02, 0x0100_0101, 50.0));
    let modded = PluginBuilder::new()
        .master("Base.esm")
        .group(record_group(
            *b"STAT",
            vec![
                stat(0x0100_0100, "arch\\kit.nif", VISIBLE_WHEN_DISTANT),
                stat(0x0100_0101, "arch\\gone.nif", VISIBLE_WHEN_DISTANT),
            ],
        ))
        .group(mod_refs)
        .build();
    std::fs::write(data.join("Mod.esp"), modded).unwrap();
    let out = temp(&format!("{name}-out"));
    let mut request = Request::new(
        data.to_str().unwrap(),
        &["Base.esm", "Mod.esp"],
        out.to_str().unwrap(),
    );
    request.terrain.enabled = false;
    request.trees.enabled = false;
    request.objects.enabled = true;
    request.limits.threads = Some(1);
    (GenerateRequest::new(request), out)
}

fn warnings_file(out: &std::path::Path) -> lodwright_contract::WarningsFile {
    serde_json::from_slice(&std::fs::read(out.join(lodwright_contract::WARNINGS_FILE)).unwrap())
        .unwrap()
}

#[test]
fn far_models_without_a_lod_model_are_one_warning_per_plugin_and_every_model_is_in_the_file() {
    let (job, out) = far_model_job("far-warn");
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated", "{result:?}");
    // one line for Base.esm (1 model, 2 objects) and two for Mod.esp (kit: found full model; gone: no model at all)
    let lines: Vec<&String> = result
        .warnings
        .iter()
        .filter(|w| w.contains("visible from far"))
        .collect();
    assert_eq!(lines.len(), 3, "{:?}", result.warnings);
    assert!(
        lines
            .iter()
            .any(|w| w.starts_with("Base.esm marks 2 objects of 1 models")
                && w.contains("parking.nif")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|w| w.starts_with("Mod.esp marks 1 objects") && w.contains("kit.nif")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|w| w.starts_with("Mod.esp marks 2 objects") && w.contains("gone.nif")),
        "{lines:?}"
    );
    // the file holds a record per plugin and kind with the models in full, and the result counts the kinds
    assert_eq!(
        result.warnings_file.as_deref(),
        Some(lodwright_contract::WARNINGS_FILE)
    );
    let file = warnings_file(&out);
    assert_eq!(file.contract, lodwright_contract::WARNINGS_CONTRACT);
    let kinds: Vec<(&str, u64)> = result
        .warning_counts
        .iter()
        .map(|c| (c.kind.as_str(), c.count))
        .collect();
    assert_eq!(
        kinds,
        [
            ("far_model_not_found", 1),
            ("far_model_without_lod_model", 2)
        ]
    );
    assert_eq!(file.warnings.len(), 3);
    let base = file
        .warnings
        .iter()
        .find(|w| w.plugin.as_deref() == Some("Base.esm"))
        .unwrap();
    assert_eq!(base.kind, "far_model_without_lod_model");
    assert_eq!(base.subjects, ["meshes/arch/parking.nif"]);
    assert_eq!(base.count, 2);
    assert_ne!(base.advice, "");
    // the file is not a file of the LOD: the manifest does not list it
    assert!(
        !manifest(&out)
            .files
            .iter()
            .any(|f| f.path == lodwright_contract::WARNINGS_FILE)
    );
}
