// SPDX-License-Identifier: MPL-2.0
//! The `lodwright` binary: a thin wrapper over [`lodwright_cli::run`].

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = lodwright_cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    ExitCode::from(code)
}
