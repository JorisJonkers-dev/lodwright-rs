---
status: accepted
date: 2026-11-20
---
# Object and tree blocks are rebuilt in full (#150, option 3)

**Accepted.** The three options of #150 were weighed; none of the reuse options is built.

## Options

1. A digest per block plus a stable atlas slot. A slot stays stable only if the packer assigns it independently of the
   other textures. The packer (`crates/lodwright-gen/src/atlas.rs`) picks the smallest atlas size that fits every texture
   of the world and shelf-packs by height and width, so one more texture can change the atlas size and the position of
   every other texture. A stable packing would need a different packer (fixed slot grid, or a slot table kept between
   runs) and a fixed atlas size per world, which wastes area the game's own atlas does not waste (its textures fill 97
   percent of it). Not worth the area and the new manifest state.
2. An atlas digest and a whole-world rebuild when it changes. Blocks would be reused only when the atlas is
   byte-identical, which is exactly the case where nothing the object blocks depend on changed; the run is then
   `up_to_date` already when the request and the plugin contents are the same, and a changed plugin that leaves the atlas
   alone is rare among the changes that matter. It needs object and tree blocks in the manifest (it lists terrain blocks
   only) for little gain.
3. No reuse. Chosen.

## Reason in numbers

The full run over Fallout New Vegas and its DLCs (30 worlds, #147) takes 55 to 67 seconds of wall time, of which
WastelandNV alone is about 30; terrain blocks (which are reused) dominate it. The time object and tree LOD would save on
a second run is a few seconds.

## Consequence

Object and tree LOD are rebuilt in full whenever a run does not end as `up_to_date` (`docs/contracts.md`, Request and
manifest); the manifest records no object or tree block digests. Output bytes do not depend on the thread count or run
order. If a measured case shows object LOD dominating a second run, this decision is revisited as option 2.
