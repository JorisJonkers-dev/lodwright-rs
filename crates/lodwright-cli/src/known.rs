// SPDX-License-Identifier: GPL-3.0-only
//! The known-differences list of `verify --known-differences FILE`: block paths with the reason a difference is
//! accepted and the release expected to remove it.
//!
//! One entry per line, three fields separated by tabs: the path relative to the folder (as `verify` prints it), the
//! release, the reason. Blank lines and lines starting with `#` are ignored. Paths compare in lower case with forward
//! slashes.

use std::collections::BTreeMap;

use lodwright_contract::{KnownDifference, VerifyDoc};

/// One entry of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The release expected to remove the difference.
    pub release: String,
    /// Why the difference is accepted.
    pub reason: String,
}

fn key(path: &str) -> String {
    path.trim().replace('\\', "/").to_ascii_lowercase()
}

/// Parse the list.
///
/// # Errors
/// The first line that does not have a path, a release and a reason, or a path listed twice, with its line number.
pub fn parse(text: &str) -> Result<BTreeMap<String, Entry>, String> {
    let mut list = BTreeMap::new();
    for (number, line) in text.lines().enumerate().map(|(i, l)| (i + 1, l)) {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let mut fields = line.splitn(3, '\t').map(str::trim);
        let (Some(path), Some(release), Some(reason)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(format!(
                "line {number}: needs a path, a release and a reason, separated by tabs"
            ));
        };
        if path.is_empty() || release.is_empty() || reason.is_empty() {
            return Err(format!("line {number}: a field is empty"));
        }
        let entry = Entry {
            release: release.to_owned(),
            reason: reason.to_owned(),
        };
        if list.insert(key(path), entry).is_some() {
            return Err(format!("line {number}: {path} is listed twice"));
        }
    }
    Ok(list)
}

/// Read and parse the list in `file`.
///
/// # Errors
/// A message naming the file and what is wrong with it.
pub fn read(file: &str) -> Result<BTreeMap<String, Entry>, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read the known-differences list {file}: {e}"))?;
    parse(&text).map_err(|e| format!("known-differences list {file}: {e}"))
}

/// Move the differences `list` names from `doc.differences` to `doc.known`, and list in `doc.stale` the entries no
/// difference matched.
pub fn apply(doc: &mut VerifyDoc, list: &BTreeMap<String, Entry>) {
    let mut matched = std::collections::BTreeSet::new();
    let (known, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut doc.differences)
        .into_iter()
        .partition(|d| list.contains_key(&key(&d.path)));
    doc.differences = rest;
    doc.known = known
        .into_iter()
        .map(|d| {
            let k = key(&d.path);
            let entry = &list[&k];
            matched.insert(k);
            KnownDifference {
                path: d.path,
                problem: d.problem,
                reason: entry.reason.clone(),
                release: entry.release.clone(),
            }
        })
        .collect();
    doc.stale = list
        .keys()
        .filter(|k| !matched.contains(*k))
        .cloned()
        .collect();
}

#[cfg(test)]
mod tests {
    use super::{apply, parse};
    use lodwright_contract::{VerifyDoc, VerifyFinding};
    use lodwright_testkit::{covers, ids};

    const LIST: &str = "# path\trelease\treason\n\
        meshes/landscape/lod/w/w.level4.x0.y0.nif\t0.7.0\twater outline not built yet\n\
        \n\
        Meshes\\Landscape\\LOD\\W\\W.level8.x0.y0.nif\t0.7.0\tsame\n";

    fn finding(path: &str) -> VerifyFinding {
        VerifyFinding {
            path: path.into(),
            problem: "water IoU 0.500 is under 0.95".into(),
        }
    }

    #[test]
    fn a_list_reads_its_entries_and_skips_comments_and_blank_lines() {
        let list = parse(LIST).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            list["meshes/landscape/lod/w/w.level8.x0.y0.nif"].reason,
            "same"
        );
    }

    #[test]
    fn a_line_without_three_fields_or_a_repeated_path_is_refused_with_its_number() {
        assert!(parse("a.nif\t0.7.0\n").unwrap_err().starts_with("line 1:"));
        assert!(
            parse("a.nif\t0.7.0\tx\nA.NIF\t0.7.0\ty\n")
                .unwrap_err()
                .contains("line 2: A.NIF is listed twice")
        );
    }

    #[test]
    fn a_listed_difference_becomes_known_and_a_listed_path_without_one_is_stale() {
        covers(&[ids::verify::folder::KNOWN_DIFFERENCES]);
        let mut doc = VerifyDoc::new();
        doc.differences = vec![
            finding("meshes/landscape/lod/w/w.level4.x0.y0.nif"),
            finding("meshes/landscape/lod/w/w.level16.x0.y0.nif"),
        ];
        apply(&mut doc, &parse(LIST).unwrap());
        assert_eq!(
            doc.differences,
            vec![finding("meshes/landscape/lod/w/w.level16.x0.y0.nif")]
        );
        assert_eq!(doc.known.len(), 1);
        assert_eq!(doc.known[0].release, "0.7.0");
        assert_eq!(
            doc.stale,
            vec!["meshes/landscape/lod/w/w.level8.x0.y0.nif".to_owned()]
        );
    }
}
