---
status: proposed
date: 2026-11-22
---
# Objects marked visible from far whose model has no LOD model are left out

Some plugins mark objects visible from far whose model has no `_lod.nif` in the game or in any mod, although the full
model exists. lodwright leaves such objects out and says so: one warning per plugin and kind (the plugin, the count of
objects, the models) and the full list in `lodwright-warnings.json`. Whether to draw them from their full model instead
is proposed here, not built.

## What the measurement found

On a 133-plugin modded TTW load order the first reading of "37 + 4 LOD models missing" was mostly wrong. Of the 41
names, 33 were parts of a static collection whose own LOD model exists and is drawn: the game draws the collection's
LOD model and never its parts, so those models need no LOD model. Counting only what is used leaves, for the two worlds:

| plugin that marks the object | objects | models |
|---|---|---|
| VNV LOD Plugins.esp | 17 | 4 (a curb, an overpass ramp, two water jet effects) |
| YUPTTW.esm | 3 | 2 (building kit pieces, parts of two collections whose LOD model could not be read until #345) |
| TTW Fallout 3 LOD.esp | 1 | 1 (a billboard) |

Two collection LOD models carried bytes after their footer and were refused as unreadable, which also left their parts
without a model: reading them as the game does (ignoring that tail) fixes those objects.

## Proposal: leave them out

- Object blocks are built from `_lod.nif` models only (the game's own blocks and the other generators' alike); an
  object without one is not in the far view of the game or of the reference output (the reference output was not
  searched for these 21 objects one by one). Drawing it would add something the
  reference does not have.
- The models are not made for far drawing: effects (water jets), a billboard of two triangles and a 183-triangle curb are
  not meant to be seen from far, and a full model carries its own textures and normal maps that fit neither the atlas nor
  the vertex budget of a block.
- The count is small: 21 objects in two worlds.

A fallback would draw the full model (with its textures into the atlas). It would be an option that is off by default,
a separate pull request with a picture comparison, and only if an owner wants a plugin's objects drawn.
