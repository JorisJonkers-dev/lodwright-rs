# A/B protocol for LOD quality changes

Every ticket that changes the quality of generated LOD says "better" the same way. This page is that way; tickets cite it
instead of restating it. The numbers named below are recorded as the pull request and the Release test notes describe
(see "Where the numbers go"); the thresholds are written in the ticket that makes the change.

## What is compared

1. **Build against build, same request.** A regression check: the previous build and the changed build run the same
   request on the same input, and every gate number is equal or better.
2. **Parameter set against parameter set, same build.** An option off against on. The off side is today's output, so
   with the option off the output and the default request digest stay unchanged (a golden pins it).
3. **Build against the oracle.** `lodwright verify <folder> --reference <game's own LOD tree>` on the owner's data:
   read-only, numbers only, never committed.

## Layers, in order

| Layer | Where it runs | What it compares | Judge |
| --- | --- | --- | --- |
| 1. Synthetic | CI, on synthetic fixtures | properties, goldens, `verify` exit codes and the min and max bands of the numbers | automatic: the thresholds written in the ticket |
| 2. Oracle numbers | the owner's machine, read-only | `verify --reference` numbers against the game's own LOD tree | the numbers, against the thresholds in the ticket and the known differences (#140) |
| 3. In game | the bench with the views scenario (nv-rs#343), nested headless | the reference views, one view at a time, against the reference frame | the owner, by eye, for R2 and R3 |

Until nv-rs#343 lands the views scenario is not fixed, so layer 3 compares what the distance shows (terrain silhouettes,
distant buildings, water), not pixels (see `docs/RELEASE-TEST.md`, section 3).

## Commands

- Verify against the reference: `lodwright verify <folder> --reference <reference folder> --json > verify.json`
  (exit 0 clean, 1 findings, 2 usage, 3 unreadable folder; `--known-differences FILE` moves listed differences out of the
  findings; `--water-iou` sets the water threshold). Fields: `docs/contracts.md`, section Verify.
- Min and max over several runs of the same inputs: `lodwright bands verify1.json verify2.json ... [--json]`.
  Equal min and max means the run is reproducible; a change is judged against the band, not against one run.
- In game: `nv-rs-dev bench run <town> ... --shots DIR,2000` with and without the generated folder, as
  `docs/RELEASE-TEST.md`, section 3 describes, once the views scenario exists.

## What a failure blocks

- A failed automated A/B check in CI (layer 1) blocks the release batch the ticket belongs to. A build-against-build
  regression blocks the merge of that pull request.
- A number that gets worse in an oracle comparison (layer 2) blocks the release batch unless the difference is listed
  as a known difference with its reason (#140).
- A gap, a seam, floating water or missing LOD in a reference view (layer 3) blocks the Release test sign-off of the
  batch.

## Where the numbers go

In the pull request that makes the change and in the Release test notes of the batch (the Release test ticket), as
numbers and verdicts only. Never in a committed file: the oracle comparison reads the owner's game data, and none of
it, no paths of the machine and no screenshots enter a repository.
