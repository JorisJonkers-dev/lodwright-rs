# Release test

A release of lodwright is cut only after this test passes on a real machine. The release pull request
(release-please, `chore(main): release X.Y.Z`) stays open until then. The results go into that pull request and into
the release test ticket as numbers and verdicts only: no game data, no screenshots, no file contents and no paths of
the machine. Nothing from the game is committed.

The game's data is read in place and never written to. Everything generated goes to a scratch folder outside the
data folder and outside any git working tree.

The pass criteria are those of gate G5 of the Toolbox (`docs/REORG.md` in rauls-trusty-toolbox):

- `lodwright verify` against the reference reports no missing or extra block in the worlds generated;
- every object block is exact, or listed with a reason;
- the water IoU is at least 0.95 per terrain block;
- in game, the generated folder loads with no error lines, and the three reference views show LOD where the reference
  run does.

## 0. Build at the release commit

The release commit is the head of the release pull request, merged locally onto the current `main` so that the
build is what the squash merge will ship.

```sh
git -C ~/Projects/JorisJonkers-dev/lodwright-rs fetch origin
git -C ~/Projects/JorisJonkers-dev/lodwright-rs worktree add \
  ~/Projects/JorisJonkers-dev/worktrees/lodwright-rs/release-test-X.Y.Z -b release-test/X.Y.Z-local origin/main
cd ~/Projects/JorisJonkers-dev/worktrees/lodwright-rs/release-test-X.Y.Z
git merge --no-ff origin/release-please--branches--main
CARGO_TARGET_DIR="<own target folder>" capped 8G cargo build --release -p lodwright-cli
lodwright --version
```

Record the version line and both commits. The local branch is never pushed.

## 1. Generate

Generate the main world with the load order the game uses (the master file alone when the game runs without a
plugin list):

```sh
lodwright generate --data "<data folder>" --plugin <master> --world <main world> --out <scratch>/gen --json
```

Pass: exit 0 and status `ok`. Record the game version the data comes from, the wall time, `peak_memory_mib`, the file counts per kind and the warnings.

## 2. Verify

Extract the game's own LOD tree (the `meshes/landscape/lod/`, `textures/landscape/lod/` and `lodsettings/` paths of
the game's archives) into `<scratch>/reference` with any archive tool, then:

```sh
lodwright verify <scratch>/gen
lodwright verify <scratch>/gen --reference <scratch>/reference
```

Pass:

- the first command exits 0 and reports 0 errors and 0 differences (the second exits 1 whenever it lists a
  difference, which the reference's other worlds always cause, so its exit code is not the pass criterion);
- against the reference, no file of the main world is missing or extra (the other worlds of the reference are not
  generated and are ignored, as is `manifest.json`);
- every object block has the reference's vertex and triangle counts, and every vertex lies within 0.05 units of one
  of the reference's, or the block is listed in an open ticket with its reason;
- the water IoU of every terrain block with water on either side is at least 0.95.

`verify --reference` measures both (`docs/contracts.md`, section Verify). Run it once more with the threshold and
keep the JSON:

```sh
lodwright verify <scratch>/gen --reference <scratch>/reference --water-iou 0.95 --json > <scratch>/verify.json
```

Record from its summary lines (`water: ...` and `object blocks: ... exact`) or from `water` and `objects` in the JSON:
the files compared, the missing and extra counts, the blocks with water, how many are under 0.95, the lowest and the
mean IoU, and the object blocks exact out of all, with the path of each block that is not exact.

A release may ship with known differences (decision D6). They are listed in the known-differences file kept with
the release test notes (format in `docs/contracts.md`, Verify) and passed with `--known-differences <file>`: a listed
block no longer fails the run, an unlisted one still does, and a stale entry (a block that no longer differs) is removed
from the file before the release. The per-level water lines (`water level L: ...`, or `water.per_level` in the JSON)
go into the release notes as the known difference per level: for each level the blocks under 0.95, the lowest and the
mean IoU. Each later release test (R2, R3) updates the file with its own batch's known differences.

## 3. In-game load

Run the game through `nv-rs-dev bench` from an nv-rs release build, as that repository's `docs/fork/BENCH.md`
describes: under the bench lock and the memory guard, in the nested headless session. Before each run, no local
inference server holds a loaded model and the card's used VRAM is under 4 GiB. A guard halt stops the test, and only
the owner clears it.

The generated folder is never copied into the game folder. Build a scratch game folder instead: a symbolic link for
every entry of the game folder except `Data`, a real `Data` folder with a link for every entry of the game's `Data`,
and the generated `meshes`, `textures` and `lodsettings` folders added to it (hard links or a copy). The bench's
`--game` option points at it. Check first that the game's `Data` has no loose `meshes`, `textures` or `lodsettings`
folder of its own; if it has, merge into a copy of it instead of linking it. The game still writes its own log files
next to the executable through the links, as in any run; nothing is written to `Data`.

Run each of the two town scenarios twice, without and with the generated folder, saving frames every 2 seconds:

```sh
W="--resolution 2560x1440 --vram-limit-pct 50 --nested headless"
nv-rs-dev bench run <town> $W --shots <scratch>/shots/ref-<town>,2000
nv-rs-dev bench run <town> $W --game <scratch>/game --shots <scratch>/shots/gen-<town>,2000
```

During each run with the generated folder, watch the opens under the scratch `Data/meshes`, `Data/textures` and
`Data/lodsettings` (inotify `IN_OPEN`), to show that the game read the generated files and not the archived ones.

The three reference views:

1. the first town, at the start of the measured script (the player placed, before the first walk);
2. the first town, after the scripted turn (the end of the second walk);
3. the second town, at the start of the measured script.

Take the frame nearest the frame number of each phase in the run's `viewer.log` (`# driver: phase`). Until the bench
has a scenario with fixed views (nv-rs#343), the heading after the turn differs between runs, so compare what the
distance shows (terrain silhouettes, distant buildings, water), not pixels.

Pass:

- the settings file, terrain blocks, object blocks and their textures of the generated folder are opened;
- the bench verdict's checks other than the hitch flags pass in both runs (frame capture itself causes hitches);
- the error lines of `viewer.log` (`error`, `fail`, `stop:`) are the same in kind and count with and without the
  generated folder, none names a LOD file, and the run's `missing.tsv` names no LOD file;
- at each view, distant LOD shows where the reference run shows it, with no holes, seams or floating water.

Record per run: the verdict, the wall time, the memory guard's caps and the card's peak use, the number of
generated files opened, and per view what the distance shows.

## Results

Post the table on the release pull request and on the release test ticket:

| Step | Verdict | Numbers |
| --- | --- | --- |
| 0. Build | | version, commits |
| 1. Generate | | wall s, peak MiB, files per kind, warnings |
| 2. Verify | | errors; missing and extra; object blocks exact; water IoU under 0.95, lowest, mean |
| 3. In-game load | | files opened, error lines, what each view shows, guard caps and peak |

Merge the release pull request only when every step passes. If a step fails, file a ticket for the cause, or add the
evidence to the ticket that already tracks it, and keep the release held.
