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

## 2026-11-14: LAND layers, LTEX and TXST

Specifier entry, from the UESP LAND page (CC-BY-SA-2.5, last edited 3 August 2024) and from xEdit's FNV and common
definitions (`TES5Edit/TES5Edit` revision `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b`, MPL-2.0). Not read: the 2016
generator C# repository, decompiled binaries, any GPL or leaked code. The GECK wiki returned a bot challenge and was not
used. The same person wrote the rows and the code, so every row stays `verified = false` until a second reader re-derives it.

- `land-vnml-layout`, `land-vclr-layout`: 33 x 33 grids of three bytes. The byte encoding of a normal is open; raw bytes are kept.
- `land-layer-header`: BTXT and ATXT, 8 bytes; the UESP "big-endian FormID" claim was not followed (xEdit and the
  rest of the reader use little endian).
- `land-vtxt-layout`: position, unused, opacity; sparse, so unlisted positions are zero (an inference).
- `land-quadrant-grid`: the quadrant to vertex mapping used by the opacity lookup.
- `ltex-texture-set`: LTEX has TNAM (TXST) and ICON in FNV; which wins is open, TXST first is the implemented choice.
- `txst-texture-paths`: TX00 and TX01 are the base and normal paths.
- `plugin-record-override`: a later plugin's LTEX, TXST or LAND replaces the earlier one as a whole.

## 2026-11-14: worldspace data for TTW (WRLD parent, land data, bounds, cell water)

Specifier entry from the UESP CELL and WRLD pages (CC-BY-SA-2.5, last edited 30 July 2024) and xEdit
(`TES5Edit/TES5Edit` revision `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b`, MPL-2.0). Not read: the 2016 generator C#
repository, decompiled binaries, leaked or GPL code. The same person wrote the rows and the code; all rows stay
`verified = false` until a second reader re-derives them. The TTW cases come from `research-lod.md` section 6 of the
umbrella repository (worlds `Wasteland` and `DCworld01`..`DCworld18`, the master chain through `Fallout3.esm`, an
adjustments plugin that overrides the worldspace); they use synthetic plugins named like the real ones, nothing else.

- `wrld-parent`: WNAM and the PNAM flag bits (xEdit).
- `wrld-land-data`: DNAM, default land and water height.
- `wrld-bounds`: NAM0 and NAM9. Open: the unit of the stored floats; not used by the planner.
- `cell-water`: DATA has-water flag and XCLW. Open: what an XCLW of 'Default' resolves to.
- `wrld-child-inherits`: what the sources say a child inherits (climate, water, map data) and what they do not say
  (LAND cells, LOD). Open question; the default is no inheritance, the option `FlaggedDefaults` takes only the default heights.
- `plugin-record-override` (existing row) is applied to WRLD: a later plugin's WRLD record replaces the earlier one whole.

## 2026-11-14: second reader pass over the BSA, LAND and worldspace rows

Second-reader entry, role held: second reader only. The facts were re-derived before the implementation was read, from
the UESP pages (raw wikitext of the Skyrim BSA, Oblivion BSA, LAND, WRLD, CELL and FormID pages) and from xEdit
`Core/wbDefinitionsCommon.pas`, `Core/wbDefinitionsFNV.pas` and `Core/wbInterface.pas` at revision
`3ccc9e7d733becfcc97940cf0c877fa3d6213c5b` (MPL-2.0). Not read: the 2016 generator C# repository, decompiled binaries,
GPL or leaked code, the GECK wiki.

Flipped to `verified = true` (the re-derivation agrees with the row): `bsa-v104-header`, `bsa-v104-folder-records`,
`bsa-v104-file-records` (one wording fix: bit 31 is stated on the Oblivion page, not the Skyrim page), `bsa-path-form`,
`land-vclr-layout`, `land-layer-header`, `land-quadrant-grid`, `ltex-texture-set`, `txst-texture-paths`, `wrld-parent`,
`wrld-land-data`, `cell-water`. Open points recorded in those rows (FormID byte order, which of TNAM and ICON wins, what an
XCLW of 'Default' means) stay open and are not what was verified.

Left `verified = false`, with an issue each: `bsa-v104-data-blocks` (no source says whether the size counts the embedded
path), `land-vnml-layout` and `land-vtxt-layout` and `wrld-child-inherits` (the row states something neither source
states), `plugin-record-override` ('as a whole' is not in the cited page), `wrld-bounds` (the note's xEdit display factor
is the Starfield branch; for FO3 and FNV xEdit uses 1/4096).

## 2026-11-14: contract v1 and release assets

The plan and result documents, their schemas, the digests and the release assets are our own design
(`docs/research/lod-pipeline-architecture.md` section 7 in the NV-Linux-Modding repository): no format fact was read
or recorded, so there is no new row. The author held the implementer role only; no input from the forbidden list was
read. The documents carry numbers, digests and relative output names, never game content.

## 2026-11-14: NIF 20.2.0.7 container and shape blocks (lodwright-nif)

Specifier entry for the generic NIF reader and writer. Read: `Core/wbDataFormatNif.pas` and
`Core/wbDataFormatNifTypes.pas` of TES5Edit/TES5Edit (the MPL-2.0 xEdit repository, not the unlicensed xLODGen
repository) at revision `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b`, both under an MPL-2.0 file header. Only the
20.2.0.7 branch of each definition was recorded. The niftools `nif.xml` was not used as a source: its repository
(niftools/nifxml) carries a GPL-3.0 licence file, so it is not an allowed input. The first 2.5 KB of it (the
version-expression macros at the top) were downloaded and looked at before that licence was noticed, and the file was
deleted unused; none of its content is in any row. No decompiled output, game file or other tool's NIF source was read.

- `nif-header-magic-version`: the first line and the version `u32`.
- `nif-header-fields`: endian, user versions, block count, the Bethesda export info and which fields depend on user version 2.
- `nif-block-table`: block type table, per-block type index and size, string table, groups, then the block bodies.
- `nif-footer-roots`: the footer.
- `nif-primitive-encodings`: strings and links as `i32` indices, bool as one byte, the small vector, colour and triangle types.
- `nif-objectnet-avobject-ninode`: `NiObjectNET`, `NiAVObject`, `NiNode`.
- `nif-trishape-geometry`: `NiGeometry` and `NiTriShape`.
- `nif-geometry-data-layout`: `NiGeometryData`, `NiTriBasedGeomData`, `NiTriShapeData`.
- `nif-property-layout`: `NiAlphaProperty`, `NiZBufferProperty`, `NiMaterialProperty`.

Roles: the same author recorded the rows and wrote the crate, so no second reader has re-derived the facts and every
row stays `verified = false`; the pull request says so. Open: whether the short-string length byte counts the trailing
NUL (the crate carries those bytes raw), and what the game itself accepts: nothing here was loaded by the engine.
No block profile for New Vegas LOD meshes is recorded; that waits for the owner's block NIF measurements.

## 2026-11-14: DDS legacy header, mip chain and BC1, BC3, BC5 layout (lodwright-dds)

Specifier entry, written from Microsoft's public DDS and Direct3D 10 block compression pages (Microsoft Learn), each page
fetched and read on this date; the page's last-updated date is in each row note. No source code of any generator and no
decompiled output was read. The existing rows `dds-header-layout` and `dds-pixel-format` are relied on unchanged.

- `dds-header-flags-caps`: the header flag and caps values a writer sets.
- `dds-fourcc-formats`: `DXT1`, `DXT5` and `ATI2` for BC1, BC3 and BC5, and why the legacy header is used.
- `dds-reader-validation`: what a reader checks before trusting a file.
- `dds-linear-size-pitch`: pitch and level size of block-compressed surfaces.
- `dds-mip-chain`: levels halve down to 1 by 1.
- `bc-block-padding`: surfaces are stored in whole 4 by 4 blocks.
- `bc1-block-layout`, `bc3-block-layout`, `bc5-block-layout`: the block sizes and their parts.

Roles: the same author recorded the rows and wrote the crate; every row stays `verified = false` and the pull request says
so. Open: rounding of odd sizes in a mip chain (the page's examples are all even; the crate rounds down, the Direct3D
convention), and which header fields (caps, flags, linear size) the game's own texture loader insists on: that is the
owner's measurement M5 on a reference file, by role `oracle:` and numbers only.



## 2026-11-14: `lodwright probe` (NIF header and geometry, DDS pixel format and BC1/BC3, no game file)

Recorded by J.W. Jonkers as specifier, read in the sources themselves (not from memory): the NIF rows from
`Core/wbDataFormatNif.pas` of the TES5Edit repository at revision `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b`
(MPL-2.0; the reader below was written from the rows, not translated), the DDS and block-compression rows from the
Microsoft documentation repository. The reader was then written as implementer from the rows only. Disclosure: to look for a
permissively licensed NIF description the author opened the niftools nifxml repository (GPL-3.0) and saw the first
2,500 bytes of its `nif.xml` (version-expression tokens) and its licence; that repository is a forbidden input, nothing
from it was used or recorded, and every NIF fact above comes from the xEdit file. The owner may want the second reader
to re-derive the NIF rows from the xEdit file alone. No game file was opened; every test builds its bytes with the testkit.

- `nif-header-layout`: the text line, version, endian, user versions, block count and the Bethesda export info.
- `probe-nif-block-table`: block types, indices, sizes, strings and groups; block data follows the header.
- `probe-nif-geometry-data-layout`: the NiGeometryData fields for 20.2.0.7 and the vector flags.
- `nif-trishape-strips-data`: the triangle and strip data blocks (16-bit indices).
- `nif-texture-set`: the texture path array.
- `dds-pixel-format-masks`: bit count, masks, flags and where mip 0 starts.
- `dds-header-dxt10`: the DX10 extension, reported only.
- `probe-bc1-block-layout`: the 8-byte block, the four- and three-colour rule and the bit order.
- `probe-bc3-block-layout`: the alpha block and the colour block that follows it.

Open: whether the real New Vegas block NIFs match these layouts is exactly what the owner's measurements M1 and M5
answer with the command. The probe reports `layout_matches: false` for a block whose fields do not add up to its size,
so a wrong row shows up as a number and not as a wrong count. Open also: the probe widens 5 and 6 bit colours by bit
replication and interpolates in 8 bits (the page leaves both open), so decoded statistics can be off by one.

## 2026-11-14: second reader pass over the `lodwright probe` rows

Second-reader entry, role held: second reader only. The facts were re-derived before the implementation was read, from
`Core/wbDataFormatNif.pas` and `Core/wbDataFormatNifTypes.pas` of TES5Edit/TES5Edit at revision
`3ccc9e7d733becfcc97940cf0c877fa3d6213c5b` (MPL-2.0) and from the Microsoft documentation repository at revision
`bc82a65f74ef2c4f2f44a3d7db5f7cfebfdc3583` (the DDS header, pixel format, DX10 header and programming guide pages, and
the Direct3D 9 opaque and alpha texture pages). Not read: any niftools file, the 2016 generator C# repository,
decompiled binaries, GPL or leaked code. The author of the probe had disclosed seeing the first 2.5 KB of a GPL-3.0
NIF description; none of its content was used here, and every NIF layout below was found in the xEdit definitions
alone. The probe's NIF reading shows no sign of a source outside the xEdit file.

Flipped to `verified = true`: `nif-header-layout`, `probe-nif-block-table`, `probe-nif-geometry-data-layout`,
`nif-trishape-strips-data`, `nif-texture-set`, `dds-pixel-format-masks`, `dds-header-dxt10`. Wording fixes made while
doing so: `probe-nif-geometry-data-layout` now states where the material CRC sits (after the vector flags, before
has-normals; the probe does not read it, which only matters for user version 12); `dds-pixel-format-masks` names the
programming guide page that gives the magic and the surface placement. That each block has the size the block size
array gives is not stated as a rule in xEdit; it holds because xEdit writes the array from each block's serialised size.

Left `verified = false`: `probe-bc1-block-layout` (layout, colour order, bit order and the three- and four-colour rule
agree with the page; the widening of 5 and 6 bit values by bit replication and the 8-bit interpolation are not in any
allowed source) and `probe-bc3-block-layout` (the six-alpha formula in the row had an index slip, `(c-1)` for `k`,
corrected here to match the page and the code; the same widening gap applies to the colour block). An open issue
covers the widening, and the 8-bit scaling of uncompressed channels and their little-endian pixel assembly, which no
row states either.
