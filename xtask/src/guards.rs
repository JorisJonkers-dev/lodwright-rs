// SPDX-License-Identifier: GPL-3.0-only
//! Clean-room guards: SPDX headers, provenance references, the game-data guard, `.gitignore` and the retired name.
//!
//! `cargo xtask guards [--root DIR] [--base REF] {spdx,provenance,data,gitignore,names,all}`
//!
//! Without `--base` every tracked file is checked. With `--base REF` only what the current branch adds relative to
//! REF (`git diff REF...HEAD`) is checked: new files for the SPDX and data checks, added lines for the provenance
//! references. Exit status 0 when clean, 1 when a rule is broken (one line per violation), 2 on misuse.
//!
//! Rules (`docs/clean-room.md` explains why):
//! - spdx: every `.rs`, `.py` and `.sh` file has `SPDX-License-Identifier: GPL-3.0-only` (or `MPL-2.0`) in its first
//!   five lines.
//! - provenance: every `// source: <id>[, <id>]` (`# source:` in Python, shell, TOML and YAML) names a row of
//!   `provenance/sources.toml` or `provenance/rows/<id>.toml` (one row per file; the table and `LOG.md` are frozen and a
//!   pull request may not add rows or entries to them); the rows are well formed (fields, unique ids, licence stated, no forbidden source) and
//!   each id of the frozen table appears in `provenance/LOG.md` (a row file carries its own `note`); a row citing the engine port names one file at a commit, its licence and
//!   a `second_reader` and `engine_notes` (`docs/adr/0001-engine-source-as-a-format-input.md`).
//! - data: no file with a game-data extension, no file over [`MAX_BYTES`] and no file with NUL bytes, except the paths
//!   in `provenance/fixture-allowlist.txt` (`<path> <reason>`, at most [`MAX_ALLOWLIST`] entries).
//! - gitignore: `.gitignore` lists every game-data extension.
//! - names: the name of the tool this project was named after appears nowhere, in file contents or paths, except
//!   the provenance notes and the files in [`NAME_ALLOWED_FILES`].

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

/// Source files that need an SPDX header.
const SOURCE_EXTS: [&str; 3] = ["rs", "py", "sh"];
/// The licences an SPDX header may name.
const SPDX_LICENCES: [&str; 2] = ["MPL-2.0", "GPL-3.0-only"];
const SPDX_TAG: &str = "SPDX-License-Identifier:";
/// How many leading lines may hold the SPDX header.
const SPDX_LINES: usize = 5;

const SOURCES_FILE: &str = "provenance/sources.toml";
/// New rows live one per file here (`<id>.toml` holding one `[[source]]`); `sources.toml` and `LOG.md` are frozen.
const ROWS_DIR: &str = "provenance/rows";
const LOG_FILE: &str = "provenance/LOG.md";
const ALLOWLIST_FILE: &str = "provenance/fixture-allowlist.txt";
const REQUIRED_FIELDS: [&str; 7] = [
    "id",
    "fact",
    "url",
    "licence",
    "date",
    "recorded_by",
    "verified",
];
const URL_PREFIXES: [&str; 2] = ["https://", "oracle:"];
const BAD_LICENCES: [&str; 7] = [
    "",
    "unknown",
    "none",
    "unlicensed",
    "all rights reserved",
    "n/a",
    "tbd",
];
/// Inputs the clean-room rule forbids, matched case-insensitively against the url and the fact.
const FORBIDDEN_SOURCES: [&str; 5] = [
    "github.com/tes5edit/xlodgen",
    "decompil",
    "disassembl",
    "leaked",
    "fnv_decomp",
];
/// The engine port's name (lower case): a row whose url or fact names it follows `docs/adr/0001`.
const ENGINE_NAME: &str = "nv-rs";
/// The only url form an engine-port row may use (lower case): the GitHub blob of one file at a commit.
const ENGINE_BLOB_PREFIXES: [&str; 2] = [
    "https://github.com/slaterain/nv-rs/blob/",
    "https://github.com/jorisjonkers-dev/nv-rs/blob/",
];
/// The SPDX identifiers an engine-port row may state, exactly (lower case); all are GPL-3.0 compatible and an expression
/// such as `A AND B` is refused. Code with no licence, or one that cannot be identified (`fnv_decomp`), is refused.
const ENGINE_LICENCES: [&str; 8] = [
    "gpl-3.0-or-later",
    "gpl-3.0-only",
    "mit",
    "bsd-2-clause",
    "bsd-3-clause",
    "apache-2.0",
    "apache-2.0 with llvm-exception",
    "zlib",
];
/// Where the notices of third-party code taken from the engine port are kept.
const NOTICES_FILE: &str = "THIRD-PARTY-NOTICES.md";

/// The retired name, matched case-insensitively.
const OLD_NAME: &str = "lodgen";
const NAME_ALLOWED_PREFIXES: [&str; 1] = ["provenance/"];
/// Files where the lineage is recorded and the retired name may appear.
pub const NAME_ALLOWED_FILES: [&str; 5] = [
    // the folder under which mods ship tree billboards is named after the old tool: a path the data folder uses
    "crates/lodwright-gen/src/trees.rs",
    "docs/clean-room.md",
    "xtask/src/guards.rs",
    "xtask/tests/guards.rs",
    "CHANGELOG.md",
];

/// Extensions of game data and tool output; never committed.
pub const GAME_DATA_EXTS: [&str; 17] = [
    ".esm", ".esp", ".esl", ".bsa", ".ba2", ".nif", ".kf", ".dds", ".btr", ".bto", ".btt", ".lod",
    ".wav", ".ogg", ".mp3", ".fos", ".ess",
];
/// Largest committed file, in bytes.
pub const MAX_BYTES: u64 = 256 * 1024;
/// Most entries the fixture allow-list may hold.
pub const MAX_ALLOWLIST: usize = 10;
/// Largest allow-listed fixture, in bytes.
pub const FIXTURE_MAX_BYTES: u64 = 64 * 1024;

/// Which checks to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// SPDX headers.
    Spdx,
    /// Provenance rows and references.
    Provenance,
    /// The game-data guard.
    Data,
    /// The `.gitignore` list.
    Gitignore,
    /// The retired name.
    Names,
    /// Everything.
    All,
}

impl Check {
    /// Parse a command-line name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "spdx" => Self::Spdx,
            "provenance" => Self::Provenance,
            "data" => Self::Data,
            "gitignore" => Self::Gitignore,
            "names" => Self::Names,
            "all" => Self::All,
            _ => return None,
        })
    }

    fn includes(self, other: Self) -> bool {
        self == Self::All || self == other
    }
}

/// Every broken rule, one line each (`<path>: <message>`).
#[derive(Debug, Default)]
pub struct Violations(pub Vec<String>);

impl Violations {
    fn add(&mut self, path: &str, message: &str) {
        self.0.push(format!("{path}: {message}"));
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git {} failed: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn split_nul(text: &str) -> Vec<String> {
    text.split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Lines added by the branch, per file.
fn added_lines(root: &Path, base: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let range = format!("{base}...HEAD");
    let out = git(
        root,
        &["diff", "-U0", "--no-color", "--diff-filter=AMR", &range],
    )?;
    let mut lines: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            current = rest.strip_prefix("b/").map(str::to_owned);
            if let Some(path) = &current {
                lines.entry(path.clone()).or_default();
            }
        } else if let (Some(path), Some(added)) = (&current, line.strip_prefix('+')) {
            lines
                .entry(path.clone())
                .or_default()
                .push(added.to_owned());
        }
    }
    Ok(lines)
}

fn read_text(root: &Path, rel: &str) -> Option<String> {
    std::fs::read(root.join(rel))
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
}

fn extension(rel: &str) -> &str {
    Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
}

/// The suffixes of a file name the way Python's `PurePath.suffixes` gives them (`a.b.c` gives `.b`, `.c`; leading
/// dots are not suffixes; a name ending in a dot has none).
fn suffixes(rel: &str) -> Vec<String> {
    let name = Path::new(rel)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if name.ends_with('.') {
        return Vec::new();
    }
    name.trim_start_matches('.')
        .split('.')
        .skip(1)
        .map(|s| format!(".{s}"))
        .collect()
}

// --- spdx --------------------------------------------------------------------------------------------------------

fn has_spdx(head: &str) -> bool {
    let mut rest = head;
    while let Some(at) = rest.find(SPDX_TAG) {
        let after = rest[at + SPDX_TAG.len()..].trim_start();
        for licence in SPDX_LICENCES {
            if let Some(tail) = after.strip_prefix(licence) {
                let next = tail.chars().next();
                if !next.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-')) {
                    return true;
                }
            }
        }
        rest = &rest[at + SPDX_TAG.len()..];
    }
    false
}

fn check_spdx(root: &Path, files: &[String], v: &mut Violations) {
    for rel in files {
        if !SOURCE_EXTS.contains(&extension(rel)) {
            continue;
        }
        let Some(text) = read_text(root, rel) else {
            v.add(rel, "cannot be read as UTF-8 text");
            continue;
        };
        let head = text.lines().take(SPDX_LINES).collect::<Vec<_>>().join("\n");
        if !has_spdx(&head) {
            v.add(
                rel,
                &format!(
                    "no `SPDX-License-Identifier: GPL-3.0-only` (or MPL-2.0, see docs/LICENSING.md) in the first {SPDX_LINES} lines"
                ),
            );
        }
    }
}

// --- provenance --------------------------------------------------------------------------------------------------

fn is_id(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_iso_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return false;
    };
    let number = |s: &str, len: usize| s.len() == len && s.chars().all(|c| c.is_ascii_digit());
    if !(number(y, 4) && number(m, 2) && number(d, 2)) {
        return false;
    }
    let (year, month, day): (u32, u32, u32) = (
        y.parse().unwrap_or(0),
        m.parse().unwrap_or(0),
        d.parse().unwrap_or(0),
    );
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year >= 1 && (1..=days).contains(&day)
}

/// The rows of one file: `label` names the file in messages.
fn parse_rows(label: &str, text: &str, v: &mut Violations) -> Vec<toml::Value> {
    let table: toml::Table = match text.parse() {
        Ok(table) => table,
        Err(error) => {
            v.add(
                label,
                &format!("not valid TOML: {}", error.to_string().trim()),
            );
            return Vec::new();
        }
    };
    table
        .get("source")
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Every row: the frozen `sources.toml` first, then one file per row under [`ROWS_DIR`] in name order. `frozen` gets
/// the ids of the frozen table, the only ones `LOG.md` must mention (a row file carries its own note).
fn load_sources(
    root: &Path,
    v: &mut Violations,
    notices_needed: &mut BTreeSet<String>,
    frozen: &mut BTreeSet<String>,
) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut batches: Vec<(String, Vec<toml::Value>, bool)> = Vec::new();
    let Some(text) = read_text(root, SOURCES_FILE) else {
        let message = if root.join(SOURCES_FILE).exists() {
            "not valid TOML: not UTF-8"
        } else {
            "missing"
        };
        v.add(SOURCES_FILE, message);
        return ids;
    };
    batches.push((
        SOURCES_FILE.to_owned(),
        parse_rows(SOURCES_FILE, &text, v),
        true,
    ));
    let mut files: Vec<String> = std::fs::read_dir(root.join(ROWS_DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| Path::new(n).extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    for name in files {
        let rel = format!("{ROWS_DIR}/{name}");
        let Some(text) = read_text(root, &rel) else {
            v.add(&rel, "not valid TOML: not UTF-8");
            continue;
        };
        let rows = parse_rows(&rel, &text, v);
        if rows.len() != 1 {
            v.add(&rel, "must hold exactly one [[source]] row");
        }
        batches.push((rel, rows, false));
    }
    for (file, rows, is_frozen) in &batches {
        for (index, row) in rows.iter().enumerate() {
            let label = format!("{file} row {}", index + 1);
            let Some(row) = row.as_table() else {
                v.add(&label, "is not a table");
                continue;
            };
            let missing: Vec<&str> = REQUIRED_FIELDS
                .iter()
                .copied()
                .filter(|f| !row.contains_key(*f))
                .collect();
            if !missing.is_empty() {
                v.add(&label, &format!("missing field(s): {}", missing.join(", ")));
                continue;
            }
            let id = row["id"].as_str().unwrap_or("");
            if !is_id(id) {
                v.add(
                    &label,
                    &format!(
                        "id {} must be lowercase letters, digits and hyphens",
                        value_text(&row["id"])
                    ),
                );
                continue;
            }
            let label = format!("{file} row {id}");
            if !*is_frozen && *file != format!("{ROWS_DIR}/{id}.toml") {
                v.add(&label, "a row file is named after its id (<id>.toml)");
            }
            if !ids.insert(id.to_owned()) {
                v.add(&label, "duplicate id");
                continue;
            }
            if *is_frozen {
                frozen.insert(id.to_owned());
            }
            check_row(&label, row, v);
            if needs_notice(row) {
                notices_needed.insert(id.to_owned());
            }
        }
    }
    ids
}

fn check_row(label: &str, row: &toml::Table, v: &mut Violations) {
    for field in ["fact", "url", "licence", "recorded_by"] {
        if row[field].as_str().is_none_or(|s| s.trim().is_empty()) {
            v.add(label, &format!("{field} must be a non-empty string"));
        }
    }
    if !row["verified"].is_bool() {
        v.add(label, "verified must be true or false");
    }
    match &row["date"] {
        toml::Value::String(stamp) if !is_iso_date(stamp) => {
            v.add(label, &format!("date '{stamp}' is not YYYY-MM-DD"));
        }
        toml::Value::String(_) => {}
        toml::Value::Datetime(stamp) if stamp.date.is_some() && stamp.time.is_none() => {}
        _ => v.add(label, "date must be YYYY-MM-DD"),
    }
    let url = value_text(&row["url"]);
    if !URL_PREFIXES.iter().any(|p| url.starts_with(p)) {
        v.add(
            label,
            "url must start with https:// or oracle: (the owner's own tool output, by role)",
        );
    }
    let licence = value_text(&row["licence"]);
    if BAD_LICENCES.contains(&licence.trim().to_lowercase().as_str()) {
        v.add(
            label,
            "the licence of the source must be stated; an unknown licence is a forbidden input",
        );
    }
    let haystack = format!("{url} {}", value_text(&row["fact"])).to_lowercase();
    for needle in FORBIDDEN_SOURCES {
        if haystack.contains(needle) {
            v.add(label, &format!("names a forbidden input ({needle})"));
        }
    }
    if haystack.contains(ENGINE_NAME) {
        check_engine_row(label, row, &url.to_lowercase(), &licence, v);
    }
}

fn value_text(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

/// A row naming the engine port: one file at a 40-character commit on GitHub (a raw url, a mirror, a directory, a
/// branch, a query or a fragment is refused), exactly one licence the rule covers, the second reader, and a note that
/// the engine's third-party list was checked.
fn check_engine_row(
    label: &str,
    row: &toml::Table,
    lower_url: &str,
    licence: &str,
    v: &mut Violations,
) {
    let at_commit = ENGINE_BLOB_PREFIXES.iter().any(|prefix| {
        lower_url.strip_prefix(prefix).is_some_and(|rest| {
            let (commit, path) = rest.split_at(rest.len().min(40));
            commit.len() == 40
                && commit.chars().all(|c| c.is_ascii_hexdigit())
                && path.len() > 1
                && path.starts_with('/')
                && !path.ends_with('/')
                && !path.contains(['?', '#'])
                && !path.chars().any(char::is_whitespace)
                && !path.split('/').any(|part| part == "..")
        })
    });
    if !at_commit {
        v.add(label, "an engine-port row cites one file at a 40-character commit (https://github.com/<owner>/nv-rs/blob/<commit>/<path>)");
    }
    let licence = licence.trim().to_lowercase();
    if !ENGINE_LICENCES.contains(&licence.as_str()) {
        v.add(
            label,
            &format!(
                "an engine-port row states the SPDX identifier of the code it relies on, exactly one of: {}",
                ENGINE_LICENCES.join(", ")
            ),
        );
    }
    let mut fields = vec![
        (
            "second_reader",
            "the reader, who is not the implementer of the format",
        ),
        (
            "engine_notes",
            "the GPL-3.0 compatibility reasoning and what was checked in the engine's LEGAL.md",
        ),
    ];
    if needs_notice(row) {
        fields.push((
            "licence_notice",
            "the copyright line and licence notice kept, and where",
        ));
    }
    for (field, why) in fields {
        if row
            .get(field)
            .and_then(toml::Value::as_str)
            .is_none_or(|r| r.trim().is_empty())
        {
            v.add(label, &format!("an engine-port row names {field}: {why}"));
        }
    }
}

/// An engine-port row whose code is not the engine's own GPL-3.0-or-later work: its notice must be kept.
fn needs_notice(row: &toml::Table) -> bool {
    let text = |key: &str| row.get(key).and_then(toml::Value::as_str).unwrap_or("");
    let haystack = format!("{} {}", text("url"), text("fact")).to_lowercase();
    haystack.contains(ENGINE_NAME) && text("licence").trim().to_lowercase() != "gpl-3.0-or-later"
}

/// `THIRD-PARTY-NOTICES.md` holds one section headed with the row id in backticks, with text, for each engine-port
/// row that needs a notice, and no section for anything else.
fn check_notices(root: &Path, needed: &BTreeSet<String>, v: &mut Violations) {
    let Some(text) = read_text(root, NOTICES_FILE) else {
        for id in needed {
            v.add(
                NOTICES_FILE,
                &format!(
                    "missing: row `{id}` relies on third-party code whose notice must be kept here"
                ),
            );
        }
        return;
    };
    let mut sections: BTreeMap<String, usize> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let heading = line
            .trim_end()
            .strip_prefix("## `")
            .and_then(|rest| rest.strip_suffix('`'))
            .filter(|id| !id.contains('`'));
        if let Some(id) = heading {
            sections.insert(id.to_owned(), 0);
            current = Some(id.to_owned());
        } else if let Some(count) = current
            .as_ref()
            .and_then(|id| sections.get_mut(id))
            .filter(|_| !line.trim().is_empty())
        {
            *count += 1;
        }
    }
    for id in needed.iter().filter(|id| !sections.contains_key(*id)) {
        v.add(
            NOTICES_FILE,
            &format!("has no section `## `{id}`` for the row that relies on third-party code"),
        );
    }
    for (id, count) in &sections {
        if !needed.contains(id) {
            v.add(
                NOTICES_FILE,
                &format!("section `{id}` is not an engine-port row that needs a notice"),
            );
        } else if *count == 0 {
            v.add(
                NOTICES_FILE,
                &format!("section `{id}` has no notice text (copyright line and licence)"),
            );
        }
    }
}

fn comment_marker(rel: &str) -> Option<&'static str> {
    match extension(rel) {
        "rs" => Some("//"),
        "py" | "sh" | "toml" | "yml" | "yaml" => Some("#"),
        _ => None,
    }
}

/// The ids of a `source:` comment line, or `None` when the line is not one.
fn reference(line: &str, marker: &str) -> Option<Vec<String>> {
    let rest = line
        .trim_start()
        .strip_prefix(marker)?
        .trim_start()
        .strip_prefix("source:")?;
    Some(
        rest.split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

fn check_refs(rel: &str, lines: &[String], known: &BTreeSet<String>, v: &mut Violations) {
    let Some(marker) = comment_marker(rel) else {
        return;
    };
    for (number, line) in lines.iter().enumerate() {
        let Some(ids) = reference(line, marker) else {
            continue;
        };
        let at = format!("{rel}:{}", number + 1);
        if ids.is_empty() {
            v.add(&at, "`source:` names no provenance id");
        }
        for id in ids {
            if !known.contains(&id) {
                v.add(&at, &format!("source `{id}` has no row in {SOURCES_FILE}"));
            }
        }
    }
}

fn check_provenance(
    root: &Path,
    files: &[String],
    lines: Option<&BTreeMap<String, Vec<String>>>,
    v: &mut Violations,
) {
    let mut notices_needed = BTreeSet::new();
    let mut frozen = BTreeSet::new();
    let ids = load_sources(root, v, &mut notices_needed, &mut frozen);
    check_notices(root, &notices_needed, v);
    match read_text(root, LOG_FILE) {
        None => v.add(LOG_FILE, "missing"),
        Some(log) => {
            for id in &frozen {
                if !log.contains(&format!("`{id}`")) {
                    v.add(
                        LOG_FILE,
                        &format!("does not mention source `{id}` (write it as `{id}`)"),
                    );
                }
            }
        }
    }
    match lines {
        None => {
            for rel in files {
                if let Some(text) = read_text(root, rel) {
                    let all: Vec<String> = text.lines().map(str::to_owned).collect();
                    check_refs(rel, &all, &ids, v);
                }
            }
        }
        Some(lines) => {
            for (rel, added) in lines {
                check_refs(rel, added, &ids, v);
            }
            // sources.toml and LOG.md are frozen: a new row is a new file under provenance/rows
            for (rel, added) in lines {
                let new_row =
                    rel == SOURCES_FILE && added.iter().any(|l| l.starts_with("[[source]]"));
                let new_entry = rel == LOG_FILE && added.iter().any(|l| l.starts_with("- `"));
                if new_row || new_entry {
                    v.add(
                        rel,
                        &format!("is frozen: add a new row as its own file {ROWS_DIR}/<id>.toml"),
                    );
                }
            }
        }
    }
}

// --- data guard --------------------------------------------------------------------------------------------------

fn load_allowlist(root: &Path, v: &mut Violations) -> BTreeSet<String> {
    let mut allowed = BTreeSet::new();
    let Some(text) = read_text(root, ALLOWLIST_FILE) else {
        return allowed;
    };
    for (number, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = format!("{ALLOWLIST_FILE}:{}", number + 1);
        let (path, reason) = line.split_once(' ').unwrap_or((line, ""));
        if reason.trim().is_empty() {
            v.add(&at, &format!("entry for {path} has no reason"));
            continue;
        }
        if path.contains(['*', '?', '['])
            || path.starts_with('/')
            || path.split('/').any(|p| p == "..")
        {
            v.add(&at, "entries are exact repository paths, no globs");
            continue;
        }
        allowed.insert(path.to_owned());
    }
    if allowed.len() > MAX_ALLOWLIST {
        v.add(
            ALLOWLIST_FILE,
            &format!("more than {MAX_ALLOWLIST} entries: the allow-list must stay tiny"),
        );
    }
    allowed
}

fn game_data_suffix(rel: &str) -> Option<String> {
    suffixes(rel)
        .into_iter()
        .map(|s| s.to_lowercase())
        .find(|s| GAME_DATA_EXTS.contains(&s.as_str()))
}

fn check_data(root: &Path, files: &[String], v: &mut Violations) {
    let allowed = load_allowlist(root, v);
    for rel in files {
        let path = root.join(rel);
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let size = meta.len();
        if allowed.contains(rel) {
            if size > FIXTURE_MAX_BYTES {
                v.add(
                    rel,
                    &format!(
                        "allow-listed fixture is {size} bytes; the limit is {FIXTURE_MAX_BYTES}"
                    ),
                );
            }
            continue;
        }
        if let Some(suffix) = game_data_suffix(rel) {
            v.add(rel, &format!("game-data extension {suffix}: game data and tool output never enter the repository"));
        } else if size > MAX_BYTES {
            v.add(
                rel,
                &format!("{size} bytes is over the {MAX_BYTES} byte limit for a committed file"),
            );
        } else if std::fs::read(&path).is_ok_and(|bytes| bytes.contains(&0)) {
            v.add(
                rel,
                "binary content (NUL byte): build fixtures in code instead",
            );
        }
    }
}

fn check_gitignore(root: &Path, v: &mut Violations) {
    let Some(text) = read_text(root, ".gitignore") else {
        v.add(".gitignore", "missing");
        return;
    };
    let entries: BTreeSet<&str> = text.lines().map(str::trim).collect();
    for ext in GAME_DATA_EXTS {
        if !entries.contains(format!("*{ext}").as_str()) {
            v.add(".gitignore", &format!("does not ignore *{ext}"));
        }
    }
}

// --- names -------------------------------------------------------------------------------------------------------

fn check_names(root: &Path, files: &[String], v: &mut Violations) {
    for rel in files {
        if NAME_ALLOWED_FILES.contains(&rel.as_str())
            || NAME_ALLOWED_PREFIXES.iter().any(|p| rel.starts_with(p))
        {
            continue;
        }
        if rel.to_lowercase().contains(OLD_NAME) {
            v.add(
                rel,
                "the path carries the retired name; it may only appear in the provenance notes",
            );
            continue;
        }
        let Some(text) = read_text(root, rel) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(OLD_NAME) {
                v.add(
                    &format!("{rel}:{}", number + 1),
                    "carries the retired name; it may only appear in the provenance notes",
                );
            }
        }
    }
}

// --- driver ------------------------------------------------------------------------------------------------------

/// Run `check` on the repository at `root`, on every tracked file or (with `base`) on what HEAD adds to `base`.
///
/// # Errors
/// A message when git cannot list the files.
pub fn run(root: &Path, check: Check, base: Option<&str>) -> Result<Violations, String> {
    let mut v = Violations::default();
    let (files, lines) = match base {
        Some(base) => {
            let range = format!("{base}...HEAD");
            let files = split_nul(&git(
                root,
                &["diff", "--name-only", "--diff-filter=AR", "-z", &range],
            )?);
            (files, Some(added_lines(root, base)?))
        }
        None => (split_nul(&git(root, &["ls-files", "-z"])?), None),
    };
    if check.includes(Check::Spdx) {
        check_spdx(root, &files, &mut v);
    }
    if check.includes(Check::Provenance) {
        check_provenance(root, &files, lines.as_ref(), &mut v);
    }
    if check.includes(Check::Data) {
        check_data(root, &files, &mut v);
    }
    if check.includes(Check::Gitignore) {
        check_gitignore(root, &mut v);
    }
    if check.includes(Check::Names) {
        check_names(root, &files, &mut v);
    }
    Ok(v)
}

/// The command line of `cargo xtask guards`: prints the violations and returns the exit status.
#[must_use]
pub fn main(args: &[String]) -> i32 {
    let mut root = std::path::PathBuf::from(".");
    let mut base = None;
    let mut check = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--root" => match iter.next() {
                Some(dir) => root = dir.into(),
                None => return misuse("--root needs a directory"),
            },
            "--base" => match iter.next() {
                Some(reference) => base = Some(reference.clone()),
                None => return misuse("--base needs a git reference"),
            },
            name => match Check::parse(name) {
                Some(c) if check.is_none() => check = Some(c),
                _ => return misuse(&format!("unexpected argument: {name}")),
            },
        }
    }
    let Some(check) = check else {
        return misuse("name a check: spdx, provenance, data, gitignore, names or all");
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let violations = match run(&root, check, base.as_deref()) {
        Ok(v) => v,
        Err(message) => {
            eprintln!("{message}");
            return 2;
        }
    };
    let mut out = String::new();
    for line in &violations.0 {
        let _ = writeln!(out, "{line}");
    }
    print!("{out}");
    if violations.0.is_empty() {
        println!(
            "clean-room guards ({}): ok",
            args.iter()
                .find(|a| Check::parse(a).is_some())
                .map_or("all", String::as_str)
        );
        0
    } else {
        eprintln!("clean-room guards: {} violation(s)", violations.0.len());
        1
    }
}

fn misuse(message: &str) -> i32 {
    eprintln!(
        "usage: cargo xtask guards [--root DIR] [--base REF] {{spdx,provenance,data,gitignore,names,all}}\n{message}"
    );
    2
}

#[cfg(test)]
mod tests {
    use super::{has_spdx, is_iso_date, reference, suffixes};

    #[test]
    fn suffixes_follow_the_python_rule() {
        assert_eq!(suffixes("a/x.tar.esm"), [".tar", ".esm"]);
        assert_eq!(suffixes(".gitignore"), Vec::<String>::new());
        assert_eq!(suffixes(".a.esm"), [".esm"]);
        assert_eq!(suffixes("x."), Vec::<String>::new());
    }

    #[test]
    fn an_spdx_licence_must_end_at_a_word_boundary() {
        assert!(has_spdx("// SPDX-License-Identifier: GPL-3.0-only"));
        assert!(has_spdx("# SPDX-License-Identifier:   MPL-2.0\n"));
        assert!(!has_spdx("// SPDX-License-Identifier: GPL-3.0-only-ish"));
        assert!(!has_spdx("// SPDX-License-Identifier: MIT"));
    }

    #[test]
    fn dates_are_checked_like_iso_calendar_dates() {
        assert!(is_iso_date("2024-02-29"));
        assert!(!is_iso_date("2023-02-29"));
        assert!(!is_iso_date("yesterday"));
        assert!(!is_iso_date("2026-1-01"));
    }

    #[test]
    fn a_reference_line_gives_its_ids() {
        assert_eq!(
            reference("   // source: a-1, b", "//"),
            Some(vec!["a-1".to_owned(), "b".to_owned()])
        );
        assert_eq!(reference("// source:", "//"), Some(vec![]));
        assert_eq!(reference("let s = \"// source: x\";", "//"), None);
    }
}
