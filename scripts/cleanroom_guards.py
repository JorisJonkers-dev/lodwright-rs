#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Clean-room guards: SPDX headers, provenance references, and the game-data guard.

    cleanroom_guards.py [--root DIR] [--base REF] {spdx,provenance,data,gitignore,all}

Without --base every tracked file is checked. With --base REF only what the current branch adds relative to REF
(`git diff REF...HEAD`) is checked: new files for the SPDX and data checks, added lines for the provenance
references. Exit status 0 when clean, 1 when a rule is broken (one line per violation), 2 on misuse.

Rules (docs/clean-room.md explains why):
  spdx        every .rs, .py and .sh file has `SPDX-License-Identifier: MPL-2.0` in its first five lines.
  provenance  every `// source: <id>[, <id>]` (`# source:` in Python, shell, TOML and YAML) names a row of
              provenance/sources.toml; the rows are well formed (fields, unique ids, licence stated, no forbidden
              source) and each id appears in provenance/LOG.md.
  data        no file with a game-data extension, no file over MAX_BYTES and no file with NUL bytes, except the
              paths in provenance/fixture-allowlist.txt ("<path> <reason>", at most MAX_ALLOWLIST entries).
  gitignore   .gitignore lists every game-data extension.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from datetime import date
from pathlib import Path

SOURCE_EXTS = {".rs", ".py", ".sh"}
SPDX_RE = re.compile(r"SPDX-License-Identifier:\s*MPL-2\.0(?![\w.-])")
SPDX_LINES = 5

# `// source:` in Rust, `# source:` where # starts a comment.
COMMENT_STYLES = {
    ".rs": "//",
    ".py": "#",
    ".sh": "#",
    ".toml": "#",
    ".yml": "#",
    ".yaml": "#",
}
ID_RE = re.compile(r"^[a-z0-9][a-z0-9-]*$")

SOURCES_FILE = "provenance/sources.toml"
LOG_FILE = "provenance/LOG.md"
ALLOWLIST_FILE = "provenance/fixture-allowlist.txt"
REQUIRED_FIELDS = ("id", "fact", "url", "licence", "date", "recorded_by", "verified")
URL_PREFIXES = ("https://", "oracle:")
BAD_LICENCES = {"", "unknown", "none", "unlicensed", "all rights reserved", "n/a", "tbd"}
# Inputs the clean-room rule forbids. Matched case-insensitively against the url and the fact.
FORBIDDEN_SOURCES = ("github.com/tes5edit/xlodgen", "decompil", "disassembl", "leaked")

GAME_DATA_EXTS = (
    ".esm", ".esp", ".esl", ".bsa", ".ba2", ".nif", ".kf", ".dds", ".btr", ".bto", ".btt", ".lod",
    ".wav", ".ogg", ".mp3", ".fos", ".ess",
)  # fmt: skip
MAX_BYTES = 256 * 1024
MAX_ALLOWLIST = 10
FIXTURE_MAX_BYTES = 64 * 1024


class Violations(list):
    def add(self, path: str, message: str) -> None:
        self.append(f"{path}: {message}")


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def tracked_files(root: Path) -> list[str]:
    return [p for p in git(root, "ls-files", "-z").split("\0") if p]


def added_files(root: Path, base: str) -> list[str]:
    out = git(root, "diff", "--name-only", "--diff-filter=AR", "-z", f"{base}...HEAD")
    return [p for p in out.split("\0") if p]


def added_lines(root: Path, base: str) -> dict[str, list[str]]:
    """Lines added by the branch, per file."""
    out = git(root, "diff", "-U0", "--no-color", "--diff-filter=AMR", f"{base}...HEAD")
    lines: dict[str, list[str]] = {}
    current = None
    for line in out.splitlines():
        if line.startswith("+++ "):
            current = line[6:] if line.startswith("+++ b/") else None
            if current is not None:
                lines.setdefault(current, [])
        elif current is not None and line.startswith("+") and not line.startswith("+++"):
            lines[current].append(line[1:])
    return lines


def read_text(root: Path, rel: str) -> str | None:
    try:
        return (root / rel).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None


# --- spdx -------------------------------------------------------------------------------------------------------


def check_spdx(root: Path, files: list[str], v: Violations) -> None:
    for rel in files:
        if Path(rel).suffix not in SOURCE_EXTS:
            continue
        text = read_text(root, rel)
        if text is None:
            v.add(rel, "cannot be read as UTF-8 text")
            continue
        head = "\n".join(text.splitlines()[:SPDX_LINES])
        if not SPDX_RE.search(head):
            v.add(rel, f"no `SPDX-License-Identifier: MPL-2.0` in the first {SPDX_LINES} lines")


# --- provenance ---------------------------------------------------------------------------------------------------


def load_sources(root: Path, v: Violations) -> dict[str, dict]:
    path = root / SOURCES_FILE
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        v.add(SOURCES_FILE, "missing")
        return {}
    except (tomllib.TOMLDecodeError, UnicodeDecodeError) as error:
        v.add(SOURCES_FILE, f"not valid TOML: {error}")
        return {}
    rows: dict[str, dict] = {}
    for index, row in enumerate(data.get("source", []), start=1):
        label = f"{SOURCES_FILE} row {index}"
        missing = [f for f in REQUIRED_FIELDS if f not in row]
        if missing:
            v.add(label, f"missing field(s): {', '.join(missing)}")
            continue
        row_id = row["id"]
        if not isinstance(row_id, str) or not ID_RE.match(row_id):
            v.add(label, f"id {row_id!r} must be lowercase letters, digits and hyphens")
            continue
        label = f"{SOURCES_FILE} row {row_id}"
        if row_id in rows:
            v.add(label, "duplicate id")
            continue
        rows[row_id] = row
        check_row(label, row, v)
    return rows


def check_row(label: str, row: dict, v: Violations) -> None:
    for field in ("fact", "url", "licence", "recorded_by"):
        if not isinstance(row[field], str) or not row[field].strip():
            v.add(label, f"{field} must be a non-empty string")
    if not isinstance(row["verified"], bool):
        v.add(label, "verified must be true or false")
    # TOML dates parse to datetime.date; a quoted string is accepted too.
    stamp = row["date"]
    if isinstance(stamp, str):
        try:
            date.fromisoformat(stamp)
        except ValueError:
            v.add(label, f"date {stamp!r} is not YYYY-MM-DD")
    elif not isinstance(stamp, date):
        v.add(label, "date must be YYYY-MM-DD")
    url = str(row["url"])
    if not url.startswith(URL_PREFIXES):
        v.add(label, "url must start with https:// or oracle: (the owner's own tool output, by role)")
    if str(row["licence"]).strip().lower() in BAD_LICENCES:
        v.add(label, "the licence of the source must be stated; an unknown licence is a forbidden input")
    haystack = f"{url} {row['fact']}".lower()
    for needle in FORBIDDEN_SOURCES:
        if needle in haystack:
            v.add(label, f"names a forbidden input ({needle})")


def comment_ref_re(suffix: str) -> re.Pattern[str] | None:
    marker = COMMENT_STYLES.get(suffix)
    if marker is None:
        return None
    return re.compile(rf"^\s*{re.escape(marker)}\s*source:\s*(?P<ids>.*?)\s*$")


def split_ids(raw: str) -> list[str]:
    return [part for part in re.split(r"[,\s]+", raw) if part]


def check_refs(rel: str, lines: list[str], known: set[str], v: Violations) -> None:
    pattern = comment_ref_re(Path(rel).suffix)
    if pattern is None:
        return
    for number, line in enumerate(lines, start=1):
        match = pattern.match(line)
        if not match:
            continue
        ids = split_ids(match.group("ids"))
        if not ids:
            v.add(f"{rel}:{number}", "`source:` names no provenance id")
        for ident in ids:
            if ident not in known:
                v.add(f"{rel}:{number}", f"source `{ident}` has no row in {SOURCES_FILE}")


def check_provenance(root: Path, files: list[str], lines: dict[str, list[str]] | None, v: Violations) -> None:
    rows = load_sources(root, v)
    log = read_text(root, LOG_FILE)
    if log is None:
        v.add(LOG_FILE, "missing")
    else:
        for ident in rows:
            if f"`{ident}`" not in log:
                v.add(LOG_FILE, f"does not mention source `{ident}` (write it as `{ident}`)")
    known = set(rows)
    if lines is None:
        for rel in files:
            text = read_text(root, rel)
            if text is not None:
                check_refs(rel, text.splitlines(), known, v)
    else:
        for rel, added in lines.items():
            check_refs(rel, added, known, v)


# --- data guard ---------------------------------------------------------------------------------------------------


def load_allowlist(root: Path, v: Violations) -> set[str]:
    text = read_text(root, ALLOWLIST_FILE)
    if text is None:
        return set()
    allowed: set[str] = set()
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        path, _, reason = line.partition(" ")
        if not reason.strip():
            v.add(f"{ALLOWLIST_FILE}:{number}", f"entry for {path} has no reason")
            continue
        if any(ch in path for ch in "*?[") or path.startswith("/") or ".." in Path(path).parts:
            v.add(f"{ALLOWLIST_FILE}:{number}", "entries are exact repository paths, no globs")
            continue
        allowed.add(path)
    if len(allowed) > MAX_ALLOWLIST:
        v.add(ALLOWLIST_FILE, f"more than {MAX_ALLOWLIST} entries: the allow-list must stay tiny")
    return allowed


def game_data_suffix(rel: str) -> str | None:
    for suffix in Path(rel).suffixes:
        if suffix.lower() in GAME_DATA_EXTS:
            return suffix.lower()
    return None


def check_data(root: Path, files: list[str], v: Violations) -> None:
    allowed = load_allowlist(root, v)
    for rel in files:
        path = root / rel
        if not path.is_file():
            continue
        size = path.stat().st_size
        if rel in allowed:
            if size > FIXTURE_MAX_BYTES:
                v.add(rel, f"allow-listed fixture is {size} bytes; the limit is {FIXTURE_MAX_BYTES}")
            continue
        suffix = game_data_suffix(rel)
        if suffix:
            v.add(rel, f"game-data extension {suffix}: game data and tool output never enter the repository")
        elif size > MAX_BYTES:
            v.add(rel, f"{size} bytes is over the {MAX_BYTES} byte limit for a committed file")
        elif b"\0" in path.read_bytes():
            v.add(rel, "binary content (NUL byte): build fixtures in code instead")


def check_gitignore(root: Path, v: Violations) -> None:
    text = read_text(root, ".gitignore")
    if text is None:
        v.add(".gitignore", "missing")
        return
    entries = {line.strip() for line in text.splitlines()}
    for ext in GAME_DATA_EXTS:
        if f"*{ext}" not in entries:
            v.add(".gitignore", f"does not ignore *{ext}")


# --- driver -----------------------------------------------------------------------------------------------------


def run(root: Path, command: str, base: str | None) -> Violations:
    v = Violations()
    if base:
        files = added_files(root, base)
        lines = added_lines(root, base)
    else:
        files = tracked_files(root)
        lines = None
    if command in ("spdx", "all"):
        check_spdx(root, files, v)
    if command in ("provenance", "all"):
        check_provenance(root, files, lines, v)
    if command in ("data", "all"):
        check_data(root, files, v)
    if command in ("gitignore", "all"):
        check_gitignore(root, v)
    return v


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["spdx", "provenance", "data", "gitignore", "all"])
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--base", help="check only what HEAD adds relative to this ref")
    args = parser.parse_args(argv)
    violations = run(args.root.resolve(), args.command, args.base)
    for line in violations:
        print(line)
    if violations:
        print(f"clean-room guards: {len(violations)} violation(s)", file=sys.stderr)
        return 1
    print(f"clean-room guards ({args.command}): ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
