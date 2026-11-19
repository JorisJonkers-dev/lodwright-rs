# LOD settings file

## 1. Name and place

`lodsettings/<world>.dlodsettings` under the data folder, as measured in the game's own files
(`game-dlodsettings-layout`). The extension is `.dlodsettings`, not `.lod`; the earlier row `lod-settings-path` is
superseded. Public excerpts write it `LODSettings\<world>.dlodsettings` (the case of the folder differs; the
measured row is followed) and add that the editor writes it when terrain LOD is built, that a
generator skips worlds without one, and that the vanilla `wastelandnv.dlodsettings` ships inside `Fallout - Misc.bsa`
(`fnv-lod-settings-dlodsettings`, from search excerpts and a forum post).

## 2. Contents

Public sources say it holds the cell coordinates of the world's south-west corner and (a GECK wiki talk page,
excerpt only) can limit which blocks and levels show. The byte layout is not published for Fallout; it was measured
on two of the owner's own files (`game-dlodsettings-layout`, numbers only). The code follows this layout
(`crates/lodwright-core/src/lod.rs`).

24 bytes, little endian:

| Offset | Type | Field |
| --- | --- | --- |
| 0 | u32 | lowest terrain level |
| 4 | u32 | highest terrain level |
| 8 | u32 | world width in cells |
| 12 | i16 | south-west cell x |
| 14 | i16 | south-west cell y |
| 16 | i16 | north-east cell x |
| 18 | i16 | north-east cell y |
| 20 | u32 | unknown; 4 in both files read, equal to the object LOD level |

The two files read decoded as (4, 32, 128, -64, -64, 63, 63, 4) and (4, 16, 64, -32, -32, 31, 31, 4): in both the width
equals north-east x minus south-west x plus one. The row is `verified = false` until a second reader re-derives it
from the reported numbers (#94).

The 16-byte Skyrim struct (`skyrim-lod-settings-struct`) was the candidate shape before the measurement. It does not
match the Fallout file and is not used.

Open: the meaning of the last field, and whether the game accepts other values in it.

## 3. Writing it

Whether the game requires a settings file for custom worlds, and what happens when its bounds disagree with the
blocks, is not documented. M8 includes a safe experiment (owner-run, own data).
