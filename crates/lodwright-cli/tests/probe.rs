// SPDX-License-Identifier: MPL-2.0
//! `lodwright probe` against files built by the testkit in a temporary folder.

use lodwright_cli::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, run};
use lodwright_testkit::dds::{Pixels, build_file};
use lodwright_testkit::golden::assert_golden;
use lodwright_testkit::probe_nif::{
    GeometryOptions, NifBuilder, texture_set, tri_shape_data, tri_strips_data,
};
use std::path::{Path, PathBuf};
use std::process::Command;

fn go(args: &[&str]) -> (u8, String, String) {
    let args: Vec<String> = args.iter().map(ToString::to_string).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-probe-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

fn nif_bytes() -> Vec<u8> {
    NifBuilder::new()
        .string("alpha")
        .block("NiNode", vec![0; 8])
        .block(
            "NiTriShapeData",
            tri_shape_data(
                4,
                2,
                GeometryOptions {
                    vector_flags: 1 | 4096,
                    has_normals: true,
                    has_vertex_colors: true,
                },
            ),
        )
        .block(
            "NiTriStripsData",
            tri_strips_data(5, &[4, 3], GeometryOptions::default()),
        )
        .block("BSShaderTextureSet", texture_set(&["a.dds", "", "bcd.dds"]))
        .build()
}

fn settings_bytes() -> Vec<u8> {
    let mut bytes = vec![];
    for n in [4_u32, 8, 16, 32] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes
}

fn dtl_bytes() -> Vec<u8> {
    (0..70_u8).collect()
}

struct Case {
    name: &'static str,
    kind: &'static str,
    bytes: Vec<u8>,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "nif",
            kind: "nif",
            bytes: nif_bytes(),
        },
        Case {
            name: "dds",
            kind: "dds",
            bytes: build_file(8, 4, 1, &Pixels::Bc3Gradient),
        },
        Case {
            name: "dds_rgba",
            kind: "dds",
            bytes: build_file(
                2,
                2,
                1,
                &Pixels::Argb32(vec![
                    [0, 0, 0, 255],
                    [255, 128, 0, 255],
                    [10, 20, 30, 40],
                    [200, 100, 50, 0],
                ]),
            ),
        },
        Case {
            name: "settings",
            kind: "settings",
            bytes: settings_bytes(),
        },
        Case {
            name: "dtl",
            kind: "dtl",
            bytes: dtl_bytes(),
        },
    ]
}

#[test]
fn json_and_text_outputs_equal_their_golden_files() {
    let dir = folder("golden");
    for case in cases() {
        let file = dir.join(format!("{}.bin", case.name));
        std::fs::write(&file, &case.bytes).unwrap();
        let path = file.to_str().unwrap();
        let (code, json, err) = go(&["probe", case.kind, path, "--json", "--allow-in-repo"]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{}", case.name);
        assert_golden(&golden(&format!("probe_{}.json", case.name)), &json);
        let (code, text, err) = go(&["probe", case.kind, path, "--allow-in-repo"]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{}", case.name);
        assert_golden(&golden(&format!("probe_{}.txt", case.name)), &text);
    }
}

#[test]
fn no_byte_of_input_beyond_the_documented_integers_reaches_the_output() {
    let marker = "ZZMARKERZZ";
    let dir = folder("marker");
    let mut files: Vec<(&str, Vec<u8>)> = vec![];
    files.push((
        "nif",
        NifBuilder::new()
            .author(marker)
            .string(marker)
            .block(marker, vec![0; 3])
            .block("BSShaderTextureSet", texture_set(&[marker]))
            .build(),
    ));
    let mut dds = build_file(4, 4, 1, &Pixels::Bc1Solid { colour: 0x07E0 });
    dds.extend_from_slice(marker.as_bytes());
    dds[84..88].copy_from_slice(b"ZZ\"\n"); // an unsafe FourCC
    files.push(("dds", dds));
    for kind in ["settings", "dtl"] {
        let mut bytes = vec![0_u8; 80];
        bytes.extend_from_slice(marker.as_bytes()); // beyond the first 64 bytes
        files.push((kind, bytes));
    }
    for (kind, bytes) in files {
        let file = dir.join(format!("{kind}.bin"));
        std::fs::write(&file, bytes).unwrap();
        for extra in [&["--json"][..], &[]] {
            let mut args = vec!["probe", kind, file.to_str().unwrap(), "--allow-in-repo"];
            args.extend_from_slice(extra);
            let (_, out, err) = go(&args);
            assert!(
                !out.contains(marker) && !err.contains(marker),
                "{kind}: {out}"
            );
            assert!(!out.contains("ZZ"), "{kind}: {out}");
            let path_text = file.to_str().unwrap();
            assert!(
                !out.contains(path_text) && !err.contains(path_text),
                "the path is echoed"
            );
        }
    }
}

#[test]
fn truncated_and_garbage_input_exits_one_with_a_message_and_never_panics() {
    let dir = folder("bad");
    for case in cases() {
        if case.kind == "settings" || case.kind == "dtl" {
            continue; // any non-empty file is readable; the empty file is covered below
        }
        let step = (case.bytes.len() / 40).max(1);
        for cut in (0..case.bytes.len().saturating_sub(4)).step_by(step) {
            let file = dir.join("cut.bin");
            std::fs::write(&file, &case.bytes[..cut]).unwrap();
            let output = Command::new(env!("CARGO_BIN_EXE_lodwright"))
                .args([
                    "probe",
                    case.kind,
                    file.to_str().unwrap(),
                    "--allow-in-repo",
                ])
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1), "{} cut at {cut}", case.name);
            assert_eq!(output.stdout.len(), 0);
            let message = String::from_utf8(output.stderr).unwrap();
            assert!(message.starts_with("lodwright: probe: "), "{message}");
            assert!(!message.contains("panicked"));
        }
    }
    let file = dir.join("garbage.bin");
    std::fs::write(&file, [0xA5_u8; 4096]).unwrap();
    for kind in ["nif", "dds"] {
        let (code, out, err) = go(&["probe", kind, file.to_str().unwrap(), "--allow-in-repo"]);
        assert_eq!((code, out.as_str()), (EXIT_FAILED, ""), "{kind}");
        assert!(err.starts_with("lodwright: probe: "), "{err}");
    }
    let empty = dir.join("empty.bin");
    std::fs::write(&empty, b"").unwrap();
    for kind in ["settings", "dtl"] {
        let (code, _, err) = go(&["probe", kind, empty.to_str().unwrap(), "--allow-in-repo"]);
        assert_eq!(code, EXIT_FAILED, "{kind}");
        assert!(err.contains("empty"), "{err}");
    }
}

#[test]
fn a_file_inside_a_git_working_tree_is_refused_unless_allowed() {
    let dir = folder("repo");
    let repo = dir.join("work");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let file = repo.join("sub").join("x.bin");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, settings_bytes()).unwrap();
    let path = file.to_str().unwrap();
    let (code, out, err) = go(&["probe", "settings", path]);
    assert_eq!((code, out.as_str()), (EXIT_FAILED, ""));
    assert!(
        err.contains("refusing to read a file inside a git working tree"),
        "{err}"
    );
    assert!(!err.contains(path));
    let (code, out, _) = go(&["probe", "settings", path, "--allow-in-repo"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.starts_with("size_bytes: 16\n"), "{out}");
}

#[test]
fn a_missing_file_and_a_folder_fail_without_naming_the_path() {
    let dir = folder("missing");
    let missing = dir.join("nope.nif");
    let (code, _, err) = go(&["probe", "nif", missing.to_str().unwrap(), "--allow-in-repo"]);
    assert_eq!(code, EXIT_FAILED);
    assert!(!err.contains("nope.nif"), "{err}");
    let (code, _, err) = go(&["probe", "nif", dir.to_str().unwrap(), "--allow-in-repo"]);
    assert_eq!(code, EXIT_FAILED);
    assert!(err.contains("not a regular file"), "{err}");
}

#[test]
fn usage_mistakes_exit_two() {
    for args in [
        vec!["probe"],
        vec!["probe", "nif"],
        vec!["probe", "png", "f"],
        vec!["probe", "nif", "f", "--bogus"],
    ] {
        assert_eq!(go(&args).0, EXIT_USAGE, "{args:?}");
    }
}
