// SPDX-License-Identifier: GPL-3.0-only
//! `generate`: the command line turned into a request (or a request file read), handed to
//! [`lodwright_api::generate`], and its outcome turned into a result document.

use std::io::Write;
use std::path::Path;

use lodwright_api::{CancelToken, GenerateOutcome, GenerateRequest, Progress, ProgressEvent};
use lodwright_contract::{Capabilities, Request, ResultDoc, Status, peak_memory_mib};

use crate::commands::{arguments_digest_of, failed_document};
use crate::failure::Failure;
use crate::options::Options;

/// The request the command line stands for.
fn request_from_options(options: &Options) -> Request {
    let plugins: Vec<&str> = options.plugins.iter().map(String::as_str).collect();
    let mut request = Request::new(
        &options.data.to_string_lossy(),
        &plugins,
        &options
            .out
            .as_deref()
            .unwrap_or(Path::new(""))
            .to_string_lossy(),
    );
    request.worlds = options.world.iter().cloned().collect();
    request.objects.enabled = !options.terrain_only;
    request.trees.enabled = !options.terrain_only && !options.no_trees;
    request.terrain.land_textures = !options.no_land_textures;
    request.terrain.water = !options.no_water;
    request.limits.threads = u32::try_from(options.threads).ok().filter(|n| *n > 0);
    request
}

/// `--events`: one `lodwright` event line per finished terrain, object or tree block, to a file or to stderr.
struct EventLines(Box<dyn Write + Send>);

impl Progress for EventLines {
    fn report(&mut self, event: &ProgressEvent) {
        if let Some(line) = event.block_line() {
            let _ = writeln!(self.0, "{}", line.to_line());
        }
    }
}

fn events_sink(target: Option<&str>) -> Result<Box<dyn Progress>, Failure> {
    Ok(match target {
        None => Box::new(lodwright_api::NoProgress),
        Some("-") => Box::new(EventLines(Box::new(std::io::stderr()))),
        Some(file) => Box::new(EventLines(Box::new(std::fs::File::create(file).map_err(
            |e| {
                Failure::new("output_failed", "cannot open the events file")
                    .with_detail(e.to_string())
            },
        )?))),
    })
}

/// Run a request through the library. The command line has no cancel, so a cancelled outcome cannot happen here.
fn run_request(request: Request, options: &Options) -> Result<ResultDoc, Failure> {
    let mut progress = events_sink(options.events.as_deref())?;
    let job = GenerateRequest {
        request,
        overwrite: options.overwrite,
        allow_in_repo: options.allow_in_repo,
    };
    match lodwright_api::generate(&job, progress.as_mut(), &CancelToken::new())? {
        GenerateOutcome::Done { result } => Ok(*result),
        GenerateOutcome::Cancelled => Err(Failure::new("cancelled", "generation was cancelled")),
    }
}

fn unsupported(version: &str) -> ResultDoc {
    let missing = Capabilities::default().missing();
    let mut doc = ResultDoc::new(
        version,
        Status::Unsupported,
        "capability_missing",
        "generate needs --data, plugins and --out, or --request FILE",
    );
    doc.missing_capabilities = missing;
    doc
}

pub(crate) fn read_request(file: &Path) -> Result<Request, Failure> {
    let bytes = std::fs::read(file).map_err(|e| {
        Failure::new("request_unreadable", "cannot read the request").with_detail(e.to_string())
    })?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| Failure::new("request_invalid", "not JSON").with_detail(e.to_string()))?;
    lodwright_contract::check_request(&value)
        .map_err(|e| Failure::new("request_invalid", e.to_string()))?;
    serde_json::from_value::<Request>(value)
        .map_err(|e| Failure::new("request_invalid", e.to_string()))
}

/// `generate`: from `--request FILE`, or from `--data`, plugins and `--out`; with neither it answers
/// `unsupported` and writes nothing.
pub fn generate(options: &Options) -> ResultDoc {
    let version = env!("CARGO_PKG_VERSION");
    let has_data = !options.data.as_os_str().is_empty();
    let request = if let Some(file) = &options.request {
        Some(read_request(file))
    } else if has_data && !options.plugins.is_empty() && options.out.is_some() {
        Some(Ok(request_from_options(options)))
    } else {
        None
    };
    let mut doc = match request {
        Some(Ok(request)) => {
            run_request(request, options).unwrap_or_else(|f| failed_document(options, &f))
        }
        Some(Err(f)) => failed_document(options, &f),
        None => {
            let mut doc = unsupported(version);
            if has_data && !options.plugins.is_empty() {
                // only read, to digest them
                doc = match lodwright_api::load::load_plugins(&options.data, &options.plugins) {
                    Ok(loaded) => {
                        let mut d = doc;
                        d.inputs_digest = Some(loaded.inputs_digest);
                        d
                    }
                    Err(f) => {
                        ResultDoc::new(version, Status::Failed, f.code, &crate::failure::reason(&f))
                    }
                };
            }
            if has_data || !options.plugins.is_empty() {
                doc.arguments_digest = Some(arguments_digest_of(options));
            }
            doc
        }
    };
    // a run that profiled its steps reset the kernel's peak at each one: its own maximum is the run's peak
    doc.peak_memory_mib = doc.peak_memory_mib.max(peak_memory_mib());
    doc
}
