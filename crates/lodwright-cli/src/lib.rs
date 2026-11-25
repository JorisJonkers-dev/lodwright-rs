// SPDX-License-Identifier: GPL-3.0-only
//! Command line of lodwright. The logic is here so tests can drive it without a process.
//!
//! Exit codes: 0 success, 1 failed (a result document is still printed with `--json`), 2 usage error or an
//! unsupported `--schema-version`, 3 unsupported (a capability is not built yet). `verify` has its own meanings for 1
//! and 3: 1 findings (an error in the folder or a difference from the reference), 3 a folder that cannot be read.
//!
//! With `--json`, result and error documents go to stdout; stderr carries human text only.

use std::io::Write;
use std::path::Path;

use lodwright_contract::{
    Channels, CorrelationBlock, CorrelationLevel, CorrelationReport, CorrelationSummary,
    DocumentKind, FarAtlas, FarMetricsLevel, FarMetricsReport, Handshake, ObjectBlock,
    ObjectReport, TerrainBlock, TerrainLevel, TerrainReport, VerifyDoc, VerifyFinding, VersionInfo,
    WaterBlock, WaterLevel, WaterProfileLevel, WaterReport, schema_json,
};

mod commands;
mod failure;
mod generate;
mod known;
mod landmarks;
mod options;
mod probe;

/// Exit code: success.
pub const EXIT_OK: u8 = 0;
/// Exit code: a check failed (for example the data folder is missing).
pub const EXIT_FAILED: u8 = 1;
/// Exit code: the arguments were not understood.
pub const EXIT_USAGE: u8 = 2;
/// Exit code: a capability is not built yet (`generate`).
pub const EXIT_UNSUPPORTED: u8 = 3;
/// Exit code of `verify`: the folder or the reference cannot be read (missing, not a folder, no permission).
pub const EXIT_UNREADABLE: u8 = 3;

const HELP: &str = "lodwright: clean-room terrain, object and tree LOD generator

Usage: lodwright [--help | --version [--json]]
       lodwright doctor <data-folder>
       lodwright plan --data <data-folder> (--plugin NAME ... | --load-order FILE) [--world NAME]
                      [--format text|json | --json] [--schema-version N] [--blocks]
       lodwright generate --data <data-folder> (--plugin NAME ... | --load-order FILE) --out <folder>
                      [--world NAME] [--threads N] [--terrain-only] [--no-land-textures] [--no-water] [--no-trees] [--overwrite] [--events FILE|-]
                      [--json] [--schema-version N]
       lodwright generate --request FILE [--allow-in-repo] [--overwrite] [--events FILE|-] [--json]
       lodwright verify <folder> [--reference <folder> [--water-iou X] [--height-tolerance U] [--known-differences FILE]] [--allow-in-repo] [--json]
       lodwright landmarks --request FILE --landmarks FILE [--output DIR] [--baseline FILE] [--write-baseline FILE]
                      [--allowed-drop X] [--json]
       lodwright schema <plan|result|request|manifest|verify|events|defaults|bands> [--schema-version N]
       lodwright contracts --json
       lodwright defaults --json
       lodwright bands <verify.json> ... [--json]
       lodwright probe <nif|dds|settings|dtl> <file> [--json | --format text|json] [--allow-in-repo]
       lodwright heightmap --data <data-folder> (--plugin NAME ... | --load-order FILE) --world NAME
                      --out <file.pgm> [--level N] [--allow-in-repo]

Commands:
  doctor <data-folder>  check that the folder exists; prints nothing taken from it
  plan                  read the plugins (in load order, masters first) and print the terrain LOD plan:
                        blocks per level, vertex and triangle counts, errors, output paths
  generate              write the terrain LOD (block meshes, textures, settings file) of the chosen worlds into
                        --out, which must not be inside the data folder, plus manifest.json; prints a result
                        document. --request reads a lodwright-request document instead (see `schema request`).
                        A second run with the same request and plugins does nothing. Without data, plugins and
                        --out, or a request, it answers `unsupported` (exit 3) and writes nothing. --events
                        writes one JSON line per finished terrain, object and tree block to FILE (- for stderr)
  verify                check a generated LOD folder against the game's formats: every NIF re-read and written back
                        byte for byte, indices, bounds, UVs, texture formats and mips, names, settings; with
                        --reference also compare names and block types with another folder, and measure per
                        block the water IoU (overlap of the water seen from above) and whether each object block is
                        exact (equal counts, every vertex within 0.05 units of one of the reference's). With
                        --water-iou X a terrain block whose water IoU is under X is a difference; the terrain
                        height of each block is compared too (largest and mean difference in game units), and
                        with --height-tolerance U a block whose largest difference is over U is a difference
  landmarks             look named landmarks (a tab separated file of references or positions with their models) up in a
                        generated output: written or not, in which block, shape (texture) and cell segment; --baseline
                        compares with a saved report and exits 1 when a landmark that was written is gone
  schema                print the JSON Schema of a document: plan, result, request, manifest, verify, events
                        (one progress event line), defaults or bands
  contracts --json      print the contracts this build speaks and their schema versions, as
                        {\"contracts\":[{\"name\":...,\"versions\":[...]}]}
  defaults --json       print every request option a user can set, with its default, range and tier
  bands                 read the `verify --json` reports of several runs of the same inputs and print, per
                        statistic, the minimum and maximum (three decimals): the tolerance bands of the A/B protocol
  probe                 print only structure and numbers of one file (NIF header and shapes, DDS header and channel
                        statistics, settings or .dtl size and leading integers); refuses a file inside a git working
                        tree unless --allow-in-repo
  heightmap             write the world's height raster as a 16-bit PGM (north up) for inspection

Plugins are read from the data folder; nothing is written there. Output is numbers and file names only.

Options:
  -h, --help     print this help
  -V, --version  print the version (with --json: tool, version and the contract versions spoken)
      --json     print JSON (an alias of --format json)

Exit codes: 0 ok, 1 failed, 2 usage error, 3 unsupported.
verify exits 0 clean, 1 findings, 2 usage error, 3 unreadable folder; a finding is an error in the folder or,
with --reference, a difference from the reference. A folder or reference inside a git working tree is a usage error
unless --allow-in-repo is given for that run.
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
        ["defaults", "--json"] => write_all(
            out,
            err,
            &(to_json(&lodwright_api::request_defaults()) + "\n"),
        ),
        ["defaults", ..] => usage(err, "defaults takes exactly one option: --json"),
        ["bands", rest @ ..] => bands_command(rest, out, err),
        ["doctor", folder] => doctor(Path::new(folder), out, err),
        ["plan", rest @ ..] => plan_command(rest, out, err),
        ["generate", rest @ ..] => generate_command(rest, out, err),
        ["schema", rest @ ..] => schema_command(rest, out, err),
        ["heightmap", rest @ ..] => heightmap_command(rest, out, err),
        ["verify", rest @ ..] => verify_command(rest, out, err),
        ["landmarks", rest @ ..] => landmarks::command(rest, out, err),
        ["probe", rest @ ..] => match probe::run(rest, out) {
            Ok(()) => EXIT_OK,
            Err(probe::Outcome::Usage(problem)) => usage(err, &problem),
            Err(probe::Outcome::Failed(problem)) => {
                let _ = writeln!(err, "lodwright: probe: {problem}");
                EXIT_FAILED
            }
        },
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
    let _ = writeln!(err, "lodwright: {}", failure::reason(failure));
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
    let doc = generate::generate(&options);
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
        return usage(
            err,
            "schema takes a document name: plan, result, request, manifest, verify or events",
        );
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

fn bands_command(args: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let json = args.contains(&"--json");
    let files: Vec<&str> = args.iter().copied().filter(|a| *a != "--json").collect();
    if let Some(other) = files.iter().find(|a| a.starts_with("--")) {
        return usage(err, &format!("unknown argument: {other}"));
    }
    if files.is_empty() {
        return usage(err, "bands needs the verify --json reports of the runs");
    }
    let mut docs = Vec::new();
    for file in files {
        let read = std::fs::read(file)
            .map_err(|e| e.to_string())
            .and_then(|b| {
                serde_json::from_slice::<serde_json::Value>(&b).map_err(|e| e.to_string())
            })
            .and_then(|v| {
                lodwright_contract::check_verify(&v).map_err(|e| e.to_string())?;
                serde_json::from_value::<VerifyDoc>(v).map_err(|e| e.to_string())
            });
        match read {
            Ok(doc) => docs.push(doc),
            Err(e) => {
                let _ = writeln!(err, "lodwright: bands: {file} is not a verify report: {e}");
                return EXIT_FAILED;
            }
        }
    }
    let doc = lodwright_contract::bands(&docs);
    let text = if json {
        to_json(&doc) + "\n"
    } else {
        use std::fmt::Write as _;
        doc.bands
            .iter()
            .fold(format!("{} runs\n", doc.runs), |mut text, b| {
                let _ = writeln!(
                    text,
                    "{} {} {} ({} runs)",
                    b.statistic, b.min, b.max, b.runs
                );
                text
            })
    };
    write_all(out, err, &text)
}

fn verify_command(args: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let (mut folder, mut reference, mut json, mut allow_in_repo) = (None, None, false, false);
    let mut threshold: Option<f64> = None;
    let mut known_file: Option<&str> = None;
    let mut tolerance: Option<f64> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match *arg {
            "--json" => json = true,
            "--allow-in-repo" => allow_in_repo = true,
            "--known-differences" => match iter.next() {
                Some(f) => known_file = Some(*f),
                None => return usage(err, "--known-differences needs a file"),
            },
            "--water-iou" => match iter.next().map(|t| t.parse::<f64>()) {
                Some(Ok(t)) if (0.0..=1.0).contains(&t) => threshold = Some(t),
                _ => return usage(err, "--water-iou needs a number from 0 to 1"),
            },
            "--height-tolerance" => match iter.next().map(|t| t.parse::<f64>()) {
                Some(Ok(t)) if t.is_finite() && t >= 0.0 => tolerance = Some(t),
                _ => {
                    return usage(
                        err,
                        "--height-tolerance needs a number of game units, 0 or more",
                    );
                }
            },
            "--reference" => match iter.next() {
                Some(r) => reference = Some(*r),
                None => return usage(err, "--reference needs a folder"),
            },
            other if folder.is_none() && !other.starts_with("--") => folder = Some(other),
            other => return usage(err, &format!("unknown argument: {other}")),
        }
    }
    let Some(folder) = folder else {
        return usage(err, "verify needs the folder to check");
    };
    if threshold.is_some() && reference.is_none() {
        return usage(err, "--water-iou needs --reference");
    }
    if tolerance.is_some() && reference.is_none() {
        return usage(err, "--height-tolerance needs --reference");
    }
    if known_file.is_some() && reference.is_none() {
        return usage(err, "--known-differences needs --reference");
    }
    let known_list = match known_file.map(known::read) {
        None => None,
        Some(Ok(list)) => Some(list),
        Some(Err(e)) => return usage(err, &e),
    };
    if let Some(code) = check_verify_folders(folder, reference, allow_in_repo, err) {
        return code;
    }
    let mut verification = lodwright_gen::verify_folder(Path::new(folder));
    let mut blocks = None;
    if let Some(reference) = reference {
        verification.differences =
            lodwright_gen::compare_folders(Path::new(folder), Path::new(reference));
        blocks = Some(lodwright_gen::compare_blocks(
            Path::new(folder),
            Path::new(reference),
        ));
    }
    let mut doc = verify_document(&verification);
    if let Some(blocks) = &blocks {
        add_block_reports(&mut doc, blocks, threshold);
        add_terrain_report(&mut doc, blocks, tolerance);
        add_far_metrics(&mut doc, blocks);
    }
    if let Some(reference) = reference {
        add_correlation_report(
            &mut doc,
            &lodwright_gen::compare_textures(Path::new(folder), Path::new(reference)),
        );
    }
    if let Some(list) = &known_list {
        known::apply(&mut doc, list);
    }
    let clean = doc.ok && doc.differences.is_empty();
    if json {
        let _ = writeln!(out, "{}", to_json(&doc));
    } else {
        let _ = writeln!(
            out,
            "{} terrain blocks, {} object blocks, {} tree blocks, {} textures, {} settings files: {} errors, {} differences",
            doc.terrain_blocks,
            doc.object_blocks,
            doc.tree_blocks,
            doc.textures,
            doc.settings,
            doc.errors.len(),
            doc.differences.len()
        );
        for f in doc.errors.iter().chain(&doc.differences) {
            let _ = writeln!(out, "{}: {}", f.path, f.problem);
        }
        print_block_reports(&doc, out);
    }
    if clean { EXIT_OK } else { EXIT_FAILED }
}

/// Refuse a folder or reference that cannot be read, or that lies inside a git working tree; `None` when both are fine.
fn check_verify_folders(
    folder: &str,
    reference: Option<&str>,
    allow_in_repo: bool,
    err: &mut dyn Write,
) -> Option<u8> {
    for (role, dir) in [("folder", Some(folder)), ("reference", reference)] {
        let Some(dir) = dir else { continue };
        if std::fs::read_dir(dir).is_err() {
            let _ = writeln!(
                err,
                "lodwright: verify: the {role} {dir} cannot be read as a folder"
            );
            return Some(EXIT_UNREADABLE);
        }
        // a folder generated from game data does not belong in a repository: refuse before reading a file of it
        let resolved = std::fs::canonicalize(dir).unwrap_or_else(|_| Path::new(dir).to_path_buf());
        if !allow_in_repo
            && [Path::new(dir), resolved.as_path()]
                .iter()
                .any(|d| commands::inside_git_tree(&d.join("x")))
        {
            return Some(usage(
                err,
                &format!(
                    "the {role} {dir} is inside a git working tree; no file was read. Generated LOD comes from \
                     game data and does not belong in a repository: move it out, or pass --allow-in-repo for this run"
                ),
            ));
        }
    }
    None
}

/// The `lodwright-verify` document of a verification.
fn verify_document(verification: &lodwright_gen::Verification) -> VerifyDoc {
    let findings = |list: &[lodwright_gen::Finding]| -> Vec<VerifyFinding> {
        list.iter()
            .map(|f| VerifyFinding {
                path: f.path.clone(),
                problem: f.problem.clone(),
            })
            .collect()
    };
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let mut doc = VerifyDoc::new();
    doc.ok = verification.errors.is_empty();
    doc.terrain_blocks = count(verification.terrain_blocks);
    doc.object_blocks = count(verification.object_blocks);
    doc.tree_blocks = count(verification.tree_blocks);
    doc.textures = count(verification.textures);
    doc.settings = count(verification.settings);
    doc.errors = findings(&verification.errors);
    doc.differences = findings(&verification.differences);
    doc
}

/// The `IoU` under which a block is counted in the water summary when `--water-iou` is not given (the G5 goal).
const DEFAULT_WATER_IOU: f64 = 0.95;

/// Three decimals, as the documents print measurements.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Add the per-block water and object reports, and with a threshold a difference per block under it.
fn add_block_reports(
    doc: &mut VerifyDoc,
    blocks: &lodwright_gen::BlockComparison,
    threshold: Option<f64>,
) {
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let limit = threshold.unwrap_or(DEFAULT_WATER_IOU);
    let per_block: Vec<WaterBlock> = blocks
        .water
        .iter()
        .map(|w| WaterBlock {
            path: w.path.clone(),
            iou: round3(w.iou),
        })
        .collect();
    let under = |w: &&lodwright_gen::WaterOverlap| w.iou < limit;
    if let Some(t) = threshold {
        for w in blocks.water.iter().filter(under) {
            doc.differences.push(VerifyFinding {
                path: w.path.clone(),
                problem: format!("water IoU {:.3} is under {t}", w.iou),
            });
        }
        doc.differences
            .sort_by(|a, b| (&a.path, &a.problem).cmp(&(&b.path, &b.problem)));
    }
    #[allow(clippy::cast_precision_loss, reason = "block counts are small")]
    let mean = (!blocks.water.is_empty()).then(|| {
        round3(blocks.water.iter().map(|w| w.iou).sum::<f64>() / blocks.water.len() as f64)
    });
    // per level, from the block's file name (`<world>.level<L>.x<X>.y<Y>.nif`)
    let mut levels: std::collections::BTreeMap<u32, Vec<f64>> = std::collections::BTreeMap::new();
    for w in &blocks.water {
        let level = w
            .path
            .rsplit(['/', '\\'])
            .next()
            .and_then(|name| name.split('.').find_map(|part| part.strip_prefix("level")))
            .and_then(|l| l.parse::<u32>().ok());
        if let Some(level) = level {
            levels.entry(level).or_default().push(w.iou);
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "block counts are small")]
    let per_level: Vec<WaterLevel> = levels
        .into_iter()
        .map(|(level, ious)| WaterLevel {
            level,
            blocks: count(ious.len()),
            under_threshold: count(ious.iter().filter(|i| **i < limit).count()),
            lowest: round3(ious.iter().copied().fold(f64::INFINITY, f64::min)),
            mean: round3(ious.iter().sum::<f64>() / ious.len() as f64),
        })
        .collect();
    let profile = water_profile_levels(blocks);
    doc.water = Some(WaterReport {
        per_level,
        profile,
        blocks: count(blocks.water.len()),
        threshold: limit,
        under_threshold: count(blocks.water.iter().filter(under).count()),
        lowest: blocks
            .water
            .iter()
            .map(|w| w.iou)
            .reduce(f64::min)
            .map(round3),
        mean,
        per_block,
    });
    let per_block: Vec<ObjectBlock> = blocks
        .objects
        .iter()
        .map(|o| ObjectBlock {
            path: o.path.clone(),
            vertices: count(o.vertices),
            reference_vertices: count(o.reference_vertices),
            triangles: count(o.triangles),
            reference_triangles: count(o.reference_triangles),
            unmatched_vertices: count(o.unmatched_vertices),
            exact: o.exact(),
        })
        .collect();
    doc.objects = Some(ObjectReport {
        blocks: count(per_block.len()),
        exact: count(per_block.iter().filter(|o| o.exact).count()),
        per_block,
    });
}

/// Add the error of the object blocks per level and the use of each atlas; nothing when there is neither.
#[allow(clippy::cast_precision_loss, reason = "block counts are small")]
fn add_far_metrics(doc: &mut VerifyDoc, blocks: &lodwright_gen::BlockComparison) {
    if blocks.far_errors.is_empty() && blocks.atlases.is_empty() {
        return;
    }
    let mut levels: std::collections::BTreeMap<u32, Vec<lodwright_gen::ShapeError>> =
        std::collections::BTreeMap::new();
    for b in &blocks.far_errors {
        if let Some(level) = level_of(&b.path) {
            levels.entry(level).or_default().push(b.error);
        }
    }
    let per_level = levels
        .into_iter()
        .map(|(level, list)| {
            let n = list.len() as f64;
            let largest = |f: fn(&lodwright_gen::ShapeError) -> f64| {
                round3(list.iter().map(f).fold(0.0, f64::max))
            };
            let mean = |f: fn(&lodwright_gen::ShapeError) -> f64| {
                round3(list.iter().map(f).sum::<f64>() / n)
            };
            let uv: Vec<f64> = list.iter().filter_map(|e| e.uv).collect();
            FarMetricsLevel {
                level,
                blocks: u64::try_from(list.len()).unwrap_or(u64::MAX),
                position_max: largest(|e| e.position_max),
                position_mean: mean(|e| e.position_mean),
                uv_max: (!uv.is_empty()).then(|| round3(uv.iter().copied().fold(0.0, f64::max))),
                uv_mean: (!uv.is_empty()).then(|| round3(uv.iter().sum::<f64>() / uv.len() as f64)),
                silhouette_max: largest(|e| e.silhouette),
                silhouette_mean: mean(|e| e.silhouette),
            }
        })
        .collect();
    let atlases = blocks
        .atlases
        .iter()
        .map(|a| FarAtlas {
            path: a.path.clone(),
            width: a.width,
            height: a.height,
            used: round3(a.used),
            wasted: round3(1.0 - a.used),
        })
        .collect();
    doc.far_metrics = Some(FarMetricsReport { per_level, atlases });
}

/// The `level<L>` of a block mesh's file name (`<world>.level<L>.x<X>.y<Y>.nif`).
fn level_of(path: &str) -> Option<u32> {
    path.rsplit(['/', '\\'])
        .next()
        .and_then(|name| name.split('.').find_map(|part| part.strip_prefix("level")))
        .and_then(|l| l.parse::<u32>().ok())
}

/// The shape of the water per level: means over the blocks with water.
fn water_profile_levels(blocks: &lodwright_gen::BlockComparison) -> Vec<WaterProfileLevel> {
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let mut shapes: std::collections::BTreeMap<u32, Vec<lodwright_gen::WaterProfile>> =
        std::collections::BTreeMap::new();
    for w in &blocks.water {
        if let Some(level) = level_of(&w.path) {
            shapes.entry(level).or_default().push(w.profile);
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "block counts are small")]
    shapes
        .into_iter()
        .map(|(level, list)| {
            let n = list.len() as f64;
            let mean = |f: fn(&lodwright_gen::WaterProfile) -> f64| {
                round3(list.iter().map(f).sum::<f64>() / n)
            };
            let agreements: Vec<f64> = list.iter().filter_map(|p| p.height_agreement).collect();
            WaterProfileLevel {
                level,
                blocks: count(list.len()),
                depth_mine: mean(|p| p.depth_mine),
                depth_theirs: mean(|p| p.depth_theirs),
                wall_mine: mean(|p| p.wall_mine),
                wall_theirs: mean(|p| p.wall_theirs),
                vertices_mine: round3(list.iter().map(|p| p.vertices_mine).sum::<f64>() / n),
                vertices_theirs: round3(list.iter().map(|p| p.vertices_theirs).sum::<f64>() / n),
                height_agreement: (!agreements.is_empty())
                    .then(|| round3(agreements.iter().sum::<f64>() / agreements.len() as f64)),
            }
        })
        .collect()
}

/// The largest height difference over which a terrain block counts in the summary when `--height-tolerance` is not
/// given, in game units.
const DEFAULT_HEIGHT_TOLERANCE: f64 = 1.0;

/// Add the terrain height report, and with a tolerance a difference per block over it.
fn add_terrain_report(
    doc: &mut VerifyDoc,
    blocks: &lodwright_gen::BlockComparison,
    tolerance: Option<f64>,
) {
    let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
    let limit = tolerance.unwrap_or(DEFAULT_HEIGHT_TOLERANCE);
    // compared as reported (three decimals): a difference of exactly the tolerance is not over it, whatever the last bits of the
    // interpolation say
    let over = |t: &&lodwright_gen::TerrainHeightDifference| round3(t.max) > limit;
    if let Some(t) = tolerance {
        for b in blocks.terrain.iter().filter(over) {
            doc.differences.push(VerifyFinding {
                path: b.path.clone(),
                problem: format!(
                    "terrain height differs by up to {:.3} units (mean {:.3}), over {t}",
                    b.max, b.mean
                ),
            });
        }
        doc.differences
            .sort_by(|a, b| (&a.path, &a.problem).cmp(&(&b.path, &b.problem)));
    }
    let mut levels: std::collections::BTreeMap<u32, Vec<&lodwright_gen::TerrainHeightDifference>> =
        std::collections::BTreeMap::new();
    for b in &blocks.terrain {
        if let Some(level) = level_of(&b.path) {
            levels.entry(level).or_default().push(b);
        }
    }
    #[allow(clippy::cast_precision_loss, reason = "block counts are small")]
    let per_level: Vec<TerrainLevel> = levels
        .into_iter()
        .map(|(level, list)| TerrainLevel {
            level,
            blocks: count(list.len()),
            over_tolerance: count(list.iter().filter(|b| round3(b.max) > limit).count()),
            max: round3(list.iter().map(|b| b.max).fold(0.0, f64::max)),
            mean: round3(list.iter().map(|b| b.mean).sum::<f64>() / list.len() as f64),
        })
        .collect();
    #[allow(clippy::cast_precision_loss, reason = "block counts are small")]
    let mean = (!blocks.terrain.is_empty()).then(|| {
        round3(blocks.terrain.iter().map(|b| b.mean).sum::<f64>() / blocks.terrain.len() as f64)
    });
    doc.terrain = Some(TerrainReport {
        blocks: count(blocks.terrain.len()),
        tolerance: limit,
        over_tolerance: count(blocks.terrain.iter().filter(over).count()),
        max: blocks
            .terrain
            .iter()
            .map(|b| b.max)
            .reduce(f64::max)
            .map(round3),
        mean,
        per_block: blocks
            .terrain
            .iter()
            .map(|b| TerrainBlock {
                path: b.path.clone(),
                max: round3(b.max),
                mean: round3(b.mean),
            })
            .collect(),
        per_level,
    });
}

fn channels(values: [f64; 3]) -> Channels {
    Channels {
        r: round3(values[0]),
        g: round3(values[1]),
        b: round3(values[2]),
    }
}

/// Mean and lowest per channel of `sets`, unrounded inputs; `None` for no block.
#[allow(clippy::cast_precision_loss, reason = "block counts are small")]
fn correlation_summary(sets: &[[f64; 3]]) -> Option<CorrelationSummary> {
    if sets.is_empty() {
        return None;
    }
    let per = |f: &dyn Fn(&mut dyn Iterator<Item = f64>) -> f64| {
        channels(std::array::from_fn(|c| f(&mut sets.iter().map(|s| s[c]))))
    };
    Some(CorrelationSummary {
        blocks: u64::try_from(sets.len()).unwrap_or(u64::MAX),
        mean: per(&|i| i.sum::<f64>() / sets.len() as f64),
        lowest: per(&|i| i.fold(f64::INFINITY, f64::min)),
    })
}

/// The diffuse and the normal-map correlations of one level.
type CorrelationSets = (Vec<[f64; 3]>, Vec<[f64; 3]>);

/// Add the per-channel correlation of the terrain textures with the reference's.
fn add_correlation_report(doc: &mut VerifyDoc, textures: &[lodwright_gen::TextureCorrelation]) {
    let diffuse: Vec<[f64; 3]> = textures.iter().map(|t| t.diffuse).collect();
    let normals: Vec<[f64; 3]> = textures.iter().filter_map(|t| t.normals).collect();
    let mut levels: std::collections::BTreeMap<u32, CorrelationSets> =
        std::collections::BTreeMap::new();
    for t in textures {
        if let Some(level) = level_of(&t.path) {
            let entry = levels.entry(level).or_default();
            entry.0.push(t.diffuse);
            entry.1.extend(t.normals);
        }
    }
    let per_level = levels
        .into_iter()
        .filter_map(|(level, (d, n))| {
            Some(CorrelationLevel {
                level,
                diffuse: correlation_summary(&d)?,
                normals: correlation_summary(&n),
            })
        })
        .collect();
    doc.correlation = Some(CorrelationReport {
        blocks: u64::try_from(textures.len()).unwrap_or(u64::MAX),
        diffuse: correlation_summary(&diffuse),
        normals: correlation_summary(&normals),
        per_block: textures
            .iter()
            .map(|t| CorrelationBlock {
                path: t.path.clone(),
                diffuse: channels(t.diffuse),
                normals: t.normals.map(channels),
            })
            .collect(),
        per_level,
    });
}

/// `r/g/b` of a per-channel number, three decimals each.
fn rgb(c: &Channels) -> String {
    format!("{:.3}/{:.3}/{:.3}", c.r, c.g, c.b)
}

fn print_correlation_line(out: &mut dyn Write, label: &str, kind: &str, s: &CorrelationSummary) {
    let _ = writeln!(
        out,
        "{label}: {kind} {} blocks, mean r/g/b {}, lowest {}",
        s.blocks,
        rgb(&s.mean),
        rgb(&s.lowest)
    );
}

/// The summary lines of the texture correlation: all blocks, then one per level.
fn print_correlation(c: &CorrelationReport, out: &mut dyn Write) {
    for (kind, summary) in [("diffuse", &c.diffuse), ("normals", &c.normals)] {
        if let Some(s) = summary {
            print_correlation_line(out, "texture correlation", kind, s);
        }
    }
    for l in &c.per_level {
        let label = format!("texture correlation level {}", l.level);
        print_correlation_line(out, &label, "diffuse", &l.diffuse);
        if let Some(n) = &l.normals {
            print_correlation_line(out, &label, "normals", n);
        }
    }
}

/// The summary lines of the block reports, and one line per object block that is not exact.
fn print_block_reports(doc: &VerifyDoc, out: &mut dyn Write) {
    if let Some(w) = &doc.water {
        let number = |v: Option<f64>| v.map_or_else(|| "-".to_owned(), |v| format!("{v:.3}"));
        let _ = writeln!(
            out,
            "water: {} blocks with water, {} under {}, lowest {}, mean {}",
            w.blocks,
            w.under_threshold,
            w.threshold,
            number(w.lowest),
            number(w.mean)
        );
        for l in &w.per_level {
            let _ = writeln!(
                out,
                "water level {}: {} blocks, {} under {}, lowest {:.3}, mean {:.3}",
                l.level, l.blocks, l.under_threshold, w.threshold, l.lowest, l.mean
            );
        }
        for p in &w.profile {
            let agreement = p
                .height_agreement
                .map_or_else(|| "-".to_owned(), |v| format!("{v:.3}"));
            let _ = writeln!(
                out,
                "water profile level {}: {} blocks, depth {:.1} (reference {:.1}), wall area {:.0} (reference {:.0}), vertices {:.1} (reference {:.1}), height agreement {}",
                p.level,
                p.blocks,
                p.depth_mine,
                p.depth_theirs,
                p.wall_mine,
                p.wall_theirs,
                p.vertices_mine,
                p.vertices_theirs,
                agreement
            );
        }
    }
    if let Some(t) = &doc.terrain {
        let number = |v: Option<f64>| v.map_or_else(|| "-".to_owned(), |v| format!("{v:.3}"));
        let _ = writeln!(
            out,
            "terrain: {} blocks compared, {} over {}, largest {}, mean {}",
            t.blocks,
            t.over_tolerance,
            t.tolerance,
            number(t.max),
            number(t.mean)
        );
        for l in &t.per_level {
            let _ = writeln!(
                out,
                "terrain level {}: {} blocks, {} over {}, largest {:.3}, mean {:.3}",
                l.level, l.blocks, l.over_tolerance, t.tolerance, l.max, l.mean
            );
        }
    }
    if let Some(c) = &doc.correlation {
        print_correlation(c, out);
    }
    for k in &doc.known {
        let _ = writeln!(
            out,
            "{}: known: {} ({}; expected gone in {})",
            k.path, k.problem, k.reason, k.release
        );
    }
    for path in &doc.stale {
        let _ = writeln!(
            out,
            "{path}: stale: listed as a known difference but no longer differs"
        );
    }
    if let Some(o) = &doc.objects {
        let _ = writeln!(out, "object blocks: {} of {} exact", o.exact, o.blocks);
        for b in o.per_block.iter().filter(|b| !b.exact) {
            let _ = writeln!(
                out,
                "{}: not exact: {} vertices (reference {}), {} triangles (reference {}), {} vertices without a reference vertex within 0.05",
                b.path,
                b.vertices,
                b.reference_vertices,
                b.triangles,
                b.reference_triangles,
                b.unmatched_vertices
            );
        }
    }
    print_far_metrics(doc, out);
}

/// The far-object error per level and the use of each atlas, when the report has them.
fn print_far_metrics(doc: &VerifyDoc, out: &mut dyn Write) {
    if let Some(m) = &doc.far_metrics {
        for l in &m.per_level {
            let uv = l
                .uv_mean
                .map_or_else(|| "unknown".to_owned(), |u| u.to_string());
            let _ = writeln!(
                out,
                "far objects level {}: {} blocks, position error max {} mean {}, UV error mean {uv}, silhouette error max {} mean {}",
                l.level,
                l.blocks,
                l.position_max,
                l.position_mean,
                l.silhouette_max,
                l.silhouette_mean
            );
        }
        for a in &m.atlases {
            let _ = writeln!(
                out,
                "{}: {}x{}, {} used, {} wasted",
                a.path, a.width, a.height, a.used, a.wasted
            );
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
    use lodwright_testkit::{covers, ids};

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
        covers(&[ids::cli::commands::VERSION_AND_HELP]);
        let (code, out, _) = go(&["--version"]);
        assert_eq!(code, EXIT_OK);
        assert_eq!(out, format!("lodwright {}\n", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn help_lists_the_doctor_command() {
        covers(&[ids::cli::commands::VERSION_AND_HELP]);
        let (code, out, _) = go(&["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.contains("doctor <data-folder>"));
    }

    #[test]
    fn no_arguments_is_a_usage_error() {
        covers(&[ids::cli::commands::USAGE_ERRORS]);
        assert_eq!(go(&[]).0, EXIT_USAGE);
    }

    #[test]
    fn unknown_argument_is_a_usage_error_naming_it() {
        covers(&[ids::cli::commands::USAGE_ERRORS]);
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
        covers(&[ids::cli::commands::DOCTOR]);
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
        covers(&[ids::cli::commands::DOCTOR]);
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
