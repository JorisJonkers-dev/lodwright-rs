// SPDX-License-Identifier: MPL-2.0
//! The `xlodgen` binary: a thin wrapper over [`xlodgen_cli::run`].

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = xlodgen_cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    ExitCode::from(code)
}
