<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Far-land tiles the game ships

`terrain.compose_shipped_tiles` (a switch, off by default, written only when on; tier 3, since 0.16.0) composes every
diffuse tile from the land textures. Off, a diffuse tile is left out of the output (the mesh and the normal map are
written as always) and the game's own tile of that name is the one the engine draws, when all of this holds:

- an archive of the game (an archive that does not belong to a plugin outside the game's own masters, never a loose
  file and never a mod's archive) holds a tile of that name, and the game's `lodsettings` file for the world starts the
  block grid a whole number of 32 cell blocks from ours, so the tile covers the same cells;
- no plugin outside the game's own masters changed the land under it. The game's own masters are Fallout: New Vegas and
  its add-ons, Fallout 3 and its add-ons and the two plugins of the Tale of Two Wastelands package (`FalloutNV.esm`,
  `DeadMoney.esm`, `HonestHearts.esm`, `OldWorldBlues.esm`, `LonesomeRoad.esm`, `GunRunnersArsenal.esm`,
  `CaravanPack.esm`, `ClassicPack.esm`, `MercenaryPack.esm`, `TribalPack.esm`, `Fallout3.esm`, `Anchorage.esm`,
  `ThePitt.esm`, `BrokenSteel.esm`, `PointLookout.esm`, `Zeta.esm`, `TaleOfTwoWastelands.esm`, `YUPTTW.esm`). The load
  order is compared with the same load order in which every other plugin is an empty shell: a cell counts as changed
  when its `LAND` record exists on one side only, or its heights, vertex colours or texture layers differ, or a land
  texture its layers name resolves to another file. A tile with a changed cell is written (4, 8, 16 and 32 cells a
  side at the four levels): the game's own picture with the composed one written into the squares of the changed cells
  only. Where composed and shipped pictures disagree over the tile's unchanged cells, each changed cell's composed
  pixels are first brought to the mean and spread per channel of the shipped pixels of that cell and the patch is
  feathered two pixels inside its edge. A tile whose every cell changed, or that the game does not ship, is composed. A cell also counts as changed when a land texture
  it uses is a file that does not come from a game archive: each texture is resolved through the load order's loose
  files and archives as the game does, and a loose file or a file from an archive that is not the game's (a name
  that starts with a game plugin's name or `Fallout - `) wins over the game's.

The game ships tiles at all four levels in every world measured (Fallout 3's `Wasteland` and the Mojave's
`WastelandNV` have 1024, 256, 64 and 16 tiles at levels 4, 8, 16 and 32), and the Capital Wasteland's are baked
pictures of the finished land with objects, rubble and shade painted in (mean luminance about 62 and 11 of spread at
every level against about 80 and 14 for a tile composed from the land textures), which a composition from land
textures does not reproduce. The result document's `tiles` object and its `reason` count the tiles left to the game
and the tiles composed. The shipped diffuse tiles of a block enter that block's digest as one line only; `verify`
accepts a diffuse tile that the manifest does not name.
