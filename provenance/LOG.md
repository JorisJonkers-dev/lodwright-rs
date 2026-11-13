# Provenance log

The human-readable side of `sources.toml`. Every row of that file is mentioned here by id, in backticks, with the
story the table cannot hold: who read what, when, and what stayed open. `scripts/cleanroom_guards.py` fails the
build if a row is not mentioned. Rules: [docs/clean-room.md](../docs/clean-room.md).

Roles: the *specifier* reads allowed inputs and records facts; the *implementer* codes from the recorded facts
(and the open MPL sources) only. One person may hold both roles at different times; the entries say which.

## 2026-11-13: setup, synthetic fixtures

Recorded by J.W. Jonkers as specifier. These facts were written down from the public pages' known content while
the repository was set up; no page was re-read afterwards, so every row is `verified = false`. They feed only the
synthetic builders in `lodwright-testkit`, never a reader of real game files.

- `dds-header-layout`: DDS magic, the 124-byte header and the offsets of height, width, mip count.
- `dds-pixel-format`: the 32-byte pixel format and the FourCC field.
- `plugin-record-header`: the 24-byte FO3/FNV record header.
- `plugin-subrecord-layout`: subrecords, and the HEDR, MAST and DATA subrecords of the TES4 record.
- `land-vhgt-layout`: the VHGT payload (offset, 33 x 33 deltas, 3 unused bytes).
- `land-vhgt-integration`: how the deltas integrate into heights, and the scale of 8.

Open: whether the real file differs from the synthetic stand-ins is checked, on the owner's machine only, by
comparing with the owner's own plugins and xLODGen output (`docs/TESTING.md`, oracle plan); nothing of that
comparison is recorded here except the resulting facts.

## 2026-11-14: plugin reader

Recorded as specifier from the public UESP pages' known content, not re-read afterwards: every row is
`verified = false`, and the implementation was written from the rows only.

- `plugin-group-layout`: the 24-byte GRUP header and its size rule.
- `plugin-group-types`: the group types the walker distinguishes (1 sets the worldspace; the others are descended into).
- `plugin-record-compression`: the compressed-record flag and the size prefix.
- `plugin-deleted-flag`: deleted records are skipped.
- `plugin-xxxx-subrecord`: oversized subrecords.
- `cell-xclc-grid`: the cell grid coordinates.
- `wrld-edid`: the worldspace name.
- `plugin-formid-masters`: FormID master index, used by the worldspace assembly (next slice).

Open: whether the real records differ from these descriptions is checked on the owner's machine against the
owner's own plugins.
