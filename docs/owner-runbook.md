# Owner runbook: what to run on real data

The repository never holds game data, and CI never has it. Everything below runs on your machine, reads your files
and prints numbers and file names. Nothing is written to the data folder, and nothing from your files goes into a
commit, a pull request or an issue: paste numbers into the notes, not content.

All commands run under the memory guard, for example `capped 6G lodwright ...`. Reading a large plugin (a 250 MB
master) takes its size in memory for the moment it is parsed; the heights kept afterwards are about 4.3 KB per cell.

## 0. Build

```text
cargo build --release          # binary: target/release/lodwright
lodwright --help
lodwright doctor "<data folder>"
```

## 1. Check the plugin reader on your plugins

Give the plugins in load order, masters first (a plugin whose master is missing from the list is an error naming the
master). The data folder is the game's `Data` folder; names are matched without regard to case.

```text
lodwright plan --data "<data folder>" --plugin FalloutNV.esm --plugin DeadMoney.esm ... --format text
lodwright plan --data "<data folder>" --load-order plugins.txt --world WastelandNV --blocks
```

Report back, per worldspace: the cell count, the bounds, the **max seam error** and the per-level block counts.

What the numbers should be:

- Cell count and bounds equal what xEdit shows for the worldspace (count of exterior cells with LAND).
- **Seam error** near 0 means neighbouring cells agree on their shared edge, as the raster assumes. A large value
  (tens of units or more) means real data disagrees and the seam rule (the larger cell wins) needs revisiting.
- Block counts per level: compare with the number of `.btr` files per level in an existing LOD output of yours
  (folders listed below). A mismatch means block alignment or naming differs from what the game's tools produce.

If a plugin fails with a message such as "the plugin ends inside ..." or "not a readable plugin", the message names
the plugin; report the message and the plugin name (not the file).

## 2. Check the file layout rows

`provenance/sources.toml` rows `lod-block-naming`, `lod-terrain-paths` and `lod-settings-path` are recorded from
notes and not yet checked. List (do not copy) the folders of an existing LOD output of yours:

```text
find "<LOD output>" -maxdepth 4 | head -50
ls "<LOD output>/meshes/landscape/lod/<World>/blocks" | head
ls "<LOD output>/textures/landscape/lod/<World>/blocks" | head
ls "<data folder>/LODSettings"
```

Report: the folder names and their case, one example file name per kind (mesh, diffuse, normal map, settings), and
whether block coordinates in the names are the south-west cell coordinates and multiples of the level. These answers
flip the rows to verified or replace them, and `lodwright plan --blocks` prints the paths the generator would use, for
comparison.

## 3. Look at the heights

```text
lodwright heightmap --data "<data folder>" --plugin ... --world WastelandNV --level 32 --out ~/heights.pgm
```

The command refuses output inside a git working tree. It writes a 16-bit greyscale PGM (north up) and prints which
heights 0 and 65535 stand for; open it in any image viewer. It should look like the worldspace; flipped or shifted
output means the cell coordinate or row orientation assumptions are wrong.

## 4. Facts needed before the writers

These need real files and are what the next implementation steps wait for. Each is a task for the specifier (see
[clean-room.md](clean-room.md)); the answer becomes a provenance row, not a file.

1. **`.btr` layout**: header, vertex and triangle record formats, whether it is a NIF container, what the 32767
   vertex cap and index width imply. Method: parse an existing `.btr` of yours with a tool of your own, record the
   layout, and compare vertex heights with `lodwright heightmap` samples at level 4.
2. **`.lod` fields**: the fields, their order and units, compared across several worldspaces.
3. **Normal-map and diffuse conventions**: size per level, compression, mip count, suffix (`lodcheck` and `ddsaudit`
   already report these).
4. **Object LOD**: `.bto` layout, the atlas and its UV remap, which references are selected. Needs the same kind of
   black-box study of an existing output and is the largest remaining piece.
5. **Quality mapping**: if you want parity with an existing generator's look, sweep its quality setting on one
   worldspace and record vertex counts per block next to `lodwright plan --blocks` for several tolerances.

Public-source specs and the measurement protocol for these items are in [spec/](spec/README.md) (M1 to M9). Note
the finding there: the block meshes are `.nif`, not `.btr`, and the settings file is `.dlodsettings`.

## 5. Not done and not planned without your decision

Publishing the repository, and contacting the author of the tool whose behaviour is being reimplemented, are your
decisions (see the last section of [clean-room.md](clean-room.md)).
