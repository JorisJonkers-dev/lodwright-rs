// SPDX-License-Identifier: MPL-2.0
//! Command line of the xLODGen port. The logic is here so tests can drive it without a process.
//!
//! Exit codes: 0 success, 1 a check failed, 2 usage error.

use std::io::Write;
use std::path::Path;

/// Exit code: success.
pub const EXIT_OK: u8 = 0;
/// Exit code: a check failed (for example the data folder is missing).
pub const EXIT_FAILED: u8 = 1;
/// Exit code: the arguments were not understood.
pub const EXIT_USAGE: u8 = 2;

const HELP: &str = "xlodgen: clean-room Rust port of xLODGen (not implemented yet)

Usage: xlodgen [--help | --version]
       xlodgen doctor <data-folder>

Commands:
  doctor <data-folder>  check that the folder exists; prints nothing taken from it

Options:
  -h, --help     print this help
  -V, --version  print the version
";

/// Run the command line `args` (without the program name). Output goes to `out`, diagnostics to `err`.
/// Returns the process exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["--help" | "-h"] => write_all(out, err, HELP),
        ["--version" | "-V"] => write_all(
            out,
            err,
            &format!("xlodgen {}\n", env!("CARGO_PKG_VERSION")),
        ),
        ["doctor", folder] => doctor(Path::new(folder), out, err),
        [] => usage(err, "no command given"),
        ["doctor", ..] => usage(err, "doctor takes exactly one argument: the data folder"),
        [other, ..] => usage(err, &format!("unknown argument: {other}")),
    }
}

fn write_all(out: &mut dyn Write, err: &mut dyn Write, text: &str) -> u8 {
    match out.write_all(text.as_bytes()) {
        Ok(()) => EXIT_OK,
        Err(e) => {
            let _ = writeln!(err, "xlodgen: cannot write output: {e}");
            EXIT_FAILED
        }
    }
}

fn usage(err: &mut dyn Write, problem: &str) -> u8 {
    let _ = writeln!(err, "xlodgen: {problem}\nTry 'xlodgen --help'.");
    EXIT_USAGE
}

/// Check that `folder` is an existing directory. Nothing inside it is read or printed.
fn doctor(folder: &Path, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if folder.is_dir() {
        write_all(out, err, "ok: data folder exists\n")
    } else {
        let _ = writeln!(
            err,
            "xlodgen: the data folder does not exist or is not a directory"
        );
        EXIT_FAILED
    }
}

#[cfg(test)]
mod tests {
    use super::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, run};

    fn go(args: &[&str]) -> (u8, String, String) {
        let args: Vec<String> = args.iter().map(ToString::to_string).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run(&args, &mut out, &mut err);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn version_prints_the_package_version() {
        let (code, out, _) = go(&["--version"]);
        assert_eq!(code, EXIT_OK);
        assert_eq!(out, format!("xlodgen {}\n", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn help_lists_the_doctor_command() {
        let (code, out, _) = go(&["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.contains("doctor <data-folder>"));
    }

    #[test]
    fn no_arguments_is_a_usage_error() {
        assert_eq!(go(&[]).0, EXIT_USAGE);
    }

    #[test]
    fn unknown_argument_is_a_usage_error_naming_it() {
        let (code, _, err) = go(&["frobnicate"]);
        assert_eq!(code, EXIT_USAGE);
        assert!(err.contains("frobnicate"));
    }

    #[test]
    fn doctor_without_a_folder_is_a_usage_error() {
        assert_eq!(go(&["doctor"]).0, EXIT_USAGE);
        assert_eq!(go(&["doctor", "a", "b"]).0, EXIT_USAGE);
    }

    #[test]
    fn doctor_accepts_an_existing_folder_and_prints_no_path() {
        let dir = std::env::temp_dir();
        let (code, out, err) = go(&["doctor", dir.to_str().unwrap()]);
        assert_eq!(code, EXIT_OK);
        assert_eq!(out, "ok: data folder exists\n");
        assert_eq!(err, "");
    }

    #[test]
    fn doctor_fails_for_a_missing_folder() {
        let (code, out, err) = go(&["doctor", "/definitely/not/here"]);
        assert_eq!(code, EXIT_FAILED);
        assert_eq!(out, "");
        assert!(!err.contains("/definitely"), "the path is not echoed");
    }

    #[test]
    fn doctor_fails_for_a_file() {
        let file = std::env::current_exe().unwrap();
        assert_eq!(go(&["doctor", file.to_str().unwrap()]).0, EXIT_FAILED);
    }

    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("closed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_closed_stdout_is_reported_as_a_failure() {
        let mut err = Vec::new();
        let code = run(&["--version".to_string()], &mut Broken, &mut err);
        assert_eq!(code, EXIT_FAILED);
        assert_ne!(err, Vec::<u8>::new());
    }
}
