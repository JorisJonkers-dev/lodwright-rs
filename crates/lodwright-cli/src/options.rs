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
pub struct Options {
    /// The game's data folder.
    pub data: PathBuf,
    /// Plugin names in load order (from `--plugin` and `--load-order`).
    pub plugins: Vec<String>,
    /// Worldspace editor id, when one is chosen.
    pub world: Option<String>,
    /// `plan` output format.
    pub format: Format,
    /// `plan`: list every block.
    pub blocks: bool,
    /// `heightmap`: cells per block edge of the raster grid.
    pub level: u32,
    /// `heightmap`: output file.
    pub out: Option<PathBuf>,
    /// `heightmap`: allow writing inside a git working tree.
    pub allow_in_repo: bool,
}

/// Parse `args` (after the command name).
pub fn parse(args: &[&str]) -> Result<Options, String> {
    let mut data = None;
    let mut plugins = Vec::new();
    let mut options = Options {
        data: PathBuf::new(),
        plugins: Vec::new(),
        world: None,
        format: Format::Text,
        blocks: false,
        level: 32,
        out: None,
        allow_in_repo: false,
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
            "--level" => {
                let text = value("--level")?;
                options.level = text.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| {
                    format!("--level must be a positive whole number, got {text:?}")
                })?;
            }
            "--out" => options.out = Some(PathBuf::from(value("--out")?)),
            "--blocks" => options.blocks = true,
            "--allow-in-repo" => options.allow_in_repo = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    options.data = data.ok_or("--data <data-folder> is required")?;
    options.plugins = plugins;
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::{Format, parse};

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
