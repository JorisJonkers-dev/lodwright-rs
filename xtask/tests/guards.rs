// SPDX-License-Identifier: GPL-3.0-only
//! Tests of the clean-room guards, in throwaway git repositories (positive and negative cases).
#![allow(
    clippy::assert_is_empty,
    clippy::format_collect,
    clippy::format_push_string,
    clippy::needless_borrows_for_generic_args
)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use xtask::guards::{self, Check, FIXTURE_MAX_BYTES, GAME_DATA_EXTS, MAX_ALLOWLIST, MAX_BYTES};

const SPDX: &str = "// SPDX-License-Identifier: GPL-3.0-only\n";
const ROW: &str = "[[source]]
id = \"dds-header\"
fact = \"The DDS header is 124 bytes after the magic\"
url = \"https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header\"
licence = \"CC-BY-4.0\"
date = 2026-11-13
recorded_by = \"tester\"
verified = true
";
const LOG: &str = "# Log\n\n- `dds-header`: recorded.\n";
const DDS_URL: &str = "https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header";
const ALLOWLIST: &str = "provenance/fixture-allowlist.txt";

fn ignore() -> String {
    GAME_DATA_EXTS.iter().map(|e| format!("*{e}\n")).collect()
}

/// A comment line built at run time, so this file never contains a literal reference marker.
fn reference(marker: &str, ids: &str) -> String {
    format!("{marker} {}: {ids}\n", "source")
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Repo {
    root: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Repo {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("xtask-guards-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let repo = Self { root };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.write("provenance/sources.toml", ROW);
        repo.write("provenance/LOG.md", LOG);
        repo.write(".gitignore", &ignore());
        repo.write("lib.rs", &format!("{SPDX}fn main() {{}}\n"));
        repo.commit();
        repo.git(&["tag", "base"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            // No user or system git config: their hooks and signing settings must not reach throwaway repositories.
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    fn write(&self, rel: &str, content: impl AsRef<[u8]>) {
        let path = self.root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn commit(&self) {
        // force: the data guard must catch what a careless force-add commits
        self.git(&["add", "-A", "-f"]);
        self.git(&["commit", "-q", "-m", "change"]);
    }

    fn check(&self, check: Check, base: Option<&str>) -> Vec<String> {
        guards::run(&self.root, check, base).unwrap().0
    }
}

fn any_contains(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|l| l.contains(needle))
}

fn real_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

// --- spdx --------------------------------------------------------------------------------------------------------

#[test]
fn a_clean_tree_passes() {
    assert_eq!(Repo::new().check(Check::All, None), Vec::<String>::new());
}

#[test]
fn a_new_rust_file_without_a_header_fails() {
    let repo = Repo::new();
    repo.write("new.rs", "fn x() {}\n");
    repo.commit();
    let result = repo.check(Check::Spdx, Some("base"));
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("new.rs"));
}

#[test]
fn a_new_rust_file_with_a_header_passes() {
    let repo = Repo::new();
    repo.write("new.rs", format!("{SPDX}fn x() {{}}\n"));
    repo.commit();
    assert!(repo.check(Check::Spdx, Some("base")).is_empty());
}

#[test]
fn python_and_shell_files_need_headers_too() {
    let repo = Repo::new();
    repo.write("a.py", "print(1)\n");
    repo.write("b.sh", "#!/usr/bin/env bash\necho hi\n");
    repo.write(
        "c.sh",
        "#!/usr/bin/env bash\n# SPDX-License-Identifier: GPL-3.0-only\n",
    );
    repo.commit();
    let failed = repo.check(Check::Spdx, Some("base")).join(" ");
    assert!(failed.contains("a.py"));
    assert!(failed.contains("b.sh"));
    assert!(!failed.contains("c.sh"));
}

#[test]
fn another_licence_is_not_accepted() {
    let repo = Repo::new();
    repo.write("mit.rs", "// SPDX-License-Identifier: MIT\n");
    repo.write(
        "lookalike.rs",
        "// SPDX-License-Identifier: GPL-3.0-only-only-not\n",
    );
    repo.commit();
    assert_eq!(repo.check(Check::Spdx, Some("base")).len(), 2);
}

#[test]
fn a_header_below_the_first_five_lines_does_not_count() {
    let repo = Repo::new();
    repo.write("late.rs", format!("{}{SPDX}", "//\n".repeat(5)));
    repo.commit();
    assert_eq!(repo.check(Check::Spdx, Some("base")).len(), 1);
}

#[test]
fn files_that_were_already_there_are_not_flagged_in_base_mode() {
    let repo = Repo::new();
    repo.write("old.rs", "fn old() {}\n");
    repo.commit();
    repo.git(&["tag", "second"]);
    repo.write("other.txt", "x");
    repo.commit();
    assert!(repo.check(Check::Spdx, Some("second")).is_empty());
    assert_eq!(
        repo.check(Check::Spdx, None).len(),
        1,
        "the whole-tree mode still finds it"
    );
}

// --- provenance --------------------------------------------------------------------------------------------------

fn add_ref(repo: &Repo, ids: &str) {
    repo.write(
        "r.rs",
        format!("{SPDX}{}fn x() {{}}\n", reference("//", ids)),
    );
    repo.commit();
}

#[test]
fn a_reference_with_a_row_passes() {
    let repo = Repo::new();
    add_ref(&repo, "dds-header");
    assert!(repo.check(Check::Provenance, Some("base")).is_empty());
    assert!(repo.check(Check::Provenance, None).is_empty());
}

#[test]
fn a_reference_without_a_row_fails() {
    let repo = Repo::new();
    add_ref(&repo, "vhgt-layout");
    let result = repo.check(Check::Provenance, Some("base"));
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("vhgt-layout"));
    assert!(result[0].contains("r.rs:2"));
}

#[test]
fn one_unknown_id_in_a_list_fails() {
    let repo = Repo::new();
    add_ref(&repo, "dds-header, nope");
    assert_eq!(repo.check(Check::Provenance, Some("base")).len(), 1);
}

#[test]
fn a_reference_naming_nothing_fails() {
    let repo = Repo::new();
    add_ref(&repo, "");
    assert_eq!(repo.check(Check::Provenance, Some("base")).len(), 1);
}

#[test]
fn python_and_toml_comments_count() {
    let repo = Repo::new();
    repo.write(
        "a.py",
        format!(
            "# SPDX-License-Identifier: GPL-3.0-only\n{}",
            reference("#", "missing-row")
        ),
    );
    repo.write("b.toml", reference("#", "missing-too"));
    repo.commit();
    assert_eq!(repo.check(Check::Provenance, Some("base")).len(), 2);
}

#[test]
fn prose_that_mentions_a_source_is_not_a_reference() {
    let repo = Repo::new();
    repo.write(
        "notes.md",
        format!("{}the source: x\n", reference("#", "heading")),
    );
    repo.write(
        "code.rs",
        format!(
            "{SPDX}let s = \"{}\";\n",
            reference("//", "inside a string").trim_end()
        ),
    );
    repo.commit();
    assert!(repo.check(Check::Provenance, Some("base")).is_empty());
}

#[test]
fn only_added_lines_are_checked_in_base_mode() {
    let repo = Repo::new();
    add_ref(&repo, "gone");
    repo.git(&["tag", "second"]);
    repo.write("r.rs", format!("{SPDX}fn y() {{}}\n")); // the reference is removed again
    repo.commit();
    assert!(repo.check(Check::Provenance, Some("second")).is_empty());
}

fn with_rows(repo: &Repo, rows: &str) -> Vec<String> {
    repo.write("provenance/sources.toml", rows);
    repo.commit();
    repo.check(Check::Provenance, None)
}

#[test]
fn a_row_with_a_missing_field_fails() {
    let repo = Repo::new();
    let result = with_rows(&repo, &ROW.replace("licence = \"CC-BY-4.0\"\n", ""));
    assert!(any_contains(&result, "missing field(s): licence"));
}

#[test]
fn an_unknown_licence_is_refused() {
    let repo = Repo::new();
    assert!(any_contains(
        &with_rows(&repo, &ROW.replace("CC-BY-4.0", "unknown")),
        "licence"
    ));
}

#[test]
fn the_unlicensed_lodgen_repository_is_refused_as_a_source() {
    let repo = Repo::new();
    let bad = ROW.replace(
        DDS_URL,
        "https://github.com/TES5Edit/xLODGen/blob/master/x.cs",
    );
    assert!(any_contains(&with_rows(&repo, &bad), "forbidden input"));
}

#[test]
fn decompiled_code_is_refused_as_a_source() {
    let repo = Repo::new();
    assert!(any_contains(
        &with_rows(&repo, &ROW.replace("124 bytes", "124 bytes (decompiled)")),
        "forbidden input"
    ));
}

#[test]
fn the_unlicensed_decompilation_reconstruction_is_refused_as_a_source() {
    let repo = Repo::new();
    assert!(any_contains(
        &with_rows(&repo, &ROW.replace("124 bytes", "124 bytes (fnv_decomp)")),
        "forbidden input"
    ));
}

#[test]
fn the_url_must_be_https_or_an_oracle_role() {
    let repo = Repo::new();
    assert!(any_contains(
        &with_rows(&repo, &ROW.replace("https://learn", "http://learn")),
        "url must start"
    ));
    assert!(with_rows(&repo, &ROW.replace(DDS_URL, "oracle:lod-file")).is_empty());
}

#[test]
fn duplicate_ids_and_bad_dates_fail() {
    let repo = Repo::new();
    assert!(any_contains(
        &with_rows(&repo, &format!("{ROW}\n{ROW}")),
        "duplicate id"
    ));
    assert!(any_contains(
        &with_rows(
            &repo,
            &ROW.replace("date = 2026-11-13", "date = \"yesterday\"")
        ),
        "YYYY-MM-DD"
    ));
}

#[test]
fn a_row_the_log_does_not_mention_fails() {
    let repo = Repo::new();
    repo.write("provenance/LOG.md", "# Log\n");
    repo.commit();
    assert!(any_contains(&repo.check(Check::Provenance, None), "LOG.md"));
}

#[test]
fn broken_toml_fails() {
    let repo = Repo::new();
    assert!(any_contains(
        &with_rows(&repo, "[[source]\n"),
        "not valid TOML"
    ));
}

fn engine_row(url: &str, licence: &str, reader: Option<&str>, notes: Option<&str>) -> String {
    let mut row = ROW.replace(DDS_URL, url).replace("CC-BY-4.0", licence);
    if let Some(reader) = reader {
        row.push_str(&format!("second_reader = \"{reader}\"\n"));
    }
    if let Some(notes) = notes {
        row.push_str(&format!("engine_notes = \"{notes}\"\n"));
    }
    row
}

fn engine_url(owner: &str, commit: &str) -> String {
    format!("https://github.com/{owner}/nv-rs/blob/{commit}/crates/bsa/src/archive.rs")
}

fn good_engine_row() -> String {
    engine_row(
        &engine_url("JorisJonkers-dev", &"a".repeat(40)),
        "GPL-3.0-or-later",
        Some("someone else"),
        Some("LEGAL.md third-party list read; original code"),
    )
}

#[test]
fn an_engine_port_row_with_a_file_at_a_commit_a_licence_and_a_reader_passes() {
    let repo = Repo::new();
    assert!(with_rows(&repo, &good_engine_row()).is_empty());
}

#[test]
fn an_engine_port_row_under_mit_with_its_notice_noted_passes() {
    let repo = Repo::new();
    let row = engine_row(
        &engine_url("slaterain", &"a".repeat(40)),
        "MIT",
        Some("x"),
        Some("n"),
    ) + "licence_notice = \"kept in LEGAL.md\"\n";
    assert!(with_rows(&repo, &row).is_empty());
}

#[test]
fn an_engine_port_row_under_mit_without_its_notice_fails() {
    let repo = Repo::new();
    let row = engine_row(
        &engine_url("slaterain", &"a".repeat(40)),
        "MIT",
        Some("x"),
        Some("n"),
    );
    assert!(any_contains(&with_rows(&repo, &row), "licence_notice"));
}

#[test]
fn an_engine_port_row_without_a_second_reader_fails() {
    let repo = Repo::new();
    let row = engine_row(
        &engine_url("slaterain", &"b".repeat(40)),
        "GPL-3.0-or-later",
        None,
        Some("n"),
    );
    assert!(any_contains(&with_rows(&repo, &row), "second_reader"));
}

#[test]
fn an_engine_port_row_without_the_third_party_check_fails() {
    let repo = Repo::new();
    let row = engine_row(
        &engine_url("slaterain", &"b".repeat(40)),
        "GPL-3.0-or-later",
        Some("x"),
        None,
    );
    assert!(any_contains(&with_rows(&repo, &row), "engine_notes"));
}

#[test]
fn an_engine_port_row_on_a_branch_instead_of_a_commit_fails() {
    let repo = Repo::new();
    let row = engine_row(
        &engine_url("slaterain", "main"),
        "GPL-3.0-or-later",
        Some("someone else"),
        Some("n"),
    );
    assert!(any_contains(&with_rows(&repo, &row), "40-character commit"));
}

#[test]
fn an_engine_port_row_must_cite_a_file_not_a_directory_query_or_fragment() {
    for tail in [
        "/crates/bsa/",
        "/crates/bsa/src/a.rs?plain=1",
        "/crates/bsa/src/a.rs#L10",
    ] {
        let repo = Repo::new();
        let url = format!(
            "https://github.com/slaterain/nv-rs/blob/{}{tail}",
            "a".repeat(40)
        );
        let row = engine_row(&url, "GPL-3.0-or-later", Some("x"), Some("n"));
        assert!(
            any_contains(&with_rows(&repo, &row), "40-character commit"),
            "{tail}"
        );
    }
}

#[test]
fn an_engine_port_row_through_a_raw_or_mirror_url_fails() {
    let commit = "a".repeat(40);
    for url in [
        format!("https://raw.githubusercontent.com/slaterain/nv-rs/{commit}/crates/bsa/src/a.rs"),
        format!("https://gitlab.com/someone/nv-rs/-/blob/{commit}/a.rs"),
        format!("https://github.com/Slaterain/NV-RS/blob/{commit}/../../other/a.rs"),
    ] {
        let repo = Repo::new();
        let row = engine_row(&url, "GPL-3.0-or-later", Some("x"), Some("n"));
        assert!(
            any_contains(&with_rows(&repo, &row), "40-character commit"),
            "{url}"
        );
    }
}

#[test]
fn an_engine_port_row_must_name_exactly_one_engine_licence() {
    for licence in [
        "CC-BY-4.0",
        "BSD-3-Clause AND GPL-3.0-or-later",
        "GPL-3.0-or-later AND BSD-3-Clause",
        "GPL-3.0-or-later-ish",
        "MIT OR Apache-2.0",
        "GPL-3.0-only",
    ] {
        let repo = Repo::new();
        let row = engine_row(
            &engine_url("slaterain", &"d".repeat(40)),
            licence,
            Some("x"),
            Some("n"),
        );
        assert!(
            any_contains(&with_rows(&repo, &row), "licence"),
            "{licence}"
        );
    }
}

#[test]
fn the_engine_licence_check_ignores_case() {
    let repo = Repo::new();
    let row = engine_row(
        &engine_url("slaterain", &"d".repeat(40)),
        "gpl-3.0-or-later",
        Some("x"),
        Some("n"),
    );
    assert!(with_rows(&repo, &row).is_empty());
}

// --- data guard --------------------------------------------------------------------------------------------------

#[test]
fn every_game_data_extension_is_refused() {
    let repo = Repo::new();
    for ext in GAME_DATA_EXTS {
        repo.write(&format!("data/file{ext}"), "x");
    }
    repo.commit();
    assert_eq!(
        repo.check(Check::Data, Some("base")).len(),
        GAME_DATA_EXTS.len()
    );
}

#[test]
fn the_extension_check_ignores_case_and_looks_at_every_suffix() {
    let repo = Repo::new();
    repo.write("a/FalloutNV.ESM", "x");
    repo.write("a/backup.esp.bak", "x");
    repo.commit();
    assert_eq!(repo.check(Check::Data, None).len(), 2);
}

#[test]
fn ordinary_text_and_source_files_pass() {
    let repo = Repo::new();
    repo.write("docs/lod.md", "# LOD\n");
    repo.write("Cargo.lock", "# lock\n");
    repo.commit();
    assert!(repo.check(Check::Data, None).is_empty());
}

#[test]
fn a_large_file_is_refused() {
    let repo = Repo::new();
    repo.write(
        "big.txt",
        "a".repeat(usize::try_from(MAX_BYTES).unwrap() + 1),
    );
    repo.commit();
    assert!(any_contains(&repo.check(Check::Data, None), "byte limit"));
}

#[test]
fn a_binary_file_is_refused_whatever_its_name() {
    let repo = Repo::new();
    repo.write("blob.dat", [0_u8, 1, 2]);
    repo.commit();
    assert!(any_contains(
        &repo.check(Check::Data, None),
        "binary content"
    ));
}

#[test]
fn an_allow_listed_fixture_passes_and_others_still_fail() {
    let repo = Repo::new();
    repo.write("fixtures/tiny.dds", b"DDS \x00");
    repo.write("fixtures/other.dds", b"DDS ");
    repo.write(
        ALLOWLIST,
        "# synthetic\nfixtures/tiny.dds generated by the testkit\n",
    );
    repo.commit();
    let result = repo.check(Check::Data, None);
    assert_eq!(result.len(), 1);
    assert!(result[0].contains("other.dds"));
}

#[test]
fn an_allow_list_entry_needs_a_reason_and_no_glob() {
    let repo = Repo::new();
    repo.write(ALLOWLIST, "fixtures/a.dds\nfixtures/*.dds why\n");
    repo.commit();
    let result = repo.check(Check::Data, None).join(" ");
    assert!(result.contains("has no reason"));
    assert!(result.contains("no globs"));
}

#[test]
fn the_allow_list_must_stay_tiny() {
    let repo = Repo::new();
    let list: String = (0..=MAX_ALLOWLIST)
        .map(|i| format!("f/{i}.dds r\n"))
        .collect();
    repo.write(ALLOWLIST, list);
    repo.commit();
    assert!(any_contains(&repo.check(Check::Data, None), "tiny"));
}

#[test]
fn an_allow_listed_fixture_has_a_size_cap() {
    let repo = Repo::new();
    repo.write(
        "f/big.dds",
        vec![b'a'; usize::try_from(FIXTURE_MAX_BYTES).unwrap() + 1],
    );
    repo.write(ALLOWLIST, "f/big.dds r\n");
    repo.commit();
    assert!(any_contains(&repo.check(Check::Data, None), "limit"));
}

#[test]
fn base_mode_only_looks_at_what_the_branch_adds() {
    let repo = Repo::new();
    repo.write("old.esm", "x");
    repo.commit();
    repo.git(&["tag", "second"]);
    repo.write("fine.txt", "x");
    repo.commit();
    assert!(repo.check(Check::Data, Some("second")).is_empty());
    assert_eq!(repo.check(Check::Data, None).len(), 1);
}

// --- names -------------------------------------------------------------------------------------------------------

#[test]
fn the_old_name_in_a_source_file_fails() {
    let repo = Repo::new();
    repo.write("lib.rs", format!("{SPDX}// port of xLODGen\n"));
    repo.commit();
    assert_eq!(repo.check(Check::Names, None).len(), 1);
}

#[test]
fn the_old_name_in_a_path_fails() {
    let repo = Repo::new();
    repo.write("docs/xlodgen-notes.txt", "notes\n");
    repo.commit();
    assert_eq!(repo.check(Check::Names, None).len(), 1);
}

#[test]
fn the_old_name_in_provenance_notes_and_the_process_document_passes() {
    let repo = Repo::new();
    repo.write("provenance/LOG.md", format!("{LOG}Read about LODGen.\n"));
    repo.write("docs/clean-room.md", "never read the LODGen source\n");
    repo.commit();
    assert!(repo.check(Check::Names, None).is_empty());
}

#[test]
fn case_does_not_hide_the_old_name() {
    let repo = Repo::new();
    repo.write("README.md", "XLODGEN\n");
    repo.commit();
    assert_eq!(repo.check(Check::Names, None).len(), 1);
}

#[test]
fn the_real_tree_is_free_of_the_old_name() {
    assert!(
        guards::run(real_root(), Check::Names, None)
            .unwrap()
            .0
            .is_empty()
    );
}

// --- gitignore ---------------------------------------------------------------------------------------------------

#[test]
fn a_missing_extension_is_reported() {
    let repo = Repo::new();
    repo.write(".gitignore", ignore().replace("*.esm\n", ""));
    repo.commit();
    assert_eq!(
        repo.check(Check::Gitignore, None),
        [".gitignore: does not ignore *.esm"]
    );
}

#[test]
fn the_real_gitignore_lists_every_extension() {
    assert!(
        guards::run(real_root(), Check::Gitignore, None)
            .unwrap()
            .0
            .is_empty()
    );
}

// --- command line ------------------------------------------------------------------------------------------------

#[test]
fn exit_codes() {
    let repo = Repo::new();
    let root = repo.root.to_string_lossy().into_owned();
    assert_eq!(
        guards::main(&["--root".into(), root.clone(), "all".into()]),
        0
    );
    repo.write("x.esm", "x");
    repo.commit();
    assert_eq!(
        guards::main(&["--root".into(), root.clone(), "data".into()]),
        1
    );
    assert_eq!(guards::main(&["--root".into(), root, "nothing".into()]), 2);
}
