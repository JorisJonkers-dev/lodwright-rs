---
status: proposed
date: 2026-11-19
---
# The engine port's source as an input, under a recorded rule

**Proposed, not accepted** (#123). This records the owner's answer D5 of the LOD completion plan (2026-11-19), the
licence facts it asked to check first, and a proposed rule. The owner confirms the licence reading before it merges.

The owner's answer: the source of the engine port (nv-rs, upstream `slaterain/nv-rs`, fork `JorisJonkers-dev/nv-rs`)
may be used as an input for LOD and archive facts when it is the best fit. Every use is a provenance row that names the
source file or crate, its licence and a second reader. A crate dependency is preferred over copying code. No game data,
ever.

## The proposed rule

This pull request writes the rule into `docs/clean-room.md` (allowed input 6) and the guard, so it takes effect when
it merges; it should merge only after the owner confirms the licence reading below (#123).

- **Allowed:** nv-rs code that nv-rs does not mark as translated from the decompiled game program, from `fnv_decomp`
  or from the Xbox debug symbols, when it is the best fit for a LOD or archive fact.
- **Not allowed:** material nv-rs marks as translated from the decompiled program, `fnv_decomp` or the symbols. This
  keeps the clean-room refusal of decompiler output and of sources without a licence. The guard already refuses a row
  whose URL or fact says "decompiled"; it now refuses "fnv_decomp" too.
- **Each use is a row** that cites one nv-rs file at a 40-character commit
  (`https://github.com/<owner>/nv-rs/blob/<commit>/<path>`), states that file's licence (GPL-3.0-or-later, or MIT for
  code contributed before 2026-10-16), and names a `second_reader` who did not implement the format and did not write
  that nv-rs code. The guard enforces the URL form, the licence and the field.
- **Only those two licences.** Third-party code inside nv-rs under another licence (BSD, Apache-2.0 WITH
  LLVM-exception, zlib, the GPL-3.0 translations of xNVSE and JIP LN) is not covered by this rule; the guard accepts a
  row only when its licence starts with `GPL-3.0-or-later` or `MIT`. Widening that is a later owner decision.
- **Preference:** a crate dependency over copied code; a described behaviour (a row, then lodwright's own code) over
  translated code. Copied code is never put into the two MPL-2.0 files.
- **No game data**, ever.

Licence reading to confirm (the owner's call): lodwright is GPL-3.0-only; nv-rs's original code is GPL-3.0-or-later,
which permits use under version 3, and MIT-era code is permissive with its notice kept. On that reading, copying or
linking nv-rs original code into any lodwright crate except the two MPL-2.0 files is allowed under GPL-3.0-only, and
describing behaviour from it is always allowed under the rule above.

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
| lodwright's CI guard refuses a provenance row whose URL or fact contains "decompil", "disassembl" or "leaked" | lodwright `xtask/src/guards.rs` (`FORBIDDEN_SOURCES`) |

## Open points before acceptance

1. **Licence of a copied or linked piece.** An nv-rs file can fall under any of the cases above: original
   GPL-3.0-or-later work, MIT-era code, third-party code under its own licence, or material translated from the game's
   program or `fnv_decomp` that, by nv-rs's own statement, its licence does not cover. Whether and how each case can
   enter a GPL-3.0-only work is the owner's call (with advice if wanted); a row has to record which case applies.
2. **Clean-room scope.** The clean-room rule forbids decompiler output and GPL source as format inputs; nv-rs states that
   some of its logic is translated from the decompiled program and from `fnv_decomp`, and it is GPL source. The
   decision has to say whether D5 amends `docs/clean-room.md` for nv-rs as a whole, or only for nv-rs code that is not
   marked as translated from the decompiled program, `fnv_decomp` or the debug symbols, and what a second reader checks
   to tell them apart.
3. **The guard.** The proposal keeps the refusal of "decompiled" and adds "fnv_decomp". If the owner wants a wider
   rule, the guard changes again; a guard change runs alone, not beside #99 (the guard port to `cargo xtask`).
4. **Who reads.** Owner answer D19: nobody second-reads rows for a format they implemented. For an nv-rs file, the
   reader also must not be the author of that nv-rs code.

## Consequences if accepted

- A row citing nv-rs names the repository, the file (or crate) at a commit, the licence case from open point 1, and the
  second reader; `LOG.md` says what was read and that no game data was involved.
- A crate dependency on nv-rs is preferred over translating code; `deny.toml` already allows GPL-3.0 licences, and the
  dependency would be pinned like any other.
- Facts that can be measured through the bench (`oracle:` rows) keep being measured; engine source is used only when it
  is the best fit, as the answer says.

## Considered options

- Keep nv-rs out of the inputs and measure behaviour through the bench only (the plan's recommendation): not chosen by
  the owner.
- Allow nv-rs with a recorded rule and a named second reader (the owner's answer D5): this record.
