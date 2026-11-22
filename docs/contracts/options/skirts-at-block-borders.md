<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Skirts at block borders

`terrain.levels[].skirts` (off by default, written only when on; tier 3, since 0.9.0) closes the gap to a neighbouring
block, of the same or another level, the way the game's own blocks do: under every border vertex a second vertex at
the constant height -14098 and two triangles per border edge, facing away from the block (`game-lod-block-borders`,
`docs/spec/terrain-lod.md` section 5). Blocks are not coupled, so no neighbour data enters a digest; the switch enters the
digest of that level's blocks only, and off the files and digests are as before. The skirt vertices count against the
level's vertex cap: the surface is meshed with the room that is left. Whether the skirts remove the visible cracks in
the game is an owner check (release test R2).
