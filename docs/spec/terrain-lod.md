# Terrain LOD (blocks, names, paths, levels)

Status: names and folders from public sources, mesh contents unknown. Row ids in backticks.

## 1. Container

A terrain LOD block mesh is a NIF file with the extension `.nif` (`fnv-lod-block-container-nif`). It is not a
`.btr`. The NIF version, block types and vertex format are **not documented publicly** for these games; the design's
"NIF 20.2.0.7" is an assumption to confirm (measurements.md M1).

## 2. Levels and span

Terrain LOD exists at levels 4, 8, 16 and 32, all produced in one generator run (`fnv-lod-levels`). Reading the level as
the block's side in cells (4 x 4 cells for level 4) is consistent with every coordinate seen in public examples
(`x-20 y-28`, `x-40 y-64` at level 4, `X32 Y32` at level 32), which are all multiples of the level, but no public page
states it: the span is **unconfirmed** (M2). A cell is 4096 units (`cell-size-units`), so the page that says a cell is
found by dividing a position by 4096 and rounding down agrees with the cell grid.

## 3. Names

```
<world>.level<L>.x<X>.y<Y>.nif
```

`<L>` is 4, 8, 16 or 32; `<X>` and `<Y>` are signed decimal integers, a minus sign directly after the letter, no
zero padding in the examples (`fnv-lod-terrain-names`, `fnv-lod-object-block-names`). Case: the vanilla example is
lower case; examples from a generator's log are mixed (`WastelandNV.Level32.X32.Y32.nif`). The game's file lookup is
case-insensitive on Windows; on Linux write exactly one case consistently (lower case) and let the owner's tree decide
(M3). Whether `<X>`,`<Y>` name the south-west cell: the settings file is said to hold the world's south-west corner
(`fnv-lod-settings-dlodsettings`), which suggests block coordinates are south-west cells; unconfirmed (M2).

## 4. Paths

```
meshes\landscape\lod\<world>\blocks\<name>.nif
```

(`fnv-lod-folders`). `<world>` is the worldspace editor id; the vanilla example uses lower case.

## 5. Open points that block the writer

- Mesh content of one block: vertices per block, whether neighbouring blocks share edge vertices, normals, UVs,
  vertex colours, number of shapes, shader/texture properties naming the block's textures. No public document. M1.
- Whether terrain and object blocks of level 4 share one name and one file (see object-lod.md). M4.
- Vertex cap (design.md uses 32767 for 16-bit indices): a NIF constraint of the engine's loader is not documented
  publicly; treat 65535 as the format limit and 32767 as the plan's choice until measured.
- Water LOD and border handling: not documented in a public source read here.
