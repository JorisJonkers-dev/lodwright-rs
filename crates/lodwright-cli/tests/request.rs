// SPDX-License-Identifier: GPL-3.0-only
//! `generate --request`: the document, the manifest, the up-to-date run and the refusals.

use lodwright_cli::run;
use lodwright_contract::Request;
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};
use std::path::{Path, PathBuf};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-request-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn data(name: &str) -> PathBuf {
    let dir = temp(name);
    std::fs::create_dir_all(&dir).unwrap();
    let world = SyntheticWorld::new(0xB00, "Island")
        .cell(2, 2, Heights::flat(16.0))
        .cell(3, 2, Heights::flat(24.0));
    std::fs::write(dir.join("Island.esm"), world_plugin(&[], &world, false)).unwrap();
    dir
}

fn go(args: &[&str]) -> (u8, serde_json::Value) {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &mut out, &mut err);
    let text = String::from_utf8(out).unwrap();
    (
        code,
        serde_json::from_str(text.lines().last().unwrap_or("{}")).unwrap_or_default(),
    )
}

fn write_request(dir: &Path, request: &Request) -> PathBuf {
    let file = dir.with_extension("request.json");
    std::fs::write(&file, serde_json::to_string(request).unwrap()).unwrap();
    file
}

#[test]
fn a_request_writes_files_and_a_manifest_and_the_second_run_does_nothing() {
    let d = data("run");
    let out = temp("run-out");
    let request = Request::new(d.to_str().unwrap(), &["Island.esm"], out.to_str().unwrap());
    let file = write_request(&out, &request);
    let (code, doc) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!(code, 0, "{doc}");
    assert_eq!(doc["code"], "generated");
    assert_eq!(doc["manifest"], "manifest.json");
    assert_eq!(doc["arguments_digest"], request.digest());
    let manifest: lodwright_contract::Manifest =
        serde_json::from_slice(&std::fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert!(
        manifest
            .files
            .iter()
            .any(|f| f.path.ends_with(".dlodsettings"))
    );
    assert!(manifest.files.iter().all(|f| out.join(&f.path).is_file()));
    // second run: nothing is rewritten
    let stamp = std::fs::metadata(out.join(&manifest.files[0].path))
        .unwrap()
        .modified()
        .unwrap();
    let (code, doc) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!((code, doc["code"].as_str()), (0, Some("up_to_date")));
    assert_eq!(
        std::fs::metadata(out.join(&manifest.files[0].path))
            .unwrap()
            .modified()
            .unwrap(),
        stamp
    );
}

#[test]
fn a_changed_request_replaces_the_old_files_and_a_deleted_file_is_regenerated() {
    let d = data("change");
    let out = temp("change-out");
    let mut request = Request::new(d.to_str().unwrap(), &["Island.esm"], out.to_str().unwrap());
    let file = write_request(&out, &request);
    assert_eq!(
        go(&["generate", "--request", file.to_str().unwrap(), "--json"]).0,
        0
    );
    let before = std::fs::read_dir(out.join("textures/landscape/lod/island/diffuse"))
        .unwrap()
        .count();
    assert!(before > 0);
    request.terrain.land_textures = false;
    request.objects.enabled = false;
    request.terrain.levels = vec![lodwright_contract::LevelRequest {
        level: 32,
        quality: 20.0,
        max_vertices: 500,
        diffuse_size: 64,
        normal_size: 64,
        specular_alpha: false,
        skirts: false,
    }];
    let file = write_request(&out, &request);
    let (code, doc) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!((code, doc["code"].as_str()), (0, Some("generated")));
    assert_eq!(
        std::fs::read_dir(out.join("textures/landscape/lod/island/diffuse"))
            .unwrap()
            .count(),
        1
    );
    // a file the manifest lists is gone: the run regenerates
    let gone = out.join("lodsettings/island.dlodsettings");
    std::fs::remove_file(&gone).unwrap();
    let (_, doc) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!(doc["code"], "generated");
    assert!(gone.is_file());
}

#[test]
fn unchanged_blocks_are_kept_changed_ones_rebuilt_and_dropped_ones_removed() {
    let d = data("reuse");
    let out = temp("reuse-out");
    let level = |level, quality| lodwright_contract::LevelRequest {
        level,
        quality,
        max_vertices: 500,
        diffuse_size: 64,
        normal_size: 64,
        specular_alpha: false,
        skirts: false,
    };
    let mut request = Request::new(d.to_str().unwrap(), &["Island.esm"], out.to_str().unwrap());
    request.terrain.land_textures = false;
    request.objects.enabled = false;
    request.terrain.levels = vec![level(4, 10.0), level(8, 10.0)];
    let file = write_request(&out, &request);
    let (_, doc) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!(doc["blocks"]["generated"], 2, "{doc}");
    let kept = out.join("meshes/landscape/lod/island/island.level4.x2.y2.nif");
    let stamp = std::fs::metadata(&kept).unwrap().modified().unwrap();
    // level 8 changes and level 16 is new: level 4 is kept untouched
    request.terrain.levels = vec![level(4, 10.0), level(8, 20.0), level(16, 10.0)];
    let file = write_request(&out, &request);
    let (code, doc) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!((code, doc["code"].as_str()), (0, Some("generated")));
    assert_eq!(doc["blocks"]["up_to_date"], 1, "{doc}");
    assert_eq!(doc["blocks"]["generated"], 2, "{doc}");
    assert_eq!(std::fs::metadata(&kept).unwrap().modified().unwrap(), stamp);
    let manifest: lodwright_contract::Manifest =
        serde_json::from_slice(&std::fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.blocks.len(), 3);
    // a kept file that was changed on disk is rebuilt, and a dropped level's files go away
    std::fs::write(&kept, b"changed").unwrap();
    request.terrain.levels = vec![level(4, 10.0)];
    let file = write_request(&out, &request);
    let (_, doc) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!(doc["blocks"]["generated"], 1, "{doc}");
    assert!(std::fs::read(&kept).unwrap().len() > 100);
    assert!(
        !out.join("meshes/landscape/lod/island/island.level16.x2.y2.nif")
            .exists()
    );
}

#[test]
fn bad_requests_and_unsafe_outputs_are_refused_with_codes() {
    let d = data("bad");
    let out = temp("bad-out");
    let mut request = Request::new(d.to_str().unwrap(), &["Island.esm"], out.to_str().unwrap());
    request.terrain.levels = vec![lodwright_contract::LevelRequest {
        level: 5,
        quality: 1.0,
        max_vertices: 100,
        diffuse_size: 64,
        normal_size: 64,
        specular_alpha: false,
        skirts: false,
    }];
    let file = write_request(&out, &request);
    let (code, doc) = go(&["generate", "--request", file.to_str().unwrap(), "--json"]);
    assert_eq!((code, doc["code"].as_str()), (1, Some("request_invalid")));
    // not JSON, and a wrong contract
    let junk = out.with_extension("junk.json");
    std::fs::write(&junk, "nope").unwrap();
    assert_eq!(
        go(&["generate", "--request", junk.to_str().unwrap(), "--json"]).1["code"],
        "request_invalid"
    );
    // non-empty output without a manifest
    let full = temp("full-out");
    std::fs::create_dir_all(&full).unwrap();
    std::fs::write(full.join("keep.txt"), "x").unwrap();
    let ok = Request::new(d.to_str().unwrap(), &["Island.esm"], full.to_str().unwrap());
    let file = write_request(&full, &ok);
    assert_eq!(
        go(&["generate", "--request", file.to_str().unwrap(), "--json"]).1["code"],
        "output_not_empty"
    );
    assert_eq!(
        go(&[
            "generate",
            "--request",
            file.to_str().unwrap(),
            "--overwrite",
            "--json"
        ])
        .0,
        0
    );
    assert!(full.join("keep.txt").is_file());
    // output inside the data folder
    let inside = Request::new(
        d.to_str().unwrap(),
        &["Island.esm"],
        d.join("lod").to_str().unwrap(),
    );
    let file = write_request(&d, &inside);
    assert_eq!(
        go(&["generate", "--request", file.to_str().unwrap(), "--json"]).1["code"],
        "output_in_data"
    );
}

#[test]
fn the_request_schema_is_printed_and_names_its_contract() {
    let args: Vec<String> = ["schema", "request"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(run(&args, &mut out, &mut err), 0);
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("lodwright-request") && text.contains("exclude_plugins"));
}

#[test]
fn progress_events_are_one_json_line_per_block_in_the_events_file() {
    let d = data("events");
    let out = temp("events-out");
    let events = temp("events.ndjson");
    let (code, _) = go(&[
        "generate",
        "--data",
        d.to_str().unwrap(),
        "--plugin",
        "Island.esm",
        "--out",
        out.to_str().unwrap(),
        "--events",
        events.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code, 0);
    let text = std::fs::read_to_string(&events).unwrap();
    let parsed: Vec<lodwright_contract::Event> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    // one cell pair: a block at each of the four levels
    assert_eq!(parsed.len(), 4);
    assert!(
        parsed
            .iter()
            .all(|e| matches!(e, lodwright_contract::Event::Block { done: 1, of: 1, .. }))
    );
}
