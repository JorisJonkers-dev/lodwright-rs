<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Water planes as polygons

`terrain.water_polygons` (a switch, off by default, written only when on; tier 3, since 0.14.0) writes the water plane
of a block at levels 8 to 32 as polygons: the squares of equal water height that touch along an edge are one region,
its outline is traced (holes included), reduced to its corners and triangulated, so a vertex that only lies on a straight
run is not written and the plane covers exactly the same squares at the same heights. The water overlap and depth of
`verify` do not change; the number of vertices in the file falls (the figures are on the pull request). Level 4 keeps
one rectangle per cell segment. A block whose outline cannot be triangulated keeps the rectangle plane (the sum of the
triangle areas is checked against the wet squares); the switch takes precedence over `terrain.water_shared_vertices`.
Own design (an outline of the wet squares, not a copy of anything), not yet looked at in the game: the owner decides.
Off, output and digests are as before; on, the switch enters the digest of every terrain block that has water.
