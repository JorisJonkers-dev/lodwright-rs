<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Specular in the normal map alpha

`terrain.levels[].specular_alpha` (off by default, written only when on; tier 3, since 0.7.0) carries specular in the
alpha channel of that level's terrain normal maps, which are then written as BC3 (DXT5) instead of BC1. The value is the
`SNAM` specular exponent of the land textures under each pixel, blended with the same opacities as the diffuse map and
scaled to 0 to 255 over the lowest and highest exponent of the load order; a texture without an exponent counts as the
lowest value and the result carries a warning naming it, and when no land texture has an exponent at all the alpha stays
opaque with one warning. This is lodwright's own design (`docs/spec/textures.md`, section 4); whether the game uses it
is an owner check. Off, the files are byte-identical to earlier versions and the block digests do not change; on, the
exponents enter the digest of that level's blocks only.
