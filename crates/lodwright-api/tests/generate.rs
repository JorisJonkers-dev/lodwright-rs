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
}

#[test]
fn a_token_cancelled_before_the_call_writes_nothing() {
    let (job, out) = job("cancel-first");
    let cancel = CancelToken::new();
    cancel.cancel();
    let outcome = generate(&job, &mut NoProgress, &cancel).unwrap();
    assert_eq!(outcome, GenerateOutcome::Cancelled);
    assert!(!out.exists());
}

#[test]
fn a_cancel_after_the_first_block_leaves_a_manifest_the_next_call_resumes_from() {
    let (job, out) = job("resume");
    let cancel = CancelToken::new();
    let stopper = cancel.clone();
    let mut progress = |e: &ProgressEvent| {
        if e.done.is_some() {
            stopper.cancel();
        }
    };
    let outcome = generate(&job, &mut progress, &cancel).unwrap();
    assert_eq!(outcome, GenerateOutcome::Cancelled);
    let partial = manifest(&out);
    assert_eq!(partial.blocks.len(), 1, "only the first block finished");
    assert_ne!(partial.request_digest, job.request.digest());
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
}

#[test]
fn a_missing_plugin_is_a_typed_error_whose_copy_text_has_no_command_line() {
    let (mut job, _) = job("missing");
    job.request.plugins = vec!["Gone.esm".into()];
    let err = generate(&job, &mut NoProgress, &CancelToken::new()).unwrap_err();
    assert_eq!(err.code, "plugin_not_found");
    let text = err.copy_text();
    assert!(text.contains("plugin_not_found"), "{text}");
    assert!(!text.contains("--"), "{text}");
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
