# Clean-room process

lodwright-rs is licensed GPL-3.0-only (docs/LICENSING.md) and reimplements parts of xLODGen whose source is closed or unlicensed. This
document is the rule for everyone who writes code or docs here. The study behind it is
`docs/research/porting/xlodgen-licence-and-design.md` in the NV-Linux-Modding repository.

## What is open and what is closed

| Part | Status | What we may do |
| --- | --- | --- |
| xEdit and the xLODGen host code in it (`Core/wbLOD.pas` and neighbours in TES5Edit/TES5Edit) | MPL-2.0 | Read and translate; translated files stay MPL-2.0 and say where they come from |
| Terrain LOD generator and `LODGen.exe` 3.x | No published source, no licence | Reimplement from file formats and observed behaviour only |
| The 2016 C# repository (TES5Edit/xLODGen) | No licence: all rights reserved | **Never read it** |
| Texconv (DirectXTex) | MIT | Not ported; DDS is written by a permissive crate |

## Roles

- **Specifier**: reads allowed inputs and writes down format facts: one row in `provenance/sources.toml` per fact,
  a story in `provenance/LOG.md`, and, when a fact needs a page of prose, a document under `docs/formats/`.
- **Implementer**: writes code from the recorded facts and the open MPL sources only. When something is not
  recorded, the implementer does not look it up in a closed source: they ask the specifier for a new row, or file
  an open question and run a black-box experiment (below).

One person may hold both roles at different times, but not for the same format at once: whoever specified a
format does not implement it without a second reader re-deriving the facts from the rows (`verified = true`). Each
LOG entry names who held which role. The pull request checklist asks for the statement.

## Allowed inputs

1. Public format documentation: UESP and Tes4Mod pages, Microsoft's DDS documentation, public tool readmes
   (the xLODGen README family, VNV and STEP guides).
2. The xEdit source under MPL-2.0 (`wbLOD.pas`, `wbDefinitionsFNV.pas`, `wbDDS.pas` and others), read at a named
   revision.
3. The game's own data files, read on the owner's machine to learn a layout. The files and anything copied
   from them stay out of the repository.
4. **Black-box output of xLODGen on the owner's own data**: same input, compare output, parameter sweeps, byte-level
   diffs. Output of a tool is a fact about files, not the tool's code. Rows cite these as `oracle:<role>` (for
   example `oracle:btr-file`), by role and never by content.
5. Permissively licensed crates (MIT, Apache-2.0, BSD) used as dependencies; `deny.toml` lists what is allowed.
6. **The engine port's source** (nv-rs: `slaterain/nv-rs` and the fork `JorisJonkers-dev/nv-rs`), under
   [ADR-0001](adr/0001-engine-source-as-a-format-input.md), when it is the best fit. Only code that nv-rs does not mark
   as translated from the decompiled game program, from `fnv_decomp` or from debug symbols, and only files whose own
   licence is exactly GPL-3.0-or-later or MIT (code from before 2026-10-16). Each use is a row that cites one file at a
   fixed commit, states that licence, names a `second_reader` who did not implement the format, and notes what was
   checked in nv-rs's third-party list. A crate dependency is preferred over copied code; copied code never goes into the two MPL-2.0 files.

## Forbidden inputs

- The unlicensed TES5Edit/xLODGen 2016 C# repository (all rights reserved): not its code, not a paste of it, not a
  summary of how it works.
- Decompiled or disassembled `LODGen.exe`, `xLODGen.exe` or the Texconv integration, and decompiler output of any
  kind.
- Leaked, forum-pasted or NDA'd source.
- Anything under an unknown licence. If the licence of a source cannot be stated, it cannot be a row.
- GPL, LGPL or AGPL *source* is not a clean-room input: reading it to learn a format taints the work. Linking a
  GPL-3.0 crate as a dependency (for example `esplugin`) is allowed under the licence map in
  [LICENSING.md](LICENSING.md); copying its code is not. The one exception is the engine-port code that allowed input 6 covers: its original
  GPL-3.0-or-later and MIT-era files. What nv-rs takes from xNVSE, JIP LN, BSD, zlib or Apache-2.0 sources is not
  covered, even when it sits inside the engine port.
- Material the engine port marks as translated from the decompiled game program, from `fnv_decomp` (no licence) or
  from debug symbols.

If you have already seen forbidden material, say so in the pull request before writing anything about that area:
the format becomes someone else's to implement. Nobody is penalised for disclosing; hiding it taints the work.

## Provenance rules

- Every format fact the code relies on is a row in `provenance/sources.toml`: fact, source URL (or `oracle:<role>`),
  the licence of the source, the date, who recorded it, the role, and whether a second reader verified it.
- Code points at rows with a `// source: <id>[, <id>]` comment next to the line that relies on the fact (`# source:`
  in Python, shell and TOML).
- `provenance/LOG.md` mentions every id (in backticks) with the story: what was read, what stayed open.
- Open format questions are listed in `docs/design.md`; a question is closed by a row, never by a guess.
- Code translated from MPL xEdit source names the file and revision in its header and stays MPL-2.0. Code taken from
  an MIT or Apache-2.0 tool of the owner (`lodcheck`, `ddsaudit`, `fnvorder`) is allowed with its notice kept
  (GPL-3.0 can include permissive code); add the notice, the origin and a licence check to the pull request.

## What CI enforces

`task guards` (`scripts/cleanroom_guards.py`, tested by `scripts/test_cleanroom_guards.py`) fails the pipeline when:

- a `.rs`, `.py` or `.sh` file lacks `SPDX-License-Identifier: GPL-3.0-only` (or `MPL-2.0` for a file translated from
  MPL-2.0 sources) in its first five lines;
- a `source:` comment names an id that has no row, or a row lacks a field, states no licence, uses a URL that is not
  `https://` or `oracle:`, names a forbidden input (the unlicensed repository, decompiled or leaked code,
  `fnv_decomp`), or is missing from `LOG.md`;
- a row citing the engine port does not cite one file at a 40-character commit, does not state GPL-3.0-or-later or MIT
  as its licence, or names no `second_reader`;
- a file has a game-data extension (`.esm .esp .esl .bsa .ba2 .nif .kf .dds .btr .bto .btt .lod .wav .ogg .mp3 .fos .ess`,
  any case, any position in the name), is over 256 KiB, or contains NUL bytes, unless it is listed in
  `provenance/fixture-allowlist.txt` (exact paths with a reason, at most ten, each at most 64 KiB; empty today);
- `.gitignore` stops listing those extensions.

The guards cannot judge where a fact really came from; they make the record complete and the data boundary hard.
The honest part is the specifier's and the reviewer's.

## Owner data and oracle output

Real plugins, archives, textures, meshes and xLODGen output live on the owner's machine. They are never committed,
attached to a pull request or issue, uploaded to CI, or used as test fixtures. The tool reads them at run time from
a path the user gives; the comparison tooling (`docs/TESTING.md`) prints numbers, not content.

## Not affiliated

No one at the xEdit team or the author of xLODGen has been contacted about this port. The owner may want to ask the
xLODGen author before any public release.
