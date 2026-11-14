# Fuzzing the future parsers

One target per parser the port will have: `plugin_records`, `land_vhgt`, `land_layers`, `lod_settings`, `dds_header`, `bsa_archive`,
`btr_mesh`, `bto_mesh`, and `nif_read` (the real NIF reader of `lodwright-nif`); `dds_header` runs the real reader and decoder of `lodwright-dds`. Today each is an empty-but-compiling harness (the readers do not exist; where the testkit
has a reference reader the harness runs it). Seed inputs come from `lodwright-testkit` builders, never from game
files; corpora and crashes are git-ignored (`fuzz/.gitignore`) and the data guard refuses committed binaries.

This directory is its own Cargo workspace, so `libfuzzer-sys` stays out of the shipped crates.
`cargo-fuzz` needs nightly; select it per invocation and do not add a `fuzz/rust-toolchain.toml`:

    cargo install --locked cargo-fuzz@0.13.2
    cargo +nightly fuzz list
    cargo +nightly fuzz run plugin_records -- -max_total_time=600

CI type-checks the harnesses on every pull request (`task fuzz:check`) and runs each target for ten minutes every
week (`.github/workflows/fuzz.yml`), uploading the artifacts of a failing run.
