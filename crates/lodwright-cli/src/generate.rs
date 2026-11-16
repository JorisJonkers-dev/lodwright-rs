// SPDX-License-Identifier: MPL-2.0
//! `generate`: from a request (a file, or the command line turned into one) to files, a manifest and a result.

use std::io::Write;
use std::path::Path;

use lodwright_contract::{
    BlockCounts, Capabilities, MANIFEST_FILE, Manifest, ManifestBlock, ManifestFile, Request,
    ResultDoc, Status, Written, peak_memory_mib,
};
use lodwright_core::terrain::{LevelSettings, TerrainSettings};
use lodwright_core::worldspace::Worldspace;
use lodwright_gen::FileRecord;

use crate::commands::{arguments_digest_of, count, failed_document, inside_git_tree, select};
use crate::failure::Failure;
use crate::load::{load_plugins, load_plugins_with};
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

fn terrain_settings(request: &Request) -> TerrainSettings {
    if request.terrain.levels.is_empty() {
        return TerrainSettings::default();
    }
    TerrainSettings {
        levels: request
            .terrain
            .levels
            .iter()
            .map(|l| LevelSettings {
                level: l.level,
                quality: l.quality,
                max_vertices: l.max_vertices as usize,
                diffuse_size: l.diffuse_size,
                normal_size: l.normal_size,
            })
            .collect(),
    }
}

fn refuse_output(request: &Request, allow_in_repo: bool) -> Result<(), Failure> {
    let (out, data) = (
        Path::new(&request.output_dir),
        Path::new(&request.data_folder),
    );
    if !allow_in_repo && inside_git_tree(out) {
        return Err(Failure::new(
            "output_in_repo",
            "refusing to write inside a git working tree; pass --allow-in-repo to override",
        ));
    }
    let resolve = |p: &Path| {
        // the nearest existing ancestor, resolved, then the rest appended: a not yet created output folder
        // inside the data folder is still caught
        let mut rest = Vec::new();
        let mut cur = p.to_path_buf();
        loop {
            if let Ok(real) = cur.canonicalize() {
                return rest.iter().rev().fold(real, |acc, c| acc.join(c));
            }
            let Some(name) = cur.file_name().map(std::ffi::OsStr::to_os_string) else {
                return p.to_path_buf();
            };
            rest.push(name);
            if !cur.pop() {
                return p.to_path_buf();
            }
        }
    };
    if resolve(out).starts_with(resolve(data)) {
        return Err(Failure::new(
            "output_in_data",
            "refusing to write inside the data folder: generate into a separate folder and install it from there",
        ));
    }
    Ok(())
}

/// Add warnings to the document, keeping at most twenty lines in all.
fn add_warnings(doc: &mut ResultDoc, warnings: Vec<String>) {
    for w in warnings {
        if doc.warnings.len() < 20 {
            doc.warnings.push(w);
        } else if doc.warnings.len() == 20 {
            doc.warnings.push("more warnings were left out".to_owned());
        }
    }
}

fn kind_of(path: &str) -> &'static str {
    if path.ends_with(".dlodsettings") {
        "settings"
    } else if path.contains("/trees/") {
        "tree_lod"
    } else if path.contains("/blocks/") {
        "object_lod"
    } else if path.to_ascii_lowercase().ends_with(".nif") {
        "terrain_mesh"
    } else {
        "terrain_texture"
    }
}

fn written_of(world: &str, files: &[ManifestFile], doc: &mut ResultDoc) {
    for kind in [
        "terrain_mesh",
        "terrain_texture",
        "settings",
        "object_lod",
        "tree_lod",
    ] {
        let of: Vec<_> = files.iter().filter(|f| kind_of(&f.path) == kind).collect();
        if !of.is_empty() {
            doc.written.push(Written {
                world: world.to_owned(),
                kind: kind.to_owned(),
                files: count(of.len()),
                bytes: of.iter().map(|f| f.bytes).sum(),
            });
        }
    }
}

fn read_manifest(out: &Path) -> Option<Manifest> {
    serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).ok()?).ok()
}

fn up_to_date(out: &Path, manifest: &Manifest) -> bool {
    manifest
        .files
        .iter()
        .all(|f| std::fs::metadata(out.join(&f.path)).is_ok_and(|m| m.len() == f.bytes))
}

/// The files of `old`'s block `key` when its inputs were `inputs` too and every file is still there unchanged
/// (same digest); the block can then be kept.
fn reusable(out: &Path, old: &Manifest, key: &str, inputs: &str) -> Option<Vec<FileRecord>> {
    let block = old
        .blocks
        .binary_search_by(|b| b.key.as_str().cmp(key))
        .ok()
        .map(|i| &old.blocks[i])
        .filter(|b| b.inputs == inputs)?;
    block
        .files
        .iter()
        .map(|path| {
            let f = old
                .files
                .binary_search_by(|f| f.path.as_str().cmp(path))
                .ok()
                .map(|i| &old.files[i])?;
            let bytes = std::fs::read(out.join(&f.path)).ok()?;
            (bytes.len() as u64 == f.bytes && lodwright_contract::sha256_digest(&bytes) == f.digest)
                .then(|| FileRecord {
                    path: f.path.clone(),
                    bytes: f.bytes,
                    digest: f.digest.clone(),
                })
        })
        .collect()
}

fn non_empty(out: &Path) -> bool {
    std::fs::read_dir(out).is_ok_and(|mut d| d.next().is_some())
}

fn to_manifest_file(f: &FileRecord) -> ManifestFile {
    ManifestFile {
        path: f.path.clone(),
        bytes: f.bytes,
        digest: f.digest.clone(),
    }
}

fn io_failure(what: &str, e: &std::io::Error) -> Failure {
    Failure::new("output_failed", format!("{what}: {e}"))
}

/// Run a validated request. `allow_in_repo` and `overwrite` are command line switches, not part of the request.
#[allow(clippy::too_many_lines)] // one pass: refuse, load, check manifest, generate, write manifest
fn run_request(
    request: &Request,
    allow_in_repo: bool,
    overwrite: bool,
    events: Option<&str>,
) -> Result<ResultDoc, Failure> {
    let version = env!("CARGO_PKG_VERSION");
    request
        .validate()
        .map_err(|e| Failure::new("request_invalid", e))?;
    refuse_output(request, allow_in_repo)?;
    let (data, out) = (
        Path::new(&request.data_folder),
        Path::new(&request.output_dir),
    );
    let read = lodwright_core::plugin::ReadOptions {
        objects: request.objects.enabled || request.trees.enabled,
        land_layers: request.terrain.land_textures,
    };
    if let Some(limit) = request.limits.memory_mib {
        let bytes: u64 = request
            .plugins
            .iter()
            .filter_map(|p| crate::load::find_plugin(data, p).ok())
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        // the parsed plugins and the rasters take a few times the file sizes
        if bytes.saturating_mul(4) / (1024 * 1024) > limit {
            return Err(Failure::new(
                "memory_limit",
                format!("the plugins need more than the {limit} MiB the request allows"),
            ));
        }
    }
    let loaded = load_plugins_with(data, &request.plugins, read)?;
    let digest = request.digest();
    let old = read_manifest(out);
    let mut doc = ResultDoc::new(version, Status::Ok, "generated", "LOD written");
    doc.inputs_digest = Some(loaded.inputs_digest.clone());
    doc.arguments_digest = Some(digest.clone());
    if let Some(old) = &old
        && old.request_digest == digest
        && old.inputs_digest == loaded.inputs_digest
        && up_to_date(out, old)
    {
        "up_to_date".clone_into(&mut doc.code);
        "the output folder already holds this request's files".clone_into(&mut doc.reason);
        written_of("", &old.files, &mut doc);
        doc.manifest = Some(MANIFEST_FILE.to_owned());
        return Ok(doc);
    }
    if old.is_none() && non_empty(out) && !overwrite {
        return Err(Failure::new(
            "output_not_empty",
            "the output folder is not empty and has no manifest: use an empty folder or pass --overwrite",
        ));
    }
    if old.is_some() {
        // the old manifest goes first, so an interrupted run leaves no manifest that claims changed files; its
        // blocks are kept where their inputs did not change and its other files are removed at the end
        let _ = std::fs::remove_file(out.join(MANIFEST_FILE));
    }
    let worlds = Worldspace::from_load_order(&loaded.plugins)
        .map_err(|e| Failure::new("load_order_invalid", e.to_string()))?;
    let wanted: Vec<Option<&str>> = if request.worlds.is_empty() {
        vec![None]
    } else {
        request.worlds.iter().map(|w| Some(w.as_str())).collect()
    };
    let mut chosen: Vec<&Worldspace> = Vec::new();
    for w in wanted {
        for found in select(&worlds, w)? {
            if !chosen.iter().any(|c| c.form_id() == found.form_id()) {
                chosen.push(found);
            }
        }
    }
    let threads = request.limits.threads.map_or_else(
        || std::thread::available_parallelism().map_or(1, |n| n.get().min(4)),
        |n| n as usize,
    );
    let need_assets =
        request.objects.enabled || request.trees.enabled || request.terrain.land_textures;
    let assets = if need_assets {
        Some(
            lodwright_gen::FolderAssets::open(data, &request.plugins)
                .map_err(|e| Failure::new("assets_unreadable", e))?,
        )
    } else {
        None
    };
    let resolver = lodwright_core::texture::TextureResolver::from_load_order(&loaded.plugins)
        .map_err(|e| Failure::new("load_order_invalid", e.to_string()))?;
    let settings = terrain_settings(request);
    std::fs::create_dir_all(out).map_err(|e| io_failure("cannot create the output folder", &e))?;
    let sink = events_sink(events)?;
    let progress = |world: &str, level: u32, done: u64, of: u64, up_to_date: bool| {
        if let Some(sink) = &sink
            && let Ok(mut w) = sink.lock()
        {
            let line = lodwright_contract::Event::Block {
                world: world.to_owned(),
                level,
                done,
                of,
                up_to_date,
            }
            .to_line();
            let _ = writeln!(w, "{line}");
        }
    };
    let reuse = |key: &str, inputs: &str| reusable(out, old.as_ref()?, key, inputs);
    let mut all: Vec<ManifestFile> = Vec::new();
    let mut blocks: Vec<ManifestBlock> = Vec::new();
    let mut counts: Option<BlockCounts> = None;
    for world in chosen {
        let mut files: Vec<ManifestFile> = Vec::new();
        if request.terrain.enabled {
            let mut warnings = Vec::new();
            let tiles = match (&assets, request.terrain.land_textures) {
                (Some(assets), true) => {
                    lodwright_gen::LandTiles::load(world, &resolver, assets, &mut warnings)
                }
                _ => lodwright_gen::LandTiles::empty(),
            };
            add_warnings(&mut doc, warnings);
            let report = lodwright_gen::generate_terrain(
                world,
                &settings,
                out,
                &lodwright_gen::Options {
                    threads,
                    land: Some(&tiles),
                    progress: Some(&progress),
                    water: request.terrain.water,
                    reuse: Some(&reuse),
                },
            )
            .map_err(|e| Failure::new("generate_failed", e.to_string()))?;
            files.extend(report.files.iter().map(to_manifest_file));
            let c = counts.get_or_insert_with(BlockCounts::default);
            for b in &report.blocks {
                if b.reused {
                    c.up_to_date += 1;
                } else {
                    c.generated += 1;
                }
            }
            blocks.extend(report.blocks.into_iter().map(|b| ManifestBlock {
                key: b.key,
                inputs: b.inputs,
                files: b.files.into_iter().map(|f| f.path).collect(),
            }));
        }
        if let (Some(assets), true) = (&assets, request.objects.enabled) {
            let instances =
                lodwright_core::objects::visible_instances(&loaded.plugins, world.form_id())
                    .map_err(|e| Failure::new("load_order_invalid", e.to_string()))?;
            let instances =
                lodwright_gen::filter_instances(instances, &request.inclusion, &request.plugins)
                    .map_err(|e| Failure::new("request_invalid", e))?;
            let objects = lodwright_gen::generate_objects(world.name(), &instances, assets, out)
                .map_err(|e| Failure::new("generate_failed", e.to_string()))?;
            add_warnings(&mut doc, objects.warnings.clone());
            files.extend(objects.files.iter().map(to_manifest_file));
        }
        if let (Some(assets), true) = (&assets, request.trees.enabled) {
            let bases = lodwright_core::trees::tree_bases(&loaded.plugins)
                .map_err(|e| Failure::new("load_order_invalid", e.to_string()))?;
            let refs =
                lodwright_core::trees::tree_references(&loaded.plugins, world.form_id(), &bases)
                    .map_err(|e| Failure::new("load_order_invalid", e.to_string()))?;
            let south_west = world.bounds().map_or((0, 0), |b| (b.min_x, b.min_y));
            let trees = lodwright_gen::generate_trees(
                world.name(),
                south_west,
                &bases,
                &refs,
                &request.plugins,
                assets,
                out,
            )
            .map_err(|e| Failure::new("generate_failed", e.to_string()))?;
            add_warnings(&mut doc, trees.warnings.clone());
            files.extend(trees.files.iter().map(to_manifest_file));
        }
        written_of(world.name(), &files, &mut doc);
        all.extend(files);
    }
    if let Some(old) = &old {
        // files of the old run that this run did not write or keep
        let kept: std::collections::BTreeSet<&str> = all.iter().map(|f| f.path.as_str()).collect();
        for f in old.files.iter().filter(|f| !kept.contains(f.path.as_str())) {
            let _ = std::fs::remove_file(out.join(&f.path));
        }
    }
    doc.blocks = counts;
    let manifest = Manifest::new(version, &digest, &loaded.inputs_digest, all).with_blocks(blocks);
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| Failure::new("output_failed", e.to_string()))?
        + "\n";
    let temp = out.join(format!("{MANIFEST_FILE}.tmp"));
    std::fs::write(&temp, text).map_err(|e| io_failure("cannot write the manifest", &e))?;
    std::fs::rename(&temp, out.join(MANIFEST_FILE))
        .map_err(|e| io_failure("cannot write the manifest", &e))?;
    doc.manifest = Some(MANIFEST_FILE.to_owned());
    Ok(doc)
}

type Sink = std::sync::Mutex<Box<dyn Write + Send>>;

fn events_sink(target: Option<&str>) -> Result<Option<Sink>, Failure> {
    Ok(match target {
        None => None,
        Some("-") => Some(std::sync::Mutex::new(Box::new(std::io::stderr()))),
        Some(file) => Some(std::sync::Mutex::new(Box::new(
            std::fs::File::create(file)
                .map_err(|e| io_failure("cannot open the events file", &e))?,
        ))),
    })
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

/// `generate`: from `--request FILE`, or from `--data`, plugins and `--out`; with neither it answers
/// `unsupported` and writes nothing.
pub fn generate(options: &Options) -> ResultDoc {
    let version = env!("CARGO_PKG_VERSION");
    let has_data = !options.data.as_os_str().is_empty();
    let request = if let Some(file) = &options.request {
        match std::fs::read(file)
            .map_err(|e| {
                Failure::new(
                    "request_unreadable",
                    format!("cannot read the request: {e}"),
                )
            })
            .and_then(|bytes| {
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|e| Failure::new("request_invalid", format!("not JSON: {e}")))?;
                lodwright_contract::check_request(&value)
                    .map_err(|e| Failure::new("request_invalid", e.to_string()))?;
                serde_json::from_value::<Request>(value)
                    .map_err(|e| Failure::new("request_invalid", e.to_string()))
            }) {
            Ok(r) => Some(Ok(r)),
            Err(f) => Some(Err(f)),
        }
    } else if has_data && !options.plugins.is_empty() && options.out.is_some() {
        Some(Ok(request_from_options(options)))
    } else {
        None
    };
    let mut doc = match request {
        Some(Ok(request)) => run_request(
            &request,
            options.allow_in_repo,
            options.overwrite,
            options.events.as_deref(),
        )
        .unwrap_or_else(|f| failed_document(options, &f)),
        Some(Err(f)) => failed_document(options, &f),
        None => {
            let mut doc = unsupported(version);
            if has_data && !options.plugins.is_empty() {
                // only read, to digest them
                doc = match load_plugins(&options.data, &options.plugins) {
                    Ok(loaded) => {
                        let mut d = doc;
                        d.inputs_digest = Some(loaded.inputs_digest);
                        d
                    }
                    Err(f) => ResultDoc::new(version, Status::Failed, f.code, &f.reason),
                };
            }
            if has_data || !options.plugins.is_empty() {
                doc.arguments_digest = Some(arguments_digest_of(options));
            }
            doc
        }
    };
    doc.peak_memory_mib = peak_memory_mib();
    doc
}
