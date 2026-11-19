# Terrain textures, normal maps and the object atlas

The measured rows below (`game-lod-terrain-layout`, `game-lod-terrain-textures`, `game-lod-object-layout`,
`game-object-atlas`) come from the owner's own game files, read by numbers and names only. They are what the code
follows. None of them has been re-derived by a second reader yet (#94), so each is `verified = false`.

## 1. Folder and names

```
textures/landscape/lod/<world>/diffuse/<world>.n.level<L>.x<X>.y<Y>.dds     diffuse
textures/landscape/lod/<world>/normals/<world>.n.level<L>.x<X>.y<Y>.dds     normal map
```

Measured in the game's own archives (`game-lod-terrain-layout`): the diffuse and normal maps carry the same file name,
`.n.` included, and differ only in the folder (`diffuse` or `normals`). World folder and file name are lower case in
the archive listing. The texture set of each terrain block names the same two files, diffuse first, in the world's
editor-id case (`game-lod-terrain-block-nif`).

This replaces the earlier public-source reading: a `blocks` folder for terrain textures (`fnv-lod-folders`) and a
diffuse name without `n` (`fnv-lod-terrain-names`, an inference from one log line). The `_n.dds` suffix guess
(`lod-terrain-paths`) is superseded too.

## 2. Image properties

Measured (`game-lod-terrain-textures`), on blocks at levels 4 and 32:

| Property | Diffuse | Normal map |
| --- | --- | --- |
| Width and height | 256 by 256 | 256 by 256 |
| Pixel format | DXT1 (BC1) | DXT1 (BC1) |
| Mip levels | 9 (256 down to 1) | 9 |
| Channel statistics | not recorded | mean red and green near 128, blue near 245 |

Levels 8 and 16 were not measured; the code uses the same size and format at every level. The size is the game's
shipped size. Generator guidance (`fnv-lod-texture-options`) offers 512 for level 4 and 256 for the other levels by
default, larger for retextured terrain, with mipmaps and normal maps optional and an option that bakes the terrain's
normal map into them: these are what a tool can write, not what the game needs.

Channel convention, as far as the measurement goes:

- Only channel means of the normal map were recorded: red and green near 128, blue near 245. Which channel holds
  which axis, and the sign of each, was not measured. It stays open.
- DXT1 has no 8-bit alpha channel (`bc1-block-layout`; a block holds two 5:6:5 colours, and at most a one-bit
  transparent entry). So the shipped textures carry no 8-bit specular in alpha. Whether any texture carries specular in
  the game's own files is not known.
- A texture written with specular in alpha needs a format with an alpha channel (DXT3 or DXT5, `bc2-block-layout`,
  `bc3-block-layout`). That such a file is accepted and used by the game in place of the DXT1 one is not measured:
  it is an in-game check for the owner.

The DDS container itself is documented (`dds-header-layout`, `dds-pixel-format`). How an odd size is halved in a mip
chain is open (`dds-mip-chain`); the measured terrain textures are powers of two, so it does not arise for them.

## 3. Object atlas

Measured (`game-lod-object-layout`, `game-object-atlas`): the main world's object atlas is
`textures/landscape/lod/<world>/blocks/<world>.buildings.dds`, DXT3, 2048 by 1024 with 12 mips, and its normal atlas
`<world>.buildings_n.dds` is 1024 by 512. The textures of the selected LOD models are packed at their native size.
Maximum atlas size and per-texture limit are generator options (`fnv-lod-texture-options`). The packing rule and the
UV scale stay open (`docs/spec/object-lod.md`).
