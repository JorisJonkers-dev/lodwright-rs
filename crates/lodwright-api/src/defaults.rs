// SPDX-License-Identifier: GPL-3.0-only
//! The effective request defaults, as a table a settings page reads instead of copying them.
//!
//! Every default is taken from the code that applies it: the request's own `Default` (through [`Request::new`]) and
//! the terrain level settings of `lodwright-core`; every range from the constants `Request::validate` checks.

use lodwright_contract::{
    DEFAULTS_CONTRACT, DROP_UNDER_WATER_RANGE, Defaults, LevelDefault, MAX_VERTICES_RANGE,
    OptionInfo, OptionKind, QUALITY_MIN, Request, TERRAIN_LEVELS, TEXTURE_SCALE_RANGE,
    TEXTURE_SIZE_RANGE, THREADS_MIN,
};
use lodwright_core::terrain::{LevelSettings, TerrainSettings};
use serde_json::Value;

/// The release that introduced the request document and every option of its first version.
const REQUEST_V1_SINCE: &str = "0.3.0";
/// The release that introduces the options added after 0.5.0.
const NEXT_RELEASE: &str = "0.6.0";
/// The release that introduces specular in the alpha of the normal maps.
const SPECULAR_SINCE: &str = "0.7.0";
/// The release that introduces skirts at block borders.
const SKIRTS_SINCE: &str = "0.9.0";
/// The release that introduces dropping terrain faces under water.
const DROP_WATER_SINCE: &str = "0.9.0";
/// The release that adds `terrain.water_walls`.
const WALLS_SINCE: &str = "0.14.0";
/// The release that adds `terrain.water_polygons`.
const POLYGONS_SINCE: &str = "0.14.0";
/// The release that adds `terrain.compose_shipped_tiles`.
const SHIPPED_SINCE: &str = "0.16.0";
/// The release that adds `output.pack`.
const PACK_SINCE: &str = "0.15.0";

/// The value at a dotted `key` of `doc`, `null` when the key is left out.
fn at(doc: &Value, key: &str) -> Value {
    key.split('.')
        .try_fold(doc, |v, part| v.get(part))
        .cloned()
        .unwrap_or(Value::Null)
}

fn option(doc: &Value, key: &str, kind: OptionKind) -> OptionInfo {
    OptionInfo {
        key: key.to_owned(),
        kind,
        default: at(doc, key),
        per_level: Vec::new(),
        minimum: None,
        maximum: None,
        power_of_two: false,
        tier: 1,
        since: REQUEST_V1_SINCE.to_owned(),
    }
}

fn per_level(key: &str, kind: OptionKind, value: impl Fn(&LevelSettings) -> Value) -> OptionInfo {
    OptionInfo {
        key: format!("terrain.levels.{key}"),
        kind,
        default: Value::Null,
        per_level: TerrainSettings::default()
            .levels
            .iter()
            .map(|l| LevelDefault {
                level: l.level,
                value: value(l),
            })
            .collect(),
        minimum: None,
        maximum: None,
        power_of_two: false,
        tier: 2,
        since: REQUEST_V1_SINCE.to_owned(),
    }
}

/// Every request option a user can set, with its effective default, its allowed range and its tier.
///
/// # Panics
/// Never: a request serialises.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one row per request option")]
pub fn request_defaults() -> Defaults {
    let doc = serde_json::to_value(Request::new("", &[], "")).expect("a request serialises");
    let mut options = vec![option(&doc, "worlds", OptionKind::List)];
    for key in ["terrain.enabled", "terrain.land_textures", "terrain.water"] {
        options.push(option(&doc, key, OptionKind::Switch));
    }
    options.push(OptionInfo {
        default: Value::Object(serde_json::Map::new()),
        minimum: Some(f64::from(TEXTURE_SCALE_RANGE.0)),
        maximum: Some(f64::from(TEXTURE_SCALE_RANGE.1)),
        tier: 3,
        since: NEXT_RELEASE.to_owned(),
        ..option(&doc, "terrain.texture_scales", OptionKind::Map)
    });
    options.push(OptionInfo {
        minimum: Some(f64::from(DROP_UNDER_WATER_RANGE.0)),
        maximum: Some(f64::from(DROP_UNDER_WATER_RANGE.1)),
        tier: 3,
        since: DROP_WATER_SINCE.to_owned(),
        ..option(&doc, "terrain.drop_under_water", OptionKind::Number)
    });
    // switches that are off by default are not written while off: their default comes from the request type
    let terrain = Request::new("", &[], "").terrain;
    for (key, default, since) in [
        ("terrain.water_walls", terrain.water_walls, WALLS_SINCE),
        (
            "terrain.water_shared_vertices",
            terrain.water_shared_vertices,
            WALLS_SINCE,
        ),
        (
            "terrain.water_polygons",
            terrain.water_polygons,
            POLYGONS_SINCE,
        ),
        (
            "terrain.compose_shipped_tiles",
            terrain.compose_shipped_tiles,
            SHIPPED_SINCE,
        ),
    ] {
        options.push(OptionInfo {
            default: Value::Bool(default),
            tier: 3,
            since: since.to_owned(),
            ..option(&doc, key, OptionKind::Switch)
        });
    }
    options.push(option(&doc, "objects.enabled", OptionKind::Switch));
    // options that are off by default are not written while off: their default comes from the request type
    let objects = Request::new("", &[], "").objects;
    for (key, default) in [
        ("objects.drop_under_terrain", objects.drop_under_terrain),
        ("objects.drop_under_water", objects.drop_under_water),
    ] {
        options.push(OptionInfo {
            default: Value::Bool(default),
            tier: 3,
            since: NEXT_RELEASE.to_owned(),
            ..option(&doc, key, OptionKind::Switch)
        });
    }
    options.push(option(&doc, "trees.enabled", OptionKind::Switch));
    // off by default and not written while off: the default comes from the request type
    options.push(OptionInfo {
        default: Value::Bool(Request::new("", &[], "").output.pack),
        tier: 3,
        since: PACK_SINCE.to_owned(),
        ..option(&doc, "output.pack", OptionKind::Switch)
    });
    options.push(OptionInfo {
        minimum: Some(f64::from(QUALITY_MIN)),
        ..per_level("quality", OptionKind::Number, |l| {
            Value::from(f64::from(l.quality))
        })
    });
    options.push(OptionInfo {
        minimum: Some(f64::from(MAX_VERTICES_RANGE.0)),
        maximum: Some(f64::from(MAX_VERTICES_RANGE.1)),
        ..per_level("max_vertices", OptionKind::Integer, |l| {
            Value::from(l.max_vertices)
        })
    });
    for (key, size) in [
        (
            "diffuse_size",
            (|l: &LevelSettings| l.diffuse_size) as fn(&LevelSettings) -> u32,
        ),
        ("normal_size", |l: &LevelSettings| l.normal_size),
    ] {
        options.push(OptionInfo {
            minimum: Some(f64::from(TEXTURE_SIZE_RANGE.0)),
            maximum: Some(f64::from(TEXTURE_SIZE_RANGE.1)),
            power_of_two: true,
            ..per_level(key, OptionKind::Integer, |l| Value::from(size(l)))
        });
    }
    options.push(OptionInfo {
        tier: 3,
        since: SPECULAR_SINCE.to_owned(),
        ..per_level("specular_alpha", OptionKind::Switch, |l| {
            Value::Bool(l.specular_alpha)
        })
    });
    options.push(OptionInfo {
        tier: 3,
        since: SKIRTS_SINCE.to_owned(),
        ..per_level("skirts", OptionKind::Switch, |l| Value::Bool(l.skirts))
    });
    for key in [
        "inclusion.exclude_plugins",
        "inclusion.only_plugins",
        "inclusion.exclude_records",
        "inclusion.exclude_models",
    ] {
        options.push(option(&doc, key, OptionKind::List));
    }
    options.push(OptionInfo {
        minimum: Some(f64::from(THREADS_MIN)),
        ..option(&doc, "limits.threads", OptionKind::Integer)
    });
    options.push(option(&doc, "limits.memory_mib", OptionKind::Integer));
    Defaults {
        schema_version: 1,
        contract: DEFAULTS_CONTRACT.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        levels: TERRAIN_LEVELS.to_vec(),
        options,
    }
}

#[cfg(test)]
mod tests {
    use super::{at, request_defaults};
    use lodwright_contract::{OptionKind, Request};
    use lodwright_core::terrain::TerrainSettings;
    use serde_json::{Value, json};

    fn base() -> Value {
        serde_json::to_value(Request::new("/data", &["A.esm"], "/out")).unwrap()
    }

    /// The default request with `key` set to `value` (for a per-level key: every default level, one field changed);
    /// `None` when the request type cannot hold the value (a negative count).
    fn with(key: &str, value: &Value) -> Option<Request> {
        let mut doc = base();
        if let Some(field) = key.strip_prefix("terrain.levels.") {
            let levels: Vec<Value> = TerrainSettings::default()
                .levels
                .iter()
                .map(|l| {
                    let mut level = json!({
                        "level": l.level,
                        "quality": l.quality,
                        "max_vertices": l.max_vertices,
                        "diffuse_size": l.diffuse_size,
                        "normal_size": l.normal_size,
                    });
                    level[field] = value.clone();
                    level
                })
                .collect();
            doc["terrain"]["levels"] = Value::from(levels);
        } else {
            let (parent, last) = key.rsplit_once('.').unwrap_or(("", key));
            let target = if parent.is_empty() {
                &mut doc
            } else {
                parent
                    .split('.')
                    .fold(&mut doc, |v, part| v.get_mut(part).unwrap())
            };
            target[last] = value.clone();
        }
        serde_json::from_value(doc).ok()
    }

    #[test]
    fn every_default_is_the_one_the_request_and_the_terrain_settings_use() {
        let defaults = request_defaults();
        let doc = base();
        let levels = TerrainSettings::default().levels;
        for o in &defaults.options {
            if o.key.starts_with("terrain.levels.") {
                assert_eq!(o.default, Value::Null, "{}", o.key);
                assert_eq!(o.per_level.len(), levels.len(), "{}", o.key);
                for (d, l) in o.per_level.iter().zip(&levels) {
                    assert_eq!(d.level, l.level);
                    // the default level, written as a request would write it, validates and holds this value
                    let request = with(&o.key, &d.value).unwrap();
                    assert_eq!(request.validate(), Ok(()), "{}", o.key);
                }
            } else {
                // an option that is empty or off by default is not written while so
                let written = match at(&doc, &o.key) {
                    Value::Null if o.kind == OptionKind::Map => {
                        Value::Object(serde_json::Map::new())
                    }
                    Value::Null if o.kind == OptionKind::Switch => Value::Bool(false),
                    v => v,
                };
                assert_eq!(o.default, written, "{}", o.key);
            }
        }
        assert_eq!(
            defaults.levels,
            levels.iter().map(|l| l.level).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_range_is_the_one_validate_checks() {
        for o in request_defaults().options {
            let accepts = |v: f64| {
                #[allow(clippy::cast_possible_truncation)] // the bounds are small whole numbers
                let value = match o.kind {
                    OptionKind::Integer => Value::from(v as i64),
                    // a map's range is the range of each value
                    OptionKind::Map => json!({ "0000ABCD": v }),
                    _ => Value::from(v),
                };
                with(&o.key, &value).is_some_and(|r| r.validate().is_ok())
            };
            if let Some(min) = o.minimum {
                assert!(accepts(min), "{} accepts its minimum", o.key);
                let below = if o.kind == OptionKind::Integer {
                    min - 1.0
                } else {
                    min - 0.5
                };
                assert!(!accepts(below), "{} refuses below its minimum", o.key);
            }
            if let Some(max) = o.maximum {
                assert!(accepts(max), "{} accepts its maximum", o.key);
                assert!(!accepts(max + 1.0), "{} refuses above its maximum", o.key);
            }
            if o.power_of_two {
                let min = o.minimum.unwrap();
                assert!(
                    !accepts(min * 3.0),
                    "{} refuses a size that is not a power of two",
                    o.key
                );
            }
        }
    }

    #[test]
    fn the_options_are_tiered_and_dated() {
        let d = request_defaults();
        assert_eq!(d.option("terrain.water").unwrap().tier, 1);
        assert_eq!(d.option("terrain.levels.quality").unwrap().tier, 2);
        assert!(d.options.iter().all(|o| o.since == "0.3.0"
            || (o.tier == 3
                && (o.since == "0.6.0"
                    || o.since == "0.7.0"
                    || o.since == "0.9.0"
                    || o.since == "0.14.0"
                    || o.since == "0.15.0"))));
        let scales = d.option("terrain.texture_scales").unwrap();
        assert_eq!((scales.tier, scales.kind), (3, OptionKind::Map));
        let drop = d.option("objects.drop_under_water").unwrap();
        assert_eq!((drop.tier, drop.default.clone()), (3, Value::Bool(false)));
        assert_eq!(d.option("limits.threads").unwrap().default, Value::Null);
        assert_eq!(
            d.option("terrain.water").unwrap().default,
            Value::Bool(true)
        );
    }
}
