<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Object faces under the ground and the water

`objects.drop_under_terrain` and `objects.drop_under_water` (both off by default, written only when on) leave out
object LOD faces that cannot be seen: a face whose three corners all lie below the ground at their positions (the
cell's height samples, interpolated between the four around each corner), or below the water height of a cell that has
water. A face that crosses the surface is kept. Vertices that no kept face uses are left out too, so the UVs of the
others stay where they were in the atlas. This is lodwright's own design, not a rule of the game's files. With either
option on, the result counts the faces left out (`dropped_faces`: `under_terrain`, `under_water`; a face under both
counts as under the ground). With both off, the output and the request digest are the same as before the options
existed.
