# Provenance rows, one file each

A new format fact is a new file here, named after its id: `provenance/rows/<id>.toml`, holding one `[[source]]` row with
the same fields as in `provenance/sources.toml` (id, fact, url, licence, date, recorded_by, role, verified, note, and for
engine-port rows `second_reader` and `engine_notes`). The row's `note` is its log entry: who read what, when, what stayed
open. `cargo xtask guards provenance` reads this folder together with the frozen table, requires the file name to match
the id and exactly one row per file, and refuses a pull request that adds a row to `sources.toml` or an entry to
`LOG.md`. A second reader who confirms a row edits that row where it lives.
