<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Where the terrain block grid starts

Not a request option: terrain blocks of every level are placed on a grid that starts at the cell of the world's minimum
corner (`NAM0`) instead of cell 0, 0, as the game's own tree does (`docs/spec/terrain-lod.md`, section 3). Block names
and manifest keys change for worlds whose minimum corner cell is not a multiple of the level (since 0.8.1): a first run
after the update rebuilds and renames those worlds' terrain blocks. Worlds that start on the grid keep their names,
files and digests.
