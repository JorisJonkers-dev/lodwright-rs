<!-- Request option note: one file per option, part of docs/contracts.md (Request and manifest). -->

### Light of the far land

`terrain.land_light` (a switch, on by default, written only when off; tier 3, since 0.17.0) darkens the composed far
land the way the near land is lit. The land near the player is the standard for far land.

Near land is drawn through each land texture's normal map; far land is drawn from one flat picture and is not. Measured
on a plain single-texture cell, straight down, player near and then far, composed far land is about 14 per cent brighter
than the loaded land beside it in both worlds, and the factor is one over the mean up component of the textures' normal
maps (0.882 Mojave, 0.865 Capital). On, each land texture's colours (and the default land texture's, whose normal map
is the file named after it with `_n`) are multiplied by the mean up component of its normal map before they are
composed; a texture with no readable normal map and a flat fill (the Strip's brown, the grey outside the cells) are not
scaled. The rule scales the whole colour: a baked picture cannot hold the sun part and the ambient part of the light
apart, so the result is exact where the sun dominates and a little dark in shade.

The game's tiles of a world (see `compose_shipped_tiles`) are left to the game, or used as the base of patched tiles,
only where they already match the near land: over the tiles of the world's two highest levels, the median position of
the shipped tile's mean luminance between the same tile composed with the factor (0) and without (1) must be at most
0.5. One answer per world: a baked tile has objects and shade painted in, which moves its own mean either way, and a
per-tile test left a patchwork of lit and baked tiles beside each other (seen around the Washington Monument). The baked Capital tiles are far below that line (about 0.78 of the unscaled composition)
and are kept; the Mojave's own tiles are composed from the land textures, sit at the unscaled line, and are composed
again, darkened. The test needs no list of worlds: it reads the shipped tile and the composition. Off, nothing is
scaled and every shipped tile is used as `compose_shipped_tiles` says, which is the output of the releases before
0.17.0 byte for byte.
