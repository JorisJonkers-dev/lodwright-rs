// SPDX-License-Identifier: GPL-3.0-only
//! Determinism under a memory budget: a run held to a low `limits.memory_mib` writes the same bytes as a run without
//! one.
//!
//! Today the budget is a preflight estimate only (the run is refused when the plugin sizes suggest more), so nothing
//! in the run changes under it yet. The test fixes the invariant the scheduling work must keep when it starts lowering
//! the worker count and the batch size under pressure: same files, same bytes, same manifest entries.

use std::path::{Path, PathBuf};

use lodwright_api::{CancelToken, GenerateOutcome, GenerateRequest, NoProgress, generate};
use lodwright_contract::{MANIFEST_FILE, Manifest, Request};
use lodwright_testkit::land::Heights;
use lodwright_testkit::plugin::PluginBuilder;
use lodwright_testkit::world::{SyntheticWorld, world_group};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-memory-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A data folder with three synthetic worlds of different sizes and shapes in one plugin.
fn data_folder(name: &str) -> PathBuf {
    let data = temp(&format!("{name}-data"));
    std::fs::create_dir_all(&data).unwrap();
    let mut builder = PluginBuilder::new();
    for (index, cells) in [(0u32, 6i32), (1, 20), (2, 3)] {
        let mut world = SyntheticWorld::new(0xB00 + index * 0x100, &format!("World{index}"));
        for cell in 0..cells {
            let height = f32::from(u16::try_from(cell * 7 % 11).unwrap()) * 8.0;
            world = world.cell(cell % 5 - 2, cell / 5, Heights::flat(height));
        }
        builder = builder.group(world_group(&world, false));
    }
    let bytes = builder.build();
    // the 1 MiB budget below must pass the preflight estimate (four times the plugin bytes), or the low-budget run
    // is refused before it starts and the comparison tests nothing
    assert!(
        bytes.len() * 4 < 1024 * 1024,
        "the fixture outgrew the budget"
    );
    std::fs::write(data.join("Worlds.esm"), bytes).unwrap();
    data
}

fn request(data: &Path, out: &Path) -> Request {
    let mut request = Request::new(
        data.to_str().unwrap(),
        &["Worlds.esm"],
        out.to_str().unwrap(),
    );
    request.objects.enabled = false;
    request.trees.enabled = false;
    request.terrain.land_textures = false;
    request
}

fn run(request: Request) {
    match generate(
        &GenerateRequest::new(request),
        &mut NoProgress,
        &CancelToken::new(),
    )
    .unwrap()
    {
        GenerateOutcome::Done { result } => assert_eq!(result.code, "generated"),
        GenerateOutcome::Cancelled => panic!("the run was cancelled"),
    }
}

/// Every file in `out` except the manifest, relative with forward slashes, with its bytes.
fn contents(out: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(out: &Path, dir: &Path, into: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(out, &path, into);
            } else {
                let rel: Vec<String> = path
                    .strip_prefix(out)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                into.push((rel.join("/"), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut files = Vec::new();
    walk(out, out, &mut files);
    files.retain(|(name, _)| name != MANIFEST_FILE);
    files.sort();
    files
}

fn manifest(out: &Path) -> Manifest {
    serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap()
}

/// The output of an unconstrained run and of a run on one worker with a 1 MiB budget, over the same data.
fn both_runs(name: &str) -> (PathBuf, PathBuf) {
    let data = data_folder(name);
    let (free, held) = (temp(&format!("{name}-free")), temp(&format!("{name}-held")));
    let mut unconstrained = request(&data, &free);
    unconstrained.limits.threads = Some(4);
    run(unconstrained);
    let mut low = request(&data, &held);
    low.limits.threads = Some(1);
    low.limits.memory_mib = Some(1);
    run(low);
    (free, held)
}

#[test]
fn a_run_under_a_low_memory_budget_writes_the_same_files_byte_for_byte() {
    let (free, held) = both_runs("bytes");
    let (a, b) = (contents(&free), contents(&held));
    assert_ne!(a.len(), 0);
    assert_eq!(
        a.iter().map(|(n, _)| n).collect::<Vec<_>>(),
        b.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    for ((name, x), (_, y)) in a.iter().zip(&b) {
        assert!(x == y, "{name} differs under the memory budget");
    }
}

#[test]
fn a_run_under_a_low_memory_budget_lists_the_same_files_and_blocks_in_its_manifest() {
    let (free, held) = both_runs("manifest");
    let (a, b) = (manifest(&free), manifest(&held));
    assert_eq!(a.inputs_digest, b.inputs_digest);
    assert_eq!(a.files, b.files);
    assert_eq!(a.blocks, b.blocks);
}

#[test]
fn a_run_under_a_low_memory_budget_builds_every_world() {
    let (_, held) = both_runs("worlds");
    let settings: Vec<String> = contents(&held)
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| n.ends_with(".dlodsettings"))
        .collect();
    assert_eq!(settings.len(), 3, "{settings:?}");
}
