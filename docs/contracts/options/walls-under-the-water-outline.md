<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Walls under the water outline

`terrain.water_walls` (a switch, off by default, written only when on; tier 3, since 0.14.0) writes a vertical wall
under every outer side of a block's water: from the water height down to -14098, the height the game's own water
outlines drop to. Sides on the block's border get none, and a side between two squares of the same water height gets
none; at level 4 the walls are triangles of the cell's own segment. The planes themselves do not change, so the water
overlap of `verify` is the same; the water profile of `verify --reference` (depth, wall area) moves toward the game's.
Own design fitted to measured numbers (`docs/spec/terrain-lod.md`, section 6), not yet looked at in the game: the owner
decides whether it is worth turning on. Off, output and digests are as before; on, the switch enters the digest of
every terrain block that has water.
