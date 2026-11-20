// SPDX-License-Identifier: GPL-3.0-only
//! Bands v1: the minimum and maximum of every statistic of several `verify` reports of the same inputs, from which
//! tolerance bands are written. A band whose minimum equals its maximum is an exact match across the runs.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::VerifyDoc;

/// The range of one statistic over the runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Band {
    /// The statistic, for example `water.mean` or `water.iou:<block path>`.
    pub statistic: String,
    /// The smallest value, rounded to three decimals.
    pub min: f64,
    /// The largest value, rounded to three decimals.
    pub max: f64,
    /// How many of the runs report the statistic (a per-block one is missing where that block is).
    pub runs: u64,
}

/// Bands v1: the answer of `lodwright bands`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BandsDoc {
    /// The document version; 1.
    #[schemars(extend("const" = 1))]
    pub schema_version: u32,
    /// The contract name, `lodwright-bands`.
    #[schemars(extend("const" = "lodwright-bands"))]
    pub contract: String,
    /// The number of reports read.
    pub runs: u64,
    /// One band per statistic, sorted by statistic.
    pub bands: Vec<Band>,
}

/// Rounded to three decimals, as the result document rounds, so the order of float operations does not reach the
/// output.
fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Every statistic of `doc`, by name. Counts are whole numbers; ratios are rounded to three decimals.
#[must_use]
#[allow(clippy::cast_precision_loss)] // counts far below 2^52
pub fn statistics(doc: &VerifyDoc) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    let mut put = |name: String, value: f64| {
        out.insert(name, round3(value));
    };
    for (name, value) in [
        ("terrain_blocks", doc.terrain_blocks),
        ("object_blocks", doc.object_blocks),
        ("tree_blocks", doc.tree_blocks),
        ("textures", doc.textures),
        ("settings", doc.settings),
        ("errors", doc.errors.len() as u64),
        ("differences", doc.differences.len() as u64),
    ] {
        put(name.to_owned(), value as f64);
    }
    if let Some(w) = &doc.water {
        put("water.blocks".into(), w.blocks as f64);
        put("water.under_threshold".into(), w.under_threshold as f64);
        if let Some(v) = w.lowest {
            put("water.lowest".into(), v);
        }
        if let Some(v) = w.mean {
            put("water.mean".into(), v);
        }
        for b in &w.per_block {
            put(format!("water.iou:{}", b.path), b.iou);
        }
    }
    if let Some(t) = &doc.terrain {
        put("terrain.blocks".into(), t.blocks as f64);
        put("terrain.over_tolerance".into(), t.over_tolerance as f64);
        if let Some(v) = t.max {
            put("terrain.max".into(), v);
        }
        if let Some(v) = t.mean {
            put("terrain.mean".into(), v);
        }
        for b in &t.per_block {
            put(format!("terrain.max:{}", b.path), b.max);
        }
    }
    if let Some(o) = &doc.objects {
        put("objects.blocks".into(), o.blocks as f64);
        put("objects.exact".into(), o.exact as f64);
        for b in &o.per_block {
            put(
                format!("objects.unmatched_vertices:{}", b.path),
                b.unmatched_vertices as f64,
            );
        }
    }
    out
}

/// The bands of `docs`: per statistic the minimum and maximum over the reports that have it.
#[must_use]
pub fn bands(docs: &[VerifyDoc]) -> BandsDoc {
    let mut ranges: BTreeMap<String, (f64, f64, u64)> = BTreeMap::new();
    for doc in docs {
        for (name, value) in statistics(doc) {
            let e = ranges.entry(name).or_insert((value, value, 0));
            e.0 = e.0.min(value);
            e.1 = e.1.max(value);
            e.2 += 1;
        }
    }
    BandsDoc {
        schema_version: 1,
        contract: crate::BANDS_CONTRACT.to_owned(),
        runs: docs.len() as u64,
        bands: ranges
            .into_iter()
            .map(|(statistic, (min, max, runs))| Band {
                statistic,
                min,
                max,
                runs,
            })
            .collect(),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // the values are rounded to three decimals, so equal means equal
mod tests {
    use super::{bands, statistics};
    use crate::{VerifyDoc, WaterBlock, WaterReport};

    fn report(iou: f64) -> VerifyDoc {
        let mut doc = VerifyDoc::new();
        doc.terrain_blocks = 4;
        doc.water = Some(WaterReport {
            blocks: 1,
            threshold: 0.95,
            under_threshold: u64::from(iou < 0.95),
            lowest: Some(iou),
            mean: Some(iou),
            per_block: vec![WaterBlock {
                path: "meshes/w.nif".into(),
                iou,
            }],
            per_level: Vec::new(),
        });
        doc
    }

    #[test]
    fn identical_runs_give_zero_width_bands() {
        let b = bands(&[report(0.9), report(0.9), report(0.9)]);
        assert_eq!(b.runs, 3);
        assert_eq!(
            b.bands.len(),
            7 + 5,
            "seven counts and five water statistics"
        );
        assert!(
            b.bands
                .iter()
                .all(|band| band.min == band.max && band.runs == 3)
        );
    }

    #[test]
    fn a_changed_run_widens_only_the_statistics_that_changed() {
        let b = bands(&[report(0.9), report(0.97)]);
        let band = |name: &str| b.bands.iter().find(|x| x.statistic == name).unwrap();
        assert_eq!(
            (band("water.mean").min, band("water.mean").max),
            (0.9, 0.97)
        );
        assert_eq!(
            (
                band("water.under_threshold").min,
                band("water.under_threshold").max
            ),
            (0.0, 1.0)
        );
        assert_eq!(band("terrain_blocks").min, band("terrain_blocks").max);
    }

    #[test]
    fn values_are_rounded_to_three_decimals_so_float_noise_does_not_widen_a_band() {
        let b = bands(&[report(0.333_333_3), report(0.333_333_4)]);
        let mean = b
            .bands
            .iter()
            .find(|x| x.statistic == "water.mean")
            .unwrap();
        assert_eq!((mean.min, mean.max), (0.333, 0.333));
    }

    #[test]
    fn a_statistic_missing_from_a_run_counts_only_the_runs_that_have_it() {
        let mut bare = report(0.9);
        bare.water = None;
        let b = bands(&[report(0.9), bare]);
        let iou = b
            .bands
            .iter()
            .find(|x| x.statistic == "water.iou:meshes/w.nif")
            .unwrap();
        assert_eq!(iou.runs, 1);
        assert!(statistics(&report(0.5)).contains_key("water.iou:meshes/w.nif"));
    }
}
