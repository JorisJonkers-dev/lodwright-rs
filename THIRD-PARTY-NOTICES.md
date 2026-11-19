# Third-party notices

Code taken from the engine port (nv-rs) under another licence than the engine's own GPL-3.0-or-later keeps its notice
here ([ADR-0001](docs/adr/0001-engine-source-as-a-format-input.md)). Each provenance row that relies on such code has
one section named `## `<row id>``, holding the copyright line and the licence text or notice as the licence requires. The
guard (`scripts/cleanroom_guards.py provenance`) checks the sections against the rows. No row relies on such code yet.
