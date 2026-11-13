## Change

What this pull request changes and why.

## Checklist

- [ ] Provenance: every format fact this change relies on has a row in `provenance/sources.toml` (with the licence
      of its source) and is mentioned in `provenance/LOG.md`; every `// source:` comment names such a row.
- [ ] No game data: no `.esm .esp .esl .bsa .ba2 .nif .kf .dds .btr .bto .btt .lod .wav .ogg .mp3 .fos .ess`, no
      extract, dump, screenshot or output of xLODGen from real data, and no committed binary. Test data is built
      in code (`xlodgen-testkit`).
- [ ] Clean-room statement: I did not read the unlicensed 2016 LODGen C# repository, decompiled or disassembled
      binaries, or leaked source for this change, and I worked from the recorded sources only. If I was the
      specifier for the formats this change implements, I did not implement them (or a second reviewer
      re-derived the facts), as `docs/clean-room.md` requires.
- [ ] New source files carry `SPDX-License-Identifier: MPL-2.0` in their first five lines.
- [ ] Checks: `task check` passes locally.

## Verification

What was run and what it showed (tests, golden files, round trips; for numeric output, the figures).

## Open questions

Format questions this change leaves open, and the black-box experiment that would settle each. "None" if none.
