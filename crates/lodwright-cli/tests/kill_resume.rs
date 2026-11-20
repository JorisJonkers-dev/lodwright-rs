// SPDX-License-Identifier: GPL-3.0-only
//! A `generate` process killed after some blocks leaves the previous output alone, and the next run resumes to the
//! files an uninterrupted run writes.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lodwright_contract::{MANIFEST_FILE, Manifest, Request};
use lodwright_testkit::land::Heights;
use lodwright_testkit::plugin::PluginBuilder;
use lodwright_testkit::world::{SyntheticWorld, world_group};

const WORLDS: u32 = 12;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-kill-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let data = temp(&format!("{name}-data"));
    std::fs::create_dir_all(&data).unwrap();
    let mut builder = PluginBuilder::new();
    for index in 0..WORLDS {
        let mut world = SyntheticWorld::new(0x1000 + index * 0x2000, &format!("World{index}"));
        for cell in 0..4_i32 {
            let height = f32::from(u16::try_from((cell + 1) * 8).unwrap());
            world = world.cell(cell % 2, cell / 2, Heights::flat(height));
        }
        builder = builder.group(world_group(&world, false));
    }
    std::fs::write(data.join("Worlds.esm"), builder.build()).unwrap();
    let out = temp(&format!("{name}-out"));
    let mut request = Request::new(
        data.to_str().unwrap(),
        &["Worlds.esm"],
        out.to_str().unwrap(),
    );
    request.objects.enabled = false;
    request.trees.enabled = false;
    request.terrain.land_textures = false;
    request.limits.threads = Some(1);
    let file = temp(&format!("{name}-request.json"));
    std::fs::write(&file, serde_json::to_string(&request).unwrap()).unwrap();
    (data, out, file)
}

fn command(request: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_lodwright"));
    c.args(["generate", "--request", request.to_str().unwrap(), "--json"]);
    c
}

fn manifest(out: &Path) -> Manifest {
    serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap()
}

fn staging(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap().to_os_string();
    name.push(".lodwright-staging");
    out.with_file_name(name)
}

/// Start `generate`, kill it with SIGKILL (or the platform's hard kill) once `blocks` event lines are written.
/// Returns whether the process was still running when it was killed.
fn kill_after(request: &Path, events: &Path, blocks: usize) -> bool {
    let mut child = command(request)
        .args(["--events", events.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        let lines = std::fs::read_to_string(events).map_or(0, |t| t.lines().count());
        if lines >= blocks || child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(120), "no events");
        std::thread::sleep(Duration::from_millis(2));
    }
    let alive = child.try_wait().unwrap().is_none();
    let _ = child.kill();
    let _ = child.wait();
    alive
}

#[test]
fn a_killed_run_resumes_to_the_bytes_of_an_uninterrupted_run() {
    let (data, out, request) = setup("killed");
    let (clean_data, clean_out, clean_request) = setup("clean");
    let run = command(&clean_request).output().unwrap();
    assert_eq!(run.status.code(), Some(0));
    for blocks in [3, 20] {
        let _ = std::fs::remove_dir_all(&out);
        let _ = std::fs::remove_dir_all(staging(&out));
        let alive = kill_after(&request, &temp("events.jsonl"), blocks);
        assert!(
            alive || blocks > 3,
            "the process ended before the kill: the test proves nothing"
        );
        // a kill never leaves a half-built output folder: either none (killed before the swap) or the finished one
        if out.exists() {
            assert!(out.join(MANIFEST_FILE).is_file(), "output without manifest");
        }
        let run = command(&request).output().unwrap();
        assert_eq!(
            run.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&run.stdout)
        );
        let (a, b) = (manifest(&out), manifest(&clean_out));
        assert_eq!(a.files, b.files, "after a kill at {blocks} events");
        for f in &a.files {
            assert_eq!(
                std::fs::read(out.join(&f.path)).unwrap(),
                std::fs::read(clean_out.join(&f.path)).unwrap(),
                "{}",
                f.path
            );
        }
        assert!(!staging(&out).exists());
    }
    for dir in [&out, &clean_out, &data, &clean_data] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_run_killed_over_an_existing_output_leaves_that_output_untouched() {
    let (data, out, request) = setup("over");
    assert_eq!(command(&request).output().unwrap().status.code(), Some(0));
    let before = manifest(&out);
    let bytes: Vec<_> = before
        .files
        .iter()
        .map(|f| std::fs::read(out.join(&f.path)).unwrap())
        .collect();
    // a different request, killed part-way
    let mut changed: Request = serde_json::from_slice(&std::fs::read(&request).unwrap()).unwrap();
    changed.terrain.water = !changed.terrain.water;
    let changed_file = temp("over-changed.json");
    std::fs::write(&changed_file, serde_json::to_string(&changed).unwrap()).unwrap();
    if kill_after(&changed_file, &temp("over-events.jsonl"), 3) {
        assert_eq!(manifest(&out), before);
        for (f, expected) in before.files.iter().zip(&bytes) {
            assert_eq!(&std::fs::read(out.join(&f.path)).unwrap(), expected);
        }
    }
    for dir in [&out, &staging(&out), &data] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
