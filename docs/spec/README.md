# Format specifications

Written by the specifier from public documentation only (see [clean-room.md](../clean-room.md)). Every fact cites a
row id in `provenance/sources.toml`; a statement without an id is marked as unknown. All rows are unverified: nothing
here has been re-read by a second reader or checked against real files yet.

| Document | Covers |
| --- | --- |
| [terrain-lod.md](terrain-lod.md) | Terrain LOD block meshes, names, paths, levels, coordinates |
| [textures.md](textures.md) | Terrain diffuse and normal maps, object atlas |
| [object-lod.md](object-lod.md) | Object LOD meshes and atlas, tree LOD files |
| [lod-settings.md](lod-settings.md) | The per-world settings file |
| [game-pickup.md](game-pickup.md) | INI settings and archive/loose-file order |
| [measurements.md](measurements.md) | What the owner measures to close the gaps |

## Headline findings

1. **There is no `.btr`/`.bto` in these games.** Public listings of New Vegas LOD show `.nif` block meshes;
   `.btr`/`.bto` are Skyrim and Fallout 4 (`fnv-lod-block-container-nif`, `fnv-btr-bto-other-games`). The implementer
   needs a NIF writer for FO3/FNV (version 20.2.0.7 per design.md), not a BTR writer.
2. **Block names are `<world>.level<L>.x<X>.y<Y>.nif`**, not `<World>.<L>.<x>.<y>` (`fnv-lod-terrain-names`,
   `fnv-lod-object-block-names`). The old rows `lod-block-naming` and `lod-terrain-paths` are superseded pending the
   owner's tree.
3. **The settings file is `LODSettings/<world>.dlodsettings`**, not `.lod` (`fnv-lod-settings-dlodsettings`). Its byte
   layout is not public; Skyrim's `.lod` struct is a candidate shape (`skyrim-lod-settings-struct`).
4. **Normal map name**: `<world>.n.level<L>.x<X>.y<Y>.dds` per one excerpt, not a `_n` suffix. Single source.
5. Not public, so a measurement protocol is given instead of a spec: NIF block contents (vertex layout, strips or
   triangles, UV, normals, shader properties), `.dlodsettings` bytes, `.dtl` bytes, the atlas UV remap, DDS format of
   the shipped textures.
