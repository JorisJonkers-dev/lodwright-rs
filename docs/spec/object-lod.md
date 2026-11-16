# Object LOD and tree LOD

## 1. Object LOD meshes

Object LOD is one level (`fnv-lod-levels`), stored as `.nif` block files (not `.bto`), named
`<world>.level4.x<X>.y<Y>.nif` in `meshes\landscape\lod\<world>\blocks\`; objects flagged high priority also go to
`<world>.level4.high.x<X>.y<Y>.nif` (`fnv-lod-object-block-names`, `fnv-lod-folders`). The standard file contains the
object plus objects of surrounding cells, which means a block covers more than the cell of the object (same row).

Terrain level 4 uses the identical `level4` name (`fnv-lod-terrain-names`). Public text does not say whether these are
one file or whether the object pass writes into the terrain file. **Unknown**; the owner's tree decides (M4). Until
then the implementer must not write both to the same path.

## 2. Inputs the generator collects

Not documented publicly beyond: the per-object LOD models are small `_lod.nif` meshes beside the full models (single
secondary excerpt, not a row), the "visible when distant" flag on the base record and a "high priority" flag on the
reference select what appears (reported by modders in the forum thread of `fnv-lod-object-block-names`). Which
references are selected, how scale and rotation are baked, how SCOL (static collection) records are expanded, how
textures are collected into the atlas: no public document. M6 gives the protocol.

## 3. Tree LOD

Settled from the xEdit source (`xedit-tree-lod-files`, `xedit-tree-lod-selection`, `xedit-tree-lod-atlas`) and measured on
tree LOD files that mods ship (`game-tree-lod-dtl`, `game-tree-lod-lst`).

Files per world: `meshes/landscape/lod/<world>/trees/<world>.level8.x<X>.y<Y>.dtl` (one per block of 8 by 8 cells, aligned to
the south-west cell of the world's settings), `.../trees/treetypes.lst`, and the atlas `textures/landscape/trees/treedeadlod.dds`
(`treeswamplod.dds` for worlds whose editor id starts with `DLC4`).

`treetypes.lst`: a u32 count, then 32-byte entries: index (u32), billboard width, billboard height, atlas left, top, right and
bottom as fractions (f32), and a zero u32. One entry per billboard of the whole load order, so all worlds of a load order share
one list.

`.dtl`: a u32 count of groups; per group the type index (u32), the tree count (u32) and 32-byte trees: x, y, z, rotation (0 to 2 pi),
scale (f32), the reference form id (u32) and two zero u32. Groups come in the order a type first occurs in the block. A block
without trees is four zero bytes; it is still written when a reference stood there, so that it replaces an earlier archive's block.

Which trees: bases (statics, activators, trees) flagged has-tree-LOD in their winning record and having a billboard texture,
which mods ship as `<plugin>/<model>_<form id>.dds` in the tree billboard folder, with an optional text file giving width,
height, shift and scale. References that are disabled, deleted, tied to an enable parent or tilted more than 30 degrees are left out.
A base without a billboard is not a tree here; it is drawn from its `_lod` model as object LOD, if it has one.

Differences from the files measured: the atlas is DXT5 with box-filtered mips; rectangles keep 16 pixels apart; the rotation is
derived from the form id; the order of trees within a group follows load and record order.
