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

Levels 8 and 16 were not measured. The code writes BC1 at every level (`crates/lodwright-gen/src/terrain.rs`), and its
default level settings use 256 at every level (`crates/lodwright-core/src/terrain.rs`); the size is a request setting. The size is the game's
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

## 4. Specular in alpha (proposal, owner decision needed, #92)

Status: a proposal for the owner to accept, change or reject. Nothing in the code follows it, and no row below is
verified. Until it is accepted, #92 stays unstarted.

What is established, and what is not:

- The shipped terrain textures are DXT1 and carry no 8-bit alpha (`game-lod-terrain-textures`, `bc1-block-layout`).
  Whether the game's own LOD textures ever carry specular is not known.
- The editor's texture sets name the normal map as the file that carries specular in its alpha channel (`txst-texture-paths`,
  TX01). That is a statement about texture sets of the land and object records, not about LOD files.
- A land texture record holds one specular exponent (`ltex-texture-set`, SNAM). No per-pixel specular source exists in the
  terrain records.

Proposed own design, declared as such (no format fact is claimed):

1. Carrier: the alpha channel of the normal map, not of the diffuse map. This follows the convention of the texture set
   above, and keeps the diffuse map as it is.
2. Value: per pixel, the specular exponent of the land textures under the pixel, weighted with the same opacities that
   the diffuse blend uses, scaled to 0 to 255 over the range of exponents in the load order. A missing exponent counts
   as the lowest value and a warning is written. A flat source gives a uniform alpha.
3. Format: BC3 (DXT5) for the normal map when the option is on, same size and mip count as the diffuse map. Off, nothing
   changes: BC1, opaque, the output is byte-identical.
4. Open and not decided here: whether the game reads alpha of a LOD normal map as specular at all, and whether it
   accepts BC3 in place of BC1 there. Both are in-game checks for the owner (the Release test of R1, #156). If the
   game ignores it, the option has no use and #92 is dropped.

Questions for the owner: is the normal map the right carrier, is the weighted exponent an acceptable stand-in for a
specular map, and does the option stay at tier 3 (advanced) as #92 says.
