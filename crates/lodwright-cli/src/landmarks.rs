// SPDX-License-Identifier: GPL-3.0-only
//! The `landmarks` command: the presence report of named landmarks in a generated output, and its comparison with a
//! saved report.

use std::io::Write;
use std::path::Path;

use lodwright_api::landmarks::{
    Comparison, LandmarksDoc, check_landmarks, compare, parse_landmarks,
};

use crate::{EXIT_FAILED, EXIT_OK, usage};

/// A landmark that was written is gone: the same exit code as a finding of `verify`.
const EXIT_LOST: u8 = 1;

/// `landmarks --request FILE --landmarks FILE [--output DIR] [--baseline FILE] [--write-baseline FILE]
/// [--allowed-drop X] [--json]`.
pub fn command(args: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let (mut request, mut list, mut output) = (None, None, None);
    let (mut baseline, mut save, mut json) = (None, None, false);
    let mut allowed_drop = 0.2_f64;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |what: &str| iter.next().copied().ok_or(format!("{what} needs a value"));
        let taken = match *arg {
            "--json" => {
                json = true;
                continue;
            }
            "--request" => value("--request").map(|v| request = Some(v)),
            "--landmarks" => value("--landmarks").map(|v| list = Some(v)),
            "--output" => value("--output").map(|v| output = Some(v)),
            "--baseline" => value("--baseline").map(|v| baseline = Some(v)),
            "--write-baseline" => value("--write-baseline").map(|v| save = Some(v)),
            "--allowed-drop" => value("--allowed-drop").and_then(|v| {
                v.parse::<f64>()
                    .ok()
                    .filter(|d| (0.0..=1.0).contains(d))
                    .map(|d| allowed_drop = d)
                    .ok_or("--allowed-drop needs a number from 0 to 1".to_owned())
            }),
            other => Err(format!("unknown argument: {other}")),
        };
        if let Err(problem) = taken {
            return usage(err, &problem);
        }
    }
    let (Some(request), Some(list)) = (request, list) else {
        return usage(err, "landmarks needs --request and --landmarks");
    };
    let request = match crate::generate::read_request(Path::new(request)) {
        Ok(r) => r,
        Err(failure) => return fail(err, &crate::failure::reason(&failure)),
    };
    let text = match std::fs::read_to_string(list) {
        Ok(t) => t,
        Err(e) => return fail(err, &format!("cannot read the landmark file: {e}")),
    };
    let landmarks = match parse_landmarks(&text) {
        Ok(l) => l,
        Err(problem) => return usage(err, &problem),
    };
    let baseline_doc = match baseline.map(|f| {
        std::fs::read(f)
            .map_err(|e| format!("cannot read the baseline: {e}"))
            .and_then(|b| {
                serde_json::from_slice::<LandmarksDoc>(&b)
                    .map_err(|e| format!("the baseline is not a landmark report: {e}"))
            })
    }) {
        None => None,
        Some(Ok(doc)) => Some(doc),
        Some(Err(problem)) => return usage(err, &problem),
    };
    let output = output.map_or_else(
        || Path::new(&request.output_dir).to_path_buf(),
        |o| Path::new(o).to_path_buf(),
    );
    let doc = match check_landmarks(&request, &output, &landmarks) {
        Ok(doc) => doc,
        Err(e) => return fail(err, &crate::failure::reason(&e)),
    };
    if let Some(file) = save {
        let written = serde_json::to_string_pretty(&doc)
            .map_err(|e| e.to_string())
            .and_then(|t| std::fs::write(file, t + "\n").map_err(|e| e.to_string()));
        if let Err(e) = written {
            return fail(err, &format!("cannot write the baseline: {e}"));
        }
    }
    let comparison = baseline_doc.map(|b| compare(&b, &doc, allowed_drop));
    let body = render(&doc, comparison.as_ref(), json);
    if out.write_all(body.as_bytes()).is_err() {
        return fail(err, "cannot write output");
    }
    match comparison {
        Some(c) if c.failed() => EXIT_LOST,
        _ => EXIT_OK,
    }
}

fn render(doc: &LandmarksDoc, comparison: Option<&Comparison>, json: bool) -> String {
    if json {
        let mut value = serde_json::to_value(doc).unwrap_or_default();
        if let (Some(c), Some(map)) = (comparison, value.as_object_mut()) {
            map.insert(
                "comparison".to_owned(),
                serde_json::json!({
                    "failed": c.failed(),
                    "lost": c.lost,
                    "returned": c.returned,
                    "added": c.added,
                    "removed": c.removed,
                }),
            );
        }
        serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
    } else {
        let mut text = doc.text();
        if let Some(c) = comparison {
            text.push_str(&c.text());
        }
        text
    }
}

fn fail(err: &mut dyn Write, text: &str) -> u8 {
    let _ = writeln!(err, "lodwright: {text}");
    EXIT_FAILED
}
