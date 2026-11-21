// SPDX-License-Identifier: GPL-3.0-only
//! A generated folder packed into an archive, extracted again and verified like loose output.

use std::path::{Path, PathBuf};

use lodwright_api::{CancelToken, GenerateOutcome, GenerateRequest, NoProgress, generate};
use lodwright_assets::{PackEntry, extract_to_temp, pack};
use lodwright_contract::{MANIFEST_FILE, Manifest, Request};
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-packed-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A synthetic world two level-4 blocks wide, generated with terrain and its textures into a fresh folder.
fn generated(name: &str) -> (PathBuf, Manifest) {
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
    request.limits.threads = Some(1);
    let job = GenerateRequest::new(request);
    match generate(&job, &mut NoProgress, &CancelToken::new()).unwrap() {
        GenerateOutcome::Done { .. } => {}
        GenerateOutcome::Cancelled => panic!("cancelled"),
    }
    let manifest =
        serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap();
    (out, manifest)
}

/// Every file the manifest lists (never the manifest itself), terrain uncompressed and textures compressed.
fn entries(out: &Path, manifest: &Manifest) -> Vec<PackEntry> {
    manifest
        .files
        .iter()
        .map(|file| PackEntry {
            path: file.path.clone(),
            data: std::fs::read(out.join(&file.path)).unwrap(),
            compress: Path::new(&file.path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dds")),
        })
        .collect()
}

#[test]
fn an_extracted_archive_of_a_generated_folder_is_the_same_files_and_verifies_clean() {
    let (out, manifest) = generated("clean");
    assert!(manifest.files.len() > 1, "the world has files");
    let loose = lodwright_gen::verify_folder(&out);
    assert!(loose.errors.is_empty(), "{:?}", loose.errors);
    assert!(loose.terrain_blocks > 0 && loose.textures > 0, "{loose:?}");

    let archive = pack::write(&entries(&out, &manifest)).unwrap();
    let extracted = extract_to_temp(&archive, None, 1 << 28).unwrap();

    let mut expected: Vec<String> = manifest
        .files
        .iter()
        .map(|f| f.path.to_ascii_lowercase())
        .collect();
    expected.sort();
    let mut got = extracted.files().to_vec();
    got.sort();
    assert_eq!(
        got, expected,
        "the archive holds what the manifest lists and nothing else"
    );
    for file in &manifest.files {
        assert_eq!(
            std::fs::read(extracted.path().join(file.path.to_ascii_lowercase())).unwrap(),
            std::fs::read(out.join(&file.path)).unwrap(),
            "{}",
            file.path
        );
    }
    assert_eq!(lodwright_gen::verify_folder(extracted.path()), loose);
}

#[test]
fn verify_of_the_extracted_archive_finds_a_block_that_was_damaged_before_packing() {
    let (out, manifest) = generated("damaged");
    let mut files = entries(&out, &manifest);
    let nif = files
        .iter_mut()
        .find(|e| e.path.to_ascii_lowercase().ends_with(".nif"))
        .expect("a block mesh");
    nif.data.truncate(nif.data.len() / 2);
    let archive = pack::write(&files).unwrap();
    let extracted = extract_to_temp(&archive, None, 1 << 28).unwrap();
    let found = lodwright_gen::verify_folder(extracted.path());
    assert!(!found.errors.is_empty(), "a half block must be reported");
}
