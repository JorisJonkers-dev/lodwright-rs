# Design

Source study: `docs/research/porting/the licence-and-design study` in the NV-Linux-Modding repository (licence
reading, clean-room process, risks). The process is [clean-room.md](clean-room.md); the test strategy is
[TESTING.md](TESTING.md). This file is the plan and the interfaces. The testkit builds the synthetic inputs (plugins with groups and compression, worldspaces, VHGT, DDS headers).

## Pipeline

1. Plugin and BSA v104 readers: records, master FormID remapping, load-order overrides.
2. Worldspace model: WRLD bounds, cell grid, LAND heightfield assembly (VHGT, 33x33 per cell), water height.
3. Terrain meshes per level 4/8/16/32: quality-driven simplification, protect borders, optimize unseen,
   16-bit index cap (max vertices 32767), `.btr` writer.
4. Terrain textures: LTEX layer blending (BTXT/ATXT/VTXT), normals, optional specular alpha, mips, BC1/BC3/BC5.
5. Object and tree LOD: select refs with LOD, merge LOD models per block, drop triangles under terrain/water,
   bin-packed atlas with UV remap, `.bto` writer.
6. `.lod` settings writer, per-worldspace output folders, optional BSA packing.

## Crates

| Crate | Role |
| --- | --- |
| `lodwright-core` | Library: one module per contract below. No I/O beyond what a module's reader or writer takes as `Read`/`Write` or bytes |
| `lodwright-cli` | Thin binary `lodwright`: `--version`, `--help`, `doctor <data-folder>`; later `generate` and `verify` (see TESTING.md) |
| `lodwright-testkit` | Synthetic builders and reference readers for tests and fuzz seeds; the only allowed test data |

## What exists

| Module (`lodwright-core`) | State |
| --- | --- |
| `plugin` | `read(&[u8]) -> Result<Plugin, Error>`: TES4 header, masters, group tree, compressed records, extended subrecords; `WRLD`, `CELL` (grid) and `LAND` (VHGT) decoded, everything else skipped |
| `land` | `decode_vhgt`: 33 x 33 heights in world units from the offset and signed deltas |
| `worldspace` | `Worldspace::from_load_order`: FormIDs resolved through master lists, later plugins override cells, bounds, seam error |
| `raster` | `BlockGrid` (cell to block mapping) and `HeightRaster` (whole blocks, shared edge vertices, no-data, cap on size, per-block crop) |
| `terrain` | `plan_world`: blocks per level, uniform-step decimation under a tolerance and the 16-bit vertex cap, texture sizes, paths |
| `layout` | output paths: meshes, textures, normal maps, `.lod`; world names checked |
| `lod`, `dds` | contracts only (the `.lod` fields and the DDS writer wait for facts and for the owner's checks) |

CLI: `lodwright doctor | plan | heightmap` (see `--help`). `verify` is designed in TESTING.md and not built.

The terrain mesh is our own design: a regular grid with one decimation step per block, the coarsest step whose
largest vertical error against the raster stays within `quality * 2` world units, with 16-bit indices, no sample
with data dropped and no quad bridging a hole. It is not a copy of another tool's simplifier, and its quality number
does not map onto another tool's. Border protection between blocks of different steps, hiding unseen faces and
water handling are not designed yet.

## Dependencies

- NIF subset reader/writer (FO3/FNV, NIF 20.2.0.7), purpose-built; no mature Rust crate is known.
- DDS: `texpresso` (MIT) or `intel_tex_2` (MIT/Apache-2.0) for BC1/BC3/BC5, `ddsfile` (MIT) for containers,
  `image_dds` (MIT) as a convenience layer. `binrw` (MIT) for layouts. Do not depend on `esplugin` (GPL-3.0; banned
  in `deny.toml`).
- Every new dependency must pass `cargo deny` (permissive or MPL-2.0 licences only) and be at least seven days old
  on the registry.

## Estimate

About 3-4 weeks to a terrain-only generator, 4-6 months to full replacement, one person.

## Open format questions

Resolve from allowed inputs only; each answer becomes a provenance row.

- `.btr`/`.bto` container layout (believed NIF). Blocks the mesh and object writers.
- Tree LOD placement for FNV.
- Exact `.lod` fields. The block sizes per level (4, 8, 16, 32 cells) are used as planning parameters, not as facts.
- Whether the file names recorded in `lod-block-naming`, `lod-terrain-paths` and `lod-settings-path` match the owner's
  own output tree (the normal-map suffix is the least certain).
- Whether real plugins differ from the recorded plugin rows in any detail the tests cover (for example compressed
  CELL records, or LAND records in groups the walker does not expect).
- How real seams between cells behave (`max_seam_error` on the owner's data answers it).
