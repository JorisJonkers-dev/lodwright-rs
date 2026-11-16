// SPDX-License-Identifier: MPL-2.0
//! Option parsing shared by `plan` and `heightmap`.

use std::path::PathBuf;

/// Output format of `plan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable lines.
    Text,
    /// One JSON document.
    Json,
}

/// Everything the world-reading commands accept.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // one flag per command line switch
pub struct Options {
    /// The game's data folder.
    pub data: PathBuf,
    /// Plugin names in load order (from `--plugin` and `--load-order`).
    pub plugins: Vec<String>,
    /// Worldspace editor id, when one is chosen.
    pub world: Option<String>,
    /// `plan` output format.
    pub format: Format,
    /// `--json` was given (an alias of `--format json`; `generate` always prints JSON).
    pub json: bool,
    /// `--schema-version N`: the version of the document to print, when one was asked for.
    pub schema_version: Option<u32>,
    /// `plan`: list every block.
    pub blocks: bool,
    /// `heightmap`: cells per block edge of the raster grid.
    pub level: u32,
    /// `heightmap`: output file.
    pub out: Option<PathBuf>,
    /// `heightmap`: allow writing inside a git working tree.
    pub allow_in_repo: bool,
    /// `generate --request FILE`: the request document.
    pub request: Option<PathBuf>,
    /// `generate --events FILE`: write progress events (one JSON line per block) to FILE, or to stderr for `-`.
    pub events: Option<String>,
    /// `generate`: write into a non-empty output folder that has no manifest.
    pub overwrite: bool,
    /// `generate`: write only terrain LOD, no object LOD.
    pub terrain_only: bool,
    /// `generate`: leave land textures out of the diffuse maps (vertex colours only; less memory and time).
    pub no_land_textures: bool,
    /// `generate`: worker threads; 0 chooses from the machine (at most 4).
    pub threads: usize,
}

/// Parse `args` (after the command name). `--data` is required when `require_data` is set.
pub fn parse(args: &[&str], require_data: bool) -> Result<Options, String> {
    let mut data = None;
    let mut plugins = Vec::new();
    let mut options = Options {
        data: PathBuf::new(),
        plugins: Vec::new(),
        world: None,
        format: Format::Text,
        json: false,
        schema_version: None,
        blocks: false,
        level: 32,
        out: None,
        allow_in_repo: false,
        threads: 0,
        request: None,
        events: None,
        overwrite: false,
        terrain_only: false,
        no_land_textures: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .map(|v| (*v).to_owned())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match *arg {
            "--data" => data = Some(PathBuf::from(value("--data")?)),
            "--plugin" => plugins.push(value("--plugin")?),
            "--load-order" => {
                let file = value("--load-order")?;
                let text = std::fs::read_to_string(&file)
                    .map_err(|e| format!("cannot read the load-order file: {e}"))?;
                plugins.extend(crate::load::parse_load_order(&text));
            }
            "--world" => options.world = Some(value("--world")?),
            "--format" => {
                options.format = match value("--format")?.as_str() {
                    "text" => Format::Text,
                    "json" => Format::Json,
                    other => return Err(format!("unknown format {other:?}: use text or json")),
                };
            }
            "--json" => {
                options.json = true;
                options.format = Format::Json;
            }
            "--schema-version" => {
                let text = value("--schema-version")?;
                options.schema_version = Some(text.parse::<u32>().map_err(|_| {
                    format!("--schema-version must be a whole number, got {text:?}")
                })?);
            }
            "--level" => {
                let text = value("--level")?;
                options.level = text.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| {
                    format!("--level must be a positive whole number, got {text:?}")
                })?;
            }
            "--threads" => {
                let text = value("--threads")?;
                options.threads =
                    text.parse::<usize>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| {
                            format!("--threads must be a positive whole number, got {text:?}")
                        })?;
            }
            "--out" => options.out = Some(PathBuf::from(value("--out")?)),
            "--blocks" => options.blocks = true,
            "--request" => options.request = Some(PathBuf::from(value("--request")?)),
            "--overwrite" => options.overwrite = true,
            "--events" => options.events = Some(value("--events")?),
            "--terrain-only" => options.terrain_only = true,
            "--no-land-textures" => options.no_land_textures = true,
            "--allow-in-repo" => options.allow_in_repo = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    options.data = match data {
        Some(data) => data,
        None if require_data => return Err("--data <data-folder> is required".into()),
        None => PathBuf::new(),
    };
    options.plugins = plugins;
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::{Format, parse as parse_with};

    fn parse(args: &[&str]) -> Result<super::Options, String> {
        parse_with(args, true)
    }

    #[test]
    fn data_is_required() {
        assert_eq!(
            parse(&["--plugin", "a.esm"]),
            Err("--data <data-folder> is required".into())
        );
    }

    #[test]
    fn plugins_accumulate_in_order_and_defaults_hold() {
        let o = parse(&["--data", "d", "--plugin", "a.esm", "--plugin", "b.esp"]).unwrap();
        assert_eq!(o.plugins, ["a.esm", "b.esp"]);
        assert_eq!(o.format, Format::Text);
        assert_eq!(o.level, 32);
        assert!(!o.blocks && !o.allow_in_repo);
        assert_eq!(o.world, None);
        assert_eq!(o.out, None);
    }

    #[test]
    fn every_option_is_read() {
        let o = parse(&[
            "--data",
            "d",
            "--world",
            "W",
            "--format",
            "json",
            "--level",
            "8",
            "--out",
            "o.pgm",
            "--blocks",
            "--allow-in-repo",
        ])
        .unwrap();
        assert_eq!(o.world.as_deref(), Some("W"));
        assert_eq!(o.format, Format::Json);
        assert_eq!(o.level, 8);
        assert_eq!(o.out.as_deref(), Some(std::path::Path::new("o.pgm")));
        assert!(o.blocks && o.allow_in_repo);
    }

    #[test]
    fn json_is_an_alias_of_the_json_format_and_the_schema_version_is_read() {
        let o = parse(&["--data", "d", "--json", "--schema-version", "1"]).unwrap();
        assert!(o.json);
        assert_eq!(o.format, Format::Json);
        assert_eq!(o.schema_version, Some(1));
        assert!(
            parse(&["--data", "d", "--schema-version", "x"])
                .unwrap_err()
                .contains("whole number")
        );
        assert!(!parse(&["--data", "d", "--format", "json"]).unwrap().json);
    }

    #[test]
    fn data_is_optional_when_the_command_does_not_need_it() {
        let o = parse_with(&["--json"], false).unwrap();
        assert!(o.data.as_os_str().is_empty());
        assert!(parse_with(&[], true).is_err());
    }

    #[test]
    fn bad_values_and_unknown_options_are_errors() {
        assert!(parse(&["--data"]).unwrap_err().contains("needs a value"));
        assert!(
            parse(&["--data", "d", "--format", "xml"])
                .unwrap_err()
                .contains("xml")
        );
        assert!(parse(&["--data", "d", "--level", "0"]).is_err());
        assert!(parse(&["--data", "d", "--level", "x"]).is_err());
        assert_eq!(
            parse(&["--data", "d", "--nope"]),
            Err("unknown option --nope".into())
        );
        assert!(
            parse(&["--data", "d", "--load-order", "/definitely/not/here"])
                .unwrap_err()
                .starts_with("cannot read the load-order file")
        );
    }

    #[test]
    fn a_load_order_file_is_read_and_combined_with_plugins() {
        let file = std::env::temp_dir().join(format!("lodwright-order-{}.txt", std::process::id()));
        std::fs::write(&file, "# order\n*a.esm\nb.esm\n").unwrap();
        let o = parse(&[
            "--data",
            "d",
            "--load-order",
            file.to_str().unwrap(),
            "--plugin",
            "c.esp",
        ])
        .unwrap();
        assert_eq!(o.plugins, ["a.esm", "b.esm", "c.esp"]);
    }
}
