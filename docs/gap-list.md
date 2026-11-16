# Gap list: what full LOD generation needs and what exists

State on 2026-11-16, after the measurements in `provenance/LOG.md` (entry of that date). "Needs" is the whole
pipeline the community generators provide for this game. Each row is one work item; the order is the build order.

| # | Part | State | What is missing |
| --- | --- | --- | --- |
| 1 | Plugin reading, load order, master order | Done (own reader, header checks through the `esplugin` crate) | Compressed `CELL` coverage on real files is checked by the real-data run |
| 2 | Archives, loose-over-archive lookup | Done (`lodwright-assets`), wired into object LOD | Textures for the terrain diffuse |
| 3 | Worldspace assembly, height raster | Done | Parent-world inheritance is decoded; its use in LOD is untested on real data |
| 4 | Terrain mesh planning | Done: adaptive (RTIN) meshes, fixed edge vertices, vertex cap, no triangles over missing ground | Hiding unseen faces, water offset, per-block quality tuning against the game's own blocks |
| 5 | Terrain block NIF | Done (`lodwright-nif`), structure matches the game's blocks | In-game load check (owner) |
| 6 | Terrain textures | Done: normal maps correlate 0.95 to 0.97 with the game's own, diffuse (land texture layers, opacity points, vertex colour) correlates 0.85 to 0.95 | Per-texture tiling scale, specular alpha option |
| 7 | Settings file | Done; byte-identical to the game's own file for the main world | Check against the game's loading |
| 8 | Output layout | Done, matches the game's own tree; files appear atomically | Manifest, resume |
| 9 | Object LOD | Done: selection, merge, atlas and block NIFs (`lodwright-gen`); 287 of the game's 295 blocks have the same vertex and triangle counts; with the shape offset rule a median 94 percent of a block's vertices coincide in position within 0.05 units (lowest block 81 percent) | The remaining 6 percent of vertices and eight blocks, the segment table meaning, in-game check (owner) |
| 10 | Tree and billboard LOD | Done (`lodwright-gen`, `lodwright-core::trees`): list file, block files and atlas from the billboard textures mods ship; types, positions, scales and form ids agree with three sets of shipped tree LOD (17,404 of 17,419 shared trees with the same type); `verify` checks the files | Order of trees inside a block (agrees as sets), the rotation is derived from the form id (the original draws it at random), in-game check (owner) |
| 11 | Inclusion rules | Done for object LOD: exclude or keep by plugin, exclude by record or model glob, from the request document | Per plugin rules for terrain cells |
| 12 | Determinism, incremental runs | Deterministic output for any thread count; manifest with digests; an unchanged request and input is detected and skipped, a changed one replaces exactly the old manifest's files | Reuse of unchanged blocks inside a run |
| 13 | Performance | Parallel terrain blocks, bounded worker count; the main worldspace takes about 9 seconds and 0.5 GiB with four workers | Memory cap option, streaming of large plugins |
| 14 | CLI | `plan`, `heightmap`, `probe`, `generate`, `verify`, `schema` | Nothing known |
| 17 | Water planes in terrain blocks | Structure measured (`game-lod-water-shape`): 164 of 1360 blocks carry one or two extra shapes | Outline and height rules; not written yet |
| 15 | Integration for the mod manager | Plan, result, request and manifest contracts; `lodwright-gen` library; stable exit codes; progress events (`--events FILE|-`, one line per terrain block) | Events for object LOD |
| 16 | In-game acceptance | Owner only | Loading generated blocks in the game |

What verification means here: tests build synthetic plugins and archives; real-data checks read the owner's install
read-only, write to a scratch folder, and report numbers only.
