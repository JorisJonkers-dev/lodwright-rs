// SPDX-License-Identifier: GPL-3.0-only
//! [`generate`]: from a request to LOD files, a manifest and a result document.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use lodwright_contract::{
    BlockCounts, MANIFEST_FILE, Manifest, ManifestBlock, ManifestFile, Request, ResultDoc, Status,
    Written, sha256_digest,
};
use lodwright_core::terrain::{LevelSettings, TerrainSettings};
use lodwright_core::worldspace::Worldspace;
use lodwright_gen::FileRecord;
use serde::{Deserialize, Serialize};

use crate::load::{load_plugins_with, select_worlds};
use crate::{CancelToken, Error, Progress, ProgressEvent, Step};

/// What [`generate`] is asked to do: a `lodwright-request` document and the two switches that are not part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerateRequest {
    /// The request: data folder, plugins in load order, output folder, worlds and what to build.
    pub request: Request,
    /// Write into an output folder that is not empty and has no manifest.
    #[serde(default)]
    pub overwrite: bool,
    /// Write into an output folder inside a git working tree.
    #[serde(default)]
    pub allow_in_repo: bool,
}

impl GenerateRequest {
    /// `request` with both switches off.
    #[must_use]
    pub fn new(request: Request) -> Self {
        Self {
            request,
            overwrite: false,
            allow_in_repo: false,
        }
    }
}

/// How a [`generate`] call ended when it did not fail.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GenerateOutcome {
    /// The LOD is written (or was already up to date); the result document says what is in the output folder.
    Done {
        /// The `lodwright-result` document (`status` `ok`; `code` `generated` or `up_to_date`).
        result: Box<ResultDoc>,
    },
    /// The cancel token stopped the call. The output folder holds a manifest of the blocks that finished, so the
    /// next call with the same request keeps them and builds the rest.
    Cancelled,
}

/// The `request_digest` of a manifest a cancelled run leaves: it never matches a request, so the next run is not
/// `up_to_date`, but it keeps every block whose inputs did not change.
const INTERRUPTED: &str = "";

/// Cells per edge of an object block and of a tree block: the `level` of their progress events.
const OBJECT_BLOCK_CELLS: u32 = lodwright_gen::BLOCK_CELLS.unsigned_abs();
const TREE_BLOCK_CELLS: u32 = lodwright_core::trees::TREE_BLOCK_CELLS.unsigned_abs();

/// A finished object or tree block of `world`. Neither is kept from an earlier run, so `up_to_date` is false.
fn block_event(step: Step, world: &str, level: u32, done: u64, total: u64) -> ProgressEvent {
    ProgressEvent {
        level: Some(level),
        done: Some(done),
        total: Some(total),
        ..ProgressEvent::start(step, Some(world))
    }
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

/// `p` with its nearest existing ancestor resolved (links followed) and the rest appended: a not yet created output
/// folder under a link is judged by where the link points.
fn resolve(p: &Path) -> std::path::PathBuf {
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
}

/// Whether `path` (the output folder) lies inside a git working tree, judged both by its own path and by the folder
/// it resolves to: a link that sits in a working tree or points into one is refused.
fn inside_git_tree(path: &Path) -> bool {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    // the folder itself counts too: an output folder that is a working tree's root is inside it
    [absolute.clone(), resolve(&absolute)]
        .iter()
        .any(|p| p.ancestors().any(|dir| dir.join(".git").exists()))
}

fn refuse_output(request: &Request, allow_in_repo: bool) -> Result<(), Error> {
    let (out, data) = (
        Path::new(&request.output_dir),
        Path::new(&request.data_folder),
    );
    if !allow_in_repo && inside_git_tree(out) {
        return Err(Error::new(
            "output_in_repo",
            "refusing to write inside a git working tree",
        ));
    }
    if resolve(out).starts_with(resolve(data)) {
        return Err(Error::new(
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
                files: u64::try_from(of.len()).unwrap_or(u64::MAX),
                bytes: of.iter().map(|f| f.bytes).sum(),
            });
        }
    }
}

/// The manifest in `out`, if it can be read and every path it lists is a plain relative path: a manifest that names a
/// path outside the output folder is not trusted, as if there were none.
fn read_manifest(out: &Path) -> Option<Manifest> {
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).ok()?).ok()?;
    let plain = manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(
            manifest
                .blocks
                .iter()
                .flat_map(|b| b.files.iter().map(String::as_str)),
        )
        .all(lodwright_gen::is_plain_relative);
    plain.then_some(manifest)
}

/// Whether every file `manifest` lists is in `out` with the size and contents it was written with. A cancelled run
/// leaves a manifest whose `request_digest` never matches, so its files are checked again by the next run that does
/// compare digests, and a file changed behind the manifest's back is rebuilt instead of reported up to date.
fn up_to_date(out: &Path, manifest: &Manifest) -> bool {
    manifest.files.iter().all(|f| {
        std::fs::read(out.join(&f.path))
            .is_ok_and(|bytes| bytes.len() as u64 == f.bytes && sha256_digest(&bytes) == f.digest)
    })
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
            (bytes.len() as u64 == f.bytes && sha256_digest(&bytes) == f.digest).then(|| {
                FileRecord {
                    path: f.path.clone(),
                    bytes: f.bytes,
                    digest: f.digest.clone(),
                }
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

fn io_error(what: &str, e: &std::io::Error) -> Error {
    Error::new("output_failed", what).with_detail(e.to_string())
}

fn write_manifest(out: &Path, manifest: &Manifest) -> Result<(), Error> {
    let text = serde_json::to_string_pretty(manifest).map_err(|e| {
        Error::new("output_failed", "cannot write the manifest").with_detail(e.to_string())
    })? + "\n";
    lodwright_gen::write_output_file(out, MANIFEST_FILE, text.as_bytes())
        .map(|_| ())
        .map_err(|e| gen_error(&e))
}

/// The manifest a cancelled run leaves: the files this run finished and every file of the earlier run that is still
/// on disk (so nothing on disk is unlisted and the next run can clean it up), and the blocks of this run plus those
/// of the earlier run that this run did not replace, that are still on disk and that were built for the same request
/// (`request_digest`): a block of a different request is stale terrain and is not carried over. The next run keeps
/// what is still valid and removes the rest.
fn interrupted_manifest(
    version: &str,
    request_digest: &str,
    inputs_digest: &str,
    old: Option<&Manifest>,
    out: &Path,
    files: Vec<ManifestFile>,
    blocks: Vec<ManifestBlock>,
) -> Manifest {
    let built: std::collections::BTreeSet<&str> = blocks.iter().map(|b| b.key.as_str()).collect();
    let mut all: BTreeMap<String, ManifestFile> = BTreeMap::new();
    let mut by_key: BTreeMap<String, ManifestBlock> = BTreeMap::new();
    if let Some(old) = old {
        all.extend(
            old.files
                .iter()
                .filter(|f| out.join(&f.path).is_file())
                .map(|f| (f.path.clone(), f.clone())),
        );
        // a block of the old run stays listed only when it was built for this request, its files are on disk and
        // this run did not build it again; a block of another request is stale terrain
        let same_request = old.request_digest == request_digest;
        let same_inputs = old.inputs_digest == inputs_digest;
        by_key.extend(
            old.blocks
                .iter()
                .filter(|b| {
                    same_request
                        && same_inputs
                        && !built.contains(b.key.as_str())
                        && b.files.iter().all(|p| all.contains_key(p))
                })
                .map(|b| (b.key.clone(), b.clone())),
        );
    }
    all.extend(files.into_iter().map(|f| (f.path.clone(), f)));
    by_key.extend(blocks.into_iter().map(|b| (b.key.clone(), b)));
    Manifest::new(
        version,
        INTERRUPTED,
        inputs_digest,
        all.into_values().collect(),
    )
    .with_blocks(by_key.into_values().collect())
}

fn gen_error(e: &lodwright_gen::GenError) -> Error {
    match e {
        lodwright_gen::GenError::LinkInOutput { .. } => Error::new(
            "output_link",
            "refusing to write through a link inside the output folder: remove the link and run again",
        )
        .with_detail(e.to_string()),
        _ => Error::new("generate_failed", "LOD generation failed").with_detail(e.to_string()),
    }
}

fn load_order_error(e: &impl std::fmt::Display) -> Error {
    Error::new("load_order_invalid", "the load order cannot be read").with_detail(e.to_string())
}

/// Generate the LOD a request asks for into its output folder, with a `manifest.json` that lists every file.
///
/// The output folder must not be inside the data folder, nor inside a git working tree unless
/// [`GenerateRequest::allow_in_repo`] is set. A second call with the same request, unchanged plugins and every file
/// of the manifest still in place with its digest writes nothing and answers `up_to_date`; a call after a change
/// keeps every terrain block whose inputs did not change.
///
/// `progress` receives a start event per step and an event per finished terrain block. `cancel` is checked before
/// each terrain block, world and step; a cancelled call returns [`GenerateOutcome::Cancelled`] and leaves a
/// manifest of every file on disk (those this call finished and those an earlier call left) that the next call
/// resumes from: its blocks are those finished for this request, so terrain built for a different request is
/// rebuilt rather than kept. Nothing is printed and the process is never ended.
///
/// # Errors
/// An [`Error`] whose `code` says why: `request_invalid`, `output_in_repo`, `output_in_data`, `output_not_empty`,
/// `memory_limit`, `plugin_not_found`, `plugin_invalid`, `master_order`, `world_not_found`, `no_worlds`,
/// `load_order_invalid`, `assets_unreadable`, `generate_failed` or `output_failed`.
#[allow(clippy::too_many_lines)] // one pass: refuse, load, check manifest, generate, write manifest
pub fn generate(
    job: &GenerateRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<GenerateOutcome, Error> {
    let request = &job.request;
    let version = env!("CARGO_PKG_VERSION");
    request
        .validate()
        .map_err(|e| Error::new("request_invalid", "the request is not valid").with_detail(e))?;
    refuse_output(request, job.allow_in_repo)?;
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
            return Err(Error::new(
                "memory_limit",
                format!("the plugins need more than the {limit} MiB the request allows"),
            ));
        }
    }
    if cancel.is_cancelled() {
        return Ok(GenerateOutcome::Cancelled);
    }
    progress.report(&ProgressEvent::start(Step::ReadPlugins, None));
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
        return Ok(GenerateOutcome::Done {
            result: Box::new(doc),
        });
    }
    if old.is_none() && non_empty(out) && !job.overwrite {
        return Err(Error::new(
            "output_not_empty",
            "the output folder is not empty and has no manifest",
        ));
    }
    if cancel.is_cancelled() {
        return Ok(GenerateOutcome::Cancelled);
    }
    if old.is_some() {
        // the old manifest goes first, so an interrupted run leaves no manifest that claims changed files; its
        // blocks are kept where their inputs did not change and its other files are removed at the end
        let _ = std::fs::remove_file(out.join(MANIFEST_FILE));
    }
    let worlds = Worldspace::from_load_order(&loaded.plugins).map_err(|e| load_order_error(&e))?;
    let wanted: Vec<Option<&str>> = if request.worlds.is_empty() {
        vec![None]
    } else {
        request.worlds.iter().map(|w| Some(w.as_str())).collect()
    };
    let mut chosen: Vec<&Worldspace> = Vec::new();
    for w in wanted {
        for found in select_worlds(&worlds, w)? {
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
            lodwright_gen::FolderAssets::open(data, &request.plugins).map_err(|e| {
                Error::new("assets_unreadable", "the game's archives cannot be read").with_detail(e)
            })?,
        )
    } else {
        None
    };
    let resolver = lodwright_core::texture::TextureResolver::from_load_order(&loaded.plugins)
        .map_err(|e| load_order_error(&e))?;
    let settings = terrain_settings(request);
    std::fs::create_dir_all(out).map_err(|e| io_error("cannot create the output folder", &e))?;
    let reuse = |key: &str, inputs: &str| reusable(out, old.as_ref()?, key, inputs);
    let stop = || cancel.is_cancelled();
    let mut all: Vec<ManifestFile> = Vec::new();
    let mut blocks: Vec<ManifestBlock> = Vec::new();
    let mut counts: Option<BlockCounts> = None;
    let mut cancelled = false;
    'worlds: for world in chosen {
        let mut files: Vec<ManifestFile> = Vec::new();
        if request.terrain.enabled {
            if stop() {
                // the token may have been cancelled by the last event of an earlier world, after that world's files
                // were collected: they belong to the interrupted manifest too
                all.extend(files);
                cancelled = true;
                break 'worlds;
            }
            progress.report(&ProgressEvent::start(Step::Terrain, Some(world.name())));
            let mut warnings = Vec::new();
            let tiles = match (&assets, request.terrain.land_textures) {
                (Some(assets), true) => {
                    lodwright_gen::LandTiles::load(world, &resolver, assets, &mut warnings)
                }
                _ => lodwright_gen::LandTiles::empty(),
            };
            add_warnings(&mut doc, warnings);
            let report = {
                let shared = Mutex::new(&mut *progress);
                let block_done = |world: &str, level: u32, done: u64, of: u64, up_to_date: bool| {
                    if let Ok(mut p) = shared.lock() {
                        p.report(&ProgressEvent {
                            step: Step::Terrain,
                            world: Some(world.to_owned()),
                            level: Some(level),
                            done: Some(done),
                            total: Some(of),
                            up_to_date,
                        });
                    }
                };
                lodwright_gen::generate_terrain(
                    world,
                    &settings,
                    out,
                    &lodwright_gen::Options {
                        threads,
                        land: Some(&tiles),
                        progress: Some(&block_done),
                        water: request.terrain.water,
                        reuse: Some(&reuse),
                        cancel: Some(&stop),
                    },
                )
                .map_err(|e| gen_error(&e))?
            };
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
            if report.cancelled {
                all.extend(files);
                cancelled = true;
                break 'worlds;
            }
        }
        if let (Some(assets), true) = (&assets, request.objects.enabled) {
            if stop() {
                all.extend(files);
                cancelled = true;
                break 'worlds;
            }
            progress.report(&ProgressEvent::start(Step::Objects, Some(world.name())));
            let instances =
                lodwright_core::objects::visible_instances(&loaded.plugins, world.form_id())
                    .map_err(|e| load_order_error(&e))?;
            let instances =
                lodwright_gen::filter_instances(instances, &request.inclusion, &request.plugins)
                    .map_err(|e| {
                        Error::new(
                            "request_invalid",
                            "the request's inclusion rules are not valid",
                        )
                        .with_detail(e)
                    })?;
            let ground = |x: f32, y: f32| world.ground_height(x, y);
            let water = |x: f32, y: f32| {
                // source: cell-size-units
                #[allow(clippy::cast_possible_truncation)] // a cell number of a world fits an i32
                let cell = ((x / 4096.0).floor() as i32, (y / 4096.0).floor() as i32);
                world.water_height(cell.0, cell.1)
            };
            let surfaces = lodwright_gen::Surfaces {
                ground: request
                    .objects
                    .drop_under_terrain
                    .then_some(&ground as lodwright_gen::HeightAt),
                water: request
                    .objects
                    .drop_under_water
                    .then_some(&water as lodwright_gen::HeightAt),
            };
            let objects = lodwright_gen::generate_objects_with_progress(
                world.name(),
                &instances,
                assets,
                out,
                surfaces,
                &mut |done, total| {
                    progress.report(&block_event(
                        Step::Objects,
                        world.name(),
                        OBJECT_BLOCK_CELLS,
                        done,
                        total,
                    ));
                },
            )
            .map_err(|e| gen_error(&e))?;
            add_warnings(&mut doc, objects.warnings.clone());
            if request.objects.drop_under_terrain || request.objects.drop_under_water {
                let d = doc.dropped_faces.get_or_insert_with(Default::default);
                d.under_terrain += objects.dropped_under_terrain as u64;
                d.under_water += objects.dropped_under_water as u64;
            }
            files.extend(objects.files.iter().map(to_manifest_file));
        }
        if let (Some(assets), true) = (&assets, request.trees.enabled) {
            if stop() {
                all.extend(files);
                cancelled = true;
                break 'worlds;
            }
            progress.report(&ProgressEvent::start(Step::Trees, Some(world.name())));
            let bases = lodwright_core::trees::tree_bases(&loaded.plugins)
                .map_err(|e| load_order_error(&e))?;
            let refs =
                lodwright_core::trees::tree_references(&loaded.plugins, world.form_id(), &bases)
                    .map_err(|e| load_order_error(&e))?;
            let south_west = world.bounds().map_or((0, 0), |b| (b.min_x, b.min_y));
            let trees = lodwright_gen::generate_trees_with_progress(
                world.name(),
                south_west,
                &bases,
                &refs,
                &request.plugins,
                assets,
                out,
                &mut |done, total| {
                    progress.report(&block_event(
                        Step::Trees,
                        world.name(),
                        TREE_BLOCK_CELLS,
                        done,
                        total,
                    ));
                },
            )
            .map_err(|e| gen_error(&e))?;
            add_warnings(&mut doc, trees.warnings.clone());
            files.extend(trees.files.iter().map(to_manifest_file));
        }
        written_of(world.name(), &files, &mut doc);
        all.extend(files);
        if stop() {
            // a cancel that came with the last event of this world's last step is seen here, not ignored
            cancelled = true;
            break 'worlds;
        }
    }
    if cancelled {
        // no manifest step event: the interrupted manifest is written, not generated
        let manifest = interrupted_manifest(
            version,
            &digest,
            &loaded.inputs_digest,
            old.as_ref(),
            out,
            all,
            blocks,
        );
        write_manifest(out, &manifest)?;
        return Ok(GenerateOutcome::Cancelled);
    }
    progress.report(&ProgressEvent::start(Step::Manifest, None));
    if let Some(old) = &old {
        // files of the old run that this run did not write or keep
        let kept: std::collections::BTreeSet<&str> = all.iter().map(|f| f.path.as_str()).collect();
        for f in old.files.iter().filter(|f| !kept.contains(f.path.as_str())) {
            // the manifest's paths are plain (read_manifest); a folder on the way that became a link is left alone
            if lodwright_gen::linked_folder(out, &f.path).is_none() {
                let _ = std::fs::remove_file(out.join(&f.path));
            }
        }
    }
    doc.blocks = counts;
    let manifest = Manifest::new(version, &digest, &loaded.inputs_digest, all).with_blocks(blocks);
    write_manifest(out, &manifest)?;
    doc.manifest = Some(MANIFEST_FILE.to_owned());
    Ok(GenerateOutcome::Done {
        result: Box::new(doc),
    })
}
