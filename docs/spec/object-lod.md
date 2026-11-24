# Object LOD and tree LOD

## 1. Object LOD meshes

Object LOD is one level (`fnv-lod-levels`), stored as `.nif` block files (not `.bto`), named
`<world>.level4.x<X>.y<Y>.nif` in `meshes\landscape\lod\<world>\blocks\`; objects flagged high priority also go to
`<world>.level4.high.x<X>.y<Y>.nif` (`fnv-lod-object-block-names`, `fnv-lod-folders`). High priority is bit 16 of the reference or of the
static it places (`oracle-high-priority-on-static`); the game keeps the high file drawn out to `fBlockLoadDistance`,
the normal one only to `fBlockLoadDistanceLow`, so a large object that should be seen from very far needs the flag. The standard file contains the
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

Measured on the game's own blocks: a reference turns its model by `game-object-rotation`; a vertex lands at
`pos + R * (s * v + T)`, where the model's translations `T` are not scaled by the reference scale `s` and the rotations
of the model's nodes are ignored (`game-object-shape-offset`). A temporary reference counts in the block of the cell that holds it, a persistent one in
the block of the cell under its position (`game-object-block-cell`).

Remaining differences from the game's own blocks of the main world (#77), all counts equal: four blocks each hold one
object whose vertices differ as a whole: one shifted by 0.09 units in height (160 vertices), one shifted by about 23
units (137 vertices), and two turned and moved (94 and 16 vertices). The records place these objects differently from
the game's blocks, which most likely were built before the placements last changed; lodwright follows the records.

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

## 4. Known differences from the game's own files

Both entries are numbers and words only. Neither is a rule lodwright follows from the game's files.

### Tree order within a block

Measured on three sets of tree LOD files that mods ship (`game-tree-lod-dtl`, `game-tree-lod-lst`): of 17,419 trees that
a set and the generated output share, 17,404 have the same type, position, scale and form id; the files agree as sets of
trees. The order of the trees inside a block file does not agree. lodwright writes load and record order; the order of the
shipped files was not derived, and no rule for it is recorded. `verify` compares trees as a set and does not report the
order, so no entry exists on the known-differences list of `docs/contracts.md` (Verify): there is no differing path to
list. Whether the order matters to the game is an in-game check for the owner. Related, already recorded above: the
rotation is derived from the form id, where the original draws it at random.

### Segment table of object blocks

Object blocks carry a segment table (`game-lod-object-block-nif`). Measured: its presence and that the counts of the
blocks compared agree (all 295 of the main world's blocks have equal vertex and triangle counts). Not measured: what a
segment stands for, what the game reads it for, and whether any value other than the one lodwright writes changes how the
block draws. **Unknown.** No meaning is assumed and none is to be guessed from other formats: a guess is a clean-room
risk. `verify` does not compare the table's contents, so it too has no list entry. A measurement that varies it in the
game is an owner step; until then the segment table is a known unknown of the object LOD (gap list row 9).

