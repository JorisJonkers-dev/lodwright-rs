# Where a run spends its time

Every `generate` result carries `steps`: one entry per step (and per world where a step runs once for each) with wall
time, CPU time of the whole process, the peak resident memory during the step, and the files and bytes it wrote.
Steps, in the order they run: `read_plugins`, `setup` (manifest check, digests), `assets_index` (archives and loose
files), then per world `terrain.land_tiles`, `terrain.shipped_plan`, `terrain.blocks`, `objects.collect`,
`objects.models`, `objects.textures`, `objects.atlas` (pack, blit, encode and write the atlases), `objects.blocks`,
`trees`, and at the end `pack` (when asked for) and `manifest`. The peak of a step is read after resetting the kernel's
high-water mark at its start (Linux; the fields are left out elsewhere).

## Measured 2026-11-23 (lodwright 0.15.1 plus the profile), all worlds with terrain, a 16 thread machine that other work
was using at the same time (load average about 10: wall times vary by a third between runs of the same input)

| step (seconds) | plain game, 18 plugins | modded, 133 plugins |
| --- | ---: | ---: |
| read_plugins, setup, assets_index | 2.3 | 3.2 |
| terrain.land_tiles + shipped_plan | 10.1 | 13.4 |
| terrain.blocks (4 threads) | 99.6 (CPU 361) | 118.2 (CPU 413) |
| terrain.blocks (12 threads) | 58.1 (CPU 514) | |
| objects.collect, models, textures | 1.5 | 7.6 |
| objects.atlas | 9.5 | 62.3 |
| objects.blocks | 1.1 | 2.4 |
| trees | 1.6 | 4.3 |
| whole run | 126 | 211 |
| peak resident memory | 3.5 GiB | 4.2 GiB |

`terrain.blocks` is the largest step (about 80 percent of a plain run, 56 percent of a modded one). It is limited by
the thread count: by default at most four threads work, however many cores there are. `objects.atlas` is the second:
one thread encodes the atlas pages (BC3), and the modded load order has pages of many megabytes.

## Guard

`crates/lodwright-api/tests/step_bench.rs` times the terrain block step on a synthetic world and the atlas encode on a
synthetic page, each against a fixed calibration loop in the same process, and fails when a ratio is four times its
recorded value. It runs with `task bench` (a release build; an ordinary `cargo test` ignores it).
