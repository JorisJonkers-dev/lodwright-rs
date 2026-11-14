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

## 2026-11-14: terrain planning and output layout

Specifier entries, written before the planner and the layout module. All rows are `verified = false`.

- `cell-size-units`: world units per cell and per LAND interval; the mesh coordinates depend on it.
- `lod-block-naming`: how a block is named and what its coordinates mean. From the owner's research notes about the
  folder layout they saw in their own LOD output; not re-checked against the files yet.
- `lod-terrain-paths`: the folders and file names of terrain meshes and textures.
- `lod-settings-path`: where the per-world settings file lives.

Open: the three layout rows are the first thing the owner checks on their machine (list the output folder of an
existing run, compare names). The contents of `.btr`, `.dds` normal maps and `.lod` are not specified yet, so the
planner writes no such file: it computes what would be written.

## 2026-11-14: public-source study of the LOD file formats (docs/spec)

Specifier entry, written from public documentation only: the UESP wiki, the Tale of Two Wastelands forum, the Viva New
Vegas guide, the STEP default-INI list, and search-result excerpts of the Nexus Mods wiki and the generator's forum
thread. No source code, binary or decompiled output of any generator was read. Several pages (the GECK wiki, the Nexus
wiki and mod pages, UESP behind its bot filter) answered 403 or a challenge page, so facts known only from a search
excerpt say so in their row and stay `verified = false`; every row here is unverified until a second reader re-reads
the page, or the owner's output tree answers it.

- `fnv-lod-block-container-nif`: block meshes are `.nif` in these games; `.btr`/`.bto` are not New Vegas files.
- `fnv-lod-object-block-names`: `<world>.level4.x-20.y-28.nif` and the `.high.` variant.
- `fnv-lod-folders`: meshes and textures folders under `landscape/lod/<world>/blocks`, and `trees`.
- `fnv-lod-terrain-names`: terrain names for levels 4, 8, 16, 32 and the `.n.` normal-map name.
- `fnv-lod-levels`: the four terrain levels, one object level, terrain before objects.
- `fnv-lod-texture-options`: generator texture-size guidance.
- `fnv-lod-atlas-name`: one sighting of an object atlas name.
- `fnv-lod-settings-dlodsettings`: the settings file is `LODSettings/<world>.dlodsettings`.
- `skyrim-lod-settings-struct`: the Skyrim 16-byte `.lod` struct, as the likely shape only.
- `fnv-tree-lod-files`: `.dtl`, `treetypes.lst` and the fixed atlas.
- `fnv-btr-bto-other-games`: why `.btr`/`.bto` do not apply.
- `fnv-ini-terrainmanager`: INI names and defaults.
- `fnv-archive-loading`: archive and loose-file precedence.

Result for the three older layout rows `lod-block-naming`, `lod-terrain-paths` and `lod-settings-path`: public sources
contradict all three (extension, level prefix, normal-map name, settings extension). Their notes say superseded; the
owner's tree decides (docs/owner-runbook.md section 2, extended in docs/spec/measurements.md). Open: byte layouts of
the block NIFs, `.dlodsettings` and `.dtl`, which no public document gives.

## 2026-11-14: BSA v104 layout (lodwright-assets)

Specifier entry for the BSA reader, written from the public UESP pages only (CC-BY-SA-2.5): the Skyrim archive page,
which states that v104 is also the Fallout 3 and New Vegas version, cross-read with the Oblivion v103 page. No source
code of any archive tool was read, no xEdit archive code was used, and no game archive was opened. Implementer entry:
the reader and the testkit builder were written from the rows below by the same person, which clean-room.md
allows only with a second reader re-deriving the facts (`verified` stays false until then).

- `bsa-v104-header`: the 36-byte header and the archive flags (0x1, 0x2, 0x4, 0x40, 0x100).
- `bsa-v104-folder-records`: 16-byte folder records and the offset-minus-name-length rule.
- `bsa-v104-file-records`: 16-byte file records, the size flag bits, the lower-case name block.
- `bsa-v104-data-blocks`: compressed and uncompressed blocks, embedded paths, zlib. Open: whether the size counts the
  embedded path.
- `bsa-path-form`: lower-case, backslash paths; the case-insensitive lookup is a project choice.

The resolver (loose files over archives, archives by plugin and name) relies on `fnv-archive-loading`, which stays
unverified and says nothing about two points the code has to decide: how an archive name is tied to its plugin (only
"shares its name") and what `.override` changes (only "an archive can replace files of earlier archives"). The resolver
therefore offers both readings as a documented option and defaults to later archives winning, as the work item asks.
Hashes are not computed (no row for the hash function), so the testkit writes stand-in hashes and the reader ignores
them.
