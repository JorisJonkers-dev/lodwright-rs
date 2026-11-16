# Licensing map

lodwright-rs is a set of crates that carry different licences. Each crate's `license` field and the SPDX header of
each source file say which one applies; this page is the map.

| Part | Licence | Why |
| --- | --- | --- |
| `lodwright-core`, `lodwright-nif`, `lodwright-dds`, `lodwright-assets`, `lodwright-contract`, `lodwright-testkit` | MPL-2.0 | Written here, some of it translated from MPL-2.0 xEdit definitions, which must stay MPL-2.0 |
| `lodwright-plugin-meta` | GPL-3.0-or-later | Wraps the GPL-3.0 `esplugin` crate for plugin header reading |
| `lodwright-cli` (the `lodwright` binary) | MPL-2.0 for its own files; the built binary is GPL-3.0 | It links `lodwright-plugin-meta` |

Rules:

- A binary that links GPL-3.0 code is distributed under GPL-3.0 terms. MPL-2.0 files stay MPL-2.0 and remain usable
  by anyone from the library crates that do not link GPL code.
- Code translated from xEdit's MPL-2.0 sources keeps MPL-2.0 and names the file and revision in its header. It is
  never moved into a GPL crate in a way that drops that notice.
- GPL-3.0 code goes into its own crate where practical, so the MPL-2.0 crates keep a permissive dependency tree.
- `deny.toml` allows permissive, MPL-2.0 and GPL-3.0 licences; LGPL and AGPL are not allowed.
- The clean-room rule ([clean-room.md](clean-room.md)) is separate from the licence: GPL source is still not an input
  for learning a format.
- No repository-wide relicensing is planned. A file changes licence only by an explicit owner decision.
