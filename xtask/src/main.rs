// SPDX-License-Identifier: GPL-3.0-only
//! `cargo xtask`: see [`xtask::HELP`].

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(xtask::main(&args));
}
