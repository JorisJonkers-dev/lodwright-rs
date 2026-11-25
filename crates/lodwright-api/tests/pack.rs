// SPDX-License-Identifier: GPL-3.0-only
//! `output.pack`: the packing step at the end of a generate.

use lodwright_testkit::{covers, ids};
use std::path::{Path, PathBuf};

use lodwright_api::{
    CancelToken, GenerateOutcome, GenerateRequest, NoProgress, ProgressEvent, Step, generate,
};
use lodwright_assets::{Archive, extract_to_temp};
use lodwright_contract::{MANIFEST_FILE, Manifest, Request, ResultDoc};
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-pack-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A job for a synthetic world named `world`, terrain and textures, into a fresh output folder.
fn job(name: &str, world: &str, pack: bool, threads: u32) -> (GenerateRequest, PathBuf) {
    let data = temp(&format!("{name}-data"));
    std::fs::create_dir_all(&data).unwrap();
    let synthetic = SyntheticWorld::new(0xB00, world)
        .cell(0, 0, Heights::flat(16.0))
        .cell(5, 0, Heights::flat(24.0));
    std::fs::write(
        data.join("Island.esm"),
        world_plugin(&[], &synthetic, false),
    )
    .unwrap();
    let out = temp(&format!("{name}-out"));
    let mut request = Request::new(
        data.to_str().unwrap(),
        &["Island.esm"],
        out.to_str().unwrap(),
    );
    request.objects.enabled = false;
    request.trees.enabled = false;
    request.limits.threads = Some(threads);
    request.output.pack = pack;
    (GenerateRequest::new(request), out)
}

fn run(job: &GenerateRequest) -> ResultDoc {
    match generate(job, &mut NoProgress, &CancelToken::new()).unwrap() {
        GenerateOutcome::Done { result } => *result,
        GenerateOutcome::Cancelled => panic!("cancelled"),
    }
}

fn manifest(out: &Path) -> Manifest {
    serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap()
}

fn names_in(out: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        // the warnings file (when the run had a warning) is beside the files of the LOD, not one of them
        .filter(|n| n != lodwright_contract::WARNINGS_FILE)
        .collect();
    names.sort();
    names
}

#[test]
fn a_packed_run_leaves_the_archive_the_stub_and_the_manifest_and_no_loose_file() {
    covers(&[ids::generate::pack::PACK_SWITCH]);
    let (job, out) = job("only", "Island", true, 1);
    let result = run(&job);
    assert_eq!(result.code, "generated");
    assert_eq!(
        names_in(&out),
        [
            "Lodwright LOD.bsa",
            "Lodwright LOD.esp",
            "Lodwright LOD.override",
            "manifest.json"
        ]
    );
    let listed: Vec<String> = manifest(&out).files.into_iter().map(|f| f.path).collect();
    assert_eq!(
        listed,
        [
            "Lodwright LOD.bsa",
            "Lodwright LOD.esp",
            "Lodwright LOD.override"
        ]
    );
    assert_eq!(
        std::fs::metadata(out.join("Lodwright LOD.override"))
            .unwrap()
            .len(),
        0,
        "the marker is empty"
    );
    assert!(
        !out.with_file_name(format!(
            "{}.lodwright-staging",
            out.file_name().unwrap().to_string_lossy()
        ))
        .exists()
    );
}

#[test]
fn the_archive_holds_what_a_loose_run_writes_and_the_stub_reads_as_an_empty_plugin() {
    covers(&[ids::generate::pack::PACK_SWITCH]);
    let (loose_job, loose_out) = job("same-loose", "Island", false, 1);
    run(&loose_job);
    let (packed_job, packed_out) = job("same-packed", "Island", true, 1);
    run(&packed_job);
    let bytes = std::fs::read(packed_out.join("Lodwright LOD.bsa")).unwrap();
    let extracted = extract_to_temp(&bytes, None, 1 << 28).unwrap();
    let loose = manifest(&loose_out);
    let mut expected: Vec<String> = loose
        .files
        .iter()
        .map(|f| f.path.to_ascii_lowercase())
        .collect();
    expected.sort();
    let mut got = extracted.files().to_vec();
    got.sort();
    assert_eq!(got, expected);
    for file in &loose.files {
        assert_eq!(
            std::fs::read(extracted.path().join(file.path.to_ascii_lowercase())).unwrap(),
            std::fs::read(loose_out.join(&file.path)).unwrap(),
            "{}",
            file.path
        );
    }
    assert!(
        !extracted
            .files()
            .iter()
            .any(|f| f.contains("manifest.json"))
    );
    assert_eq!(
        lodwright_gen::verify_folder(extracted.path()),
        lodwright_gen::verify_folder(&loose_out)
    );
    let header = lodwright_plugin_meta::read_header(
        "Lodwright LOD.esp",
        &std::fs::read(packed_out.join("Lodwright LOD.esp")).unwrap(),
    )
    .unwrap();
    // the game opens a plugin's archives only when the plugin has a master: the stub names the first plugin
    assert_eq!(header.masters, ["Island.esm"]);
    assert!(!header.is_master);
    let stub = std::fs::read(packed_out.join("Lodwright LOD.esp")).unwrap();
    let size = std::fs::metadata(Path::new(&packed_job.request.data_folder).join("Island.esm"))
        .unwrap()
        .len();
    assert_eq!(
        stub[stub.len() - 8..],
        size.to_le_bytes(),
        "the stored master size"
    );
    assert!(Archive::open(&mut std::io::Cursor::new(&bytes)).is_ok());
}

#[test]
fn the_switch_changes_neither_the_request_digest_nor_a_block() {
    covers(&[ids::generate::pack::PACK_NOT_IN_DIGESTS]);
    let (loose_job, out) = job("toggle", "Island", false, 1);
    let loose = run(&loose_job);
    let built = loose.blocks.unwrap();
    assert_ne!(built.generated, 0);
    // the same request with the switch on: same digest, every block kept from the loose run, then packed
    let mut packed_job = loose_job.clone();
    packed_job.request.output.pack = true;
    assert_eq!(packed_job.request.digest(), loose_job.request.digest());
    let packed = run(&packed_job);
    assert_eq!(packed.code, "generated");
    assert_eq!(packed.arguments_digest, loose.arguments_digest);
    let counts = packed.blocks.unwrap();
    assert_eq!((counts.generated, counts.up_to_date), (0, built.generated));
    assert_eq!(
        names_in(&out),
        [
            "Lodwright LOD.bsa",
            "Lodwright LOD.esp",
            "Lodwright LOD.override",
            "manifest.json"
        ]
    );
}

#[test]
fn a_packed_output_is_up_to_date_and_a_changed_archive_is_rebuilt() {
    covers(&[ids::generate::pack::PACKED_OUTPUT_UP_TO_DATE]);
    let (job, out) = job("uptodate", "Island", true, 1);
    run(&job);
    assert_eq!(run(&job).code, "up_to_date");
    let archive = out.join("Lodwright LOD.bsa");
    let mut bytes = std::fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&archive, bytes).unwrap();
    assert_eq!(run(&job).code, "generated");
    assert_eq!(run(&job).code, "up_to_date");
}

#[test]
fn switching_the_option_off_rebuilds_the_loose_files_and_the_output_never_holds_both() {
    covers(&[ids::generate::pack::PACK_OFF_GOES_LOOSE]);
    let (job, out) = job("off", "Island", true, 1);
    run(&job);
    let mut loose_job = job.clone();
    loose_job.request.output.pack = false;
    let result = run(&loose_job);
    assert_eq!(result.code, "generated");
    let names = names_in(&out);
    assert!(
        names.iter().all(|n| !n.starts_with("Lodwright LOD")),
        "{names:?}"
    );
    assert!(manifest(&out).files.iter().all(|f| f.path.contains('/')));
    assert_eq!(run(&loose_job).code, "up_to_date");
}

#[test]
fn a_lost_override_marker_is_written_again_and_going_loose_removes_it() {
    covers(&[ids::generate::pack::PACK_OFF_GOES_LOOSE]);
    let (job, out) = job("marker", "Island", true, 1);
    run(&job);
    let marker = out.join("Lodwright LOD.override");
    std::fs::remove_file(&marker).unwrap();
    assert_eq!(
        run(&job).code,
        "generated",
        "a packed output without its marker is not up to date"
    );
    assert_eq!(std::fs::metadata(&marker).unwrap().len(), 0);
    assert_eq!(run(&job).code, "up_to_date");
    let mut loose_job = job.clone();
    loose_job.request.output.pack = false;
    run(&loose_job);
    assert!(!marker.exists(), "the marker goes with the packed form");
}

#[test]
fn a_packing_step_that_fails_leaves_the_loose_output_as_it_was() {
    covers(&[ids::generate::pack::PACK_FAILURE_IS_SAFE]);
    let (loose_job, out) = job("fail", "Island", false, 1);
    run(&loose_job);
    let before = (
        names_in(&out),
        std::fs::read(out.join(MANIFEST_FILE)).unwrap(),
    );
    let mut packed_job = loose_job.clone();
    packed_job.request.output.pack = true;
    // when the pack step starts, one generated file disappears from the staging folder, so reading it fails
    let mut stage = out.file_name().unwrap().to_os_string();
    stage.push(".lodwright-staging");
    let stage = out.with_file_name(stage);
    let mut sabotage = |e: &ProgressEvent| {
        if e.step == Step::Pack && e.done.is_none() {
            let nif = lodwright_gen::verify_folder(&stage);
            assert_ne!(
                nif.terrain_blocks, 0,
                "the staging folder holds the generated files"
            );
            std::fs::remove_file(stage.join("meshes/landscape/lod/island/island.level4.x0.y0.nif"))
                .unwrap();
        }
    };
    let error = generate(&packed_job, &mut sabotage, &CancelToken::new()).unwrap_err();
    assert_eq!(error.code, "output_failed");
    assert_eq!(
        (
            names_in(&out),
            std::fs::read(out.join(MANIFEST_FILE)).unwrap()
        ),
        before
    );
    assert_eq!(lodwright_gen::verify_folder(&out).errors.len(), 0);
    // the next run (no sabotage) packs and finishes
    assert_eq!(run(&packed_job).code, "generated");
    assert_eq!(
        names_in(&out),
        [
            "Lodwright LOD.bsa",
            "Lodwright LOD.esp",
            "Lodwright LOD.override",
            "manifest.json"
        ]
    );
}

#[test]
fn the_archive_is_the_same_bytes_for_any_thread_count() {
    covers(&[ids::generate::pack::PACK_SAME_BYTES_AT_ANY_THREAD_COUNT]);
    let (one, out_one) = job("threads-1", "Island", true, 1);
    let (four, out_four) = job("threads-4", "Island", true, 4);
    run(&one);
    run(&four);
    assert_eq!(
        std::fs::read(out_one.join("Lodwright LOD.bsa")).unwrap(),
        std::fs::read(out_four.join("Lodwright LOD.bsa")).unwrap()
    );
    assert_eq!(
        std::fs::read(out_one.join("Lodwright LOD.esp")).unwrap(),
        std::fs::read(out_four.join("Lodwright LOD.esp")).unwrap()
    );
}

#[test]
fn the_pack_phase_sits_between_the_worlds_and_the_manifest_and_the_run_total_never_goes_back() {
    covers(&[ids::generate::pack::PACK_PROGRESS_PHASE]);
    let (job, _) = job("progress", "Island", true, 1);
    let mut events: Vec<ProgressEvent> = Vec::new();
    let mut progress = |e: &ProgressEvent| events.push(e.clone());
    generate(&job, &mut progress, &CancelToken::new()).unwrap();
    let steps: Vec<Step> = events.iter().map(|e| e.step).collect();
    let first_pack = steps.iter().position(|s| *s == Step::Pack).unwrap();
    assert!(steps[..first_pack].iter().all(|s| *s != Step::Manifest));
    assert!(
        steps[first_pack..]
            .iter()
            .skip_while(|s| **s == Step::Pack)
            .all(|s| *s == Step::Manifest)
    );
    assert_eq!(steps.last(), Some(&Step::Manifest));
    assert!(events.iter().all(|e| e.run_total == Some(10_000)));
    let done: Vec<u64> = events.iter().map(|e| e.run_done.unwrap()).collect();
    assert!(done.windows(2).all(|w| w[0] <= w[1]), "{done:?}");
    // read, terrain, pack, manifest
    assert!(events[1..].iter().all(|e| e.phase_count == Some(4)));
    let pack: Vec<&ProgressEvent> = events.iter().filter(|e| e.step == Step::Pack).collect();
    let counted = pack.iter().rfind(|e| e.done.is_some()).unwrap();
    assert_eq!(counted.done, counted.total);
    assert_eq!(counted.phase_done, counted.phase_total);
    // the pack events have no events-file line (they are not blocks)
    assert!(pack.iter().all(|e| e.block_line().is_none()));
    assert_eq!(pack[0].label_key(), "lod.pack");
}
