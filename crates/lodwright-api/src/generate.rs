// SPDX-License-Identifier: GPL-3.0-only
//! [`generate`]: from a request to LOD files, a manifest and a result document.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use lodwright_contract::{
    BlockCounts, MANIFEST_FILE, Manifest, ManifestBlock, ManifestFile, Request, ResultDoc, Status,
    WARNINGS_FILE, WarningRecord, WarningsFile, Written, sha256_digest,
};
use lodwright_core::terrain::{LevelSettings, TerrainSettings};
use lodwright_core::worldspace::Worldspace;
use lodwright_gen::FileRecord;
use serde::{Deserialize, Serialize};

use crate::load::{load_plugins_with, select_worlds};
use crate::progress::RunTracker;
use crate::{CancelToken, Error, Progress, Step};

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
    /// The cancel token stopped the call. The output folder is untouched; the staging folder next to it
    /// (`<output>.lodwright-staging`) holds a checkpoint of the blocks that finished, so the next call with the same
    /// request keeps them and builds the rest.
    Cancelled,
}

/// Cells per edge of an object block and of a tree block: the `level` of their progress events.
const OBJECT_BLOCK_CELLS: u32 = lodwright_gen::BLOCK_CELLS.unsigned_abs();
const TREE_BLOCK_CELLS: u32 = lodwright_core::trees::TREE_BLOCK_CELLS.unsigned_abs();

/// What a run that writes specular in the alpha channel has to say about the exponents of the land textures: that
/// none exists (the alpha stays opaque), or which textures have none (their specular counts as the lowest value).
/// `SDefaultLandDiffuseTexture` from the game's `Fallout_default.ini` (in the folder above the data folder), else the
/// documented default.
fn default_land_texture(data: &Path) -> String {
    let ini = data.parent().and_then(|game| {
        std::fs::read_dir(game).ok()?.flatten().find(|e| {
            e.file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Fallout_default.ini")
        })
    });
    ini.and_then(|e| std::fs::read_to_string(e.path()).ok())
        .and_then(|text| lodwright_gen::default_land_texture_from_ini(&text))
        .unwrap_or_else(|| lodwright_gen::DEFAULT_LAND_TEXTURE.to_owned())
}

fn specular_warnings(tiles: &lodwright_gen::LandTiles) -> Vec<String> {
    if tiles.specular_range().is_none() {
        return vec![
            "specular alpha: no land texture of the load order has a specular exponent, so the normal maps stay opaque"
                .to_owned(),
        ];
    }
    tiles
        .without_specular()
        .into_iter()
        .map(|id| format!("specular alpha: land texture {id:08X} has no specular exponent, counted as the lowest"))
        .collect()
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
                specular_alpha: l.specular_alpha,
                skirts: l.skirts,
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

/// Add warnings to the document, keeping at most twenty lines and a last line that counts the rest.
fn add_warnings(doc: &mut ResultDoc, warnings: Vec<String>) {
    const LIMIT: usize = 20;
    const MORE: &str = " more warnings were left out";
    let mut left_out: usize = doc
        .warnings
        .last()
        .and_then(|w| w.strip_suffix(MORE))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    if left_out > 0 {
        doc.warnings.pop();
    }
    for w in warnings {
        // a warning that two steps of a run both produce is said once
        if doc.warnings.contains(&w) {
            continue;
        }
        if doc.warnings.len() < LIMIT {
            doc.warnings.push(w);
        } else {
            left_out += 1;
        }
    }
    if left_out > 0 {
        doc.warnings.push(format!("{left_out}{MORE}"));
    }
}

/// A record for a warning line of a step that does not build records itself: the kind comes from the line's wording.
fn classify(world: Option<&str>, text: &str) -> WarningRecord {
    // the path is the word that names the file: "land texture textures/x.dds not found"
    let path = |head: &str| head.rsplit(' ').next().unwrap_or(head).to_owned();
    let (kind, subject) = if let Some((head, _)) = text.split_once(" not found") {
        if head.starts_with("land texture") {
            ("land_texture_not_found", path(head))
        } else {
            ("texture_not_found", path(head))
        }
    } else if let Some((head, _)) = text.split_once(" found but not readable") {
        ("texture_unreadable", path(head))
    } else if text.starts_with("specular alpha:") {
        ("specular_exponent_missing", text.to_owned())
    } else {
        ("other", text.to_owned())
    };
    WarningRecord::new(kind, world, None, [subject], None, text.to_owned())
}

/// Warning lines of a step: into the result's short list, and each as a record.
fn note(doc: &mut ResultDoc, records: &mut Vec<WarningRecord>, world: &str, lines: Vec<String>) {
    records.extend(lines.iter().map(|l| classify(Some(world), l)));
    add_warnings(doc, lines);
}

/// Count the records per kind into the document and write the file of them into `stage`; nothing is written when
/// the run had no warning.
fn write_warnings(
    doc: &mut ResultDoc,
    stage: &Path,
    records: Vec<WarningRecord>,
) -> Result<(), Error> {
    if records.is_empty() {
        return Ok(());
    }
    let file = WarningsFile::new(records);
    let text = serde_json::to_string_pretty(&file).map_err(|e| {
        Error::new("output_failed", "cannot write the warnings file").with_detail(e.to_string())
    })? + "\n";
    lodwright_gen::write_output_file(stage, WARNINGS_FILE, text.as_bytes()).map_err(|e| {
        Error::new("output_failed", "cannot write the warnings file").with_detail(e.to_string())
    })?;
    doc.warning_counts = file.counts;
    doc.warnings_file = Some(WARNINGS_FILE.to_owned());
    Ok(())
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
    crate::staging::read_manifest_file(out, MANIFEST_FILE)
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

/// The smallest light radius (game units) that gets a far light point (`objects.far_lights`): street lamps and
/// building lights, not candles and small fixtures. Own design.
const FAR_LIGHT_MIN_RADIUS: u32 = 256;

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
/// `load_order_invalid`, `assets_unreadable`, `generate_failed`, `pack_failed`, `pack_too_large` or `output_failed`.
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
    // the parsed plugins and the rasters take a few times the file sizes; the same estimate stands in for the
    // resident size where the platform does not report it
    let plugins_mib = request.limits.memory_mib.map(|_| {
        let bytes: u64 = request
            .plugins
            .iter()
            .filter_map(|p| crate::load::find_plugin(data, p).ok())
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        bytes.saturating_mul(4) / (1024 * 1024)
    });
    if let (Some(limit), Some(need)) = (request.limits.memory_mib, plugins_mib)
        && need > limit
    {
        return Err(Error::new(
            "memory_limit",
            format!("the plugins need more than the {limit} MiB the request allows"),
        ));
    }
    if cancel.is_cancelled() {
        return Ok(GenerateOutcome::Cancelled);
    }
    let mut run = RunTracker::new();
    let clock = crate::profile::StepClock::new();
    progress.report(&run.start(Step::ReadPlugins, None));
    clock.begin("read_plugins", None);
    let loaded = load_plugins_with(data, &request.plugins, read)?;
    clock.begin("setup", None);
    let digest = request.digest();
    // an output folder that is a link is replaced where it points: the swap renames the folder, never the link
    let target = std::fs::canonicalize(out).unwrap_or_else(|_| out.to_path_buf());
    crate::staging::recover_swap(&target)?;
    let mut old = read_manifest(out);
    // an output written by another version of lodwright is rebuilt whole: a fix of what a version writes (#294)
    // must reach folders that already hold the same request's files
    // (#422) and so is one written by another build of the same version: two builds between releases both carry the
    // version number, and the later one may write different files
    let same_version = old
        .as_ref()
        .is_none_or(|m| m.tool.version == version && m.build.as_deref() == Some(crate::BUILD));
    if !same_version && let Some(m) = old.as_mut() {
        m.blocks.clear();
    }
    let mut doc = ResultDoc::new(version, Status::Ok, "generated", "LOD written");
    doc.inputs_digest = Some(loaded.inputs_digest.clone());
    doc.arguments_digest = Some(digest.clone());
    if let Some(old) = &old
        && same_version
        && old.request_digest == digest
        && old.inputs_digest == loaded.inputs_digest
        && crate::pack::is_packed(&old.files) == request.output.pack
        && up_to_date(out, old)
    {
        "up_to_date".clone_into(&mut doc.code);
        "the output folder already holds this request's files".clone_into(&mut doc.reason);
        written_of("", &old.files, &mut doc);
        doc.manifest = Some(MANIFEST_FILE.to_owned());
        // the warnings of the run that wrote the files are still beside them
        if let Some(file) = std::fs::read(out.join(WARNINGS_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<WarningsFile>(&b).ok())
        {
            doc.warning_counts = file.counts;
            doc.warnings_file = Some(WARNINGS_FILE.to_owned());
        }
        doc.steps = clock.finish();
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
    // the run builds beside the output folder and replaces it only when it succeeds: a kill, a cancel or a failure
    // leaves the previous output untouched and the staging folder to resume from
    let stage = crate::staging::staging_of(&target);
    let resume = crate::staging::resume_or_clear(&stage, &digest, &loaded.inputs_digest)?;
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
    let terrain_levels: Vec<u32> = terrain_settings(request)
        .levels
        .iter()
        .map(|l| l.level)
        .collect();
    run.plan(
        &chosen.iter().map(|w| w.name()).collect::<Vec<_>>(),
        request.terrain.enabled.then_some(terrain_levels.as_slice()),
        request.objects.enabled,
        request.trees.enabled,
        request.output.pack,
    );
    let threads = request
        .limits
        .threads
        .map_or_else(crate::budget::default_threads, |n| n as usize);
    let need_assets =
        request.objects.enabled || request.trees.enabled || request.terrain.land_textures;
    clock.begin("assets_index", None);
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
    // validate() has checked that every key is eight hex digits
    let texture_scales: BTreeMap<u32, f32> = request
        .terrain
        .texture_scales
        .iter()
        .filter_map(|(id, scale)| Some((u32::from_str_radix(id, 16).ok()?, *scale)))
        .collect();
    // room for the staging copy: about the size of the previous output, less what staging already holds
    let staged: u64 = resume
        .as_ref()
        .map_or(0, |c| c.files.iter().map(|f| f.bytes).sum());
    let need = old
        .as_ref()
        .map_or(0, |o| o.files.iter().map(|f| f.bytes).sum::<u64>())
        .saturating_sub(staged);
    if let Some(free) = crate::staging::free_bytes(&target) {
        crate::staging::check_room(need, free)?;
    }
    std::fs::create_dir_all(&stage)
        .map_err(|e| io_error("cannot create the staging folder", &e))?;
    let reuse = |key: &str, inputs: &str| {
        if let Some(found) = resume
            .as_ref()
            .and_then(|c| reusable(&stage, c, key, inputs))
        {
            return Some(found);
        }
        let found = reusable(out, old.as_ref()?, key, inputs)?;
        // a kept block of the previous output is linked into the staging folder (copied where links fail)
        found
            .iter()
            .all(|f| crate::staging::link_or_copy(out, &stage, &f.path).is_some())
            .then_some(found)
    };
    let checkpoint = |files: &[ManifestFile], blocks: &[ManifestBlock]| {
        crate::staging::write_checkpoint(
            &stage,
            version,
            &digest,
            &loaded.inputs_digest,
            files,
            blocks,
        )
    };
    let stop = || cancel.is_cancelled();
    let mut all: Vec<ManifestFile> = Vec::new();
    // the default land texture of the game's ini, next to the data folder; the documented default without one
    let default_land = default_land_texture(Path::new(&request.data_folder));
    let mut blocks: Vec<ManifestBlock> = Vec::new();
    let mut counts: Option<BlockCounts> = None;
    let mut patched_tiles = 0_u64;
    let mut cancelled = false;
    let mut records: Vec<WarningRecord> = Vec::new();
    // the base plugins' load order is assembled once for every world's decision about the game's own tiles
    let shipped_base = if assets.is_some()
        && request.terrain.enabled
        && request.terrain.land_textures
        && !request.terrain.compose_shipped_tiles
    {
        Some(
            lodwright_core::shipped::ShippedBase::new(&loaded.plugins, &|name| {
                lodwright_core::shipped::is_game_master(name)
            })
            .map_err(|e| load_order_error(&e))?,
        )
    } else {
        None
    };
    // the tree bases do not depend on the world: found once for every world's trees
    let tree_bases = if assets.is_some() && request.trees.enabled {
        lodwright_core::trees::tree_bases(&loaded.plugins).map_err(|e| load_order_error(&e))?
    } else {
        Vec::new()
    };
    'worlds: for world in chosen {
        // the grid every level's blocks start from, as the terrain stage finds it (after dropping outlying edges)
        let anchor = world.without_outliers().grid_anchor();
        let mut files: Vec<ManifestFile> = Vec::new();
        if request.terrain.enabled {
            if stop() {
                // the token may have been cancelled by the last event of an earlier world, after that world's files
                // were collected: they belong to the interrupted manifest too
                all.extend(files);
                cancelled = true;
                break 'worlds;
            }
            progress.report(&run.start(Step::Terrain, Some(world.name())));
            clock.begin("terrain.land_tiles", Some(world.name()));
            let mut warnings = Vec::new();
            let tiles = match (&assets, request.terrain.land_textures) {
                (Some(assets), true) => lodwright_gen::LandTiles::load_with_threads(
                    world,
                    &resolver,
                    assets,
                    &mut warnings,
                    threads,
                )
                .with_scales(&texture_scales)
                .with_default_texture(&default_land, assets, &mut warnings)
                .with_world_fill(world.name()),
                _ => lodwright_gen::LandTiles::empty(),
            };
            // far land is lit as the near land is: the unscaled tiles are kept for the test of a game tile
            let unlit = request.terrain.land_light.then(|| tiles.clone());
            let tiles = if request.terrain.land_light {
                tiles.lit()
            } else {
                tiles
            };
            if settings.levels.iter().any(|l| l.specular_alpha) {
                warnings.extend(specular_warnings(&tiles));
            }
            // the diffuse tiles the game ships and nothing outside its masters changed are left to the game
            clock.begin("terrain.shipped_plan", Some(world.name()));
            let keep = match (&assets, &shipped_base) {
                (Some(assets), Some(base)) => crate::shipped::plan(
                    world,
                    base,
                    &loaded.plugins,
                    &resolver,
                    assets,
                    anchor,
                    &mut warnings,
                )
                .map_err(|e| load_order_error(&e))?,
                _ => None,
            };
            let keep_fn =
                |level: u32, cell: (i32, i32)| keep.as_ref().is_some_and(|k| k.keeps(level, cell));
            let patch_fn =
                |level: u32, cell: (i32, i32)| keep.as_ref().and_then(|k| k.patch(level, cell));
            let mean_fn =
                |level: u32, cell: (i32, i32)| keep.as_ref().and_then(|k| k.mean(level, cell));
            note(&mut doc, &mut records, world.name(), warnings);
            // hold the budget: as many workers as fit beside what the process holds now (the tiles are loaded)
            let workers = crate::budget::workers(
                request
                    .limits
                    .memory_mib
                    .or_else(crate::budget::default_budget_mib),
                crate::budget::resident_mib().or(plugins_mib).unwrap_or(0),
                crate::budget::raster_mib(world, &settings),
                crate::budget::worker_mib(&settings),
                threads,
            );
            if workers < threads {
                note(
                    &mut doc,
                    &mut records,
                    world.name(),
                    vec![format!(
                        "{}: the memory budget leaves room for {workers} of {threads} terrain workers",
                        world.name()
                    )],
                );
            }
            clock.begin("terrain.blocks", Some(world.name()));
            let report = {
                let shared = Mutex::new((&mut *progress, &mut run));
                let block_done = |world: &str, level: u32, done: u64, of: u64, up_to_date: bool| {
                    if let Ok(mut guard) = shared.lock() {
                        let (p, run) = &mut *guard;
                        let event = run.block(Step::Terrain, world, level, done, of, up_to_date);
                        p.report(&event);
                    }
                };
                lodwright_gen::generate_terrain(
                    world,
                    &settings,
                    &stage,
                    &lodwright_gen::Options {
                        threads: workers,
                        land: Some(&tiles),
                        progress: Some(&block_done),
                        water: request.terrain.water,
                        drop_under_water: request.terrain.drop_under_water,
                        water_walls: request.terrain.water_walls,
                        water_shared_vertices: request.terrain.water_shared_vertices,
                        water_polygons: request.terrain.water_polygons,
                        keep_shipped: keep.is_some().then_some(&keep_fn),
                        patch_shipped: keep.is_some().then_some(&patch_fn),
                        shipped_mean: keep.is_some().then_some(&mean_fn),
                        unlit: unlit.as_ref(),
                        reuse: Some(&reuse),
                        cancel: Some(&stop),
                    },
                )
                .map_err(|e| gen_error(&e))?
            };
            let first = files.len();
            files.extend(report.files.iter().map(to_manifest_file));
            clock.wrote(&files[first..]);
            let c = counts.get_or_insert_with(BlockCounts::default);
            if keep.is_some() {
                let t = doc.tiles.get_or_insert_with(Default::default);
                for b in &report.blocks {
                    if b.kept_diffuse {
                        t.kept += 1;
                    } else {
                        t.composed += 1;
                        patched_tiles += u64::from(b.patched_diffuse);
                    }
                }
            }
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
            progress.report(&run.start(Step::Objects, Some(world.name())));
            clock.begin("objects.collect", Some(world.name()));
            let mut instances =
                lodwright_core::objects::visible_instances(&loaded.plugins, world.form_id())
                    .map_err(|e| load_order_error(&e))?;
            let include = lodwright_gen::IncludeRules::new(
                &request.inclusion.include_rules,
                &request.plugins,
            )
            .map_err(|e| {
                Error::new(
                    "request_invalid",
                    "the request's inclusion rules are not valid",
                )
                .with_detail(e)
            })?;
            if !include.is_empty() {
                let matched = lodwright_core::objects::references_where(
                    &loaded.plugins,
                    world.form_id(),
                    &|b| {
                        b.model
                            .as_deref()
                            .is_some_and(|m| include.matches_base(m, b.signature))
                    },
                )
                .map_err(|e| load_order_error(&e))?;
                let held = include.take(&mut instances, matched.taken, &matched.held_back);
                if held > 0 {
                    let text = format!(
                        "the include rules match {held} references in {} that are disabled when the game starts (themselves or through their enable parent); the far view shows what stands at the start, so they are left out",
                        world.name()
                    );
                    records.push(WarningRecord::new(
                        "include_rule_reference_not_enabled",
                        Some(world.name()),
                        None,
                        std::iter::empty::<String>(),
                        Some(held as u64),
                        text.clone(),
                    ));
                    add_warnings(&mut doc, vec![text]);
                }
            }
            if request.objects.far_model_without_flag {
                instances.extend(far_model_without_flag(
                    &loaded.plugins,
                    world.form_id(),
                    assets,
                    &instances,
                )?);
            }
            let instances =
                lodwright_gen::filter_instances(instances, &request.inclusion, &request.plugins)
                    .map_err(|e| {
                        Error::new(
                            "request_invalid",
                            "the request's inclusion rules are not valid",
                        )
                        .with_detail(e)
                    })?;
            // the models the request takes from the shipped far blocks in this world: their shapes are read from the
            // games' own archives, never from a mod's
            let own_models: Vec<&str> = request
                .objects
                .shapes_from_shipped_blocks
                .iter()
                .filter(|rule| rule.world.eq_ignore_ascii_case(world.name()))
                .flat_map(|rule| rule.models.iter().map(String::as_str))
                .collect();
            let own_references = lodwright_gen::filter_instances(
                lodwright_core::objects::references_of_models(
                    &loaded.plugins,
                    world.form_id(),
                    &own_models,
                )
                .map_err(|e| load_order_error(&e))?,
                &request.inclusion,
                &request.plugins,
            )
            .map_err(|e| {
                Error::new(
                    "request_invalid",
                    "the request's inclusion rules are not valid",
                )
                .with_detail(e)
            })?;
            let own_bounds = if own_references.is_empty() {
                std::collections::HashMap::new()
            } else {
                lodwright_core::objects::base_bounds(&loaded.plugins)
                    .map_err(|e| load_order_error(&e))?
            };
            let read_own = |path: &str| {
                assets.shipped_read(path, &|archive| {
                    !lodwright_core::shipped::is_game_archive(archive)
                })
            };
            let ground = |x: f32, y: f32| world.ground_height(x, y);
            let water = |x: f32, y: f32| {
                // source: cell-size-units
                #[allow(clippy::cast_possible_truncation)] // a cell number of a world fits an i32
                let cell = ((x / 4096.0).floor() as i32, (y / 4096.0).floor() as i32);
                world.water_height(cell.0, cell.1)
            };
            let phase = |name: &str| clock.begin(&format!("objects.{name}"), Some(world.name()));
            let rule_shapes = |i: &lodwright_core::objects::Instance| include.generate_for(i);
            let far_lights = if request.objects.far_lights {
                lodwright_core::lights::far_light_sources(
                    &loaded.plugins,
                    world.form_id(),
                    FAR_LIGHT_MIN_RADIUS,
                )
                .map_err(|e| load_order_error(&e))?
            } else {
                Vec::new()
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
                grid_anchor: anchor,
                plugins: &request.plugins,
                phase: Some(&phase),
                threads,
                own: Some(lodwright_gen::OwnSource {
                    references: &own_references,
                    bounds: &own_bounds,
                    read: &read_own,
                }),
                full_models: request.objects.shapes_from_full_models.map(|shapes| {
                    lodwright_gen::FarLimits {
                        keep: shapes.keep,
                        max_error: shapes.max_error,
                    }
                }),
                rule_shapes: include
                    .generates()
                    .then_some(&rule_shapes as lodwright_gen::RuleShapes<'_>),
                far_lights: &far_lights,
                glow: request.objects.far_glow,
            };
            let objects = lodwright_gen::generate_objects_with_progress(
                world.name(),
                &instances,
                assets,
                &stage,
                surfaces,
                &mut |done, total| {
                    progress.report(&run.block(
                        Step::Objects,
                        world.name(),
                        OBJECT_BLOCK_CELLS,
                        done,
                        total,
                        false,
                    ));
                },
            )
            .map_err(|e| gen_error(&e))?;
            add_warnings(&mut doc, objects.warnings.clone());
            records.extend(objects.records.iter().cloned());
            if request.objects.drop_under_terrain || request.objects.drop_under_water {
                let d = doc.dropped_faces.get_or_insert_with(Default::default);
                d.under_terrain += objects.dropped_under_terrain as u64;
                d.under_water += objects.dropped_under_water as u64;
            }
            let first = files.len();
            files.extend(objects.files.iter().map(to_manifest_file));
            clock.wrote(&files[first..]);
        }
        if let (Some(assets), true) = (&assets, request.trees.enabled) {
            if stop() {
                all.extend(files);
                cancelled = true;
                break 'worlds;
            }
            progress.report(&run.start(Step::Trees, Some(world.name())));
            clock.begin("trees", Some(world.name()));
            let bases = &tree_bases;
            let refs =
                lodwright_core::trees::tree_references(&loaded.plugins, world.form_id(), bases)
                    .map_err(|e| load_order_error(&e))?;
            let south_west = anchor;
            let trees = lodwright_gen::generate_trees_with_progress(
                world.name(),
                south_west,
                bases,
                &refs,
                &request.plugins,
                assets,
                &stage,
                &mut |done, total| {
                    progress.report(&run.block(
                        Step::Trees,
                        world.name(),
                        TREE_BLOCK_CELLS,
                        done,
                        total,
                        false,
                    ));
                },
            )
            .map_err(|e| gen_error(&e))?;
            note(&mut doc, &mut records, world.name(), trees.warnings.clone());
            let first = files.len();
            files.extend(trees.files.iter().map(to_manifest_file));
            clock.wrote(&files[first..]);
        }
        written_of(world.name(), &files, &mut doc);
        all.extend(files);
        checkpoint(&all, &blocks)?;
        if stop() {
            // a cancel that came with the last event of this world's last step is seen here, not ignored
            cancelled = true;
            break 'worlds;
        }
    }
    if cancelled {
        // no manifest step event: the checkpoint is written, not generated; the output folder is not touched
        checkpoint(&all, &blocks)?;
        return Ok(GenerateOutcome::Cancelled);
    }
    if request.output.pack {
        if stop() {
            checkpoint(&all, &blocks)?;
            return Ok(GenerateOutcome::Cancelled);
        }
        progress.report(&run.start(Step::Pack, None));
        clock.begin("pack", None);
        // the stub's master: the first plugin of the load order and its size (the game warns on a different size)
        let master = request.plugins.first().map_or("", String::as_str);
        let master_size = crate::load::find_plugin(data, master)
            .ok()
            .and_then(|p| std::fs::metadata(p).ok())
            .map_or(0, |m| m.len());
        let packed = crate::pack::pack_staging(&stage, &all, (master, master_size), |done, of| {
            progress.report(&run.files(Step::Pack, done, of));
        })?;
        // the loose files are gone from staging: the manifest lists the archive and its stub, and no block (a block's
        // files are the loose ones), so a later run builds every block again
        all = packed;
        blocks.clear();
    }
    progress.report(&run.start(Step::Manifest, None));
    clock.begin("manifest", None);
    doc.blocks = counts;
    if let Some(t) = doc.tiles {
        doc.reason = format!(
            "LOD written; diffuse tiles: {} left to the game's own, {} composed ({} of them written into the game's tile for the changed cells only)",
            t.kept, t.composed, patched_tiles
        );
    }
    let manifest = Manifest::new(version, &digest, &loaded.inputs_digest, all)
        .with_blocks(blocks)
        .with_build(crate::BUILD);
    // staging holds exactly the manifest's files: whatever else a killed run left in it is dropped
    crate::staging::prune(&stage, &manifest.files);
    write_warnings(&mut doc, &stage, records)?;
    crate::staging::finish_manifest(&stage, &manifest)?;
    crate::staging::swap(&target, &stage)?;
    doc.manifest = Some(MANIFEST_FILE.to_owned());
    doc.steps = clock.finish();
    // the peak of the whole run: the steps reset the kernel's high-water mark as they start
    doc.peak_memory_mib = doc.steps.iter().filter_map(|s| s.peak_memory_mib).max();
    Ok(GenerateOutcome::Done {
        result: Box::new(doc),
    })
}

/// The references of `world` that `objects.far_model_without_flag` adds to `taken`: nothing marks them visible from
/// far, their model has a far model (`<model>_lod.nif`) that some source holds, their base is not drawn as a tree
/// billboard (has-tree-LOD), and they stand when the game starts (themselves and through their enable parent). A
/// reference `taken` already holds is not added again. Own design, after the behaviour measured on another
/// generator's output (lodwright #426).
fn far_model_without_flag(
    plugins: &[(String, lodwright_core::plugin::Plugin)],
    world: u32,
    assets: &lodwright_gen::FolderAssets,
    taken: &[lodwright_core::objects::Instance],
) -> Result<Vec<lodwright_core::objects::Instance>, Error> {
    let has_far_model = |model: &str| {
        !matches!(
            assets.winner(&lodwright_gen::lod_model_path(model)),
            lodwright_gen::Winner::Missing
        )
    };
    // has-tree-LOD is the bit only on the base types tree LOD reads (`xedit-tree-lod-selection`); on another type the
    // same bit means something else
    let tree_billboard = |b: &lodwright_core::plugin::BaseObject| {
        [*b"STAT", *b"ACTI", *b"TREE"].contains(&b.signature)
            && b.flags & lodwright_core::objects::HAS_TREE_LOD != 0
    };
    let matched = lodwright_core::objects::references_where(plugins, world, &|b| {
        !tree_billboard(b) && b.model.as_deref().is_some_and(has_far_model)
    })
    .map_err(|e| load_order_error(&e))?;
    let present: std::collections::HashSet<u32> = taken.iter().map(|i| i.reference).collect();
    Ok(matched
        .taken
        .into_iter()
        .filter(|i| !present.contains(&i.reference))
        .collect())
}

#[cfg(test)]
mod specular_tests {
    use super::specular_warnings;

    #[test]
    fn no_exponent_anywhere_warns_once_that_the_alpha_stays_opaque() {
        let warnings = specular_warnings(&lodwright_gen::LandTiles::empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("stay opaque"));
    }
}

#[cfg(test)]
mod classify_tests {
    use super::classify;
    use lodwright_testkit::{covers, ids};

    #[test]
    fn a_land_texture_that_is_in_no_folder_or_archive_is_its_own_kind_and_names_the_path_only() {
        covers(&[ids::terrain::textures::MISSING_TEXTURE_WARNING]);
        let r = classify(
            Some("WastelandNV"),
            "land texture textures/Landscape/BurntGround01.dds not found",
        );
        assert_eq!(r.kind, "land_texture_not_found");
        assert_eq!(r.subjects, ["textures/Landscape/BurntGround01.dds"]);
        let r = classify(None, "texture textures/a/b.dds found but not readable (x)");
        assert_eq!(
            (r.kind.as_str(), r.subjects[0].as_str()),
            ("texture_unreadable", "textures/a/b.dds")
        );
    }
}

#[cfg(test)]
mod warning_tests {
    use super::add_warnings;
    use lodwright_contract::{ResultDoc, Status};
    use lodwright_testkit::{covers, ids};

    #[test]
    fn warnings_are_capped_at_twenty_and_the_last_line_counts_the_rest() {
        covers(&[ids::generate::run::WARNINGS_CAPPED]);
        let mut doc = ResultDoc::new("0.0.0", Status::Ok, "generated", "LOD written");
        add_warnings(&mut doc, (0..15).map(|i| format!("a{i}")).collect());
        assert_eq!(doc.warnings.len(), 15);
        add_warnings(&mut doc, (0..10).map(|i| format!("b{i}")).collect());
        assert_eq!(doc.warnings.len(), 21);
        assert_eq!(doc.warnings[20], "5 more warnings were left out");
        add_warnings(&mut doc, vec!["c".to_owned()]);
        assert_eq!(doc.warnings.len(), 21);
        assert_eq!(doc.warnings[20], "6 more warnings were left out");
    }
}
