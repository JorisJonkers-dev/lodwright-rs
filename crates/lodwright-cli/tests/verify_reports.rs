// SPDX-License-Identifier: GPL-3.0-only
//! `verify` reports as a caller reads them: golden reports for folder pairs built from synthetic worlds, proof that
//! no file content reaches a report, and the refusal of a folder inside a git working tree.

use std::path::{Path, PathBuf};

use lodwright_cli::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, run};
use lodwright_testkit::dds::{HEADER_LEN, build_header, parse_header};
use lodwright_testkit::golden::assert_golden;
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lodwright-verify-report-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

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

/// Generate the terrain LOD of a two-cell island, its ground raised by `lift` units, into a fresh folder.
fn generate(name: &str, lift: f32) -> PathBuf {
    let data = temp(&format!("{name}-data"));
    std::fs::create_dir_all(&data).unwrap();
    let world = SyntheticWorld::new(0xB00, "Island")
        .cell(2, 2, Heights::flat(16.0 + lift))
        .cell(3, 2, Heights::flat(24.0 + lift));
    std::fs::write(data.join("Island.esm"), world_plugin(&[], &world, false)).unwrap();
    let out = temp(name);
    let (code, text, _) = go(&[
        "generate",
        "--data",
        data.to_str().unwrap(),
        "--plugin",
        "Island.esm",
        "--out",
        out.to_str().unwrap(),
        "--terrain-only",
        "--json",
    ]);
    assert_eq!(code, EXIT_OK, "{text}");
    out
}

/// Every file below `dir`, relative with forward slashes, sorted.
fn files(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, into: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, into);
            } else {
                into.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// `verify mine --reference theirs` as text and as JSON, with the exit code of each.
fn reports(mine: &Path, theirs: &Path) -> String {
    let (a, b) = (mine.to_str().unwrap(), theirs.to_str().unwrap());
    let (text_code, text, _) = go(&["verify", a, "--reference", b]);
    let (json_code, json, _) = go(&["verify", a, "--reference", b, "--json"]);
    assert_eq!(text_code, json_code);
    let pretty =
        serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(&json).unwrap())
            .unwrap();
    format!("exit {text_code}\n\n{text}\n{pretty}\n")
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

#[test]
fn verify_reports_equal_their_golden_files_for_four_folder_pairs() {
    let base = generate("base", 0.0);
    let twin = generate("twin", 0.0);
    let shifted = generate("shifted", 64.0);
    // a copy of the base without its first terrain texture
    let missing = generate("missing", 0.0);
    let gone = files(&missing)
        .into_iter()
        .find(|f| Path::new(f).extension().is_some_and(|e| e == "dds"))
        .unwrap();
    std::fs::remove_file(missing.join(&gone)).unwrap();
    // a copy of the base whose first terrain texture claims one mip level fewer
    let mips = generate("mips", 0.0);
    let short = files(&mips)
        .into_iter()
        .find(|f| Path::new(f).extension().is_some_and(|e| e == "dds"))
        .unwrap();
    let mut bytes = std::fs::read(mips.join(&short)).unwrap();
    let mut header = parse_header(&bytes).unwrap();
    header.mip_count -= 1;
    // only the mip-count field changes; the rest of the generator's header stays as written
    // source: dds-header-layout
    let rebuilt = build_header(&header);
    assert_eq!(rebuilt.len(), HEADER_LEN);
    bytes[28..32].copy_from_slice(&rebuilt[28..32]);
    std::fs::write(mips.join(&short), bytes).unwrap();
    for (name, mine) in [
        ("verify_identical.txt", &twin),
        ("verify_shifted_heights.txt", &shifted),
        ("verify_missing_file.txt", &missing),
        ("verify_wrong_mips.txt", &mips),
    ] {
        assert_golden(&golden(name), &reports(mine, &base));
    }
    assert_eq!(
        go(&[
            "verify",
            twin.to_str().unwrap(),
            "--reference",
            base.to_str().unwrap()
        ])
        .0,
        EXIT_OK
    );
    assert_eq!(go(&["verify", mips.to_str().unwrap()]).0, EXIT_FAILED);
}

#[test]
fn no_file_content_reaches_a_report() {
    const MARKER: &str = "MARKER-7f3a-never-in-a-report";
    let mine = generate("marker", 0.0);
    // the marker replaces files verify reads and refuses: a block mesh, a texture and the settings file
    let all = files(&mine);
    for ext in [".nif", ".dds", ".dlodsettings"] {
        let f = all.iter().find(|f| f.ends_with(ext)).unwrap();
        std::fs::write(mine.join(f), format!("{MARKER}{MARKER}")).unwrap();
    }
    let theirs = generate("marker-reference", 0.0);
    std::fs::write(theirs.join("extra.txt"), MARKER).unwrap();
    let a = mine.to_str().unwrap();
    let b = theirs.to_str().unwrap();
    for args in [
        vec!["verify", a],
        vec!["verify", a, "--json"],
        vec!["verify", a, "--reference", b],
        vec!["verify", a, "--reference", b, "--json"],
    ] {
        let (code, out, err) = go(&args);
        assert_eq!(
            code, EXIT_FAILED,
            "{args:?}: the damaged files are findings"
        );
        assert!(
            !out.contains(MARKER) && !err.contains(MARKER),
            "{args:?} printed file content"
        );
    }
}

#[test]
fn a_folder_inside_a_git_working_tree_is_refused_unless_allowed_for_that_run() {
    let root = temp("in-repo");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    let inside = root.join("lod");
    std::fs::create_dir_all(&inside).unwrap();
    let outside = temp("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let (code, out, err) = go(&["verify", inside.to_str().unwrap()]);
    assert_eq!(code, EXIT_USAGE);
    assert!(out.is_empty(), "nothing is reported");
    assert!(
        err.contains("git working tree")
            && err.contains("no file was read")
            && err.contains("--allow-in-repo"),
        "{err}"
    );
    // a reference inside a working tree is refused the same way
    let (code, _, err) = go(&[
        "verify",
        outside.to_str().unwrap(),
        "--reference",
        inside.to_str().unwrap(),
    ]);
    assert_eq!(code, EXIT_USAGE, "{err}");
    // the flag lifts the refusal for that run only
    assert_eq!(
        go(&["verify", inside.to_str().unwrap(), "--allow-in-repo"]).0,
        EXIT_OK
    );
    assert_eq!(go(&["verify", inside.to_str().unwrap()]).0, EXIT_USAGE);
}
