# lodwright in more detail

The [README](../README.md) is the short version. This page has what recent work changed, why lodwright is its own
project, the clean-room rule and where to read next.

## Recent work

- **Far objects as the game's own.** All 295 object blocks of the main world have the game's vertex and triangle
  counts (99.98 percent of vertices in place). Far-object textures keep their own size on several atlases, and a
  block over the 16-bit limits is written as several shapes instead of losing objects.
- **Objects without a far model** can be drawn from a simplified full model, and include rules take chosen objects
  into the far view (0.17.0).
- **Far land lit as the near land is.** The game's own far-land tiles are kept where no mod changed the land, and a
  composed tile is darkened to match the land near the player.
- **Far water.** Lakes and rivers are drawn at a distance, with water walls at block borders so no gaps show.
- **Speed.** On a 133-plugin load order, the older community generator averages about 37 minutes and lodwright takes 64 seconds on 16
  threads; step-by-step figures are in [docs/performance.md](performance.md).

What is still open, with measurements, is in [docs/gap-list.md](gap-list.md).

## Why a separate project

lodwright is one of the Components raul is made of, next to [nv-rs](https://github.com/JorisJonkers-dev/nv-rs),
[nvmod-rs](https://github.com/JorisJonkers-dev/nvmod-rs), [loot-rs](https://github.com/JorisJonkers-dev/loot-rs) and
[ttwright-rs](https://github.com/JorisJonkers-dev/ttwright-rs). Each does one job, is tested on its own and releases
on its own schedule; raul links them into one program and ships one tested set. lodwright in particular is checked
without raul: its output is compared block by block with the game's own far view.

## Clean-room rule

Format facts come from public format documents, the MPL-2.0 xEdit source, public readmes, Microsoft's DDS
documentation, the game's own data formats on a contributor's machine, and black-box output of an existing generator
run on the contributor's own data. Decompiled or leaked code and unlicensed source are never read. Every fact the
code relies on is recorded in `provenance/` (CI checks the record). Game data and reference output never go in this
repository; test fixtures are synthetic. The rule is in [docs/clean-room.md](clean-room.md).

## Where to read next

- [docs/gap-list.md](gap-list.md): what full generation needs and what exists.
- [docs/design.md](design.md): pipeline, crates, interfaces, open questions.
- [docs/RELEASE-TEST.md](RELEASE-TEST.md): the checklist a release passes on real data.
- [docs/TESTING.md](TESTING.md): synthetic fixtures, properties and the `verify` design.
- [docs/LICENSING.md](LICENSING.md): which crate carries which licence.
- [docs/PRACTICES.md](PRACTICES.md), [CONTRIBUTING.md](../CONTRIBUTING.md), [SECURITY.md](../SECURITY.md).
