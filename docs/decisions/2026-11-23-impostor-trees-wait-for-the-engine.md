---
status: proposed
date: 2026-11-23
---
# Impostor trees wait for the engine (#371)

The question was whether far trees should become octahedral impostors (colour, normal and depth baked from many
directions and lit in the engine) instead of the game's flat billboards, and what the engine must draw for that.

## Verdict

**No generator change now.** lodwright keeps writing the game's tree LOD: billboard atlas, type list and block files. If
the far trees are to improve, the cheaper first trial is **crossed quads in the engine**, using the billboards lodwright
already writes. Octahedral impostors come later, only if that trial falls short.

## The options

| | crossed quads | octahedral impostors |
| --- | --- | --- |
| new generated data | none | an atlas per tree type: colour, normal and depth from many directions |
| baking | none | a renderer that draws each full tree model from every direction; lodwright has none |
| engine work | draw two or three quads per tree | a dedicated shader that picks and blends views, and optionally marches depth |
| look from the side | the same picture from each angle, with visible crossing | a correct view from any direction |
| original game | unaffected | unaffected, but cannot use the new atlases |

## What nv-rs would have to render for octahedral impostors

1. Map the view direction to the octahedral grid and blend the nearest three baked views.
2. Light the result from the baked normals, so far trees follow the sun, sky light and weather.
3. Use baked depth for parallax and for depth written to the frame, so trees meet the ground and each other properly.
4. Optionally draw them into the far shadow map, which connects this to #367.

The generator work starts only after the engine has this shader, as the ticket requires. It would be a separate output
beside the game's files, off by default, and would need its own atlas budget: many views per tree type multiply texture
memory.

## Sources

Ideas only; no code is taken from any of these.

- Godot Octahedral Impostors: <https://github.com/SIsilicon/Godot-Octahedral-Impostors>
- Impostor baking and rendering, a technical write-up: <https://arycama.github.io/2024/10/05/Imposters.html>
- Octahedral imposters, a technical write-up: <https://smailikhappy.github.io/posts/Octahedral_imposters/>
- Unreal Engine impostor baker documentation:
  <https://dev.epicgames.com/documentation/unreal-engine/impostor-baker-plugin-in-unreal-engine>
