// SPDX-License-Identifier: MPL-2.0
//! The `plan` and `heightmap` commands.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

use lodwright_contract::{
    Bounds, Capabilities, LevelPlan, PLAN_CONTRACT, Plan, PlannedBlock, ResultDoc, Status,
    ToolInfo, WorldPlan, Written, arguments_digest, peak_memory_mib, round_error,
};
use lodwright_core::raster::{BlockGrid, HeightRaster};
use lodwright_core::terrain::{TerrainPlan, TerrainSettings, plan_world};
use lodwright_core::worldspace::{Inheritance, WorldIndex, Worldspace};

use crate::failure::Failure;
use crate::load::{load_plugins, load_plugins_with, load_worlds};
use crate::options::{Format, Options};

fn select<'a>(
    worlds: &'a [Worldspace],
    wanted: Option<&str>,
) -> Result<Vec<&'a Worldspace>, Failure> {
    let names = || {
        worlds
            .iter()
            .map(Worldspace::name)
            .collect::<Vec<_>>()
            .join(", ")
    };
    match wanted {
        Some(name) => {
            let found: Vec<&Worldspace> = worlds
                .iter()
                .filter(|w| w.name().eq_ignore_ascii_case(name))
                .collect();
            if found.is_empty() {
                Err(Failure::new(
                    "world_not_found",
                    format!("no worldspace named {name}; found: {}", names()),
                ))
            } else {
                Ok(found)
            }
        }
        None if worlds.is_empty() => Err(Failure::new(
            "no_worlds",
            "the plugins define no worldspace with terrain",
        )),
        None => Ok(worlds.iter().collect()),
    }
}

/// The worlds `wanted` names (all when `None`), as `FormID`s from the index.
fn select_ids(index: &WorldIndex, wanted: Option<&str>) -> Result<Vec<u32>, Failure> {
    let names = || {
        index
            .worlds()
            .map(|(_, name)| name)
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(name) = wanted {
        let found: Vec<u32> = index
            .worlds()
            .filter(|(_, world)| world.eq_ignore_ascii_case(name))
            .map(|(id, _)| id)
            .collect();
        if found.is_empty() {
            return Err(Failure::new(
                "world_not_found",
                format!("no worldspace named {name}; found: {}", names()),
            ));
        }
        return Ok(found);
    }
    let all: Vec<u32> = index.worlds().map(|(id, _)| id).collect();
    if all.is_empty() {
        Err(Failure::new(
            "no_worlds",
            "the plugins define no worldspace with terrain",
        ))
    } else {
        Ok(all)
    }
}

struct LevelTotals {
    level: u32,
    blocks: usize,
    vertices: usize,
    triangles: usize,
    max_error: f32,
}

fn totals(plan: &TerrainPlan) -> Vec<LevelTotals> {
    let mut out: Vec<LevelTotals> = Vec::new();
    for block in &plan.blocks {
        if out.last().is_none_or(|t| t.level != block.id.level) {
            out.push(LevelTotals {
                level: block.id.level,
                blocks: 0,
                vertices: 0,
                triangles: 0,
                max_error: 0.0,
            });
        }
        let Some(t) = out.last_mut() else { continue };
        t.blocks += 1;
        t.vertices += block.vertices;
        t.triangles += block.triangles;
        t.max_error = t.max_error.max(block.max_error);
    }
    out
}

fn bounds_text(world: &Worldspace) -> String {
    world.bounds().map_or_else(
        || "no cells".to_owned(),
        |b| format!("x {}..{} y {}..{}", b.min_x, b.max_x, b.min_y, b.max_y),
    )
}

fn text_report(world: &Worldspace, plan: &TerrainPlan, blocks: bool) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "world {} (form {:08X}): {} cells, {}, max seam error {:.2}",
        world.name(),
        world.form_id(),
        world.cell_count(),
        bounds_text(world),
        world.max_seam_error()
    );
    for t in totals(plan) {
        let _ = writeln!(
            out,
            "  level {}: {} blocks, {} vertices, {} triangles, max error {:.2}",
            t.level, t.blocks, t.vertices, t.triangles, t.max_error
        );
    }
    let _ = writeln!(out, "  settings: {}", plan.settings_path);
    if blocks {
        for b in &plan.blocks {
            let _ = writeln!(
                out,
                "  block L{} ({},{}) step {} vertices {} triangles {} error {:.2} {}",
                b.id.level,
                b.id.x,
                b.id.y,
                b.step,
                b.vertices,
                b.triangles,
                b.max_error,
                b.mesh_path
            );
        }
    }
    out
}

/// The normalised argument lines the arguments digest covers: the load order (names folded to lower case, order
/// kept), the chosen world, whether blocks are listed and the document version. The data folder is not part of it
/// (the inputs digest covers what is read from there), so a digest does not depend on where a folder is mounted.
fn argument_lines(options: &Options, version: u32) -> Vec<String> {
    let mut lines: Vec<String> = options
        .plugins
        .iter()
        .map(|p| format!("plugin={}", p.to_ascii_lowercase()))
        .collect();
    lines.push(format!(
        "world={}",
        options
            .world
            .as_deref()
            .map_or_else(String::new, str::to_ascii_lowercase)
    ));
    lines.push(format!("include_blocks={}", options.blocks));
    lines.push(format!("schema_version={version}"));
    lines
}

/// The arguments digest of `options`.
pub fn arguments_digest_of(options: &Options) -> String {
    arguments_digest(&argument_lines(options, 1))
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

fn world_document(world: &Worldspace, plan: &TerrainPlan, blocks: bool) -> WorldPlan {
    WorldPlan {
        name: world.name().to_owned(),
        form_id: format!("{:08X}", world.form_id()),
        cells: count(world.cell_count()),
        bounds: world.bounds().map(|b| Bounds {
            west: b.min_x,
            south: b.min_y,
            east: b.max_x,
            north: b.max_y,
        }),
        max_seam_error: round_error(world.max_seam_error()),
        settings_path: plan.settings_path.clone(),
        levels: totals(plan)
            .iter()
            .map(|t| LevelPlan {
                level: t.level,
                blocks: count(t.blocks),
                vertices: count(t.vertices),
                triangles: count(t.triangles),
                max_error: round_error(t.max_error),
            })
            .collect(),
        blocks: blocks.then(|| {
            plan.blocks
                .iter()
                .map(|b| PlannedBlock {
                    level: b.id.level,
                    x: b.id.x,
                    y: b.id.y,
                    step: count(b.step),
                    vertices: count(b.vertices),
                    triangles: count(b.triangles),
                    max_error: round_error(b.max_error),
                    mesh: b.mesh_path.clone(),
                    diffuse: b.diffuse_path.clone(),
                    normal: b.normal_path.clone(),
                    texture_size: [b.diffuse_size, b.normal_size],
                    mips: b.mip_levels,
                })
                .collect()
        }),
    }
}

fn print(out: &mut dyn Write, text: &str) -> Result<(), Failure> {
    out.write_all(text.as_bytes())
        .map_err(|e| Failure::new("output_failed", format!("cannot write output: {e}")))
}

/// `plan`: read the plugins, plan the terrain LOD of the chosen worlds, print the plan.
pub fn plan(options: &Options, out: &mut dyn Write) -> Result<(), Failure> {
    let loaded = load_plugins(&options.data, &options.plugins)?;
    let index = WorldIndex::new(&loaded.plugins)
        .map_err(|e| Failure::new("load_order_invalid", e.to_string()))?;
    let chosen = select_ids(&index, options.world.as_deref())?;
    let settings = TerrainSettings::default();
    let mut documents = Vec::new();
    let mut text = String::new();
    // one world at a time: each assembled world is dropped before the next is built
    for id in chosen {
        let Some(world) = index.build(&loaded.plugins, id, Inheritance::None) else {
            continue;
        };
        let plan = plan_world(&world, &settings)
            .map_err(|e| Failure::new("plan_failed", format!("world {}: {e}", world.name())))?;
        match options.format {
            Format::Text => text.push_str(&text_report(&world, &plan, options.blocks)),
            Format::Json => documents.push(world_document(&world, &plan, options.blocks)),
        }
    }
    if options.format == Format::Json {
        let document = Plan {
            schema_version: 1,
            contract: PLAN_CONTRACT.to_owned(),
            tool: ToolInfo::new(env!("CARGO_PKG_VERSION")),
            arguments_digest: arguments_digest_of(options),
            inputs_digest: loaded.inputs_digest,
            capabilities: Capabilities::current(),
            worlds: documents,
            warnings: Vec::new(),
        };
        text = serde_json::to_string(&document)
            .map_err(|e| Failure::new("output_failed", e.to_string()))?
            + "\n";
    }
    print(out, &text)
}

/// `generate`: with `--data`, plugins and `--out`, writes the terrain LOD of the chosen worlds (all worlds when
/// `--world` is absent) and answers with a result document; with nothing to read it answers `unsupported`.
pub fn generate(options: &Options) -> ResultDoc {
    let version = env!("CARGO_PKG_VERSION");
    let has_data = !options.data.as_os_str().is_empty();
    let mut doc = unsupported(version);
    if has_data && !options.plugins.is_empty() {
        doc = match options.out.as_deref() {
            Some(out) => match write_terrain(options, out, version) {
                Ok(doc) => doc,
                Err(failure) => failed_document(options, &failure),
            },
            None => match load_plugins(&options.data, &options.plugins) {
                Ok(loaded) => {
                    let mut doc = doc;
                    doc.inputs_digest = Some(loaded.inputs_digest);
                    doc
                }
                Err(failure) => {
                    ResultDoc::new(version, Status::Failed, failure.code, &failure.reason)
                }
            },
        };
    }
    if has_data || !options.plugins.is_empty() {
        doc.arguments_digest = Some(arguments_digest_of(options));
    }
    doc.peak_memory_mib = peak_memory_mib();
    doc
}

fn refuse_output(options: &Options, out: &Path) -> Result<(), Failure> {
    if !options.allow_in_repo && inside_git_tree(out) {
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
    if resolve(out).starts_with(resolve(&options.data)) {
        return Err(Failure::new(
            "output_in_data",
            "refusing to write inside the data folder: generate into a separate folder and install it from there",
        ));
    }
    Ok(())
}

fn write_terrain(options: &Options, out: &Path, version: &str) -> Result<ResultDoc, Failure> {
    refuse_output(options, out)?;
    let read = lodwright_core::plugin::ReadOptions {
        objects: !options.terrain_only,
        ..lodwright_core::plugin::ReadOptions::default()
    };
    let loaded = load_plugins_with(&options.data, &options.plugins, read)?;
    let worlds = Worldspace::from_load_order(&loaded.plugins)
        .map_err(|e| Failure::new("load_order_invalid", e.to_string()))?;
    let chosen = select(&worlds, options.world.as_deref())?;
    let threads = if options.threads > 0 {
        options.threads
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get().min(4))
    };
    let mut doc = ResultDoc::new(version, Status::Ok, "generated", "terrain LOD written");
    doc.inputs_digest = Some(loaded.inputs_digest);
    let assets = if options.terrain_only {
        None
    } else {
        Some(
            lodwright_gen::FolderAssets::open(&options.data, &options.plugins)
                .map_err(|e| Failure::new("assets_unreadable", e))?,
        )
    };
    for world in chosen {
        let report = lodwright_gen::generate_terrain(
            world,
            &TerrainSettings::default(),
            out,
            &lodwright_gen::Options { threads },
        )
        .map_err(|e| Failure::new("generate_failed", e.to_string()))?;
        for (kind, suffix) in [
            ("terrain_mesh", ".nif"),
            ("terrain_texture", ".dds"),
            ("settings", ".dlodsettings"),
        ] {
            let files: Vec<_> = report
                .files
                .iter()
                .filter(|f| f.path.ends_with(suffix))
                .collect();
            doc.written.push(Written {
                world: report.world.clone(),
                kind: kind.to_owned(),
                files: count(files.len()),
                bytes: files.iter().map(|f| f.bytes).sum(),
            });
        }
        if let Some(assets) = &assets {
            let instances =
                lodwright_core::objects::visible_instances(&loaded.plugins, world.form_id())
                    .map_err(|e| Failure::new("load_order_invalid", e.to_string()))?;
            let objects = lodwright_gen::generate_objects(world.name(), &instances, assets, out)
                .map_err(|e| Failure::new("generate_failed", e.to_string()))?;
            if !objects.files.is_empty() {
                doc.written.push(Written {
                    world: world.name().to_owned(),
                    kind: "object_lod".to_owned(),
                    files: count(objects.files.len()),
                    bytes: objects.files.iter().map(|f| f.bytes).sum(),
                });
            }
        }
    }
    Ok(doc)
}

fn unsupported(version: &str) -> ResultDoc {
    let missing = Capabilities::default().missing();
    let mut doc = ResultDoc::new(
        version,
        Status::Unsupported,
        "capability_missing",
        &format!(
            "generate cannot write {} yet, and takes no request document yet",
            missing.join(", ")
        ),
    );
    doc.missing_capabilities = missing;
    doc
}

/// The failure document of a command that failed with `failure` (for `--json`).
pub fn failed_document(options: &Options, failure: &Failure) -> ResultDoc {
    let mut doc = ResultDoc::new(
        env!("CARGO_PKG_VERSION"),
        Status::Failed,
        failure.code,
        &failure.reason,
    );
    if !options.plugins.is_empty() {
        doc.arguments_digest = Some(arguments_digest_of(options));
    }
    doc.peak_memory_mib = peak_memory_mib();
    doc
}

/// Whether `path` (a file to be written) lies inside a git working tree.
fn inside_git_tree(path: &Path) -> bool {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    absolute
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .any(|dir| dir.join(".git").exists())
}

/// Normalise heights to 16 bits (north at the top) and encode a binary PGM.
fn encode_pgm(raster: &HeightRaster) -> (Vec<u8>, (f32, f32)) {
    let (low, high) = raster.height_range().unwrap_or((0.0, 0.0));
    let span = high - low;
    let mut bytes = format!("P5\n{} {}\n65535\n", raster.width(), raster.height()).into_bytes();
    for y in (0..raster.height()).rev() {
        for x in 0..raster.width() {
            let value = raster.sample(x, y).map_or(0.0, |h| {
                if span > 0.0 {
                    (h - low) / span * 65535.0
                } else {
                    32768.0
                }
            });
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let level = value.round().clamp(0.0, 65535.0) as u16;
            bytes.extend_from_slice(&level.to_be_bytes());
        }
    }
    (bytes, (low, high))
}

/// `heightmap`: write a 16-bit PGM of one world's height raster.
pub fn heightmap(options: &Options, out: &mut dyn Write) -> Result<(), Failure> {
    let target = options
        .out
        .as_deref()
        .ok_or_else(|| Failure::new("out_required", "--out <file.pgm> is required"))?;
    if !options.allow_in_repo && inside_git_tree(target) {
        return Err(Failure::new(
            "output_in_repo",
            "refusing to write inside a git working tree (heights derived from game data must not enter a repository); \
             pass --allow-in-repo to override",
        ));
    }
    let worlds = load_worlds(&options.data, &options.plugins)?;
    let chosen = select(&worlds, options.world.as_deref())?;
    let [world] = chosen.as_slice() else {
        return Err(Failure::new(
            "world_required",
            "choose one worldspace with --world",
        ));
    };
    let grid =
        BlockGrid::new(options.level).map_err(|e| Failure::new("level_invalid", e.to_string()))?;
    let raster = HeightRaster::build(world, grid)
        .map_err(|e| Failure::new("raster_failed", e.to_string()))?
        .ok_or_else(|| Failure::new("no_terrain", "the worldspace has no cells with terrain"))?;
    let (bytes, (low, high)) = encode_pgm(&raster);
    std::fs::write(target, bytes).map_err(|e| {
        Failure::new(
            "output_failed",
            format!("cannot write the output file: {e}"),
        )
    })?;
    let message = format!(
        "wrote {}x{} samples; 0 is {low} and 65535 is {high} world units; origin cell {:?}\n",
        raster.width(),
        raster.height(),
        raster.origin_cell()
    );
    print(out, &message)
}

#[cfg(test)]
mod tests {
    use super::inside_git_tree;

    #[test]
    fn a_folder_with_a_git_directory_is_inside_a_tree() {
        let dir = std::env::temp_dir().join(format!("lodwright-git-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        assert!(inside_git_tree(&dir.join("sub").join("x.pgm")));
        let plain = std::env::temp_dir().join(format!("lodwright-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&plain).unwrap();
        assert!(!inside_git_tree(&plain.join("x.pgm")));
    }
}
