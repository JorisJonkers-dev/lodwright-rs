# Testing strategy

Standards: [workspace testing standards](https://github.com/JorisJonkers-dev/workspace/blob/main/docs/testing-standards.md)
(one behaviour per test, named as that behaviour, no retries, quarantine at most 14 days). This file adds what is
specific to a tool that must never hold game data.

## The rule

**Synthetic fixtures only.** Every test builds its bytes in memory with `lodwright-testkit` (plugin bytes, LAND
heights, DDS headers, and later `.lod` files and meshes). Nothing binary is committed; `provenance/fixture-allowlist.txt`
is empty on purpose. The data guard (`task guards`) fails the build on a game-data extension, a large file or a
binary file. The tool itself never writes game data into a repository: it reads the owner's data from a path given
at run time, and CI never has it.

## Layers

| Layer | What | Where |
| --- | --- | --- |
| Unit | One function or type, no I/O | next to the code (`#[cfg(test)]`) |
| Round trip | Every writer is read back by its reader, every reader by its writer, over generated inputs | each format module |
| Golden file | A writer's output for a fixed synthetic input equals a stored expectation | `crates/*/tests/golden/` as text (hex dumps or JSON, never raw binary), updated with `UPDATE_GOLDEN=1` and reviewed in the diff |
| Property | Invariants of the height and tile math, over random inputs | see below |
| Library API | Each call in process: result, progress, cancel and resume, error codes | `crates/lodwright-api/tests/` |
| CLI | Exit codes and output of the binary | `crates/lodwright-cli/tests/` |
| Fuzz | Arbitrary bytes never panic a parser | `fuzz/`, weekly in CI; one target per parser |
| Oracle comparison | Generated output against the owner's existing reference-generator output | owner's machine only, see below |

Coverage: 80% of lines, enforced by `task test` (cargo-llvm-cov) in the pipeline.

## Properties for the height and tile math

Property tests use a small seeded generator inside the testkit (no extra dependency at first; `proptest` may be
added later under `deny.toml`). The properties to hold, once the code exists:

1. **VHGT round trip**: for heights on the quantisation grid whose neighbouring differences fit a signed byte,
   `decode(encode(h)) == h` exactly; for arbitrary heights the error is at most half a quantisation step.
2. **Integration order**: shifting every height by one step changes only the offset, not any delta.
3. **Block mapping**: for every cell `c` and block size `n`, `block_origin(block_of(c)) <= c < block_origin(block_of(c)) + n`;
   blocks tile the plane without gaps or overlap (negative cells included).
4. **Raster alignment**: a block-aligned raster's width and height are multiples of the block size, and cropping it
   to any block returns exactly that block's cells.
5. **Neighbour consistency**: the shared edge of two adjacent cells reads the same height from either cell once
   borders are resolved.
6. **Monotone simplification (later)**: a lower quality setting never yields more vertices; the vertex count stays
   at or under the 16-bit index cap; every kept vertex height equals the source height.
7. **Writer/reader identity**: for any valid `.lod` settings value, `parse(write(x)) == x`.

## Oracle comparison plan

The existing Windows-only generator is the oracle, and its output is a fact about files (`docs/clean-room.md`). The owner runs it locally on
their own data and keeps the output out of every repository. Byte equality with it is not the goal
(simplification heuristics differ); the aim is statistical parity and equal or better visuals, judged in game.

1. **Pin the oracle**: record the generator versions and the settings used for the reference run in the
   owner's private notes; reference them in `provenance/LOG.md` by role only.
2. **Facts first**: format questions (`.btr`/`.bto`/`.lod` layout) are answered from the open sources and by parsing
   oracle files on the owner's machine; the answer enters the repository as a provenance row, never as a file.
3. **Heights**: for each worldspace, `lodwright verify` samples the vertices of an existing `.btr` and checks them
   against the height raster built from the plugins (unsimplified level-4 vertices must coincide with LAND samples
   within the quantisation tolerance).
4. **Settings and structure**: `.lod` files equal field by field; block counts and names per level equal.
5. **Statistics**: vertex and triangle counts per block, maximum and mean height error, texture dimensions and
   mip counts (what `lodcheck` already reports).
6. **In game**: the owner's viewpoints and `fnvbench` frame times, A/B against the oracle output.

## The `verify` subcommand (design, not implemented)

```text
lodwright verify --generated <lod-folder> --reference <lod-folder> [--worldspace <name>] [--tolerance <units>] [--format text|json]
```

Compares a generated LOD folder against an existing one and prints a numeric diff report. Nothing from the files
is echoed except names of files and numbers.

- Output (text or JSON): per worldspace and level: files present in only one side; for files in both, per kind
  (`.lod`: field-by-field equality; `.btr`: vertex and triangle counts, max, mean and 95th-percentile absolute height
  error at matching positions, count of vertices with no counterpart; DDS: width, height, format, mip count equal or
  not). A final summary with the worst offenders by number.
- Exit codes (implemented, see `docs/contracts.md`): 0 clean, 1 findings (an error in the folder, or a difference from
  the reference), 2 usage error (including a folder inside a git working tree without `--allow-in-repo`), 3 unreadable
  folder. Each has a CLI test in `crates/lodwright-cli/tests/cli.rs`.
- Rules: read-only; never copies or embeds data from either folder into its output beyond counts and statistics.
  It refuses a folder or a `--reference` inside a git working tree (usage error, exit 2, no file read) unless
  `--allow-in-repo` is given; the flag applies to that run only.
- Testing it: golden reports for testkit-built generated and reference folders (identical, shifted heights, a missing
  file, a wrong mip count).

## Tolerance bands for A/B comparisons

A number from one run is not yet a measurement: before an A/B comparison (the protocol is #129) the A side is run
several times on the same inputs, each run is checked with `lodwright verify <run> --reference <reference> --json`, and
`lodwright bands run1.json run2.json run3.json` prints, per statistic, the minimum and maximum over those runs (three
decimals, see `docs/contracts.md`, Bands). A zero-width band means the statistic is exact across runs, which is what a
deterministic generator should give; a band with width says how much a statistic moves without any change. The B side
is then verified the same way, and a statistic only counts as changed by the B side where its value leaves the A band.
Bands are numbers only (block paths and values); the reports they come from stay out of the repository like any
output made from game data.

## Benchmarks policy

No benchmark runs in CI (shared runners are noisy and the machine's timing-sensitive work must not be disturbed).
Benchmarks are `cargo bench` targets over synthetic inputs, run by hand on the owner's machine, and a number enters a
pull request only with the input size, the machine and the command. A performance claim without those is not
accepted. Benchmark data is generated by the testkit; no game data is benchmarked in the repository.
