# Design

Source study: `docs/research/porting/xlodgen-licence-and-design.md` in the NV-Linux-Modding repository (licence
reading, clean-room process, risks). The process is [clean-room.md](clean-room.md); the test strategy is
[TESTING.md](TESTING.md). This file is the plan and the interfaces. **No format code exists yet**: the testkit has
synthetic stand-ins, the core has contracts.

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
| `xlodgen-core` | Library: one module per contract below. No I/O beyond what a module's reader or writer takes as `Read`/`Write` or bytes |
| `xlodgen-cli` | Thin binary `xlodgen`: `--version`, `--help`, `doctor <data-folder>`; later `generate` and `verify` (see TESTING.md) |
| `xlodgen-testkit` | Synthetic builders and reference readers for tests and fuzz seeds; the only allowed test data |

## First slice: interfaces only

Goal: LAND heightfield reader, `.lod` writer and a block-aligned raster for one worldspace, verified by synthetic
VHGT round trips in CI and, on the owner's machine only, by comparing against an existing `.btr` and `.lod`.
The shapes below are the contract the implementer works to; they change only together with this file.

```text
plugin::read(bytes: &[u8]) -> Result<Plugin, Error>            // TES4 header, masters, records; LAND/WRLD/CELL decoded, rest opaque
land::decode_vhgt(payload: &[u8]) -> Result<CellHeights, Error> // 33 x 33 heights in world units
land::Worldspace::from_plugins(&[Plugin]) -> Worldspace          // cells by (x, y), overrides resolved by load order
raster::BlockGrid::new(block_cells) -> Result<BlockGrid, Error>  // exists today: cell <-> block mapping
raster::HeightRaster::build(&Worldspace, BlockGrid) -> HeightRaster // block-aligned; width and height multiples of the block size
lod::LodSettings::{parse(&[u8]), write(&self) -> Vec<u8>}        // per-worldspace .lod settings, round-trips
```

Properties and golden files for each are listed in TESTING.md. Each function relies on rows in
`provenance/sources.toml`; code carries `// source:` comments pointing at them.

Second slice: `.btr` reader, then writer (feeds `lodcheck`-style mesh checks). An object-LOD bounds and atlas planner
(report only, no NIF writer) is the alternative first slice.

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

- `.btr`/`.bto` container layout (believed NIF).
- Tree LOD placement for FNV.
- Exact `.lod` fields and the block size per LOD level (the raster takes it as a parameter until then).
- Whether the real VHGT and record layouts differ from the testkit's stand-ins in any detail the tests cover.
