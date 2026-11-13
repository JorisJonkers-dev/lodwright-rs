# Design

Source study: `docs/research/porting/xlodgen-licence-and-design.md` in the NV-Linux-Modding repository
(licence reading, clean-room process, risks). Summary of the build plan follows.

## Pipeline
1. Plugin and BSA v104 readers: records, master FormID remapping, load-order overrides.
2. Worldspace model: WRLD bounds, cell grid, LAND heightfield assembly (VHGT, 33x33 per cell), water height.
3. Terrain meshes per level 4/8/16/32: quality-driven simplification, protect borders, optimize unseen,
   16-bit index cap (max vertices 32767), `.btr` writer.
4. Terrain textures: LTEX layer blending (BTXT/ATXT/VTXT), normals, optional specular alpha, mips, BC1/BC3/BC5.
5. Object and tree LOD: select refs with LOD, merge LOD models per block, drop triangles under terrain/water,
   bin-packed atlas with UV remap, `.bto` writer.
6. `.lod` settings writer, per-worldspace output folders, optional BSA packing.

## Dependencies
- NIF subset reader/writer (FO3/FNV, NIF 20.2.0.7), purpose-built; no mature Rust crate is known.
- DDS: `texpresso` (MIT) or `intel_tex_2` (MIT/Apache-2.0) for BC1/BC3/BC5, `ddsfile` (MIT) for containers,
  `image_dds` (MIT) as a convenience layer. `binrw` (MIT) for layouts. Do not depend on `esplugin` (GPL-3.0).

## First slice
LAND heightfield reader plus `.lod` writer plus a block-aligned height raster for one worldspace. Verified by
synthetic VHGT round trip in CI, and on the owner's machine by comparing vertex heights of an existing `.btr`
and the `.lod` file against the output (all real data stays local). Then the `.btr` reader and writer. An
object-LOD bounds and atlas planner (report only, no NIF writer) is the alternative first slice.

## Estimate
About 3-4 weeks to a terrain-only generator, 4-6 months to full replacement, one person.

## Open format questions (resolve from allowed inputs only)
`.btr`/`.bto` container layout (believed NIF), tree LOD placement for FNV, exact `.lod` fields.
