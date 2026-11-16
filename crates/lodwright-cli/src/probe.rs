// SPDX-License-Identifier: GPL-3.0-only
//! The `probe` command: numbers and structure of one file, never its content.

use std::io::{Read, Write};
use std::path::Path;

use lodwright_core::probe::{self, MAX_FILE_BYTES, Value, flat};

/// Which reader to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Nif,
    Dds,
    Settings,
    Dtl,
}

/// Parsed arguments of `probe`.
#[derive(Debug)]
struct Arguments {
    kind: Kind,
    file: String,
    json: bool,
    allow_in_repo: bool,
}

/// Why `probe` stopped: a usage mistake (exit 2) or a failed check (exit 1).
pub enum Outcome {
    /// The arguments were not understood.
    Usage(String),
    /// The file was refused, unreadable or not the expected format.
    Failed(String),
}

fn parse(args: &[&str]) -> Result<Arguments, String> {
    let mut positional = Vec::new();
    let mut json = false;
    let mut allow_in_repo = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match *arg {
            "--json" => json = true,
            "--format" => match iter.next().copied() {
                Some("json") => json = true,
                Some("text") => json = false,
                Some(other) => return Err(format!("unknown format {other:?}: use text or json")),
                None => return Err("--format needs a value".into()),
            },
            "--allow-in-repo" => allow_in_repo = true,
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            value => positional.push(value),
        }
    }
    let [kind, file] = positional.as_slice() else {
        return Err("probe takes a kind (nif, dds, settings or dtl) and one file".into());
    };
    let kind = match *kind {
        "nif" => Kind::Nif,
        "dds" => Kind::Dds,
        "settings" => Kind::Settings,
        "dtl" => Kind::Dtl,
        other => {
            return Err(format!(
                "unknown kind {other:?}: use nif, dds, settings or dtl"
            ));
        }
    };
    Ok(Arguments {
        kind,
        file: (*file).to_owned(),
        json,
        allow_in_repo,
    })
}

/// Whether `file` or any folder above it holds a `.git` entry.
fn inside_git_tree(file: &Path) -> bool {
    let absolute = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    absolute
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .any(|dir| dir.join(".git").exists())
}

fn read_report(arguments: &Arguments) -> Result<Value, String> {
    let path = Path::new(&arguments.file);
    let metadata =
        std::fs::metadata(path).map_err(|e| format!("cannot read the file: {}", e.kind()))?;
    if !metadata.is_file() {
        return Err("not a regular file".into());
    }
    let open =
        || std::fs::File::open(path).map_err(|e| format!("cannot read the file: {}", e.kind()));
    match arguments.kind {
        Kind::Settings | Kind::Dtl => {
            let mut head = Vec::new();
            open()?
                .take(flat::LEADING_BYTES as u64)
                .read_to_end(&mut head)
                .map_err(|e| format!("cannot read the file: {}", e.kind()))?;
            flat::probe(metadata.len(), &head).map_err(|e| e.to_string())
        }
        Kind::Nif | Kind::Dds => {
            if metadata.len() > MAX_FILE_BYTES {
                return Err("the file is too large to probe".into());
            }
            let mut data = Vec::new();
            open()?
                .take(MAX_FILE_BYTES)
                .read_to_end(&mut data)
                .map_err(|e| format!("cannot read the file: {}", e.kind()))?;
            let report = if arguments.kind == Kind::Nif {
                probe::nif::probe(&data)
            } else {
                probe::dds::probe(&data)
            };
            report.map_err(|e| e.to_string())
        }
    }
}

/// Run `probe` with `args` (after the command name), printing the report to `out`.
pub fn run(args: &[&str], out: &mut dyn Write) -> Result<(), Outcome> {
    let arguments = parse(args).map_err(Outcome::Usage)?;
    if !arguments.allow_in_repo && inside_git_tree(Path::new(&arguments.file)) {
        return Err(Outcome::Failed(
            "refusing to read a file inside a git working tree (a report about game data must not be taken from a repository checkout); \
             pass --allow-in-repo to override"
                .into(),
        ));
    }
    let report = read_report(&arguments).map_err(Outcome::Failed)?;
    let text = if arguments.json {
        report.to_json() + "\n"
    } else {
        report.to_text()
    };
    out.write_all(text.as_bytes())
        .map_err(|e| Outcome::Failed(format!("cannot write output: {e}")))
}

#[cfg(test)]
mod tests {
    use super::{Kind, parse};

    #[test]
    fn kind_and_file_are_required_and_flags_are_read_in_any_place() {
        let a = parse(&["--json", "nif", "x.nif", "--allow-in-repo"]).unwrap();
        assert_eq!(a.kind, Kind::Nif);
        assert_eq!(a.file, "x.nif");
        assert!(a.json && a.allow_in_repo);
        for (word, kind) in [
            ("dds", Kind::Dds),
            ("settings", Kind::Settings),
            ("dtl", Kind::Dtl),
        ] {
            assert_eq!(parse(&[word, "f"]).unwrap().kind, kind);
        }
        assert!(!parse(&["dds", "f"]).unwrap().json);
    }

    #[test]
    fn usage_mistakes_are_named() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["nif"]).is_err());
        assert!(parse(&["nif", "a", "b"]).is_err());
        assert!(parse(&["png", "a"]).unwrap_err().contains("unknown kind"));
        assert!(
            parse(&["nif", "a", "--bogus"])
                .unwrap_err()
                .contains("--bogus")
        );
        assert!(parse(&["nif", "a", "--format", "xml"]).is_err());
        assert!(parse(&["nif", "a", "--format"]).is_err());
        assert!(parse(&["nif", "a", "--format", "json"]).unwrap().json);
        assert!(
            !parse(&["nif", "a", "--json", "--format", "text"])
                .unwrap()
                .json
        );
    }
}
