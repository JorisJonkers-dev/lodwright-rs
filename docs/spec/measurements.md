# Measurement protocol for the owner

Everything below runs on your machine, on your own files, under `capped`. Report numbers and names only; never paste or
commit file content. Existing generator output is allowed as black-box input (`oracle:` rows).

`lodwright probe <nif|dds|settings|dtl> <file>` takes M1, M5, M7 and M8 for you: it prints only structure and numbers
(NIF header and block table, per-shape vertex and triangle counts, DDS header and channel statistics, the size and
first 64 bytes as little-endian integers of a settings or `.dtl` file), and refuses a file inside a git working tree.
Add `--json` for one line of JSON. A NIF block whose fields do not add up reports `layout_matches: false`.

Common: pick `WastelandNV` and one small DLC world. Use a hex dump limited to the first 64 bytes unless stated.

- **M1 block NIF.** For three blocks (one per level 4, 16, 32): file size; first line of the file (the NIF header
  text: version string); any NIF viewer's block list (types, counts), vertex and triangle counts, vertex attributes
  present, index width, whether triangles or strips, NiTriShape count, texture paths in the properties, material or
  shader properties. Compare vertex heights to `lodwright heightmap` samples at that level.
- **M2 span and corner.** List all names at level 4 and 32: min and max X and Y, the step between coordinates, and
  whether every coordinate is a multiple of the level. Compare with the cell bounds `lodwright plan` prints.
- **M3 case and separators.** `ls` the blocks folder; report case of folders and names, and a count per level.
- **M4 terrain vs object.** Generate terrain only, list the blocks; then generate objects into a fresh output; compare
  name lists: do level-4 names collide, does a `.high.` file exist, which files appear in which pass.
- **M5 textures.** For one block per level: diffuse and normal file names; DDS header fields (width, height, mip count,
  FourCC or bit masks); the average of each channel of the normal map (a flat map has red and green near 128, blue
  near 255).
- **M6 object content.** For one object block: the shapes, the atlas file it references, UV range per shape; which
  object references in the cell are present (compare with xEdit's reference list for that cell).
- **M7 trees.** `.dtl` and `treetypes.lst` sizes and first 64 bytes of one each; count of entries in the list.
- **M8 settings file.** Size of `wastelandnv.dlodsettings` (from the extracted archive or your own build) and hex of the
  whole file if at most 64 bytes; same for a DLC world and for a file from a different world. Report the integers.
  Safe experiment: copy to a scratch folder, change one value in the copy, never in the data folder.
- **M9 pickup.** With a loose settings file and the archived one both present, report which the game uses (see in
  the game whether LOD appears); same for a loose block mesh over an archived one.

Each answer becomes a provenance row with `oracle:<role>` roles, for example `oracle:block-nif`.
