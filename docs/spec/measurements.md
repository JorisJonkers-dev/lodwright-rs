# Measurement protocol for the owner

Everything below runs on your machine, on your own files, under `capped`. Report numbers and names only; never paste or
commit file content. Existing generator output is allowed as black-box input (`oracle:` rows).

`lodwright probe <nif|dds|settings|dtl> <file>` takes M1, M5, M7 and M8 for you: it prints only structure and numbers
(NIF header and block table, per-shape vertex and triangle counts, DDS header and channel statistics, the size and
first 64 bytes as little-endian integers of a settings or `.dtl` file), and refuses a file inside a git working tree.
Add `--json` for one line of JSON. A NIF block whose fields do not add up reports `layout_matches: false`.

Common: pick `WastelandNV` and one small DLC world. Use a hex dump limited to the first 64 bytes unless stated.

- **M1 block NIF.** For three blocks (one per level 4, 16, 32): file size; first line of the file (the NIF header
  text: version string); any NIF viewer's block list (types, counts), vertex and triangle counts, vertex attributes
  present, index width, whether triangles or strips, NiTriShape count, texture paths in the properties, material or
  shader properties. Compare vertex heights to `lodwright heightmap` samples at that level.
- **M2 span and corner.** List all names at level 4 and 32: min and max X and Y, the step between coordinates, and
  whether every coordinate is a multiple of the level. Compare with the cell bounds `lodwright plan` prints.
- **M3 case and separators.** `ls` the blocks folder; report case of folders and names, and a count per level.
- **M4 terrain vs object.** Generate terrain only, list the blocks; then generate objects into a fresh output; compare
  name lists: do level-4 names collide, does a `.high.` file exist, which files appear in which pass.
- **M5 textures.** For one block per level: diffuse and normal file names; DDS header fields (width, height, mip count,
  FourCC or bit masks); the average of each channel of the normal map (a flat map has red and green near 128, blue
  near 255).
- **M6 object content.** For one object block: the shapes, the atlas file it references, UV range per shape; which
  object references in the cell are present (compare with xEdit's reference list for that cell).
- **M7 trees.** `.dtl` and `treetypes.lst` sizes and first 64 bytes of one each; count of entries in the list.
- **M8 settings file.** Size of `wastelandnv.dlodsettings` (from the extracted archive or your own build) and hex of the
  whole file if at most 64 bytes; same for a DLC world and for a file from a different world. Report the integers.
  Safe experiment: copy to a scratch folder, change one value in the copy, never in the data folder.
- **M9 pickup.** With a loose settings file and the archived one both present, report which the game uses (see in
  the game whether LOD appears); same for a loose block mesh over an archived one. Full protocol below (#93).
- **M10 embedded path size.** Whether a BSA file record's stored size counts the embedded path when archive flag 0x100
  is set. Full protocol below (#52).

Each answer becomes a provenance row with `oracle:<role>` roles, for example `oracle:block-nif`.

## M9 in full: loose LOD against archived LOD (#93)

Three experiments, one variable at a time, in a scratch game folder. The question for each: when a loose file and an
archived file have the same path, which one does the game open?

**Scratch folder.** Build a game folder whose files are symbolic links to your install (`cp -rs "<game>" "<scratch>"`),
so nothing is copied and the install cannot change. Never edit a linked file: before you change a file (an INI, a
plugin list), replace its link with a real copy (`cp --remove-destination "$(readlink -f f)" f`). Loose test files
are new real files in the scratch `Data` folder. Take the bench lock for the whole session.

**What to record with every run** (one line per run):

| Field | Values |
| --- | --- |
| Engine | nv-rs (the Toolbox default; its version) or the original game through Proton (its version) |
| `[Archive]` | `bInvalidateOlderFiles`, `bUseArchives`, `SInvalidationFile` and whether that file exists, `SArchiveList` |
| JIP LN | present or not; an `.override` file next to any archive: present or not |
| Times | modification time of the loose file and of the archive that holds the same path (older, newer) |
| Observation | the loose path opened: yes or no; the archived copy used: yes or no; error lines: count |

**How to observe without looking at content.** Watch which loose paths the process opens: `strace -f -e trace=openat
-o <log> <launch command>` under Proton, or `inotifywait -m -r -e open "<scratch>/Data/meshes/landscape/lod"
"<scratch>/Data/textures/landscape/lod" "<scratch>/Data/lodsettings"` for either engine. Report only whether the test
path appears and how often. A visible change in the game (below) is the second check, described in words.

1. **Settings file.** Put a loose `lodsettings/<world>.dlodsettings` for a world whose settings file sits in an archive
   (`fnv-lod-settings-dlodsettings`). Make the loose copy differ in one observable value: the highest level 16 instead
   of 32 (`game-dlodsettings-layout`). Observe: is the loose path opened; does level-32 LOD still appear at long range.
2. **Block mesh.** Put one loose terrain block mesh at the path of an archived one (`game-lod-terrain-layout`), for
   example lodwright's own output for that block, or the archived block with its height raised in a copy. Observe: is
   the loose path opened; does the block change in the game.
3. **Texture.** The same with one block's diffuse texture (a lodwright-generated one, or a uniformly tinted copy).

Repeat each experiment with the variables changed one at a time: `bInvalidateOlderFiles` 0 and 1; the loose file
older and newer than the archive; JIP LN with and without `.override`. Five runs per experiment cover it.

**What the result decides.** Recorded as an `oracle:pickup` row and in `game-pickup.md`: what the Toolbox must do to
install a generated folder (for example: loose files always win, so install last and nothing else; or loose files win
only with an INI setting or newer timestamps, so the Toolbox sets that). If loose files always win, packing (#44) may
not be wanted: say so on #93 (owner decision D1).

Do not answer this question by reading the engine port's source; it is measured (D5 does not apply to #93).

## M10 in full: does the stored size count the embedded path (#52)

**Find an archive with flag 0x100.** The archive flags are the u32 at byte 12 of the file (`bsa-v104-header`):

```text
for f in "<data folder>"/*.bsa; do printf '%s ' "$(basename "$f")"; od -An -tx4 -j12 -N4 "$f"; done
```

Flag 0x100 is set when the third hex digit from the right is odd (for example `00000107`, `00000703`). Report the
archive names and their flag values (names of the game's own archives are fine to report; no content).

**Measure.** The script below reads one archive and prints only counts. It follows the recorded rows
(`bsa-v104-header`, `bsa-v104-folder-records`, `bsa-v104-file-records`, `bsa-v104-data-blocks`). Save it outside any
git working tree and run `python3 -I m10.py "<archive>"`. For each file it compares the stored size with the distance
to the next file's data (the "span"), and, for compressed files, with the bytes the zlib stream really uses:

- `span=size`: the stored size covers the embedded path, the original-size field and the data;
- `span=size+path`: the stored size leaves out the path (one length byte and the path);
- `zlib: size counts path` or `zlib: size excludes path`: the same question, answered from the compressed stream
  itself, which does not depend on the blocks being back to back.

```python
import struct, sys, zlib
data = open(sys.argv[1], "rb").read()
magic, version, folders_at, flags, n_folders, n_files, dir_len, name_len, file_flags = struct.unpack_from("<4sIIIIIIIH", data, 0)
print("version", version, "flags", hex(flags), "folders", n_folders, "files", n_files)
if magic != b"BSA\0" or version != 104 or not flags & 0x100:
    sys.exit("not a v104 archive with flag 0x100: nothing to measure")
pos, records = folders_at, []
folders = [struct.unpack_from("<QII", data, folders_at + 16 * i) for i in range(n_folders)]
pos = folders_at + 16 * n_folders
for _, count, _ in folders:
    if flags & 0x1:
        pos += 1 + data[pos]
    for _ in range(count):
        _, size, offset = struct.unpack_from("<QII", data, pos)
        records.append((offset, size))
        pos += 16
records.sort()
counts = {}
for i, (offset, size) in enumerate(records):
    stored = size & 0x3FFFFFFF
    compressed = bool(flags & 0x4) != bool(size & 0x40000000)
    end = records[i + 1][0] if i + 1 < len(records) else len(data)
    span = end - offset
    path_len = data[offset]
    head = 1 + path_len
    verdict = []
    if stored == span:
        verdict.append("span=size")
    if stored + head == span:
        verdict.append("span=size+path")
    if compressed:
        z = zlib.decompressobj()
        z.decompress(data[offset + head + 4: end])
        used = span - head - 4 - len(z.unused_data)
        if used + 4 + head == stored:
            verdict.append("zlib: size counts path")
        if used + 4 == stored:
            verdict.append("zlib: size excludes path")
    key = ("compressed" if compressed else "stored", " & ".join(verdict) or "neither")
    counts[key] = counts.get(key, 0) + 1
for (kind, verdict), n in sorted(counts.items()):
    print(kind, "|", verdict, "|", n)
```

Report the printed lines: the flags line and one line per kind (`stored` or `compressed`), verdict and count. A
clean answer is every file of a kind under one verdict. Run it on two archives if you have them, one with compressed
files and one with stored files. If no archive has flag 0x100, say so on #52: the writer then does not set the flag
and the row keeps its assumption with that reason (#52's fallback).

The script was checked on synthetic archives built both ways (size with and without the path, compressed and
stored); each gave the expected verdict for every file.

