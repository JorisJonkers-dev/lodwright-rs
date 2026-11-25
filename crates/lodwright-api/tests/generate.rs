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
fn an_output_written_by_another_build_of_the_same_version_is_rebuilt() {
    let (job, out) = job("other-build");
    generate(&job, &mut NoProgress, &CancelToken::new()).unwrap();
    let mut m = manifest(&out);
    assert_eq!(
        m.build.as_deref(),
        Some(lodwright_api::BUILD),
        "the manifest names its build"
    );
    // the same version number, another build (#422)
    m.build = Some(format!("{}+0000000000000000", m.tool.version));
    std::fs::write(out.join(MANIFEST_FILE), serde_json::to_vec(&m).unwrap()).unwrap();
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated");
    assert_eq!(manifest(&out).build.as_deref(), Some(lodwright_api::BUILD));
    // a manifest from before builds were named counts as another build too
    let mut m = manifest(&out);
    m.build = None;
    std::fs::write(out.join(MANIFEST_FILE), serde_json::to_vec(&m).unwrap()).unwrap();
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated");
    // and the same build with the same request is still up to date
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "up_to_date");
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
fn shipped_shape_rules_act_only_in_the_world_they_name() {
    let kinds = |name: &str, rule_world: Option<&str>| {
        let (mut job, out) = far_model_job(name);
        if let Some(world) = rule_world {
            job.request.objects.shapes_from_shipped_blocks =
                vec![lodwright_contract::ShippedShapeRule {
                    world: world.to_owned(),
                    models: vec!["arch/parking.nif".to_owned()],
                }];
        }
        let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
        assert_eq!(result.code, "generated", "{result:?}");
        warnings_file(&out)
            .warnings
            .iter()
            .filter(|w| w.kind.starts_with("own_far"))
            .map(|w| (w.kind.clone(), w.count))
            .collect::<Vec<_>>()
    };
    assert_eq!(kinds("shipped-none", None), vec![]);
    assert_eq!(kinds("shipped-other", Some("NoSuchWorld")), vec![]);
    assert_eq!(
        kinds("shipped-named", Some("ISLAND")),
        vec![("own_far_blocks_not_found".to_owned(), 2)]
    );
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

// ---- include rules: objects nothing marks visible from far, taken by the request's rules (#385) ----

/// `Base.esm` plus `Lamps.esp`, which adds a lamp nothing marks visible from far (full model, no LOD model) and
/// places it five times: enabled, initially disabled, persistent and disabled, with an enable parent, and enabled.
fn include_job(name: &str) -> (GenerateRequest, PathBuf) {
    // the parent is the disabled lamp, so the parented lamp is disabled too
    include_job_with_parent(name, 0x0100_0D01, 0)
}

/// The lamps of [`include_job`], the fourth lamp's enable parent being `parent` with the XESP flags word `flags`
/// (bit 0: the opposite of the parent's state).
fn include_job_with_parent(name: &str, parent: u32, flags: u32) -> (GenerateRequest, PathBuf) {
    lamps_job(name, parent, flags, 0)
}

/// The lamps of [`include_job_with_parent`], the lamp's base carrying the record flags `base_flags`.
fn lamps_job(name: &str, parent: u32, flags: u32, base_flags: u32) -> (GenerateRequest, PathBuf) {
    use lodwright_testkit::plugin::{Group, PluginBuilder};
    use lodwright_testkit::world::record_group;
    const INITIALLY_DISABLED: u32 = 0x0800;
    const PERSISTENT: u32 = 0x0400;
    let (mut job, out) = far_model_job(name);
    let data = PathBuf::from(&job.request.data_folder);
    std::fs::create_dir_all(data.join("meshes/lights")).unwrap();
    std::fs::write(data.join("meshes/lights/streetlamp.nif"), b"not a model").unwrap();
    let flagged = |id: u32, x: f32, flags: u32| {
        let mut r = refr(id, 0x0100_0200, x);
        r.flags = flags;
        r
    };
    // XESP: the parent's form id and a flags word
    let parent = [parent.to_le_bytes(), flags.to_le_bytes()].concat();
    let refs = Group::new(1, 0xB00_u32.to_le_bytes())
        .with(flagged(0x0100_0D00, 60.0, 0))
        .with(flagged(0x0100_0D01, 70.0, INITIALLY_DISABLED))
        .with(flagged(0x0100_0D02, 80.0, INITIALLY_DISABLED | PERSISTENT))
        .with(flagged(0x0100_0D03, 90.0, 0).with_subrecord(*b"XESP", parent))
        .with(flagged(0x0100_0D04, 100.0, 0));
    let lamps = PluginBuilder::new()
        .master("Base.esm")
        .group(record_group(
            *b"STAT",
            vec![stat(0x0100_0200, "lights\\StreetLamp.nif", base_flags)],
        ))
        .group(refs)
        .build();
    std::fs::write(data.join("Lamps.esp"), lamps).unwrap();
    job.request.plugins.push("Lamps.esp".to_owned());
    (job, out)
}

fn rule(models: &str, plugin: Option<&str>) -> lodwright_contract::IncludeRule {
    lodwright_contract::IncludeRule {
        models: models.to_owned(),
        plugin: plugin.map(str::to_owned),
        record_type: Some("STAT".to_owned()),
        generate: None,
    }
}

/// A full model that the simplifier can read: one untextured triangle, 100 units on a side.
fn triangle_model() -> Vec<u8> {
    use lodwright_nif::{
        AvObject, Block, Body, ExportInfo, GeometryData, Header, NiNode, NiTriShape,
        NiTriShapeData, NifFile, ObjectNet,
    };
    let av = || AvObject {
        net: ObjectNet {
            name: -1,
            extra_data: vec![],
            controller: -1,
        },
        flags: 14,
        translation: [0.0; 3],
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        properties: vec![],
        collision_object: -1,
    };
    NifFile {
        header: Header {
            user_version: 11,
            bethesda: Some((
                34,
                ExportInfo {
                    author: vec![0],
                    unknown_int: 0,
                    process_script: vec![0],
                    export_script: vec![0],
                    max_filepath: vec![],
                },
            )),
            block_types: vec![
                b"NiNode".to_vec(),
                b"NiTriShape".to_vec(),
                b"NiTriShapeData".to_vec(),
            ],
            strings: vec![],
            groups: vec![],
        },
        blocks: vec![
            Block {
                type_index: 0,
                body: Body::Node(NiNode {
                    av: av(),
                    children: vec![1],
                    effects: vec![],
                }),
            },
            Block {
                type_index: 1,
                body: Body::TriShape(NiTriShape {
                    av: av(),
                    data: 2,
                    skin_instance: -1,
                    material_names: vec![],
                    material_extra_data: vec![],
                    active_material: 0,
                    material_needs_update: 0,
                    shader_property: -1,
                    alpha_property: -1,
                }),
            },
            Block {
                type_index: 2,
                body: Body::TriShapeData(NiTriShapeData {
                    geometry: GeometryData {
                        group_id: 0,
                        num_vertices: 3,
                        keep_flags: 0,
                        compress_flags: 0,
                        vertices: Some(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]]),
                        vector_flags: 1,
                        material_crc: 0,
                        normals: None,
                        tangents: None,
                        center: [0.0; 3],
                        radius: 0.0,
                        vertex_colors: None,
                        uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
                        consistency: 0,
                        additional_data: -1,
                    },
                    num_triangles: 1,
                    num_triangle_points: 3,
                    triangles: Some(vec![[0, 1, 2]]),
                    match_groups: vec![],
                }),
            },
        ],
        roots: vec![0],
    }
    .write()
    .unwrap()
}

/// The include job with readable full models for the lamp and for `Base.esm`'s parking model (marked visible from
/// far, no LOD model either), so a far shape can be made of both.
fn include_job_with_models(name: &str) -> (GenerateRequest, PathBuf) {
    let (job, out) = include_job(name);
    let data = PathBuf::from(&job.request.data_folder);
    for m in ["lights/streetlamp.nif", "arch/parking.nif"] {
        std::fs::write(data.join("meshes").join(m), triangle_model()).unwrap();
    }
    (job, out)
}

/// (kind, plugin, count, subjects) of the far-model records of a run.
fn far_records(out: &std::path::Path) -> Vec<(String, Option<String>, u64, Vec<String>)> {
    warnings_file(out)
        .warnings
        .into_iter()
        .filter(|w| w.kind.starts_with("far_model"))
        .map(|w| (w.kind, w.plugin, w.count, w.subjects))
        .collect()
}

#[test]
fn a_rule_with_generate_gives_far_shapes_to_its_objects_only() {
    let shapes = lodwright_contract::FullModelShapes {
        keep: 1.0,
        max_error: 0.0,
    };
    let (mut job, out) = include_job_with_models("include-generate");
    let mut lamps = rule("lights/street*", Some("lamps.esp"));
    lamps.generate = Some(shapes);
    job.request.inclusion.include_rules = vec![lamps];
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated", "{result:?}");
    let records = far_records(&out);
    // the lamp is drawn from its full model; the parking model, which only a far-view flag names, stays out as before
    assert!(
        records.contains(&(
            "far_model_simplified".to_owned(),
            None,
            1,
            vec!["meshes/lights/streetlamp.nif".to_owned()]
        )),
        "{records:?}"
    );
    assert!(
        records.contains(&(
            "far_model_without_lod_model".to_owned(),
            Some("Base.esm".to_owned()),
            2,
            vec!["meshes/arch/parking.nif".to_owned()]
        )),
        "{records:?}"
    );
    assert!(
        !records
            .iter()
            .any(|r| r.0 == "far_model_simplified" && r.3.iter().any(|s| s.contains("parking"))),
        "{records:?}"
    );
    // the same rule without generate draws nothing new, and the switch for every object draws both
    let (mut plain, plain_out) = include_job_with_models("include-generate-plain");
    plain.request.inclusion.include_rules = vec![rule("lights/street*", Some("lamps.esp"))];
    done(generate(&plain, &mut NoProgress, &CancelToken::new()).unwrap());
    assert!(
        !far_records(&plain_out)
            .iter()
            .any(|r| r.0 == "far_model_simplified"),
        "{:?}",
        far_records(&plain_out)
    );
    let (mut every, every_out) = include_job_with_models("include-generate-every");
    every.request.inclusion.include_rules = vec![rule("lights/street*", Some("lamps.esp"))];
    every.request.objects.shapes_from_full_models = Some(shapes);
    done(generate(&every, &mut NoProgress, &CancelToken::new()).unwrap());
    let simplified: Vec<Vec<String>> = far_records(&every_out)
        .into_iter()
        .filter(|r| r.0 == "far_model_simplified")
        .map(|r| r.3)
        .collect();
    assert_eq!(
        simplified,
        [vec![
            "meshes/arch/parking.nif".to_owned(),
            "meshes/lights/streetlamp.nif".to_owned()
        ]]
    );
}

/// The lamp lines of a run: (kind, plugin, count) of the records that name the lamp model or the include rules.
fn lamp_records(out: &std::path::Path) -> Vec<(String, Option<String>, u64)> {
    warnings_file(out)
        .warnings
        .into_iter()
        .filter(|w| {
            w.kind.starts_with("include_rule")
                || w.subjects.iter().any(|s| s.contains("streetlamp"))
        })
        .map(|w| (w.kind, w.plugin, w.count))
        .collect()
}

#[test]
fn without_include_rules_an_object_nothing_marks_visible_from_far_stays_out_unreported() {
    let (job, out) = include_job("include-none");
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated", "{result:?}");
    assert_eq!(lamp_records(&out), vec![]);
}

#[test]
fn an_include_rule_takes_the_enabled_references_and_reports_the_held_back_ones() {
    let (mut job, out) = include_job("include-rule");
    job.request.inclusion.include_rules = vec![rule("lights/street*", Some("lamps.esp"))];
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated", "{result:?}");
    // the two enabled lamps reach the far-model step (which reports them: they have no LOD model); the disabled one,
    // the persistent disabled one and the one its disabled enable parent keeps out are held back and counted
    assert_eq!(
        lamp_records(&out),
        vec![
            (
                "far_model_without_lod_model".to_owned(),
                Some("Lamps.esp".to_owned()),
                2
            ),
            ("include_rule_reference_not_enabled".to_owned(), None, 3),
        ]
    );
}

/// An include rule run over [`include_job_with_parent`]: the lamp lines it reports.
fn include_rule_records_with_parent(
    name: &str,
    parent: u32,
    flags: u32,
) -> Vec<(String, Option<String>, u64)> {
    let (mut job, out) = include_job_with_parent(name, parent, flags);
    job.request.inclusion.include_rules = vec![rule("lights/street*", Some("lamps.esp"))];
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated", "{result:?}");
    lamp_records(&out)
}

#[test]
fn an_include_rule_takes_a_reference_whose_enable_parent_stands_at_the_start() {
    // the parent is the enabled lamp: the parented lamp stands too, so three lamps are taken and two held back
    assert_eq!(
        include_rule_records_with_parent("include-parent-enabled", 0x0100_0D00, 0),
        vec![
            (
                "far_model_without_lod_model".to_owned(),
                Some("Lamps.esp".to_owned()),
                3
            ),
            ("include_rule_reference_not_enabled".to_owned(), None, 2),
        ]
    );
}

#[test]
fn an_include_rule_takes_a_reference_set_opposite_to_its_disabled_enable_parent() {
    // the parent is the disabled lamp and the opposite flag is set: the parented lamp stands at the start
    assert_eq!(
        include_rule_records_with_parent("include-parent-opposite", 0x0100_0D01, 1),
        vec![
            (
                "far_model_without_lod_model".to_owned(),
                Some("Lamps.esp".to_owned()),
                3
            ),
            ("include_rule_reference_not_enabled".to_owned(), None, 2),
        ]
    );
}

#[test]
fn an_exclusion_rule_beats_an_include_rule_and_a_rule_for_another_plugin_takes_nothing() {
    let (mut job, out) = include_job("include-excluded");
    job.request.inclusion.include_rules = vec![rule("lights/*", None)];
    job.request.inclusion.exclude_models = vec!["lights/streetlamp.nif".to_owned()];
    done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(
        lamp_records(&out),
        vec![("include_rule_reference_not_enabled".to_owned(), None, 3)]
    );
    let (mut job, out) = include_job("include-other-plugin");
    job.request.inclusion.include_rules = vec![rule("lights/*", Some("Mod.esp"))];
    done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(lamp_records(&out), vec![]);
}

#[test]
fn an_include_rule_for_a_plugin_outside_the_load_order_is_a_request_error() {
    let (mut job, _) = include_job("include-bad");
    job.request.inclusion.include_rules = vec![rule("lights/*", Some("Gone.esp"))];
    let err = generate(&job, &mut NoProgress, &CancelToken::new()).unwrap_err();
    assert!(format!("{err:?}").contains("Gone.esp"), "{err:?}");
}

#[test]
fn include_rules_give_the_same_bytes_on_every_run() {
    let run = |name: &str| {
        let (mut job, out) = include_job(name);
        job.request.inclusion.include_rules = vec![rule("lights/*", None)];
        done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
        output_bytes(&out)
    };
    let first = run("include-det-a");
    assert_ne!(first.len(), 0);
    assert_eq!(run("include-det-b"), first);
}

/// Every file of `out` except the manifest (whose request digest covers the thread limit), with its bytes.
fn output_bytes(out: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    files_on_disk(out)
        .into_iter()
        .filter(|f| f != MANIFEST_FILE)
        .map(|f| {
            let bytes = std::fs::read(out.join(&f)).unwrap();
            (f, bytes)
        })
        .collect()
}

#[test]
fn the_output_files_are_the_same_bytes_at_any_thread_count() {
    let mut runs = Vec::new();
    for threads in [1u32, 4, 16] {
        let (mut terrain, out_t) = job(&format!("det-t{threads}"));
        terrain.request.limits.threads = Some(threads);
        done(generate(&terrain, &mut NoProgress, &CancelToken::new()).unwrap());
        let (mut far, out_f) = far_model_job(&format!("det-f{threads}"));
        far.request.limits.threads = Some(threads);
        done(generate(&far, &mut NoProgress, &CancelToken::new()).unwrap());
        runs.push((output_bytes(&out_t), output_bytes(&out_f)));
    }
    assert!(!runs[0].0.is_empty() && !runs[0].1.is_empty());
    for run in &runs[1..] {
        assert_eq!(
            run.0, runs[0].0,
            "terrain output differs with the thread count"
        );
        assert_eq!(
            run.1, runs[0].1,
            "object output differs with the thread count"
        );
    }
}

// ---- objects.far_lights: small light points beside far objects, one marked shape per block (#339) ----

/// [`far_model_job`] with a far model for `Base.esm`'s parking model, plus `Lights.esp`: one steady light of radius
/// 512 beside the parking objects, one 5000 units away from every object and one too small.
fn far_lights_job(name: &str) -> (GenerateRequest, PathBuf) {
    use lodwright_testkit::plugin::{Group, PluginBuilder, Record};
    use lodwright_testkit::world::record_group;
    let (mut job, out) = far_model_job(name);
    let data = PathBuf::from(&job.request.data_folder);
    std::fs::write(data.join("meshes/arch/parking_lod.nif"), triangle_model()).unwrap();
    let light = |id: u32, radius: u32| {
        let mut d = Vec::new();
        d.extend_from_slice(&0_i32.to_le_bytes()); // time
        d.extend_from_slice(&radius.to_le_bytes());
        d.extend_from_slice(&[255, 200, 100, 0]); // colour
        d.extend_from_slice(&0_u32.to_le_bytes()); // flags: steady
        for v in [1.0_f32, 90.0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&0_u32.to_le_bytes());
        d.extend_from_slice(&0.0_f32.to_le_bytes());
        Record::new(*b"LIGH", id).with_subrecord(*b"DATA", d)
    };
    let refs = Group::new(1, 0xB00_u32.to_le_bytes())
        .with(refr(0x0100_0E00, 0x0100_0300, 60.0))
        .with(refr(0x0100_0E01, 0x0100_0300, 4000.0))
        .with(refr(0x0100_0E02, 0x0100_0301, 70.0));
    let lights = PluginBuilder::new()
        .master("Base.esm")
        .group(record_group(
            *b"LIGH",
            vec![light(0x0100_0300, 512), light(0x0100_0301, 64)],
        ))
        .group(refs)
        .build();
    std::fs::write(data.join("Lights.esp"), lights).unwrap();
    job.request.plugins.push("Lights.esp".to_owned());
    (job, out)
}

/// The output's files under `meshes` and `textures`, lower case with `/`.
fn lod_files(out: &std::path::Path) -> Vec<String> {
    files_on_disk(out)
        .into_iter()
        .map(|f| f.to_ascii_lowercase())
        .filter(|f| f.starts_with("meshes/") || f.starts_with("textures/"))
        .collect()
}

#[test]
fn far_lights_write_one_marked_light_shape_per_block_and_the_light_texture_only_with_the_option() {
    let (mut on, on_out) = far_lights_job("far-lights-on");
    on.request.objects.far_lights = true;
    let result = done(generate(&on, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated", "{result:?}");
    let files = lod_files(&on_out);
    assert!(
        files.iter().any(|f| f.ends_with("island.farlights.dds")),
        "{files:?}"
    );
    let block = files
        .iter()
        .find(|f| f.starts_with("meshes/") && f.rsplit('.').next() == Some("nif"))
        .unwrap();
    let nif = lodwright_nif::NifFile::read(&std::fs::read(on_out.join(block)).unwrap()).unwrap();
    assert_eq!(
        nif.header.strings,
        vec![lodwright_nif::FAR_LIGHTS_MARKER.as_bytes().to_vec()]
    );
    // the one light beside the objects: three crossed squares, 12 vertices; the far one and the small one are left out
    let light_data = nif
        .blocks
        .iter()
        .filter_map(|b| match &b.body {
            lodwright_nif::Body::TriShapeData(d) if d.geometry.vertex_colors.is_some() => {
                Some(d.geometry.num_vertices)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(light_data, vec![12]);

    let (off, off_out) = far_lights_job("far-lights-off");
    done(generate(&off, &mut NoProgress, &CancelToken::new()).unwrap());
    let files = lod_files(&off_out);
    assert!(!files.iter().any(|f| f.contains("farlights")), "{files:?}");
    let block = files
        .iter()
        .find(|f| f.starts_with("meshes/") && f.rsplit('.').next() == Some("nif"))
        .unwrap();
    let nif = lodwright_nif::NifFile::read(&std::fs::read(off_out.join(block)).unwrap()).unwrap();
    assert_eq!(nif.header.strings, Vec::<Vec<u8>>::new());
    assert_eq!(nif.blocks.len(), 7);
}

// ---- objects.far_model_without_flag: references nothing marks visible from far, taken for their far model (#426) ----

/// The lamps of [`lamps_job`] with a far model next to the lamp's full model. The far model does not read as a model,
/// so every lamp generation takes is counted in one `far_model_unreadable` record.
fn far_model_lamps(name: &str, base_flags: u32, on: bool) -> Vec<(String, Option<String>, u64)> {
    let (mut job, out) = lamps_job(name, 0x0100_0D01, 0, base_flags);
    let data = PathBuf::from(&job.request.data_folder);
    std::fs::write(
        data.join("meshes/lights/streetlamp_lod.nif"),
        b"not a model",
    )
    .unwrap();
    job.request.objects.far_model_without_flag = on;
    let result = done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    assert_eq!(result.code, "generated", "{result:?}");
    lamp_records(&out)
}

#[test]
fn far_model_without_flag_takes_the_lamps_that_stand_at_the_start_and_only_with_the_option() {
    // of the five lamps two stand at the start: the initially disabled one, the persistent disabled one and the one
    // its disabled enable parent keeps out stay out
    assert_eq!(
        far_model_lamps("far-model-on", 0, true),
        vec![(
            "far_model_unreadable".to_owned(),
            Some("Lamps.esp".to_owned()),
            2
        )]
    );
    assert_eq!(far_model_lamps("far-model-off", 0, false), vec![]);
}

#[test]
fn far_model_without_flag_leaves_out_a_base_drawn_as_a_tree_billboard() {
    const HAS_TREE_LOD: u32 = 0x40;
    assert_eq!(
        far_model_lamps("far-model-tree", HAS_TREE_LOD, true),
        vec![]
    );
}

#[test]
fn far_model_without_flag_does_not_take_a_reference_twice_or_one_without_a_far_model() {
    // the far-view flag already takes the lamps; the option adds no second copy of them. The parking model of
    // Base.esm has no far model and stays reported as before.
    let (mut job, out) = lamps_job("far-model-flagged", 0x0100_0D01, 0, VISIBLE_WHEN_DISTANT);
    let data = PathBuf::from(&job.request.data_folder);
    std::fs::write(
        data.join("meshes/lights/streetlamp_lod.nif"),
        b"not a model",
    )
    .unwrap();
    job.request.objects.far_model_without_flag = true;
    done(generate(&job, &mut NoProgress, &CancelToken::new()).unwrap());
    let records = far_records(&out);
    assert!(
        records.contains(&(
            "far_model_unreadable".to_owned(),
            Some("Lamps.esp".to_owned()),
            3,
            vec!["meshes/lights/streetlamp.nif".to_owned()]
        )),
        "{records:?}"
    );
    assert!(
        records.contains(&(
            "far_model_without_lod_model".to_owned(),
            Some("Base.esm".to_owned()),
            2,
            vec!["meshes/arch/parking.nif".to_owned()]
        )),
        "{records:?}"
    );
}

#[test]
fn far_model_without_flag_off_is_not_written_and_keeps_the_request_digest() {
    let (job, _) = far_model_job("far-model-digest");
    let mut on = job.request.clone();
    on.objects.far_model_without_flag = true;
    let text = serde_json::to_string(&job.request).unwrap();
    assert!(!text.contains("far_model_without_flag"), "{text}");
    let back: Request = serde_json::from_str(&text).unwrap();
    assert_eq!(back.digest(), job.request.digest());
    assert_ne!(on.digest(), job.request.digest());
}
