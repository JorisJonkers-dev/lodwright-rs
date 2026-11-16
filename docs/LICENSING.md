# Licensing map

lodwright-rs is licensed GPL-3.0-only, like the other Components of Raul's tRusty Toolbox. Each crate's `license`
field and the SPDX header of each source file say which licence applies; this page is the map.

| Part | Licence | Why |
| --- | --- | --- |
| Every crate, the `lodwright` binary, scripts and fuzz targets | GPL-3.0-only | Our own code |
| `crates/lodwright-core/src/trees.rs`, `crates/lodwright-gen/src/trees.rs` | MPL-2.0 | Translated from xEdit's MPL-2.0 sources, which stay MPL-2.0 |
| `lodwright-core`, `lodwright-gen` (crate `license` field) | `GPL-3.0-only AND MPL-2.0` | Each contains one of those files |

The full texts are `LICENSE` (GPL-3.0) and `LICENSES/MPL-2.0.txt`.

Rules:

- New files are GPL-3.0-only and say so in their first five lines.
- Code translated from xEdit's MPL-2.0 sources keeps `SPDX-License-Identifier: MPL-2.0` and names the file and
  revision in its header. MPL-2.0 allows these files to be distributed as part of the GPL-3.0 work, since xEdit's
  sources carry no "Incompatible With Secondary Licenses" notice; the files themselves stay MPL-2.0.
- esplugin declares plain `GPL-3.0` (GPL-3.0-only), so the project says GPL-3.0-only, not "or later".
- `deny.toml` allows permissive, MPL-2.0 and GPL-3.0 licences for dependencies; LGPL and AGPL are not allowed.
- The clean-room rule ([clean-room.md](clean-room.md)) is separate from the licence: GPL source is still not an input
  for learning a format.

History: the project was MPL-2.0 until 2026-11-16, when the owner, its only copyright holder, relicensed it to
GPL-3.0-only. The two translated files above were never ours to relicense and stay MPL-2.0.
