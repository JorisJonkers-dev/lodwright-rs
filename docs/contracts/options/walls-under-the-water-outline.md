<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Walls under the water outline

`terrain.water_walls` (a switch, on by default, written only when off; tier 3, since 0.14.0; on by default and meaning
the game's borders from the release after 0.15.2) writes what the game's own level blocks do where water meets a
block's border: a vertical wall from the water height down to -14098 (the height the game's water outlines drop to)
along every border side that has water, and at level 4 a plane that reaches 32 units past the south and west borders
and past the north border (the east border stays), so that two planes overlap across a seam. Measured on the game's
own blocks (`game-lod-water-walls`): no wall stands inside a level 4 block. In the engine, a lake that crosses block
seams shows a pale band along the loaded cells' edge and a dotted line at the seam without them, and neither with them
(`high-lake-mead`, same camera, game's own against ours, opened). Earlier releases wrote walls under the inside
outlines of the water and none on the border; that is not what the game does and is gone. Off, the water planes are
exactly the rectangles; the switch enters the digest of every terrain block that has water.
