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

### Where the grid starts

The block grid of a world does not start at cell 0, 0: it starts at the smallest grid coordinate of the world's `CELL`
records, x and y separately, with or without LAND, and repeats every `<L>` cells, the same start for every level
(`game-lod-block-grid`, `game-lod-block-grid-cell-records`, measured in #147 and #229). So a world whose smallest cell is
(-6, -5) has level 4 blocks named at x of 2 modulo 4 and y of 3 modulo 4. Where that cell is a multiple of the level, the
names are the old ones. Object blocks, tree blocks and the south-west cell of the settings file start at the same cell.
One exception is needed for the main world, whose smallest `CELL` is -65 where the game starts at -64: when the first
cell with LAND (after dropping outlying edges) lies on the 32 grid exactly one cell above the start, it is the start. A
world with no `CELL` record starts at the cell of its minimum corner `NAM0` (4096 world units to a cell, rounded down). A world without corners,
or with corners that are not finite or lie beyond a million cells, starts at 0, 0. The start is part of the digest of a
terrain block (when it is not a multiple of the level) and each block's manifest key names its cell, so a run with a
different start rebuilds the block.

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
- Water LOD and border handling: not documented in a public source read here; see section 6 for what was measured.
- Border scheme (own design, from the measurement `game-lod-block-borders`): the game's blocks neither share border
  vertices nor leave the border open; every border vertex has a second vertex at the constant height -14098 (a vertical
  skirt). lodwright's option `terrain.levels[].skirts` writes exactly that and is off by default (it adds up to one
  vertex per border vertex, inside the level's vertex cap). Shared edge vertices were not chosen: the game's two sides are
  simplified separately, so coupling the blocks would change digest semantics for no match with the reference.

## 6. Water planes

Measured on the game's own blocks of the main world (`game-lod-water-shape`, `game-lod-water-height`): a block with
ground below water carries a second shape after the terrain, without properties, whose data has positions only
(no UVs, normals or colours). It is a `BSSegmentedTriShape` at level 4, with one segment per cell in the order
x * 4 + y, and a plain `NiTriShape` at the higher levels. Its translation repeats the block origin. The heights are
the cells' water heights; an `XCLW` of the largest float means the world's default water height.

The outline the game uses is not established: its planes have sloped edges down to about -14,100 units and inset
outlines. lodwright writes its own design (`crates/lodwright-gen/src/water.rs`): flat rectangles at each cell's
water height over the squares (128 units at level 4, doubling per level) whose mean ground height lies below it.
Against the game's planes, sampled every 128 units at level 4 (every 256, 512 and 1024 units above), the covered
ground overlaps by these intersection-over-union shares, with the same height wherever both have water:

| level | blocks with water (game, ours) | overlap |
| --- | --- | --- |
| 4 | 97, 97 | 0.986 |
| 8 | 39, 38 | 0.978 |
| 16 | 18, 18 | 0.913 |
| 32 | 8, 8 | 0.905 |

`verify --reference` counts a plane as equal when it covers half to twice the reference's area. Outside that band in
the main world: 16 small ponds where our plane covers 2 to 5 times the game's inset outline, three blocks where the
game draws water from a neighbouring cell's height and we draw none, and one block where the game has none. 25
blocks differ in block types because the game adds a second, empty segmented shape of one vertex.
`--no-water` (request `terrain.water: false`) leaves the planes out.

## 7. Faces under the water (own design)

The option `terrain.drop_under_water` (off by default) leaves out terrain faces the water plane hides, so a coast block
spends no triangles on the sea floor. Rule: a triangle goes when its three corners all lie at least `offset` world units
below the water height of the cell its centre is in; the offset is the owner's margin against the water plane's slope at
the edge. A heightfield mesh has no other unseen faces (no face points down and none is covered by another), so "unseen
faces" are exactly these. Not measured against the game: its own blocks keep faces below the water; this option is an
addition for fewer triangles, and the in-game check is the owner's (release test R2).
