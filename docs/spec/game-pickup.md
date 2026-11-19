# How the game picks LOD up

## INI

`[TerrainManager]` in Fallout.ini and FalloutPrefs.ini controls whether and how far blocks load
(`fnv-ini-terrainmanager`): `bUseNewTerrainSystem`, `bUseDistantObjectBlocks`, `bUseDistantTrees`,
`bKeepLowDetailTerrain`, the three `f*BlockLoadDistanceLow` values, `fBlockLoadDistance`, `fBlockLoadDistanceLow` and
the tree distances; `[LOD]` has `bDisplayLODLand`. A guide raises `fBlockLoadDistanceLow` and `fBlockLoadDistance`
for more LOD distance (`fnv-lod-levels` guide, same page as `fnv-archive-loading`). No setting that selects a LOD
folder or a settings file is documented; location is by convention (paths in terrain-lod.md and lod-settings.md).
The generator needs no INI.

## Archives and loose files

From `fnv-archive-loading`: loose files override archives; an archive loads with the same-named plugin; archives of
one plugin load alphanumerically; `[Archive]` has `bInvalidateOlderFiles`, `bUseArchives`, `SInvalidationFile` and
`SArchiveList`; replacing files of earlier archives needs the JIP LN plugin and an empty `.override` file next to
the archive.

Implication for the tool: write loose files into an output folder the user installs last (public guidance says
generated LOD must load after texture mods). Packing into a BSA is optional; terrain archives are uncompressed and
object texture archives compressed per the same guide. BSA writing is out of scope until its format is specified
from a public document.

Unknown: lookup order between a loose `.dlodsettings` and the BSA copy, and between loose and archived block meshes and
textures (M9; the full protocol is in [measurements.md](measurements.md#m9-in-full-loose-lod-against-archived-lod-93)).
