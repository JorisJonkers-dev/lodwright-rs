// SPDX-License-Identifier: MPL-2.0
//! Command line of lodwright. The logic is here so tests can drive it without a process.
//!
//! Exit codes: 0 success, 1 failed (a result document is still printed with `--json`), 2 usage error or an
//! unsupported `--schema-version`, 3 unsupported (a capability is not built yet).
//!
//! With `--json`, result and error documents go to stdout; stderr carries human text only.

use std::io::Write;
use std::path::Path;

use lodwright_contract::{DocumentKind, Handshake, VersionInfo, schema_json};

mod commands;
mod failure;
mod load;
mod options;

/// Exit code: success.
pub const EXIT_OK: u8 = 0;
/// Exit code: a check failed (for example the data folder is missing).
pub const EXIT_FAILED: u8 = 1;
/// Exit code: the arguments were not understood.
pub const EXIT_USAGE: u8 = 2;
/// Exit code: a capability is not built yet (`generate`).
pub const EXIT_UNSUPPORTED: u8 = 3;

const HELP: &str = "lodwright: clean-room terrain and object LOD generator (terrain planning so far)

Usage: lodwright [--help | --version [--json]]
       lodwright doctor <data-folder>
       lodwright plan --data <data-folder> (--plugin NAME ... | --load-order FILE) [--world NAME]
                      [--format text|json | --json] [--schema-version N] [--blocks]
       lodwright generate [--data <data-folder> --plugin NAME ...] [--json] [--schema-version N]
       lodwright schema <plan|result> [--schema-version N]
       lodwright contracts --json
       lodwright heightmap --data <data-folder> (--plugin NAME ... | --load-order FILE) --world NAME
                      --out <file.pgm> [--level N] [--allow-in-repo]

Commands:
  doctor <data-folder>  check that the folder exists; prints nothing taken from it
  plan                  read the plugins (in load order, masters first) and print the terrain LOD plan:
                        blocks per level, vertex and triangle counts, errors, output paths
  generate              not built yet: prints a result document (status unsupported, exit 3), writes nothing
  schema                print the JSON Schema of a document (plan or result)
  contracts --json      print the contracts this build speaks and their schema versions, as
                        {\"contracts\":[{\"name\":...,\"versions\":[...]}]}
  heightmap             write the world's height raster as a 16-bit PGM (north up) for inspection

Plugins are read from the data folder; nothing is written there. Output is numbers and file names only.

Options:
  -h, --help     print this help
  -V, --version  print the version (with --json: tool, version and the contract versions spoken)
      --json     print JSON (an alias of --format json)

Exit codes: 0 ok, 1 failed, 2 usage error, 3 unsupported.
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
            &format!("lodwright {}\n", env!("CARGO_PKG_VERSION")),
        ),
        ["--version" | "-V", "--json"] | ["--json", "--version" | "-V"] => {
            let info = VersionInfo::current(env!("CARGO_PKG_VERSION"));
            write_all(out, err, &(to_json(&info) + "\n"))
        }
        ["contracts", "--json"] => write_all(out, err, &(to_json(&Handshake::current()) + "\n")),
        ["contracts", ..] => usage(err, "contracts takes exactly one option: --json"),
        ["doctor", folder] => doctor(Path::new(folder), out, err),
        ["plan", rest @ ..] => plan_command(rest, out, err),
        ["generate", rest @ ..] => generate_command(rest, out, err),
        ["schema", rest @ ..] => schema_command(rest, out, err),
        ["heightmap", rest @ ..] => heightmap_command(rest, out, err),
        [] => usage(err, "no command given"),
        ["doctor", ..] => usage(err, "doctor takes exactly one argument: the data folder"),
        [other, ..] => usage(err, &format!("unknown argument: {other}")),
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_owned())
}

/// Refuse a `--schema-version` this build does not speak for `kind`.
fn check_version(kind: DocumentKind, asked: Option<u32>) -> Result<(), String> {
    match asked {
        Some(n) if !kind.versions().contains(&n) => Err(format!(
            "{} schema_version {n} is not supported; this build speaks {:?}",
            kind.name(),
            kind.versions()
        )),
        _ => Ok(()),
    }
}

fn plan_command(args: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let options = match options::parse(args, true) {
        Ok(options) => options,
        Err(problem) => return usage(err, &problem),
    };
    if let Err(problem) = check_version(DocumentKind::Plan, options.schema_version) {
        return usage(err, &problem);
    }
    match commands::plan(&options, out) {
        Ok(()) => EXIT_OK,
        Err(failure) => fail(&options, &failure, out, err),
    }
}

fn heightmap_command(args: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let options = match options::parse(args, true) {
        Ok(options) => options,
        Err(problem) => return usage(err, &problem),
    };
    match commands::heightmap(&options, out) {
        Ok(()) => EXIT_OK,
        Err(failure) => fail(&options, &failure, out, err),
    }
}

/// Report a failure: human text on stderr, and with `--json` the failure document on stdout.
fn fail(
    options: &options::Options,
    failure: &failure::Failure,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let _ = writeln!(err, "lodwright: {failure}");
    if options.json {
        let doc = commands::failed_document(options, failure);
        let _ = writeln!(out, "{}", to_json(&doc));
    }
    EXIT_FAILED
}

fn generate_command(args: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let options = match options::parse(args, false) {
        Ok(options) => options,
        Err(problem) => return usage(err, &problem),
    };
    if let Err(problem) = check_version(DocumentKind::Result, options.schema_version) {
        return usage(err, &problem);
    }
    let doc = commands::generate(&options);
    if !options.json {
        let _ = writeln!(err, "lodwright: {}", doc.reason);
    }
    if out.write_all((to_json(&doc) + "\n").as_bytes()).is_err() {
        let _ = writeln!(err, "lodwright: cannot write output");
        return EXIT_FAILED;
    }
    doc.exit_code()
}

fn schema_command(args: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut kind = None;
    let mut version = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match *arg {
            "--schema-version" => {
                let Some(text) = iter.next() else {
                    return usage(err, "--schema-version needs a value");
                };
                let Ok(n) = text.parse::<u32>() else {
                    return usage(
                        err,
                        &format!("--schema-version must be a whole number, got {text:?}"),
                    );
                };
                version = Some(n);
            }
            name if kind.is_none() && DocumentKind::parse(name).is_some() => {
                kind = DocumentKind::parse(name);
            }
            other => return usage(err, &format!("unknown argument: {other}")),
        }
    }
    let Some(kind) = kind else {
        return usage(err, "schema takes a document name: plan or result");
    };
    let version = version.unwrap_or_else(|| kind.versions().last().copied().unwrap_or(1));
    match schema_json(kind, version) {
        Ok(text) => write_all(out, err, &text),
        Err(problem) => usage(err, &problem.to_string()),
    }
}

fn write_all(out: &mut dyn Write, err: &mut dyn Write, text: &str) -> u8 {
    match out.write_all(text.as_bytes()) {
        Ok(()) => EXIT_OK,
        Err(e) => {
            let _ = writeln!(err, "lodwright: cannot write output: {e}");
            EXIT_FAILED
        }
    }
}

fn usage(err: &mut dyn Write, problem: &str) -> u8 {
    let _ = writeln!(err, "lodwright: {problem}\nTry 'lodwright --help'.");
    EXIT_USAGE
}

/// Check that `folder` is an existing directory. Nothing inside it is read or printed.
fn doctor(folder: &Path, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if folder.is_dir() {
        write_all(out, err, "ok: data folder exists\n")
    } else {
        let _ = writeln!(
            err,
            "lodwright: the data folder does not exist or is not a directory"
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
        assert_eq!(out, format!("lodwright {}\n", env!("CARGO_PKG_VERSION")));
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
