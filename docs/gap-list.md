# Gap list: what full LOD generation needs and what exists

State on 2026-11-16, after the measurements in `provenance/LOG.md` (entry of that date). "Needs" is the whole
pipeline the community generators provide for this game. Each row is one work item; the order is the build order.

| # | Part | State | What is missing |
| --- | --- | --- | --- |
| 1 | Plugin reading, load order, master order | Done (own reader, header checks through the `esplugin` crate) | Compressed `CELL` coverage on real files is checked by the real-data run |
| 2 | Archives, loose-over-archive lookup | Done (`lodwright-assets`) | Wiring into the generator for textures and models |
| 3 | Worldspace assembly, height raster | Done | Parent-world inheritance is decoded; its use in LOD is untested on real data |
| 4 | Terrain mesh planning | Done: adaptive (RTIN) meshes, fixed edge vertices, vertex cap, no triangles over missing ground | Hiding unseen faces, water offset, per-block quality tuning against the game's own blocks |
| 5 | Terrain block NIF | Done (`lodwright-nif`), structure matches the game's blocks | In-game load check (owner) |
| 6 | Terrain textures | Normal maps done (correlate 0.95 with the game's own); diffuse is vertex colours only | Layer blending of land textures into the diffuse |
| 7 | Settings file | Done; byte-identical to the game's own file for the main world | Check against the game's loading |
| 8 | Output layout | Done, matches the game's own tree; files appear atomically | Manifest, resume |
| 9 | Object LOD | Block structure measured (`game-lod-object-block-nif`), nothing built | Selecting references (base record flag, high-priority flag), decoding `STAT`, `SCOL`, `REFR`, loading small LOD models from archives, merging per block, removing faces under terrain, atlas packing with UV remap, writing the block NIFs and the atlas DDS pair |
| 10 | Tree and billboard LOD | Not measured | `.dtl` and `treetypes.lst` layouts, tree record decoding, the fixed atlas |
| 11 | Inclusion rules | Not built | Per plugin and per record include and exclude lists, reading them from a request file |
| 12 | Determinism, incremental runs | Terrain ready to build on | Stable ordering everywhere, content hashes per block, a manifest and reuse of unchanged blocks |
| 13 | Performance | Parallel terrain blocks, bounded worker count; the main worldspace takes about 9 seconds and 0.5 GiB with four workers | Memory cap option, streaming of large plugins |
| 14 | CLI | `plan`, `heightmap`, `probe`, `generate` (terrain) | `verify`, a request document |
| 15 | Integration for the mod manager | Contracts exist (plan, result) | A request document, progress events, stable exit codes, a library entry point that does not print |
| 16 | In-game acceptance | Owner only | Loading generated blocks in the game |

What verification means here: tests build synthetic plugins and archives; real-data checks read the owner's install
read-only, write to a scratch folder, and report numbers only.
