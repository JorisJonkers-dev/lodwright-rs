# SPDX-License-Identifier: GPL-3.0-only
"""Tests of the clean-room guards, in throwaway git repositories (positive and negative cases)."""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

import cleanroom_guards as g

SPDX = "// SPDX-License-Identifier: GPL-3.0-only\n"
ROW = "\n".join(
    [
        "[[source]]",
        'id = "dds-header"',
        'fact = "The DDS header is 124 bytes after the magic"',
        'url = "https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header"',
        'licence = "CC-BY-4.0"',
        "date = 2026-11-13",
        'recorded_by = "tester"',
        "verified = true",
        "",
    ]
)
LOG = "# Log\n\n- `dds-header`: recorded.\n"
IGNORE = "".join(f"*{ext}\n" for ext in g.GAME_DATA_EXTS)


def src(text: str) -> str:
    """A comment line built at run time, so this file never contains a literal reference marker."""
    return text


class Repo:
    def __init__(self, test: unittest.TestCase) -> None:
        tmp = tempfile.TemporaryDirectory()
        test.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.git("init", "-q", "-b", "main")
        self.write("provenance/sources.toml", ROW)
        self.write("provenance/LOG.md", LOG)
        self.write(".gitignore", IGNORE)
        self.write("lib.rs", SPDX + "fn main() {}\n")
        self.commit("base")
        self.git("tag", "base")

    def git(self, *args: str) -> None:
        subprocess.run(
            ["git", "-C", str(self.root), "-c", "user.name=t", "-c", "user.email=t@example.invalid", *args],
            check=True,
            capture_output=True,
            # No user or system git config: their hooks and signing settings must not reach throwaway repos.
            env={**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_SYSTEM": os.devnull},
        )

    def write(self, rel: str, content: str | bytes) -> None:
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, bytes):
            path.write_bytes(content)
        else:
            path.write_text(content, encoding="utf-8")

    def commit(self, message: str = "change") -> None:
        self.git("add", "-A", "-f")  # force: the data guard must catch what a careless force-add commits
        self.git("commit", "-q", "-m", message)

    def check(self, command: str, base: str | None = None) -> list[str]:
        return list(g.run(self.root, command, base))


class SpdxTests(unittest.TestCase):
    def test_a_clean_tree_passes(self) -> None:
        self.assertEqual(Repo(self).check("all"), [])

    def test_a_new_rust_file_without_a_header_fails(self) -> None:
        repo = Repo(self)
        repo.write("new.rs", "fn x() {}\n")
        repo.commit()
        result = repo.check("spdx", "base")
        self.assertEqual(len(result), 1)
        self.assertIn("new.rs", result[0])

    def test_a_new_rust_file_with_a_header_passes(self) -> None:
        repo = Repo(self)
        repo.write("new.rs", SPDX + "fn x() {}\n")
        repo.commit()
        self.assertEqual(repo.check("spdx", "base"), [])

    def test_python_and_shell_files_need_headers_too(self) -> None:
        repo = Repo(self)
        repo.write("a.py", "print(1)\n")
        repo.write("b.sh", "#!/usr/bin/env bash\necho hi\n")
        repo.write("c.sh", "#!/usr/bin/env bash\n# SPDX-License-Identifier: GPL-3.0-only\n")
        repo.commit()
        failed = " ".join(repo.check("spdx", "base"))
        self.assertIn("a.py", failed)
        self.assertIn("b.sh", failed)
        self.assertNotIn("c.sh", failed)

    def test_another_licence_is_not_accepted(self) -> None:
        repo = Repo(self)
        repo.write("mit.rs", "// SPDX-License-Identifier: MIT\n")
        repo.write("lookalike.rs", "// SPDX-License-Identifier: GPL-3.0-only-only-not\n")
        repo.commit()
        self.assertEqual(len(repo.check("spdx", "base")), 2)

    def test_a_header_below_the_first_five_lines_does_not_count(self) -> None:
        repo = Repo(self)
        repo.write("late.rs", "//\n" * 5 + SPDX)
        repo.commit()
        self.assertEqual(len(repo.check("spdx", "base")), 1)

    def test_files_that_were_already_there_are_not_flagged_in_base_mode(self) -> None:
        repo = Repo(self)
        repo.write("old.rs", "fn old() {}\n")
        repo.commit()
        repo.git("tag", "second")
        repo.write("other.txt", "x")
        repo.commit()
        self.assertEqual(repo.check("spdx", "second"), [])
        self.assertEqual(len(repo.check("spdx")), 1)  # the whole-tree mode still finds it


class ProvenanceTests(unittest.TestCase):
    def add_ref(self, repo: Repo, ident: str, name: str = "r.rs") -> None:
        repo.write(name, SPDX + src("// " + "source: ") + ident + "\nfn x() {}\n")
        repo.commit()

    def test_a_reference_with_a_row_passes(self) -> None:
        repo = Repo(self)
        self.add_ref(repo, "dds-header")
        self.assertEqual(repo.check("provenance", "base"), [])
        self.assertEqual(repo.check("provenance"), [])

    def test_a_reference_without_a_row_fails(self) -> None:
        repo = Repo(self)
        self.add_ref(repo, "vhgt-layout")
        result = repo.check("provenance", "base")
        self.assertEqual(len(result), 1)
        self.assertIn("vhgt-layout", result[0])
        self.assertIn("r.rs:2", result[0])

    def test_one_unknown_id_in_a_list_fails(self) -> None:
        repo = Repo(self)
        self.add_ref(repo, "dds-header, nope")
        self.assertEqual(len(repo.check("provenance", "base")), 1)

    def test_a_reference_naming_nothing_fails(self) -> None:
        repo = Repo(self)
        self.add_ref(repo, "")
        self.assertEqual(len(repo.check("provenance", "base")), 1)

    def test_python_and_toml_comments_count(self) -> None:
        repo = Repo(self)
        repo.write("a.py", "# SPDX-License-Identifier: GPL-3.0-only\n# " + "source: missing-row\n")
        repo.write("b.toml", "# " + "source: missing-too\n")
        repo.commit()
        self.assertEqual(len(repo.check("provenance", "base")), 2)

    def test_prose_that_mentions_a_source_is_not_a_reference(self) -> None:
        repo = Repo(self)
        repo.write("notes.md", "# source: heading\nthe source: x\n")
        repo.write("code.rs", SPDX + "let s = \"// source: inside a string\";\n")
        repo.commit()
        self.assertEqual(repo.check("provenance", "base"), [])

    def test_only_added_lines_are_checked_in_base_mode(self) -> None:
        repo = Repo(self)
        self.add_ref(repo, "gone")
        repo.git("tag", "second")
        repo.write("r.rs", SPDX + "fn y() {}\n")  # the reference is removed again
        repo.commit()
        self.assertEqual(repo.check("provenance", "second"), [])

    def test_a_row_with_a_missing_field_fails(self) -> None:
        repo = Repo(self)
        repo.write("provenance/sources.toml", ROW.replace('licence = "CC-BY-4.0"\n', ""))
        repo.commit()
        self.assertTrue(any("missing field(s): licence" in line for line in repo.check("provenance")))

    def test_an_unknown_licence_is_refused(self) -> None:
        repo = Repo(self)
        repo.write("provenance/sources.toml", ROW.replace("CC-BY-4.0", "unknown"))
        repo.commit()
        self.assertTrue(any("licence" in line for line in repo.check("provenance")))

    def test_the_unlicensed_lodgen_repository_is_refused_as_a_source(self) -> None:
        repo = Repo(self)
        bad = ROW.replace(
            "https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header",
            "https://github.com/TES5Edit/xLODGen/blob/master/x.cs",
        )
        repo.write("provenance/sources.toml", bad)
        repo.commit()
        self.assertTrue(any("forbidden input" in line for line in repo.check("provenance")))

    def test_decompiled_code_is_refused_as_a_source(self) -> None:
        repo = Repo(self)
        repo.write("provenance/sources.toml", ROW.replace("124 bytes", "124 bytes (decompiled)"))
        repo.commit()
        self.assertTrue(any("forbidden input" in line for line in repo.check("provenance")))

    def engine_row(self, url: str, licence: str = "GPL-3.0-or-later", reader: str | None = "someone else") -> str:
        row = ROW.replace(
            "https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header", url
        ).replace("CC-BY-4.0", licence)
        if reader is not None:
            row += f'second_reader = "{reader}"\n'
        return row

    def test_an_engine_port_row_with_a_file_at_a_commit_a_licence_and_a_reader_passes(self) -> None:
        repo = Repo(self)
        url = "https://github.com/JorisJonkers-dev/nv-rs/blob/" + "a" * 40 + "/crates/bsa/src/archive.rs"
        repo.write("provenance/sources.toml", self.engine_row(url))
        repo.commit()
        self.assertEqual(repo.check("provenance"), [])

    def test_an_engine_port_row_without_a_second_reader_fails(self) -> None:
        repo = Repo(self)
        url = "https://github.com/slaterain/nv-rs/blob/" + "b" * 40 + "/crates/bsa/src/archive.rs"
        repo.write("provenance/sources.toml", self.engine_row(url, reader=None))
        repo.commit()
        self.assertTrue(any("second_reader" in line for line in repo.check("provenance")))

    def test_an_engine_port_row_on_a_branch_instead_of_a_commit_fails(self) -> None:
        repo = Repo(self)
        url = "https://github.com/slaterain/nv-rs/blob/main/crates/bsa/src/archive.rs"
        repo.write("provenance/sources.toml", self.engine_row(url))
        repo.commit()
        self.assertTrue(any("40-character commit" in line for line in repo.check("provenance")))

    def test_an_engine_port_row_without_the_engine_licence_fails(self) -> None:
        repo = Repo(self)
        url = "https://github.com/slaterain/nv-rs/blob/" + "c" * 40 + "/crates/bsa/src/archive.rs"
        repo.write("provenance/sources.toml", self.engine_row(url, licence="CC-BY-4.0"))
        repo.commit()
        self.assertTrue(any("licence" in line for line in repo.check("provenance")))

    def test_the_unlicensed_decompilation_reconstruction_is_refused_as_a_source(self) -> None:
        repo = Repo(self)
        repo.write("provenance/sources.toml", ROW.replace("124 bytes", "124 bytes (fnv_decomp)"))
        repo.commit()
        self.assertTrue(any("forbidden input" in line for line in repo.check("provenance")))

    def test_the_url_must_be_https_or_an_oracle_role(self) -> None:
        repo = Repo(self)
        repo.write("provenance/sources.toml", ROW.replace("https://learn", "http://learn"))
        repo.commit()
        self.assertTrue(any("url must start" in line for line in repo.check("provenance")))
        repo.write("provenance/sources.toml", ROW.replace("https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header", "oracle:lod-file"))
        repo.commit()
        self.assertEqual(repo.check("provenance"), [])

    def test_duplicate_ids_and_bad_dates_fail(self) -> None:
        repo = Repo(self)
        repo.write("provenance/sources.toml", ROW + "\n" + ROW)
        repo.commit()
        self.assertTrue(any("duplicate id" in line for line in repo.check("provenance")))
        repo.write("provenance/sources.toml", ROW.replace("date = 2026-11-13", 'date = "yesterday"'))
        repo.commit()
        self.assertTrue(any("YYYY-MM-DD" in line for line in repo.check("provenance")))

    def test_a_row_the_log_does_not_mention_fails(self) -> None:
        repo = Repo(self)
        repo.write("provenance/LOG.md", "# Log\n")
        repo.commit()
        self.assertTrue(any("LOG.md" in line for line in repo.check("provenance")))

    def test_broken_toml_fails(self) -> None:
        repo = Repo(self)
        repo.write("provenance/sources.toml", "[[source]\n")
        repo.commit()
        self.assertTrue(any("not valid TOML" in line for line in repo.check("provenance")))


class DataGuardTests(unittest.TestCase):
    def test_every_game_data_extension_is_refused(self) -> None:
        repo = Repo(self)
        for ext in g.GAME_DATA_EXTS:
            repo.write(f"data/file{ext}", "x")
        repo.commit()
        result = repo.check("data", "base")
        self.assertEqual(len(result), len(g.GAME_DATA_EXTS))

    def test_the_extension_check_ignores_case_and_looks_at_every_suffix(self) -> None:
        repo = Repo(self)
        repo.write("a/FalloutNV.ESM", "x")
        repo.write("a/backup.esp.bak", "x")
        repo.commit()
        self.assertEqual(len(repo.check("data")), 2)

    def test_ordinary_text_and_source_files_pass(self) -> None:
        repo = Repo(self)
        repo.write("docs/lod.md", "# LOD\n")
        repo.write("Cargo.lock", "# lock\n")
        repo.commit()
        self.assertEqual(repo.check("data"), [])

    def test_a_large_file_is_refused(self) -> None:
        repo = Repo(self)
        repo.write("big.txt", "a" * (g.MAX_BYTES + 1))
        repo.commit()
        self.assertTrue(any("byte limit" in line for line in repo.check("data")))

    def test_a_binary_file_is_refused_whatever_its_name(self) -> None:
        repo = Repo(self)
        repo.write("blob.dat", b"\x00\x01\x02")
        repo.commit()
        self.assertTrue(any("binary content" in line for line in repo.check("data")))

    def test_an_allow_listed_fixture_passes_and_others_still_fail(self) -> None:
        repo = Repo(self)
        repo.write("fixtures/tiny.dds", b"DDS \x00")
        repo.write("fixtures/other.dds", b"DDS ")
        repo.write(g.ALLOWLIST_FILE, "# synthetic\nfixtures/tiny.dds generated by the testkit\n")
        repo.commit()
        result = repo.check("data")
        self.assertEqual(len(result), 1)
        self.assertIn("other.dds", result[0])

    def test_an_allow_list_entry_needs_a_reason_and_no_glob(self) -> None:
        repo = Repo(self)
        repo.write(g.ALLOWLIST_FILE, "fixtures/a.dds\nfixtures/*.dds why\n")
        repo.commit()
        result = " ".join(repo.check("data"))
        self.assertIn("has no reason", result)
        self.assertIn("no globs", result)

    def test_the_allow_list_must_stay_tiny(self) -> None:
        repo = Repo(self)
        repo.write(g.ALLOWLIST_FILE, "".join(f"f/{i}.dds r\n" for i in range(g.MAX_ALLOWLIST + 1)))
        repo.commit()
        self.assertTrue(any("tiny" in line for line in repo.check("data")))

    def test_an_allow_listed_fixture_has_a_size_cap(self) -> None:
        repo = Repo(self)
        repo.write("f/big.dds", b"a" * (g.FIXTURE_MAX_BYTES + 1))
        repo.write(g.ALLOWLIST_FILE, "f/big.dds r\n")
        repo.commit()
        self.assertTrue(any("limit" in line for line in repo.check("data")))

    def test_base_mode_only_looks_at_what_the_branch_adds(self) -> None:
        repo = Repo(self)
        repo.write("old.esm", "x")
        repo.commit()
        repo.git("tag", "second")
        repo.write("fine.txt", "x")
        repo.commit()
        self.assertEqual(repo.check("data", "second"), [])
        self.assertEqual(len(repo.check("data")), 1)


class NameTests(unittest.TestCase):
    def test_the_old_name_in_a_source_file_fails(self) -> None:
        repo = Repo(self)
        repo.write("lib.rs", SPDX + "// port of xLODGen\n")
        repo.commit()
        self.assertEqual(len(repo.check("names")), 1)

    def test_the_old_name_in_a_path_fails(self) -> None:
        repo = Repo(self)
        repo.write("docs/xlodgen-notes.txt", "notes\n")
        repo.commit()
        self.assertEqual(len(repo.check("names")), 1)

    def test_the_old_name_in_provenance_notes_and_the_process_document_passes(self) -> None:
        repo = Repo(self)
        repo.write("provenance/LOG.md", LOG + "Read about LODGen.\n")
        repo.write("docs/clean-room.md", "never read the LODGen source\n")
        repo.commit()
        self.assertEqual(repo.check("names"), [])

    def test_case_does_not_hide_the_old_name(self) -> None:
        repo = Repo(self)
        repo.write("README.md", "XLODGEN\n")
        repo.commit()
        self.assertEqual(len(repo.check("names")), 1)

    def test_the_real_tree_is_free_of_the_old_name(self) -> None:
        root = Path(__file__).resolve().parent.parent
        self.assertEqual(list(g.run(root, "names", None)), [])


class GitignoreTests(unittest.TestCase):
    def test_a_missing_extension_is_reported(self) -> None:
        repo = Repo(self)
        repo.write(".gitignore", IGNORE.replace("*.esm\n", ""))
        repo.commit()
        self.assertEqual(repo.check("gitignore"), [".gitignore: does not ignore *.esm"])

    def test_the_real_gitignore_lists_every_extension(self) -> None:
        root = Path(__file__).resolve().parent.parent
        violations = g.Violations()
        g.check_gitignore(root, violations)
        self.assertEqual(list(violations), [])


class CommandLineTests(unittest.TestCase):
    def test_exit_codes(self) -> None:
        repo = Repo(self)
        self.assertEqual(g.main(["--root", str(repo.root), "all"]), 0)
        repo.write("x.esm", "x")
        repo.commit()
        self.assertEqual(g.main(["--root", str(repo.root), "data"]), 1)


if __name__ == "__main__":
    unittest.main()
