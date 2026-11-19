// SPDX-License-Identifier: GPL-3.0-only
//! Repository tasks of lodwright-rs, run as `cargo xtask <command>` (the alias is in `.cargo/config.toml`).

pub mod guards;
pub mod release;
pub mod settings;

/// The help text.
pub const HELP: &str = "\
cargo xtask <command>

  guards [--root DIR] [--base REF] {spdx,provenance,data,gitignore,names,all}
                          the clean-room guards (docs/clean-room.md)
  release-assets <out-dir>
                          build the release assets: binary, schemas, SHA256SUMS
  setup-repo [--audit] [owner/repo]
                          audit or apply the GitHub settings that cannot live in a file (owner only)
  apply-rulesets [owner/repo]
                          apply .github/rulesets/*.json (owner only)
";

/// Dispatch a command line (without the program name) and return the exit status.
#[must_use]
pub fn main(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        Some("guards") => guards::main(&args[1..]),
        Some("release-assets") => {
            if let Some([out]) = args.get(1..) {
                match release::build(std::path::Path::new(out)) {
                    Ok(()) => 0,
                    Err(message) => {
                        eprintln!("{message}");
                        1
                    }
                }
            } else {
                eprintln!("usage: cargo xtask release-assets <out-dir>");
                2
            }
        }
        Some("setup-repo") => settings::setup_main(&args[1..]),
        Some("apply-rulesets") => settings::rulesets_main(&args[1..]),
        Some("--help" | "-h") => {
            print!("{HELP}");
            0
        }
        _ => {
            eprint!("{HELP}");
            2
        }
    }
}
