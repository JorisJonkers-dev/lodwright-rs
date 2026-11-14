# Terrain textures, normal maps and the object atlas

## 1. Folder and names

```
textures\landscape\lod\<world>\blocks\<world>.level<L>.x<X>.y<Y>.dds        diffuse (name inferred)
textures\landscape\lod\<world>\blocks\<world>.n.level<L>.x<X>.y<Y>.dds      normal map
```

Folder: `fnv-lod-folders`. Normal-map name: `fnv-lod-terrain-names`, from a single search excerpt of a generator
log (`wastelandnv.n.level4.x-40.y-64.dds`); the diffuse name without `n` is inferred from it, not seen. The earlier
guess of an `_n.dds` suffix (`lod-terrain-paths`) has no support in any public source read here. Measurement M5
confirms both names from the owner's tree before the writer relies on them.

## 2. Image properties

Public text gives only generator settings, not what the game requires: width and height 512 for level 4 and 256 for
levels 8, 16 and 32 by default, larger for retextured terrain; mipmaps optional; normal maps optional, with an
option that bakes the terrain's normal map into them (`fnv-lod-texture-options`). Unknown from public sources: the
DDS pixel format (DXT1/DXT5/BC5/uncompressed), whether mip chains are present in shipped files, the normal-map
channel convention (which channel is Y, whether alpha holds specular). The DDS container layout itself is
documented (`dds-header-layout`, `dds-pixel-format`). M5 lists what to measure with a header dump of owner files.

## 3. Object atlas

One sighting of `textures\landscape\lod\WastelandNV\Blocks\WastelandNV.Buildings.dds` (`fnv-lod-atlas-name`). Maximum
atlas size and per-texture limit are generator options (`fnv-lod-texture-options`). The UV remap of object meshes
onto the atlas lives in the NIF content, which is not publicly documented (M6).
