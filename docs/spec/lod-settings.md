# LOD settings file

## 1. Name and place

`LODSettings\<world>.dlodsettings` under the data folder (`fnv-lod-settings-dlodsettings`). The extension is
`.dlodsettings`, not `.lod`; the earlier row `lod-settings-path` is superseded. The editor writes it when terrain LOD is
built, a generator skips worlds without one, and vanilla `wastelandnv.dlodsettings` ships inside `Fallout - Misc.bsa`
(same row, from search excerpts and a forum post).

## 2. Contents

Public sources say it holds the cell coordinates of the world's south-west corner and (a GECK wiki talk page,
excerpt only) can limit which blocks and levels show. The byte layout is not published for Fallout.

Candidate shape, from the Skyrim sibling only (`skyrim-lod-settings-struct`), little endian, 16 bytes:

| Offset | Type | Field |
| --- | --- | --- |
| 0 | i16 | left (west cell x) |
| 2 | i16 | bottom (south cell y) |
| 4 | i32 | stride_pow2 |
| 8 | i32 | lowestsize_lod |
| 12 | i32 | highestsize_lod |

Do not implement this for Fallout until M8 shows the owner's file is 16 bytes and decodes sensibly (for
`wastelandnv`: left and bottom a plausible world corner, a level range of 4 to 32 or its power-of-two form).
If the size or values differ, stop and record the measured layout instead.

## 3. Writing it

Whether the game requires a settings file for custom worlds, and what happens when its bounds disagree with the
blocks, is not documented. M8 includes a safe experiment (owner-run, own data).
