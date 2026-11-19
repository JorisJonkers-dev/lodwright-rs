# BSA writer and stub plugin (design, not implemented)

Status: proposal for the owner's review, for #130 (writer core), #131 (stub plugin) and the packing step #136. Nothing
here is built yet. Owner decisions: packing is opt-in and gated (D1), the archive is registered by a stub plugin of the
same name once an in-game test confirms it (D2, #132), packing is a step at the end of a generate (D3), and the Mod
never holds loose and packed output at the same time (G2).

Every format fact below names its row. Rows marked *draft* are in #165 (C1, `verified = false`); the writer may rely on
them only after a second reader has verified them and #124's public test vector exists. Whoever specified those rows
does not implement the writer without that second reading (`docs/clean-room.md`).

## 1. What the writer takes and gives

```text
lodwright_assets::bsa::write(entries: &[PackEntry], options: &PackOptions) -> Result<Vec<u8>, PackError>
PackEntry   { path: String (relative, forward or back slashes), data: bytes, compress: bool }
PackOptions { compressed_by_default: bool }
```

- The writer builds the whole archive in memory from already generated files (an archive is at most 4 GiB because
  offsets are u32, section 5); the packing step (#136) calls it once per archive and streams the result to a temporary file that is
  renamed into place (the staging rule of `docs/interrupted-runs.md`).
- `compress` per entry comes from the caller's per-kind rule (terrain uncompressed, object textures compressed). This
  is a performance default taken from the guide page that `fnv-archive-loading` cites (`docs/spec/game-pickup.md`);
  no row records it and it is not a format fact. The archive's compressed-by-
  default flag is set to the majority choice, and entries that differ carry the invert bit (`bsa-v104-file-records`).
- The result reads back through the existing reader (`Archive::open`, `Archive::extract`): the first test.

## 2. Names

- Each path is normalised to lower case with backslashes (`bsa-path-form`), then split at the last backslash into
  folder and file name. A path with no folder part is refused (the format keeps every file in a folder).
- Choice: folder names longer than 254 bytes are refused. The folder name is a bzstring (`bsa-v104-folder-records`);
  the reader takes its length from one byte that counts the NUL, so 254 bytes plus the NUL is the longest name writer
  and reader agree on.
- Two input paths that normalise to the same name are refused (`PackError::CaseCollision` naming both). Linux file
  systems allow them; the archive cannot hold both.

## 3. Hashes and order (draft rows)

- Folder hash: `bsa-name-hash` over the folder name, unsplit. File hash: the same function with the name split at its
  last dot (draft).
- Records: folders ascending by folder hash, each folder's files ascending by file hash, compared unsigned
  (`bsa-record-order`, draft; whether the game requires it is open, so the writer follows the one public writer).
- Two different folder names with equal hashes, or two files of one folder with equal hashes, are refused
  (`PackError::HashCollision`). What the game does with equal hashes is unknown (#124 asks); refusing is the safe
  answer and is deterministic.

## 4. Layout

All from the verified rows `bsa-v104-header`, `bsa-v104-folder-records`, `bsa-v104-file-records`,
`bsa-v104-data-blocks`:

1. Header, 36 bytes: `BSA\0`, version 104, folder-record offset 36, archive flags `0x1 | 0x2` plus `0x4` when
   compressed by default, folder count, file count, total folder-name length (with the NULs, without the length
   bytes), total file-name length (with the NULs), file flags, padding.
2. Folder records, 16 bytes each: hash, file count, offset. The row says to subtract the total file-name length from
   the stored offset to get the folder block's position in the file, so the writer stores *position + total file-name
   length* (`bsa-v104-folder-records`).
3. Folder blocks in record order: the bzstring folder name, then 16-byte file records (hash, size with bit 30 as the
   invert flag, offset of the data from the start of the archive).
4. The file-name block: the lower-case file names with a NUL each, in file-record order.
5. Data in file-record order: a compressed file is the u32 original size then the zlib stream, and its stored size is
   `4 + compressed length`; a stored file is the bytes.

- **Choice: flag 0x100 (embedded names) is never set.** It avoids the open question of #52 for the writer. One public writer
  warns that archives with embedded names holding non-texture files may crash FO3 and FNV, and that cubemap textures
  need the flag (lead recorded in #165's log; LOD has no cubemaps).
- File flags: lodwright's reader does not use them (`crates/lodwright-assets/src/bsa.rs`); the value to write is open (#124 can record it from the public page or
  from an owner archive's header, numbers only). Until then the writer writes 0 and the in-game check (#132) decides.

## 5. Limits (refused with a named error; which cases to refuse is a choice, the limits come from the rows)

| Case | Rule | Error |
| --- | --- | --- |
| One file's stored size over `0x3FFF_FFFF` bytes | the top two bits of the size are flags (`bsa-v104-file-records`) | `FileTooLarge` |
| Any data offset over `u32::MAX` | offsets are u32 | `ArchiveTooLarge` |
| Counts over `u32::MAX` | header fields are u32 | `ArchiveTooLarge` |
| Same name twice after normalisation | section 2 | `CaseCollision` |
| Equal hashes for different names | section 3 | `HashCollision` |
| No folder part, folder name over 254 bytes | section 2 | `BadPath` |
| No file at all | an empty archive is not written | `Empty` |
| Total folder-name or file-name length over `u32::MAX` | header fields are u32 | `ArchiveTooLarge` |

## 6. Determinism

- Order is fixed by the hashes and the names, never by input order or thread completion.
- Compression: `miniz_oxide` at a pinned level, one stream per file. Files may be compressed in parallel, but the
  results are placed in record order. A `miniz_oxide` version change changes bytes: a `Cargo.lock` change that moves it
  refreshes the golden and says so.
- The thread-count determinism test is extended to the writer (#130).

## 7. Tests (synthetic, in memory)

- Round trip: writer to `Archive::open` and `extract`, for stored, compressed and mixed archives.
- A golden of a tiny archive as hex text under the crate's test goldens (never a `.bsa`; the guard refuses it).
- The hash function against the public test vector once #124 records it, and against the draft row's worked cases.
- One test per refusal in section 5 (the size cases use a fake size, not gigabytes of data).
- Property: any set of distinct short names round-trips and the records come out sorted.

A round trip through our own reader proves only that writer and reader agree. Correctness for the game comes from the
verified rows, the test vector and the in-game check (#132).

## 8. Stub plugin (#131)

- A plugin whose only record is the TES4 header record (`plugin-record-header`, `plugin-subrecord-layout`): HEDR with
  the version, a record count of 0 and the next object id; no masters, since it holds no records that refer to a
  master. Named `<archive base name>.esp` next to `<archive base name>.bsa`.
- Which HEDR version value and header flags the game accepts for an empty plugin has no row yet. The stub copies
  nothing from a game file; the values are recorded as rows first (an owner header read of a small own plugin, numbers
  only, or a public page), then #132 checks the load in the game.
- Registration is by name. xEdit's model (draft `bsa-archive-registration`) loads every archive whose name starts with
  the plugin's name, so `<name>.bsa` with `<name>.esp` is enough; the game's behaviour is what #132 measures.
- The stub takes a slot in the plugin list; the Toolbox shows it as generated LOD (owner follow-up decision G3).

## 9. The packing step and the manifest (#136)

- The packing option is refused ("not available yet") until #132 has confirmed on the target engine that the stub
  plugin registers the archive (D2); #136 depends on #132.
- After a generate finishes in staging, the packing step reads the manifest, builds the archive from the listed files
  (never `manifest.json`), writes the stub plugin and, only if the owner decides it is needed (it brings a JIP LN
  dependency, `fnv-archive-loading`; not decided), the `.override` marker, removes the loose files it
  packed from staging, and the usual swap publishes the result (G2: never both).
- Proposed manifest rule: the manifest keeps listing every generated file with its digest and adds the archive and the
  stub as files with their own digests and a `packed_into` field per member. `up_to_date` compares the members'
  digests (the request did not change) and the archive's digest (the archive is intact), so toggling packing does not
  change the request digest (D3) and a later loose run rebuilds the loose files from the same blocks.

## Open questions

- The file flags value (section 4) and the stub's HEDR version and flags (section 8): rows needed.
- Whether the game needs hash-sorted records or only tolerates them, and what it does with equal hashes (#124).
- One archive with mixed compression, or one archive per kind: one archive is enough for the format (the invert bit);
  two archives may be preferred for the guide's per-kind advice. The packing step can do either; one is proposed.
