---
status: accepted
date: 2026-11-19
---
# The engine port's source as an input, under a recorded rule

**Accepted** (#123). This records the owner's answer D5 of the LOD completion plan (2026-11-19), the licence facts it
asked to check first, the owner's licence reading (second round, ADRENG) and the rule. The rule takes effect when this
pull request merges.

The owner's answer: the source of the engine port (nv-rs, upstream `slaterain/nv-rs`, fork `JorisJonkers-dev/nv-rs`)
may be used as an input for LOD and archive facts when it is the best fit. Every use is a provenance row that names the
source file or crate, its licence and a second reader. A crate dependency is preferred over copying code. No game data,
ever.

## The rule

This pull request writes the rule into `docs/clean-room.md` (allowed input 6) and the guard.

- **Allowed:** nv-rs code that nv-rs does not mark as translated from the decompiled game program, from `fnv_decomp`
  or from the Xbox debug symbols, when it is the best fit for a LOD or archive fact.
- **Not allowed:** material nv-rs marks as translated from the decompiled program, `fnv_decomp` or the symbols. This
  keeps the clean-room refusal of decompiler output and of sources without a licence. The guard already refuses a row
  whose URL or fact says "decompiled"; it now refuses "fnv_decomp" too.
- **Each use is a row** that cites one nv-rs file on GitHub at a 40-character commit
  (`https://github.com/<owner>/nv-rs/blob/<commit>/<path>`; a raw URL, a mirror, a directory, a branch, a query or a
  fragment is refused) and names:
  - `licence`: the exact SPDX identifier of the code the row relies on, which is not always nv-rs's own licence.
    Allowed: `GPL-3.0-or-later`, `GPL-3.0-only` (the xNVSE and JIP LN translations), `MIT`, `BSD-2-Clause`,
    `BSD-3-Clause`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception` and `Zlib`. One identifier, never an expression such
    as `A AND B`. A licence that cannot be identified, or code with no licence such as `fnv_decomp`, stays refused.
  - `second_reader`: someone who did not implement the format and did not write that nv-rs code.
  - `engine_notes`: the GPL-3.0 compatibility reasoning (Apache-2.0, BSD, zlib and MIT are compatible with GPL-3.0; the
    GPL-3.0-only and GPL-3.0-or-later code is used under GPL-3.0-only) and what was checked in nv-rs's `LEGAL.md`
    third-party list and for translation markers.
  - `licence_notice`, for every licence except `GPL-3.0-or-later`: the copyright line and the licence notice kept, and
    where.
- **Notices.** For every such row the copyright line and licence text or notice are kept in `THIRD-PARTY-NOTICES.md` at
  the repository root, in a section named after the row id, and shipped where the licence requires it (release
  packaging includes that file). The guard checks the file against the rows: a section for each row that needs one, text
  in each section, and no section for anything else. The guard checks that the compatibility note exists, not that it is
  right; the second reader checks that.
- **Preference:** a crate dependency over copied code; a described behaviour (a row, then lodwright's own code) over
  translated code. Copied code is never put into the two MPL-2.0 files.
- **No game data**, ever.

**Licence reading (owner, 2026-11-19): compatible, record it.** Code that nv-rs licenses as GPL-3.0-or-later may be
used under the terms of GPL-3.0-only, so copying or linking it into a lodwright crate other than the two MPL-2.0 files
is allowed. Each use records its licence in its own provenance row, and the engine's provenance notes (`LEGAL.md`,
third-party code) are checked for third-party parts of the file before the row is written. MIT-era code is used with
its notice kept. Third-party code inside nv-rs is covered by the notice rules above. Describing behaviour from nv-rs is
always allowed under the rule above.

## Licence facts (checked 2026-11-19)

Read: `LICENSE`, `LEGAL.md`, `FORK.md` and the `license` fields of `Cargo.toml` in nv-rs, on the fork's branches
`fork` (`05e2a7d`) and `main` and on upstream `main` (`777b508`); GitHub's licence detection for both repositories;
`LICENSE`, `docs/LICENSING.md`, `docs/clean-room.md` and the `license` fields in lodwright-rs. Facts only; no legal
conclusion is drawn here.

| Fact | Where |
| --- | --- |
| nv-rs's `LICENSE` is the GNU GPL version 3 text; the workspace `license` field is `GPL-3.0-or-later`; `LEGAL.md` says original code is GPL version 3 "or (at your option) any later version" | nv-rs `LICENSE`, `Cargo.toml`, `LEGAL.md` |
| The fork and upstream carry byte-identical `LICENSE` and `LEGAL.md` and the same `license` field; `FORK.md` says the licence is upstream's and the fork keeps it | nv-rs, both remotes |
| Code contributed to nv-rs before 2026-10-16 was released under MIT (or Apache-2.0) and is included under the MIT terms, whose notice must be kept | nv-rs `LEGAL.md` |
| nv-rs learns the game's behaviour from the game's data files, recordings, and analysis of the game's program with a decompiler; since 2026-10-05 also from Xbox 360 development-build debug symbols and logic translated from the decompiled program (its ADR-0002, ADR-0003); since 2026-11-02 also from `fnv_decomp`, a C++ reconstruction whose repository grants no licence (its ADR-0009) | nv-rs `LEGAL.md` |
| `LEGAL.md` says the project licence "grants no rights to the game, its files, its program or anything translated from it", and that material from `fnv_decomp` is not covered by the project's licence; such material is marked where it is used | nv-rs `LEGAL.md` |
| nv-rs translates parts of xNVSE 6.4.9 and JIP LN NVSE (GPL-3.0), and carries code under BSD (libvorbis, MT19937), MIT (Algohol), Apache-2.0 WITH LLVM-exception (MSVC STL) and zlib (DXVK) notices | nv-rs `LEGAL.md`, "Third-party code" |
| lodwright-rs is GPL-3.0-only ("esplugin declares plain GPL-3.0"), except two files translated from xEdit that stay MPL-2.0 | lodwright `docs/LICENSING.md` |
| lodwright's clean-room rule forbids decompiled or disassembled programs and decompiler output of any kind, anything under an unknown licence, and GPL, LGPL or AGPL *source* as an input for learning a format (linking a GPL-3.0 crate is allowed; copying its code is not) | lodwright `docs/clean-room.md` |
| lodwright's CI guard refuses a provenance row whose URL or fact contains "decompil", "disassembl" or "leaked" | lodwright `scripts/cleanroom_guards.py` (`FORBIDDEN_SOURCES`) |

## Decisions on the points that were open

1. **Licence of a copied or linked piece: decided.** The reading is the owner's, quoted above: compatible, recorded per
   row, with nv-rs's third-party notes checked. A second owner decision (2026-11-19) widens the scope: third-party code
   inside nv-rs (xNVSE, JIP LN, BSD, zlib, Apache-2.0) may be used per row, provided the row names its exact SPDX
   identifier, keeps its notice in `THIRD-PARTY-NOTICES.md` and records the GPL compatibility reasoning in
   `engine_notes`. Code whose licence cannot be identified, or that has none (`fnv_decomp`), stays refused, and so does
   material that nv-rs marks as translated from the game's program or the debug symbols, which nv-rs's own licence does
   not cover.
2. **Clean-room scope: decided.** D5 amends `docs/clean-room.md` only for nv-rs code that is not marked as translated
   from the decompiled program, `fnv_decomp` or the debug symbols, not for nv-rs as a whole. A lint cannot tell the two
   apart, because the marking lives in nv-rs, not in lodwright's rows. The second reader tells them apart by reading
   the file and nv-rs's `LEGAL.md` and records the result in `engine_notes`; the guard checks that the note exists, not
   that it is true. This is a human check by design.
3. **The guard.** It keeps the refusal of "decompiled" and adds "fnv_decomp". A wider rule means another guard change,
   made after the xtask port (#99, #172) has landed, in the Rust guard.
4. **Who reads.** Owner answer D19: nobody second-reads rows for a format they implemented. For an nv-rs file, the
   reader also must not be the author of that nv-rs code.

## Consequences if accepted

- A row citing nv-rs names the repository, the file (or crate) at a commit, the licence case from open point 1, and the
  second reader; `LOG.md` says what was read and that no game data was involved.
- A crate dependency on nv-rs is preferred over translating code; `deny.toml` allows `GPL-3.0`
  and `GPL-3.0-only` and now also `GPL-3.0-or-later` (nv-rs declares it), and the dependency would be pinned like any
  other.
- Facts that can be measured through the bench (`oracle:` rows) keep being measured; engine source is used only when it
  is the best fit, as the answer says.

## Considered options

- Keep nv-rs out of the inputs and measure behaviour through the bench only (the plan's recommendation): not chosen by
  the owner.
- Allow nv-rs with a recorded rule and a named second reader (the owner's answer D5): this record.
