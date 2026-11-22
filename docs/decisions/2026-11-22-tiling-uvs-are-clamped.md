---
status: accepted
date: 2026-11-22
---
# Far-object textures meant to repeat are clamped, not repeated

A far model whose UVs run outside 0 to 1 (a texture meant to repeat, such as bark or a long wall) cannot repeat inside
an atlas rectangle. lodwright holds such a coordinate at the rectangle's edge, as the game's own object blocks do and as
the atlas steps of other generators do (row `game-object-uv-clamp`). On a 133-plugin modded TTW load order 12 far textures are used
this way; their far objects show the edge texels stretched where the model repeats the texture. This is accepted: it is
what the game's own far view looks like, and the other options (a separate texture per tiling model, or cutting the
triangles at each repeat) would leave the game's block layout.
