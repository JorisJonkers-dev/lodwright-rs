<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Land texture tiling

`terrain.texture_scales` (empty by default, written only when set) says how often a land texture repeats across a
cell quadrant in the terrain diffuse maps, by the land texture's absolute `FormID` as eight hex digits, for example
`{"0000ABCD": 2.0}`; a texture that is not listed repeats once, as before. Each scale lies between 0.0625 and 16
(`TEXTURE_SCALE_RANGE`). No source is known for a per-texture scale in the game's records, so this is lodwright's own
setting, not a value read from the plugins. A scale other than 1 enters the digest of every terrain block that uses the
texture, so changing it rebuilds those blocks and keeps the others; with no scale set, block digests and output are the
same as before the option existed.
