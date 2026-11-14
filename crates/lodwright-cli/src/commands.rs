// SPDX-License-Identifier: MPL-2.0
//! The `plan` and `heightmap` commands.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

use lodwright_core::raster::{BlockGrid, HeightRaster};
use lodwright_core::terrain::{TerrainPlan, TerrainSettings, plan_world};
use lodwright_core::worldspace::{Inheritance, WorldIndex, Worldspace};

use crate::load::{load_plugins, load_worlds};

/// The version of the JSON that `plan --format json` prints; it grows only on a breaking change.
pub const SCHEMA_VERSION: u32 = 1;
use crate::options::{Format, Options};

fn select<'a>(
    worlds: &'a [Worldspace],
    wanted: Option<&str>,
) -> Result<Vec<&'a Worldspace>, String> {
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
                Err(format!("no worldspace named {name}; found: {}", names()))
            } else {
                Ok(found)
            }
        }
        None if worlds.is_empty() => Err("the plugins define no worldspace with terrain".into()),
        None => Ok(worlds.iter().collect()),
    }
}

/// The worlds `wanted` names (all when `None`), as `FormID`s from the index.
fn select_ids(index: &WorldIndex, wanted: Option<&str>) -> Result<Vec<u32>, String> {
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
            return Err(format!("no worldspace named {name}; found: {}", names()));
        }
        return Ok(found);
    }
    let all: Vec<u32> = index.worlds().map(|(id, _)| id).collect();
    if all.is_empty() {
        Err("the plugins define no worldspace with terrain".into())
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

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_report(world: &Worldspace, plan: &TerrainPlan, blocks: bool) -> String {
    let bounds = world.bounds().map_or_else(
        || "null".to_owned(),
        |b| {
            format!(
                "{{\"min_x\":{},\"min_y\":{},\"max_x\":{},\"max_y\":{}}}",
                b.min_x, b.min_y, b.max_x, b.max_y
            )
        },
    );
    let levels: Vec<String> = totals(plan)
        .iter()
        .map(|t| {
            format!(
                "{{\"level\":{},\"blocks\":{},\"vertices\":{},\"triangles\":{},\"max_error\":{:.3}}}",
                t.level, t.blocks, t.vertices, t.triangles, t.max_error
            )
        })
        .collect();
    let mut out = format!(
        "{{\"name\":{},\"form_id\":\"{:08X}\",\"cells\":{},\"bounds\":{bounds},\"max_seam_error\":{:.3},\"settings_path\":{},\"levels\":[{}]",
        json_string(world.name()),
        world.form_id(),
        world.cell_count(),
        world.max_seam_error(),
        json_string(&plan.settings_path),
        levels.join(",")
    );
    if blocks {
        let list: Vec<String> = plan
            .blocks
            .iter()
            .map(|b| {
                format!(
                    "{{\"level\":{},\"x\":{},\"y\":{},\"step\":{},\"vertices\":{},\"triangles\":{},\"max_error\":{:.3},\"mesh\":{},\"diffuse\":{},\"normal\":{},\"texture_size\":[{},{}],\"mips\":{}}}",
                    b.id.level, b.id.x, b.id.y, b.step, b.vertices, b.triangles, b.max_error,
                    json_string(&b.mesh_path), json_string(&b.diffuse_path), json_string(&b.normal_path),
                    b.diffuse_size, b.normal_size, b.mip_levels
                )
            })
            .collect();
        let _ = write!(out, ",\"blocks\":[{}]", list.join(","));
    }
    out.push('}');
    out
}

/// `plan`: read the plugins, plan the terrain LOD of the chosen worlds, print the plan.
pub fn plan(options: &Options, out: &mut dyn Write) -> Result<(), String> {
    let plugins = load_plugins(&options.data, &options.plugins)?;
    let index = WorldIndex::new(&plugins).map_err(|e| e.to_string())?;
    let chosen = select_ids(&index, options.world.as_deref())?;
    let settings = TerrainSettings::default();
    let mut reports = Vec::new();
    // one world at a time: each assembled world is dropped before the next is built
    for id in chosen {
        let Some(world) = index.build(&plugins, id, Inheritance::None) else {
            continue;
        };
        let plan =
            plan_world(&world, &settings).map_err(|e| format!("world {}: {e}", world.name()))?;
        reports.push(match options.format {
            Format::Text => text_report(&world, &plan, options.blocks),
            Format::Json => json_report(&world, &plan, options.blocks),
        });
    }
    let text = match options.format {
        Format::Text => reports.concat(),
        Format::Json => format!(
            "{{\"schema_version\":{SCHEMA_VERSION},\"worlds\":[{}]}}\n",
            reports.join(",")
        ),
    };
    out.write_all(text.as_bytes())
        .map_err(|e| format!("cannot write output: {e}"))
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
pub fn heightmap(options: &Options, out: &mut dyn Write) -> Result<(), String> {
    let target = options
        .out
        .as_deref()
        .ok_or("--out <file.pgm> is required")?;
    if !options.allow_in_repo && inside_git_tree(target) {
        return Err(
            "refusing to write inside a git working tree (heights derived from game data must not enter a repository); \
             pass --allow-in-repo to override"
                .into(),
        );
    }
    let worlds = load_worlds(&options.data, &options.plugins)?;
    let chosen = select(&worlds, options.world.as_deref())?;
    let [world] = chosen.as_slice() else {
        return Err("choose one worldspace with --world".into());
    };
    let grid = BlockGrid::new(options.level).map_err(|e| e.to_string())?;
    let raster = HeightRaster::build(world, grid)
        .map_err(|e| e.to_string())?
        .ok_or("the worldspace has no cells with terrain")?;
    let (bytes, (low, high)) = encode_pgm(&raster);
    std::fs::write(target, bytes).map_err(|e| format!("cannot write the output file: {e}"))?;
    let message = format!(
        "wrote {}x{} samples; 0 is {low} and 65535 is {high} world units; origin cell {:?}\n",
        raster.width(),
        raster.height(),
        raster.origin_cell()
    );
    out.write_all(message.as_bytes())
        .map_err(|e| format!("cannot write output: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{inside_git_tree, json_string};

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");
        assert_eq!(json_string("plain"), "\"plain\"");
    }

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
