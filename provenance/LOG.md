# Provenance log

The human-readable side of `sources.toml`. Every row of that file is mentioned here by id, in backticks, with the
story the table cannot hold: who read what, when, and what stayed open. `cargo xtask guards` fails the
build if a row is not mentioned. Rules: [docs/clean-room.md](../docs/clean-room.md).

Frozen: this file is history up to 2026-11-21. A new row is its own file under `provenance/rows/`, whose `note` is its
log entry; nothing is appended here any more.

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

## 2026-11-14: second reading of the NIF and DDS rows

Second reader for `lodwright-nif` and `lodwright-dds`, independent of the specifier and the implementer of both. The
facts were re-derived before the crates were opened, then compared with the rows and with the code.

NIF: the only inputs were `Core/wbDataFormatNif.pas`, `Core/wbDataFormatNifTypes.pas` and, for the primitive types,
`Core/wbDataFormat.pas` of TES5Edit/TES5Edit at revision `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b` (MPL-2.0 file
headers). No `nif.xml` and no GPL source was opened. Every condition and field order below was read from the `NiHeader`,
`NiFooter`, `NiObjectNET`, `NiAVObject`, `NiNode`, `NiGeometry`, `NiGeometryData`, `NiTriBasedGeomData`,
`NiTriShapeData`, `NiAlphaProperty`, `NiMaterialProperty` and `NiZBufferProperty` definitions and the `NiHeader_En*`,
`NiAVObject_*`, `NiGeometry_En*`, `NiGeometryData_*` and `NiMaterialProperty_En*` conditions, and agreed with the row and
with the crate: `nif-header-magic-version`, `nif-header-fields`, `nif-block-table`, `nif-footer-roots`,
`nif-primitive-encodings`, `nif-objectnet-avobject-ninode`, `nif-trishape-geometry`, `nif-geometry-data-layout` and
`nif-property-layout` are now `verified = true`. Points checked with care: the user version 2 condition of the Bethesda
header block (20.2.0.7 with user version 3 or more); the three export-info conditions on user version 2 (130, 131, 103);
the u16 or u32 flags switch at user version 2 of 26; the property list at 34 or less; effects below 130; the UV set count
as `(Num UV Sets and 63) or (Vector Flags and 1)` where the first field exists only before 4.2.2.0; the tangent test on
bit 0x1000 with normals present; the block size array being filled from each block's serialised size.

Nothing in the crate encodes a NIF fact that the xEdit sources do not support, so there is no sign that anything was
derived from outside the allowed set, and no taint review issue was needed. Two remarks, neither a discrepancy: the crate
keeps shapes and geometry data at user version 2 of 100 or more opaque, which is more cautious than xEdit (it keeps the
material data for non-particle shapes at those versions), and the `nif-trishape-geometry` note now says its row covers
the below-100 case only. The crate rejects a block whose parsed length differs from its declared size and rejects bytes
after the footer: those are the crate's own strictness, not format facts, and the sources do not contradict them.

The open question of the short-string length byte is answered by `TdfChars.ReadChars` in `wbDataFormat.pas`: a prefixed
string defined as terminated drops its last byte from the value only when that byte is the terminator, so the length byte
covers a trailing NUL whenever one is stored. The header short strings are defined that way and the block type and string
table entries (sized strings) are not. Carrying the bytes raw, as the crate does, is consistent with that.

DDS: re-derived from Microsoft Learn's DDS_HEADER, DDS_PIXELFORMAT, Programming Guide for DDS, DDS Texture Example and
Block Compression (Direct3D 10) pages (the MicrosoftDocs/win32 sources of those pages, plus the three layout diagrams of
the block compression page for the order of the BC3 and BC5 parts). `dds-header-flags-caps`, `dds-fourcc-formats`,
`dds-reader-validation`, `dds-linear-size-pitch`, `bc-block-padding`, `bc1-block-layout`, `bc3-block-layout` and
`bc5-block-layout` agree and are `verified = true`. The BC3 diagram shows the alpha block first and the colour block
second, the BC5 diagram red first and green second, as the rows say. `dds-mip-chain` stays `verified = false`: the row is
accurate, but the crate rounds odd sizes down and cites that row for it while the row says that rounding is open; the
60 by 40 example on the block compression page (30x20, 15x10) does not settle it either. Issue 60 tracks it.



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

## 2026-11-16: layout, terrain block structure, textures and settings from the owner's own game files

Specifier entry, oracle input class 3 of clean-room.md: the owner's install was read read-only (archive listings and
three extracted blocks in a scratch folder outside the repository, removed afterwards; nothing copied in). Only names,
counts and numbers were kept. No xLODGen source or binary was read. Rows: `game-lod-terrain-layout`,
`game-lod-terrain-block-nif`, `game-lod-terrain-textures`, `game-dlodsettings-layout`, `game-lod-object-layout`.
They supersede `lod-block-naming`, `lod-terrain-paths` and `lod-settings-path` and settle M1 (block structure), M2/M3
(names, span: the coordinates are the south-west cell and multiples of the level), M5 (texture formats) and the first
part of M8 (the 24-byte settings layout). Open: the contents of the additional-data block beyond its per-vertex float
channel, the sentinel low heights in some blocks, the object block structure (M4, M6) and the tree files (M7). A second
reader has not verified these rows.

## 2026-11-16: checksum of compressed records in the game's own master

`game-plugin-zlib-checksum`: reading the owner's master plugin read-only showed LAND records whose zlib checksum does
not match; the reader now falls back to raw inflate for that failure only and still checks the stated size. Found by
running `lodwright plan` on the real file (numbers only: the reader failed before the fix, then read 16 386 cells of
the main worldspace).

## 2026-11-16: object LOD block structure

`game-lod-object-block-nif`: one object block of the game's own tree was read read-only (structure, counts and
number ranges only; the file was removed afterwards). Settles part of M6: block structure and atlas reference. Open:
the segment table meaning, the atlas packing rule and UV scale, which references and which `_lod` models enter a
block (692 `_lod` models exist in the base archive), tree LOD files (M7).

## 2026-11-16: object records

`fnv-object-records`: flags and subrecords of the records that can appear in object LOD, read in the xEdit FNV
definitions (MPL-2.0) at the pinned revision. Open: the placement record size inside SCOL parts.

## 2026-11-16: BC2

`bc2-block-layout`: the DXT3 block layout from the Microsoft block compression page, used to read and write DXT3 textures. Not yet verified by a second reader.

## 2026-11-16: object LOD selection, rotation and atlas

`game-object-selection`, `game-object-rotation`, `game-object-atlas`: found by generating blocks from the records and
comparing with the game's own blocks read-only (numbers only). One block matched in vertex count (1715) and triangle
count (2071) exactly and in every vertex position once the rotation convention was fixed. Open: the meaning of the
segment table, the shader flag words, and the in-game behaviour of tiling textures inside an atlas.

## 2026-11-16: object block rules

`game-object-block-rules`: the remaining rules (disabled references, high priority in both files, collections drawn
with their own model) were found by comparing block by block, which raised the blocks with equal counts from 196 to
287 of 295.

## 2026-11-16: object shape offsets

`game-object-shape-offset`: found by fitting, model by model, the offset between generated vertices and the vertices of
the game's own object blocks (read-only, numbers only). The translation of a model's shapes is added after the
reference scale, and the rotation of the model's nodes is ignored. The median share of matching vertices per block
went from 0.881 to 0.942 and the lowest block from 0.108 to 0.814. Open: the remaining 6 percent of vertices per block.

## 2026-11-16: object block cell

`game-object-block-cell`: the two blocks with unequal counts swapped one object of 139 vertices, a temporary
reference just south of its cell's edge; the game's blocks follow the cell that holds it. The earlier 94 percent
vertex share was an artefact of comparing vertices rounded to a 0.05 grid; a one-to-one match within 0.05 units gives
99.98 percent, all 295 blocks with equal counts and 291 with every vertex matched. Open: four blocks with one displaced
object each.

## 2026-11-16: water planes

`game-lod-water-height` and the extended `game-lod-water-shape`: measured on the game's own terrain blocks (read-only,
numbers only) by comparing the water shapes' structure, heights and covered ground with the cells' water records.
The planes lodwright writes are its own design (flat rectangles over the ground below water); the overlap with the
game's planes is 0.986 at level 4 and 0.91 to 0.98 above. Open: the game's outline and edge slopes.

## 2026-11-16: water shapes in terrain blocks

`game-lod-water-shape`: found by running `lodwright verify --reference` on the game's own tree against the generated
one: file names and counts are identical, and 122 terrain blocks differ in block types because the game's blocks carry
a water plane. Open: how the outline and height of the plane are derived.

## 2026-11-16: land texture diffuse

The diffuse blend is our own design (base texture per quadrant, layers by opacity point, times vertex colour, textures reduced to 32 by 32 tiles). No new format facts; checked against the game's own diffuse maps for three blocks: channel correlation 0.85 to 0.95 and means within 3 of 255.

## 2026-11-16: tree LOD

`xedit-tree-lod-files`, `xedit-tree-lod-selection`, `xedit-tree-lod-atlas`: read in the tree sections of `wbLOD.pas` of
xEdit (MPL-2.0, revision 081cd8672bd366306eaafbbc8771f3a72c6cea7a); the code translated from it says so. Specifier and
implementer were the same person, so the rows are unverified and want a second reader.

`game-tree-lod-dtl`, `game-tree-lod-lst`: tree LOD files that mods ship (three sets, 29 to 193 block files each) were read
read-only and compared by numbers with output generated from the base game's records and the billboard textures of two
mods; no file, mod name or content is recorded here. Result: list layout, block layout, positions, scales, form ids and
types agree; the rotation is random in the original and lodwright derives it from the form id. The packed rectangles
differ in placement (gaps of 12 to 16 pixels against 2 in the original code, widths rounded to 4), which does not
change what the game reads. Open: the order of references inside a block (the sets agree with ours as sets only) and
the in-game result (owner).

## 2026-11-19: `wrld-bounds` note corrected (issue 51)

Specifier entry, role held: specifier for the correction only (not the implementer of the worldspace reader). Read
`Core/wbDefinitionsCommon.pas` (`TwbGameDefCommon.wbWorldObjectBounds`) and `Core/wbInterface.pas` (`TwbGameDef.IsSF1`,
`TwbFloatDef.ToValue`, `TwbFloatDef.FromValue`) of TES5Edit/TES5Edit at revision
`3ccc9e7d733becfcc97940cf0c877fa3d6213c5b` (MPL-2.0 file headers), and `Core/wbDefinitionsFNV.pas` to confirm that the
FNV WRLD definition uses `wbWorldObjectBounds`. Not read: the 2016 generator C# repository,
decompiled binaries, GPL or leaked code.

The fact of `wrld-bounds` (NAM0 minimum and NAM9 maximum, two f32 each, x then y) is unchanged. The note had the xEdit
display factor the wrong way round: 1/100 is the Starfield branch of `IsSF1`; FO3 and FNV use 1/4096, and xEdit shows
the stored value multiplied by that factor (and divides a value typed in by it before storing). The unit of the
stored floats stays open: the factor converts between the stored f32 and the number xEdit shows; it does not state
the unit. The row stays `verified = false` until a second reader re-reads the corrected note
against the same two files. What that reader checks: the `IsSF1` argument order in `wbWorldObjectBounds`; that `IsSF1`
returns the first argument only for Starfield; that `ToValue` multiplies by the scale. An owner check (NAM0 and NAM9 of
one plugin's worldspace against the cell range of its LAND records, numbers only) would settle the unit.

## 2026-11-19: `plugin-record-override` narrowed to what its source says (issue 53)

Specifier entry, role held: specifier for the correction only (not the implementer of `texture.rs` or `worldspace.rs`).
The UESP page `Tes4Mod:FormID` answered HTTP 403 to an automated read on this date, so it was not re-read; the quoted
sentence is the one the specifier and the second reader recorded on 2026-11-14. A search for a public source with a
stated licence that says the game replaces an overridden record whole was not successful: xEdit (MPL-2.0, revision
`3ccc9e7d733becfcc97940cf0c877fa3d6213c5b`) models a winning override per record in its conflict view, but that is the
tool's model and states nothing about the game. Two public pages discuss the point but cannot be rows: the xEdit
documentation (repository TES5Edit/TES5Edit.github.io, no licence stated) and LOOT's "introduction to load orders"
(repository loot/loot.github.io, GPL-3.0); neither was used beyond a search-result excerpt, and the owner may decide
whether a GPL-3.0 documentation page counts as an allowed input. Not read: the 2016 generator C# repository, decompiled
binaries, GPL or leaked code.

Change: the fact now says only what the page supports (a record with the master's FormID replaces the master's data
object) and that the page is silent on whole versus field-level replacement. Whole-record replacement is recorded in
the note as the project's choice. The code was checked against that choice: `TextureResolver::from_load_order`
(`texture.rs`) inserts each later LTEX and TXST over the earlier one by FormID, and `WorldIndex` and `build`
(`worldspace.rs`) replace a WRLD record's resolved fields and a cell's heights, layers and water from the later plugin;
nothing merges fields from two plugins. One small exception, consistent with the choice: a later WRLD without an editor
id keeps the earlier name, which is a display label only.

The row stays `verified = false`. What a second reader checks: the quoted sentence on the UESP page and its licence
footer; that the new fact claims nothing beyond it. What settles the open question (owner, numbers only): two plugins
that both override one LAND record, the later one changing only part of the record (for example VHGT but not the
layers); compare the game's own terrain LOD or the in-game ground built from that load order with lodwright's output
for both readings.

## 2026-11-19: `land-vnml-layout`, `land-vtxt-layout` and `wrld-child-inherits` narrowed (issue 54)

Specifier entry, role held: specifier for the corrections only (not the implementer of `layers.rs` or
`worldspace.rs`). Nothing was re-read on this date: the UESP LAND and WRLD pages answered HTTP 403 to automated reads,
and the discrepancies are the ones the second reader recorded on 2026-11-14 (issue 54) from those pages and from xEdit
(revision `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b`, MPL-2.0). Not read: the 2016 generator C# repository, decompiled
binaries, GPL or leaked code. Each fact now states only what its source states; what the code does beyond it is
labelled as the project's choice in the note, with the open question.

- `land-vnml-layout`: "rows from the south" removed from the fact. The reader shares one grid accessor with VCLR, whose
  row order is recorded; nothing that generates LOD reads VNML, so no output depends on the clause today.
- `land-vtxt-layout`: "positions not listed have no opacity" moved from the fact to the note as the project's choice.
  The diffuse blend relies on it, so it is a LOD question, not only a record question: if the game treats an unlisted
  position differently, generated terrain colours differ near layer edges.
- `wrld-child-inherits`: now only the UESP Oblivion statement (climate, water and map data). The FNV flag names are
  `wrld-parent` (already verified). The merged sentence about what the flags make a child take is an open question;
  the code's default (no inheritance) and the `FlaggedDefaults` option are labelled as choices. Which worlds get LOD in
  an all-worlds run depends on this.

All three stay `verified = false`. What a second reader checks: that each fact now matches the UESP page (and, for VNML
and VTXT, xEdit `wbLandNormals` and `wbLandLayers`) with nothing added. What settles the open points (owner, numbers
only): for VTXT, in one owner plugin count, per ATXT layer, how many of the 289 positions are listed and whether listed
points with opacity 0.0 occur (if every layer lists all 289, the question has no effect on output); for the child
worlds, compare the game's own terrain LOD of a child world whose PNAM has "use land data" clear with lodwright's
output under each inheritance option; for VNML, the row order can be read from a LAND record with a known slope
direction (normal x or y component sign by row).

## 2026-11-19: odd sizes in a mip chain (issue 60)

Specifier entry, role held: specifier only (not the implementer of `lodwright-dds`). Read the Microsoft Learn page
"Introduction To Textures in Direct3D 11" in the MicrosoftDocs/win32 repository (CC-BY-4.0) at revision
`bc82a65f74ef2c4f2f44a3d7db5f7cfebfdc3583`. No generator source, decompiled output or GPL code was read.

- `dds-mip-odd-size` (new): the page's one odd-size example, 5 by 1 to 2 by 1 to 1 by 1, which is rounding down. The
  page is about Direct3D 11 resources, not the DDS file format, and its general sentence ("the next lower level is
  always even size") does not hold for rounding down in general, so only the example is recorded.

`dds-mip-chain` is unchanged and stays `verified = false`. `crates/lodwright-dds/src/resize.rs` (`mip_size`,
`mip_count`) now says in its comments that rounding an odd size down is the project's choice and cites both rows; no
behaviour changed. What a second reader checks: the example and its numbers on that page, that the fact claims nothing
beyond the example, and whether a Direct3D 11 resource page may stand for the DDS files the game reads (or whether an
`oracle:` measurement is needed). What settles it on the game's side (owner, numbers only): `lodwright probe dds` on any
own texture with an odd dimension, reporting width, height and mip count.

## 2026-11-19: block-compression precision and channel scaling of the probe decoder (issue 62)

Specifier entry, role held: specifier only (not the implementer of `lodwright probe`). Read the Direct3D 11.3
functional specification (`d3d/archive/D3D11_3_FunctionalSpec.htm` in microsoft/DirectX-Specs, CC-BY-4.0, revision
`ae2cc81d8f97df9ce8597e25d84405273f09d08c`), sections 3.2.3.5, 3.2.3.6, 19.5.3, 19.5.6 to 19.5.9 and 25.1.1, and the Microsoft Learn pages `d3dformat`
and the DDS programming guide in MicrosoftDocs/win32 (CC-BY-4.0) at revision `bc82a65f74ef2c4f2f44a3d7db5f7cfebfdc3583`. No decoder source code of any
project and no GPL, decompiled or leaked code was read.

- `bc-endpoint-promotion`: 5 and 6 bit endpoints widen by MSB extension (bit replication); BC1 derived colours are
  computed from the widened 8-bit values; error bound 1/255.
- `bc3-alpha-interpolation`: BC3 alpha follows BC4U, computed in float with UNORM16 precision.
- `unorm-conversion`: UNORM to float and float to UNORM (round half up) rules.
- `d3d9-format-memory-order`: channel order of a pixel in memory, least significant bit first.

Comparison with `crates/lodwright-core/src/probe/dds.rs` (read after the rows were written):
- widening (`expand5`, `expand6`) is MSB extension: agrees;
- BC1 four-colour and BC3 alpha integer formulas equal the specification's exact value rounded half up to 8 bits:
  agrees with `bc-endpoint-promotion`, `bc3-alpha-interpolation` and `unorm-conversion`;
- BC1 three-colour mode truncates `(a + b) / 2`; the rounded value is one higher when `a + b` is odd. This is inside
  the specification's 1/255 bound, so the probe stays a conforming decoder, but its statistics can sit one below a
  decoder that rounds. Recorded, not changed (a decoder change belongs to the probe's implementer);
- uncompressed channels: `(value * 255 + max / 2) / max` equals UNORM to float followed by float to UNORM8 (with
  `max` odd there is no exact half), so it agrees with `unorm-conversion`; pixels are assembled least significant
  byte first, which agrees with `d3d9-format-memory-order`. That legacy DDS masked pixels follow the Direct3D 9
  in-memory order rests on the programming guide's mapping table; no DDS page states the byte order in words.

`probe-bc1-block-layout` and `probe-bc3-block-layout` stay `verified = false` and their notes now point at the new
rows. What a second reader checks: each new row against the named sections (including the BC3 section's formula,
which is printed with unpromoted names); the integer-rounding equivalences above; whether the three-colour truncation
should be left (inside the bound) or changed to round half up. The rows say what a conforming decoder may do; they say
nothing about the game, which never decodes in the probe's way.

## 2026-11-19: DRAFT rows for BSA writing: name hash, record order, archive registration (work item C1)

Specifier entry, role held: specifier only; whoever specified these rows must not implement the BSA writer (issue 44)
without a second reader. Draft: the rows are recorded so the owner or a separate reading session can complete them;
they are not facts the writer may rely on until verified.

Read: `Core/wbHash.pas`, `Core/wbBSArchive.pas`, `Core/wbImplementation.pas`, `Core/wbInterface.pas` and
`xEdit/xeMainForm.pas` of TES5Edit/TES5Edit at revision `3ccc9e7d733becfcc97940cf0c877fa3d6213c5b` (MPL-2.0 file headers and repository licence). xEdit is an
allowed input (docs/clean-room.md, input 2). Not read: the 2016 generator C# repository, decompiled binaries, GPL or
leaked code, and no game engine source (owner decision D5 does not apply to these rows).

Could not be read: the UESP pages `Skyrim_Mod:Archive_File_Format` and `Tes4Mod:BSA_File_Format` (HTTP 403 to automated
reads on this date). They are the public format pages owner decision D4 names first; a human reader compares the hash
row with their hash section and records whether they give a test vector.

- `bsa-name-hash`: the v104 hash as xEdit's archive writer computes it. No public test vector was found.
- `bsa-record-order`: folder records by folder hash, file records by file hash, as xEdit writes them. Whether the game
  requires this order is open.
- `bsa-archive-registration`: how xEdit models archive loading for FO3 and FNV (INI list, Update.bsa, archives whose
  name starts with a plugin's name). A model of a tool, not a measurement.

Leads seen in the same file, recorded here and not as rows (they belong to issues 52 and 44): xEdit's writer sets the
stored size of a v104 file to everything it wrote for that file, embedded name and uncompressed-size field included
(comment in `TwbBSArchive.PackData`: "Size includes embedded filename and uncompressed size field"); and its checks warn
that archives of the v104 type (FO3, FNV, Skyrim) or Skyrim SE with the embedded-names flag holding non-texture files may crash the game, and
that cubemap textures need that flag. Issue 52 can use the first as a public source beside its measurement.

What a second reader checks for each row: the functions named in its note at the same revision; that the row states
how xEdit behaves and nothing about the game beyond that. What the owner checks (numbers and names only): the folder
and file hashes of a few records of an own v104 archive against the hash row; that its records ascend by hash; and the
in-game load of a plugin-named archive on the target engine (D2).

Status (2026-11-19): all three rows merge as `verified = false`, marked UNVERIFIED in their notes. They are drafts by design: a human second reader checks them as described above, and nothing implements the writer or the hash from them before that.


## 2026-11-20: specular in alpha, own design (#92)

`docs/spec/textures.md` section 4 states the carrier and value for specular in the terrain texture alpha channel as
lodwright's own design, built from `txst-texture-paths` and `ltex-texture-set`. It records no new format fact and adds
no row. The owner accepted it on 2026-11-20; whether the game reads it is an in-game check.

## 2026-11-20: known differences of object and tree LOD (#159)

`docs/spec/object-lod.md` section 4 records the order of trees within a block (agrees only as a set) and the segment table
of object blocks (meaning unknown) as known differences, with numbers already recorded in the rows above. No new format
fact, no row. Neither is visible to `verify`, so neither can be on the known-differences list.

## 2026-11-20: owner measurements run on the Linux machine (#52, #148, #152, #155, #93, #125)

Measurements on the local game files, read in place, numbers only; scratch tools outside every repository. Role: specifier
(measurements); nothing here was implemented from them, and no row is flipped to verified.

- `bsa-v104-data-blocks` (#52): the M10 protocol on the three archives with flag 0x100 (New Vegas texture archives 1 and
  2, Fallout 3 texture archive; flags 0x107). All 34355 files are compressed, and for every one the stored size counts the
  embedded path (the distance to the next block and the bytes the zlib stream uses both agree). No archive with the flag
  and stored files exists on the machine, so that case is unmeasured. Row note updated; `verified` stays false (no second
  reader).
- `game-lod-water-shape`, `game-lod-water-height` (#148): the per-level water table of `verify --reference` for the main
  world (lodwright 0.6.0), 164 blocks with water, 82 under an IoU of 0.95, lowest 0.000, mean 0.829; level 4: 98 blocks,
  39 under, mean 0.874; level 8: 39, 19, 0.831; level 16: 19, 16, 0.650; level 32: 8, 8, 0.688. Recorded on the issue;
  the rows are not changed, the outline vertex counts, slope depth and inset of the ticket text are not in verify's output.
- #152: not decidable from file times (every file carries the install stamp); the record edit stamps put two of the four
  shifted blocks at the newest visible-when-distant placements of the worldspace and the other two at ranks 32 and 43 of
  289. Not confirmed for any block; no known-differences entry added.
- `wrld-bounds` (#155): stored NAM0 and NAM9 are world units, multiples of 4096, and contain the LAND cell range in all 30
  worldspaces of the master and four DLC plugins. `plugin-record-override`: 7 of 42467 LAND records are overridden, all
  with the same subrecord set as the master, so whole replacement and merging do not differ on stock data.
  `land-vtxt-layout`: 76046 layers, 75854 sparse (minimum 1 position), no listed entry with opacity 0.0.
  `wrld-child-inherits`: 17 child worldspaces, all with only the map-data bit set, all with LAND of their own, all with
  LOD in the game's own tree. The four rows stay `verified = false`.
- #93: partial. Loose files at archived paths were opened by nv-rs in the 0.3.0 release test (INI default
  bInvalidateOlderFiles=1); drawn content, the INI and timestamp variables and the Proton case are not measured.
- `game-lod-block-borders` (#125): new row, `verified = false`. Every border vertex of the game's terrain blocks has a skirt
  vertex at the constant height -14098 (175259 of 175262 at level 4, with the other levels equally complete); neighbours
  share border heights where both have a vertex, not the vertex sets (same set in 43 of 2600 same-level pairs).
- `game-lod-block-grid` (#219): new row, `verified = false`. The block grid of a world in the game's own tree starts at the cell of
  the world's minimum corner (NAM0 / 4096), the same for every level; level 4 block coordinates modulo 4 against the corner
  cell in five worlds of the #147 run (corner (-6, -5) gives (2, 3), and so on). Not second-read.
- `game-lod-block-grid` (#219, second step): object blocks, tree blocks and the settings file's south-west cell start at the
  same cell as the terrain blocks in all 26 worlds with a reference; WastelandNV needs the exception recorded in the row
  (corner cell -65, game -64). Still not second-read.
- `game-lod-object-block-nif` (#294): corrected. The block origin is the translation of the BSSegmentedTriShape; the BSMultiBoundNode root has
  translation zero (the earlier text had it the other way round). Measured on the game's own object NIFs (numbers only), and in the engine:
  object blocks with the origin on the root are not drawn, with it on the shape they are.
- `game-lod-block-borders` (#316): effect measured in the engine. Terrain blocks without skirts leave cracks between blocks of different levels through which the background shows (thin pale slivers on mountain ridges, seen from 14000 units up over the Strip; they vanish with the skirts on and with the game's own level 32 blocks in place). Skirts now on by default. Row stays `verified = false`.
