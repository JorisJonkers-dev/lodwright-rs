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
fn a_second_call_with_the_same_request_is_up_to_date() {
    let (job, _) = job("again");
    generate(&job, &mut NoProgress, &CancelToken::new()).unwrap();
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
