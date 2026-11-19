// SPDX-License-Identifier: GPL-3.0-only
//! Defaults v1: every request option a user can set, with its effective default, its allowed values and its tier, so
//! that a settings page is driven by this table instead of a copy of lodwright's defaults.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What kind of value an option takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(extend("x-open-enum" = true))]
pub enum OptionKind {
    /// On or off.
    Switch,
    /// A whole number.
    Integer,
    /// A number with a fraction.
    Number,
    /// A list of text entries.
    List,
}

/// The default of one terrain level, for an option set per level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelDefault {
    /// The level (cells per block edge).
    pub level: u32,
    /// The default at that level.
    pub value: Value,
}

/// One option of the request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptionInfo {
    /// Where the option sits in the request, dotted: `terrain.water`, or `terrain.levels.quality` for a per-level
    /// option.
    pub key: String,
    /// The kind of value.
    pub kind: OptionKind,
    /// The default, as the request holds it when the key is left out; `null` when the default is "the machine
    /// decides" or "no limit". For a per-level option, see `per_level`.
    pub default: Value,
    /// For a per-level option, the default of each level; empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub per_level: Vec<LevelDefault>,
    /// The lowest value `validate` accepts, for a number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    /// The highest value `validate` accepts, for a number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    /// The value must be a power of two.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub power_of_two: bool,
    /// 1: shown to every user; 2: per-level terrain settings; 3: an option a release added, shown behind the
    /// advanced gate.
    pub tier: u8,
    /// The lodwright release that introduced the option.
    pub since: String,
}

/// Defaults v1: the answer of `lodwright defaults --json` and of `lodwright_api::request_defaults`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Defaults {
    /// The document version; 1.
    #[schemars(extend("const" = 1))]
    pub schema_version: u32,
    /// The contract name, `lodwright-defaults`.
    #[schemars(extend("const" = "lodwright-defaults"))]
    pub contract: String,
    /// The lodwright version these defaults belong to.
    pub version: String,
    /// The terrain levels a request may list.
    pub levels: Vec<u32>,
    /// Every option a user can set, in request order.
    pub options: Vec<OptionInfo>,
}

impl Defaults {
    /// The option with `key`.
    #[must_use]
    pub fn option(&self, key: &str) -> Option<&OptionInfo> {
        self.options.iter().find(|o| o.key == key)
    }
}
