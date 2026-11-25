# lodwright-rs

**lodwright builds the distant view of Fallout: New Vegas for your own mods**: the land, water, buildings and trees
you see far away. Without it, mods change the world up close while the horizon still shows the old, unmodded one.

<p align="center">
  <img src="docs/images/how-it-works.svg" alt="lodwright-rs reads your load order and builds a far view (land, water, buildings and trees) that matches it" width="100%">
</p>

## Before and after

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/docs/images/lod-modded-strip.jpg" alt="The Strip from above, side by side: without a generated far view casinos and buildings are missing (left); with lodwright's they are there (right)" width="100%">
  <br><em>The Strip on a Tale of Two Wastelands setup with 312 mods: the game's far view (left) and lodwright's (right). The casinos are back.</em>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/docs/images/lod-overpasses-freeside.jpg" alt="Side by side: on the left the I-15 overpasses north of Freeside vanish at mid distance; on the right they stay in view" width="100%">
  <br><em>Large far objects stay in view: the I-15 overpasses north of Freeside no longer vanish at mid distance.</em>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/JorisJonkers-dev/rauls-trusty-toolbox/main/docs/images/lod-plain-hoover.jpg" alt="Hoover Dam and Lake Mead from above on the plain game: the game's own far view and lodwright's look the same" width="100%">
  <br><em>On the unmodded game it matches the original: Hoover Dam, the game's far view (left) and lodwright's (right).</em>
</p>

## Fast

<p align="center">
  <img src="docs/images/speed.svg" alt="Building the far view for a load order of 133 plugins: the older tool, xLODGen, takes about 37 minutes; lodwright-rs 64 seconds" width="100%">
</p>

- 🖥️ Runs on Linux, Windows and macOS; the older tools run on Windows only.
- 🌊 Draws land, lakes and rivers, buildings and trees far away.
- 🔁 Run it again after you change your mods; far land that did not change is kept.

## What it covers

<p align="center">
  <img src="docs/coverage/tally.svg" alt="Feature catalogue: items, tagged, passing and covered per area" width="100%">
</p>

Every promise lodwright makes is listed in a catalogue, and the chart shows how many of them a passing test on Linux, Windows and macOS proves.

## How it fits together

<p align="center">
  <img src="docs/images/raul-parts.svg" alt="raul and its five parts: nv-rs the game engine, nvmod-rs mods, loot-rs load order, lodwright-rs distant view (this project), ttwright-rs Tale of Two Wastelands" width="100%">
</p>

lodwright is one part of [raul](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox). As its own project it is
tested on its own: its output is compared, piece by piece, with the game's own far view.

## Get it

**[Download raul](https://github.com/JorisJonkers-dev/rauls-trusty-toolbox/releases/latest)**: lodwright is built
in; open the LOD page (the far view) and press Generate. lodwright also runs on its own from the command line
([docs/USAGE.md](docs/USAGE.md)).

## Learn more

- [Using lodwright](docs/USAGE.md): raul's LOD page, the commands, downloads and checking them.
- [More detail](docs/OVERVIEW.md): recent work, the clean-room rule, and where to read next.
- [What is still open](docs/gap-list.md) and [speed measurements](docs/performance.md).

Licence: GPL-3.0-only, with two files translated from xEdit kept under MPL-2.0 ([docs/LICENSING.md](docs/LICENSING.md)).
