// SPDX-License-Identifier: GPL-3.0-only
//! The output-folder guards of [`lodwright_api::generate`] against symbolic links.
//!
//! The policy these tests hold:
//!
//! * An output folder that is itself a symbolic link is accepted, and the guards judge the folder it points to: a
//!   user may keep the LOD on another disk behind a link, and refusing every link would refuse that for no gain.
//!   A link that resolves into the data folder is refused as `output_in_data`, and one that resolves into a git
//!   working tree is refused as `output_in_repo`.
//! * A not yet existing output folder under a linked parent is judged by where it will be created.
//! * Nothing is ever written, replaced or removed outside the output folder: not through a link planted inside
//!   it, and not through a manifest entry that climbs out of it.
//!
//! Unix only: creating a symbolic link on Windows needs a privilege or developer mode that CI and most users do not
//! have, and the guards resolve links through the same `canonicalize` call on every platform.
#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use lodwright_api::{CancelToken, GenerateOutcome, GenerateRequest, NoProgress, generate};
use lodwright_contract::{MANIFEST_FILE, Manifest, ManifestFile, Request, sha256_digest};
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};

/// A fresh, empty folder for one test.
fn temp(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("lodwright-symlinks-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A data folder under `root` with one small synthetic world, and a terrain-only request on one thread that writes
/// to `out`.
fn job(root: &Path, out: &Path) -> GenerateRequest {
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let world = SyntheticWorld::new(0xB00, "Island")
        .cell(0, 0, Heights::flat(16.0))
        .cell(5, 0, Heights::flat(24.0));
    std::fs::write(data.join("Island.esm"), world_plugin(&[], &world, false)).unwrap();
    let mut request = Request::new(
        data.to_str().unwrap(),
        &["Island.esm"],
        out.to_str().unwrap(),
    );
    request.objects.enabled = false;
    request.trees.enabled = false;
    request.terrain.land_textures = false;
    request.limits.threads = Some(1);
    GenerateRequest::new(request)
}

fn run(job: &GenerateRequest) -> Result<GenerateOutcome, lodwright_api::Error> {
    generate(job, &mut NoProgress, &CancelToken::new())
}

fn manifest(out: &Path) -> Manifest {
    serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap()
}

/// The files a plain run writes for [`job`], relative to the output folder.
fn written_paths(name: &str) -> Vec<String> {
    let root = temp(name);
    let out = root.join("out");
    run(&job(&root, &out)).unwrap();
    manifest(&out).files.into_iter().map(|f| f.path).collect()
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn an_output_folder_that_is_a_link_into_the_data_folder_is_refused() {
    let root = temp("link-into-data");
    let out = root.join("out");
    let job = job(&root, &out);
    std::fs::create_dir_all(root.join("data/lod")).unwrap();
    symlink(root.join("data/lod"), &out).unwrap();
    let err = run(&job).unwrap_err();
    assert_eq!(err.code, "output_in_data");
    assert_eq!(entries(&root.join("data/lod")), Vec::<String>::new());
}

#[test]
fn an_output_folder_that_is_a_link_to_the_data_folder_itself_is_refused() {
    let root = temp("link-to-data");
    let out = root.join("out");
    let job = job(&root, &out);
    symlink(root.join("data"), &out).unwrap();
    let err = run(&job).unwrap_err();
    assert_eq!(err.code, "output_in_data");
    assert_eq!(entries(&root.join("data")), ["Island.esm"]);
}

#[test]
fn a_new_output_folder_under_a_linked_parent_that_points_into_the_data_folder_is_refused() {
    let root = temp("new-under-link");
    let link = root.join("link");
    let job = job(&root, &link.join("not-yet/lod"));
    symlink(root.join("data"), &link).unwrap();
    let err = run(&job).unwrap_err();
    assert_eq!(err.code, "output_in_data");
    assert!(!root.join("data/not-yet").exists());
}

#[test]
fn an_output_folder_inside_the_data_folder_named_through_a_linked_data_folder_is_refused() {
    let root = temp("linked-data");
    let mut job = job(&root, &root.join("data/lod"));
    symlink(root.join("data"), root.join("data-link")).unwrap();
    job.request.data_folder = root.join("data-link").to_str().unwrap().to_owned();
    let err = run(&job).unwrap_err();
    assert_eq!(err.code, "output_in_data");
}

#[test]
fn an_output_folder_that_is_a_link_to_a_separate_folder_is_written_through_and_stays_up_to_date() {
    let root = temp("link-elsewhere");
    let (out, target) = (root.join("out"), root.join("other-disk/lod"));
    std::fs::create_dir_all(&target).unwrap();
    symlink(&target, &out).unwrap();
    let job = job(&root, &out);
    run(&job).unwrap();
    assert!(std::fs::symlink_metadata(&out).unwrap().is_symlink());
    assert!(
        manifest(&target)
            .files
            .iter()
            .all(|f| target.join(&f.path).is_file())
    );
    let GenerateOutcome::Done { result } = run(&job).unwrap() else {
        panic!("cancelled");
    };
    assert_eq!(result.code, "up_to_date");
}

#[test]
fn a_link_at_the_path_of_an_output_file_is_replaced_and_its_target_left_alone() {
    let path = written_paths("file-link-paths").remove(0);
    let root = temp("file-link");
    let (out, victim) = (root.join("out"), root.join("victim.txt"));
    std::fs::write(&victim, b"not LOD").unwrap();
    let planted = out.join(&path);
    std::fs::create_dir_all(planted.parent().unwrap()).unwrap();
    symlink(&victim, &planted).unwrap();
    let mut job = job(&root, &out);
    job.overwrite = true;
    run(&job).unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"not LOD");
    assert!(!std::fs::symlink_metadata(&planted).unwrap().is_symlink());
}

#[test]
fn an_output_folder_that_is_a_link_into_a_git_working_tree_is_refused() {
    let root = temp("link-into-repo");
    let out = root.join("out");
    let job = job(&root, &out);
    std::fs::create_dir_all(root.join("repo/.git")).unwrap();
    std::fs::create_dir_all(root.join("repo/lod")).unwrap();
    symlink(root.join("repo/lod"), &out).unwrap();
    let err = run(&job).unwrap_err();
    assert_eq!(err.code, "output_in_repo");
}

#[test]
fn nothing_is_written_through_a_linked_folder_inside_the_output_folder() {
    let path = written_paths("dir-link-paths").remove(0);
    let first = path.split('/').next().unwrap().to_owned();
    let root = temp("dir-link");
    let (out, outside) = (root.join("out"), root.join("outside"));
    std::fs::create_dir_all(&out).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, out.join(&first)).unwrap();
    let mut job = job(&root, &out);
    job.overwrite = true;
    let _ = run(&job);
    assert_eq!(entries(&outside), Vec::<String>::new());
}

#[test]
fn a_link_at_the_temporary_path_of_an_output_file_does_not_overwrite_its_target() {
    let path = written_paths("temp-link-paths").remove(0);
    let root = temp("temp-link");
    let (out, victim) = (root.join("out"), root.join("victim.txt"));
    std::fs::write(&victim, b"not LOD").unwrap();
    let planted = out.join(&path).with_extension("tmp");
    std::fs::create_dir_all(planted.parent().unwrap()).unwrap();
    symlink(&victim, &planted).unwrap();
    let mut job = job(&root, &out);
    job.overwrite = true;
    let _ = run(&job);
    assert_eq!(std::fs::read(&victim).unwrap(), b"not LOD");
}

#[test]
fn an_old_manifest_entry_outside_the_output_folder_is_never_removed() {
    let root = temp("manifest-escape");
    let (out, victim) = (root.join("out"), root.join("victim.txt"));
    std::fs::write(&victim, b"not LOD").unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let old = Manifest::new(
        "0.0.0-test",
        "sha256:old",
        "sha256:old",
        vec![ManifestFile {
            path: "../victim.txt".into(),
            bytes: 7,
            digest: sha256_digest(b"not LOD"),
        }],
    );
    std::fs::write(
        out.join(MANIFEST_FILE),
        serde_json::to_string(&old).unwrap(),
    )
    .unwrap();
    let _ = run(&job(&root, &out));
    assert!(victim.is_file());
}
