# Interrupted runs: staging, swap, cancel and resume (design, not implemented)

Status: proposal for the owner's review (issue #43, "kill and resume across worlds"). Nothing here is built yet.
The contract today is in [contracts.md](contracts.md) under "Request and manifest".

## The problem

`generate` writes straight into the output folder. Each file appears whole (it is written to a temporary name and
renamed), but the folder as a whole does not:

- A **cancel** (the `CancelToken`) is cooperative. The run writes an interrupted manifest of the blocks it finished
  and of every older file still on disk, and the next run with the same request keeps those blocks. This works and is
  tested in `crates/lodwright-api/tests/generate.rs`.
- A **kill** (the process ends: `SIGKILL`, a crash, power loss, the Toolbox closing) writes nothing more. A run that
  started from an older manifest removed that manifest first, so the folder now holds a mix of old and new files and
  no manifest. The next run refuses it as `output_not_empty` unless `--overwrite` is given, and the Toolbox clears it
  (`discard_partial`). The work done is lost and, worse, when the output folder is the installed Mod, **the installed LOD is
  broken until a run completes**: the game reads a half-old, half-new folder.

The acceptance criterion in #43 is that a run killed after N blocks resumes and the final folder equals an
uninterrupted run byte for byte. The design below also makes sure the folder the game reads is never half-built.

## Proposal

### 1. Build in a staging folder, swap on success

- The run builds into a **staging folder** next to the output folder, on the same file system so renames are atomic:
  `<output>.lodwright-staging/` (a sibling, so it is never inside the folder that is installed as a Mod).
- Blocks that a previous complete run already holds and whose inputs did not change (the #84 reuse) are **hard
  linked** from the output folder into staging (same file system), falling back to a copy where links are not
  possible. Their digests are checked as today.
- When every world and step is done, the run removes `checkpoint.json` from staging (it is never part of the
  output, and the lock is not in staging: section 4), writes the final manifest into staging and **swaps**:
  1. write `<output>.lodwright-swap` (a small marker naming the two folders, written to a temporary name and
     renamed);
  2. rename `<output>` to `<output>.lodwright-old` (skipped on a first run, when there is no `<output>`);
  3. rename staging to `<output>`;
  4. remove the marker, then `<output>.lodwright-old`.
- A directory cannot be renamed over a non-empty one on Linux, Windows or macOS, so the swap is two renames, not one.
  Between steps 2 and 3 `<output>` does not exist: a kill there leaves no LOD folder until the next `generate` (or the
  Toolbox at start, through a library call) finishes the swap from the marker (section 3). That window is two renames
  long; making it disappear needs a link the game follows (a symbolic link or junction as the Mod folder), which
  this design does not propose.
- Every file, the checkpoint and the marker are written to a temporary name, flushed (`fsync`), renamed, and the
  parent folder is flushed after the rename, so a power loss leaves either the old or the new entry, never an empty
  one. Before the swap the staging folder and its parent are flushed.
- Staging must be on the same file system as `<output>`: a rename that fails as cross-device refuses the run
  (`output_failed`) instead of falling back to a copy. Network file systems are not supported targets for this
  guarantee.
- The sibling names pass the same guards as the output folder (not in the data folder, not in a git working tree,
  not through a link).
- On Windows a folder cannot be renamed while a process holds a file in it open (the game, an antivirus scan). The
  swap retries for a short time and then fails with `output_failed`, leaving staging complete so the next run only
  swaps.
- Disk use: up to the size of the old output plus the new files (reused blocks are links, not copies, where the file
  system allows).

Effect: the output folder is always either the previous complete LOD or the new complete LOD, except for the
two-rename window above, after which it is missing (never half-built) until recovery. The Toolbox can keep the Mod
enabled during a run.

### 2. Checkpoints in staging

- After each finished block (or batch of blocks, see "Scheduling" below) and after each world step, the run updates
  `staging/checkpoint.json` by writing a temporary file and renaming it. It has the same form as today's interrupted
  manifest: request digest, inputs digest, the finished files with sizes and digests, and the finished blocks with
  their input digests.
- A file is listed only after it has been renamed into place, so a listed file is always whole. A file on disk that is
  not listed (killed between the rename and the checkpoint) is removed or rebuilt by the next run.
- Object and tree LOD are checkpointed per world and step: a world whose object step finished is not rebuilt; one that
  was interrupted is rebuilt in full (today they are rebuilt whenever a run does not end `up_to_date`).
- The digests are the ones the manifest has today: the request digest covers every request field, the inputs digest
  the sizes and contents of every plugin in load order, and each block's input digest what its files depend on
  (generator version, level settings, heights, water, land layers and texture pixels).
- A **cancel** does the same as a kill, but writes the checkpoint at once and returns `Cancelled`; the output folder
  is not touched.

### 3. What the next `generate` does

At start, after taking the lock (section 4) and before anything else, the run looks for the sibling names. With a
swap marker, which rename finished is read from which folders exist:

| Marker | `<output>` | `-old` | Staging | Meaning | Action |
| --- | --- | --- | --- | --- | --- |
| yes | yes | no | yes | Killed before step 2 | Continue the swap from step 2 |
| yes | no | yes | yes | Killed between steps 2 and 3 | Rename staging to `<output>`, then step 4 |
| yes | yes | yes | no | Killed before step 4 | Step 4 |
| yes | yes | no | no | Killed during step 4 | Remove the marker |
| yes | no | no | yes | First run, killed before step 3 | Rename staging to `<output>`, remove the marker |
| yes | any other | | | Inconsistent | Change nothing, refuse with `output_failed` and name the folders |
| no | | yes | | Leftover of a finished swap | Remove `-old` |

Recovery never renames onto an existing `<output>`, so it cannot replace a good folder with an older one. Without a
marker, staging is judged by its checkpoint:

| Found | Meaning | Action |
| --- | --- | --- |
| Staging with a checkpoint for the **same** request digest and inputs digest | Killed or cancelled run of this request | **Resume**: keep every listed file whose digest still matches, remove unlisted files, build the rest |
| Staging with a checkpoint for another request or other inputs | Interrupted run of an older request | Remove staging, start fresh (reusing blocks from `<output>` as today) |
| Staging without a checkpoint | Killed before the first checkpoint | Remove staging, start fresh |

Then, as today, an `<output>` whose manifest matches the request and inputs answers `up_to_date`.

A resumed run produces the same bytes as an uninterrupted one because every block's bytes depend only on its inputs
(the determinism invariant: not on the thread count, the batch size, the memory budget, or the order in which blocks
finished), and a kept file is kept only when its digest matches.

### 4. One run at a time

The run first takes an operating-system lock on a fixed sibling file, `<output>.lodwright-lock` (`flock` on Linux
and macOS, `LockFileEx` on Windows), and holds it until it returns. The lock file stays where it is: it is not in
staging (which cleanup removes and the swap renames) and not in the output. A second run on the same output folder
while the lock is held is refused with a new error code, `output_busy`. The operating system drops the lock when
the process ends, so a killed run leaves no stale lock to judge by process id. The Toolbox should also not start a
second run itself, but the library must not rely on that.

### 5. Settings edited during a run

The run works from the request it was given at start; its digest is fixed for the whole run. An edit the user makes in
the settings page while a run is going on applies to the **next** run. When that next run starts, the checkpoint's
request digest no longer matches, so the interrupted staging is discarded (section 3), and blocks whose inputs did
not change are still reused from the last complete output. The Toolbox should show that the current run uses the
settings it started with, and offer to restart.

### 6. Scheduling and memory (for the budget work)

Checkpoints are written per batch of blocks. Lowering the batch size or the worker count under memory pressure
changes how often a checkpoint is written, never what is in a file. The memory-pressure determinism test
(`crates/lodwright-api/tests/memory_pressure.rs`) holds that invariant.

## Contract changes

- `docs/contracts.md`: the output folder is replaced only on success; the four sibling names (`-staging`, `-swap`,
  `-old`, `-lock`) are reserved; the new
  error `output_busy`; what the next run does after an interruption (the table above).
- The result gains nothing required. `GenerateOutcome::Cancelled` keeps its meaning; its doc changes from "the output
  folder holds a manifest of the blocks that finished" to "the staging folder holds a checkpoint".
- `plan` capabilities: `resume_after_kill` becomes true once this lands.
- `output_not_empty` stays for a non-empty output folder without a manifest. A folder left half-built by today's
  behaviour (killed run, no manifest) still needs `--overwrite` or the Toolbox's `discard_partial` once; after that
  the output folder is never left half-built.

## Tests

- In process: cancel after N blocks, resume, compare every file and the manifest with an uninterrupted run.
- **Killed process**: run the CLI as a child process with `--events FILE`, kill it with `SIGKILL` once N block events
  are in the file (Linux and macOS; `TerminateProcess` on Windows), run again, compare every file with an
  uninterrupted run. Repeat with N at a world border and inside a world.
- Swap recovery: build each state of the table in section 3 by hand (a marker with and without `<output>`, `-old`
  alone, staging for the same and for another request, staging without a checkpoint) and check the next run's action
  and final bytes.
- A second run while the lock is held answers `output_busy`; a run after a killed lock holder takes the lock.
- After a resumed run, staging, the marker, `-old` and the checkpoint are gone and the output holds exactly the
  manifest's files.
- Synthetic fixtures only, built by `lodwright-testkit`.

## Questions for the owner

1. Staging as a sibling (`<output>.lodwright-staging`) or inside a lodwright work folder the Toolbox chooses (for
   example next to its Instance data)? A sibling keeps renames on one file system without asking; a work folder keeps
   the Mods folder clean but may be on another disk, which would make the swap a copy.
2. Is doubling the disk use during a run acceptable when hard links are not available (for example an exFAT disk)?
3. Should the Toolbox keep the Mod enabled during a run (safe with this design) or disable it until the run ends?
4. Should a cancel from the UI keep the staging folder for a later resume (proposed), or discard it? A kept staging
   folder is removed by the next run of a different request, so it never outlives one more run.
