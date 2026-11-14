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

Files: `meshes\landscape\lod\<world>\trees\<world>.*.*.*.dtl` (block data, similar in spirit to Skyrim's `.btt`),
`trees\treetypes.lst`, atlases `textures\landscape\trees\treedeadlod.dds` (all worlds) and `treeswamplod.dds` (world
editor id starting with `DLC4`) (`fnv-tree-lod-files`). The `.dtl` and `.lst` byte layouts are not public: M7.
