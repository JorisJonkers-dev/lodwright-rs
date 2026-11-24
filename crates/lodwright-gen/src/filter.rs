// SPDX-License-Identifier: GPL-3.0-only
//! Inclusion rules for object LOD: which of the game's visible objects the caller wants left out or kept.

use lodwright_contract::{IncludeRule, Inclusion};
use lodwright_core::objects::Instance;

/// Whether `text` matches `pattern`, where `*` matches any run of characters (including none).
#[must_use]
pub fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if parts.len() == 1 {
        return pattern == text;
    }
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    for mid in &parts[1..parts.len() - 1] {
        let Some(at) = rest.find(mid) else {
            return false;
        };
        rest = &rest[at + mid.len()..];
    }
    rest.ends_with(last)
}

fn model_key(model: &str) -> String {
    let model = model.to_ascii_lowercase().replace('\\', "/");
    model
        .strip_prefix("meshes/")
        .map_or(model.clone(), str::to_owned)
}

/// The include rules of a request (`inclusion.include_rules`), checked against the load order.
#[derive(Debug, Clone, Default)]
pub struct IncludeRules {
    rules: Vec<(String, Option<usize>, Option<[u8; 4]>)>,
}

impl IncludeRules {
    /// Check `rules` against the load order `plugins`.
    ///
    /// # Errors
    /// A message when a rule names a plugin that is not in the load order or a record type that is not four bytes.
    pub fn new(rules: &[IncludeRule], plugins: &[String]) -> Result<Self, String> {
        let mut out = Vec::with_capacity(rules.len());
        for rule in rules {
            let plugin = match &rule.plugin {
                Some(name) => Some(
                    plugins
                        .iter()
                        .position(|p| p.eq_ignore_ascii_case(name))
                        .ok_or_else(|| {
                            format!("plugin {name} in the include rules is not in the load order")
                        })?,
                ),
                None => None,
            };
            let kind = match &rule.record_type {
                Some(kind) => Some(<[u8; 4]>::try_from(kind.as_bytes()).map_err(|_| {
                    format!("record type {kind:?} in the include rules is not four letters")
                })?),
                None => None,
            };
            out.push((model_key(&rule.models), plugin, kind));
        }
        Ok(Self { rules: out })
    }

    /// Whether there are no rules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Whether a base object with this model and record signature can match a rule (the plugin is not known yet).
    #[must_use]
    pub fn matches_base(&self, model: &str, signature: [u8; 4]) -> bool {
        let model = model_key(model);
        self.rules
            .iter()
            .any(|(p, _, kind)| kind.is_none_or(|k| k == signature) && glob(p, &model))
    }

    /// Whether `instance` matches a rule: its model, its base's record type and the plugin of its reference or base.
    #[must_use]
    pub fn matches(&self, instance: &Instance) -> bool {
        let model = model_key(&instance.model);
        self.rules.iter().any(|(p, plugin, kind)| {
            kind.is_none_or(|k| k == instance.signature)
                && plugin
                    .is_none_or(|i| i == instance.reference_plugin || i == instance.base_plugin)
                && glob(p, &model)
        })
    }

    /// Add to `visible` the references of `candidates` that match a rule and are not in it yet, in the candidates'
    /// order, and return how many matching references were held back (`held_back`: disabled at the start or with an
    /// enable parent) that are not in `visible` either.
    pub fn take(
        &self,
        visible: &mut Vec<Instance>,
        candidates: Vec<Instance>,
        held_back: &[Instance],
    ) -> usize {
        let present: std::collections::HashSet<u32> = visible.iter().map(|i| i.reference).collect();
        visible.extend(
            candidates
                .into_iter()
                .filter(|i| !present.contains(&i.reference) && self.matches(i)),
        );
        held_back
            .iter()
            .filter(|i| !present.contains(&i.reference) && self.matches(i))
            .count()
    }
}

/// Apply `inclusion` to `instances`. `plugins` are the plugin names in load order; the instances carry load-order
/// indices.
///
/// # Errors
/// A message when a rule names a plugin that is not in the load order.
pub fn filter_instances(
    instances: Vec<Instance>,
    inclusion: &Inclusion,
    plugins: &[String],
) -> Result<Vec<Instance>, String> {
    let index_of = |name: &String| {
        plugins
            .iter()
            .position(|p| p.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("plugin {name} in the inclusion rules is not in the load order"))
    };
    let exclude: Vec<usize> = inclusion
        .exclude_plugins
        .iter()
        .map(index_of)
        .collect::<Result<_, _>>()?;
    let only: Vec<usize> = inclusion
        .only_plugins
        .iter()
        .map(index_of)
        .collect::<Result<_, _>>()?;
    let records: Vec<u32> = inclusion
        .exclude_records
        .iter()
        .filter_map(|r| u32::from_str_radix(r, 16).ok())
        .collect();
    let patterns: Vec<String> = inclusion
        .exclude_models
        .iter()
        .map(|p| p.to_ascii_lowercase().replace('\\', "/"))
        .collect();
    Ok(instances
        .into_iter()
        .filter(|i| {
            let from = [i.reference_plugin, i.base_plugin];
            if from.iter().any(|p| exclude.contains(p)) {
                return false;
            }
            if !only.is_empty() && !from.iter().any(|p| only.contains(p)) {
                return false;
            }
            if records.contains(&i.reference) || records.contains(&i.base) {
                return false;
            }
            let model = i.model.to_ascii_lowercase().replace('\\', "/");
            let model = model.strip_prefix("meshes/").unwrap_or(&model);
            !patterns.iter().any(|p| glob(p, model))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{IncludeRules, filter_instances, glob};
    use lodwright_contract::{IncludeRule, Inclusion};
    use lodwright_core::objects::Instance;
    use lodwright_core::plugin::Placement;

    fn inst(model: &str, reference: u32, ref_plugin: usize, base_plugin: usize) -> Instance {
        Instance {
            base: 7,
            signature: *b"STAT",
            model: model.to_owned(),
            placement: Placement {
                pos: [0.0; 3],
                rot: [0.0; 3],
                scale: 1.0,
            },
            high_priority: false,
            has_tree_lod: false,
            reference,
            reference_plugin: ref_plugin,
            base_plugin,
            flagged_by: base_plugin,
            part: false,
            cell: None,
        }
    }

    #[test]
    fn globs_match_runs_and_anchor_both_ends() {
        assert!(glob("clutter/*", "clutter/fence/a.nif"));
        assert!(glob("*/fence/*.nif", "clutter/fence/a.nif"));
        assert!(!glob("clutter/*.nif", "xclutter/a.nif"));
        assert!(glob("a.nif", "a.nif") && !glob("a.nif", "b.nif"));
        assert!(glob("*", "") && glob("a*", "a"));
    }

    #[test]
    fn rules_exclude_by_plugin_record_and_model_and_only_keeps_listed_plugins() {
        let plugins = vec!["Base.esm".to_owned(), "Mod.esp".to_owned()];
        let list = || {
            vec![
                inst("Clutter\\Fence\\A.nif", 0x10, 0, 0),
                inst("architecture/b.nif", 0x11, 1, 0),
                inst("architecture/c.nif", 0x12, 0, 1),
            ]
        };
        let none = Inclusion::default();
        assert_eq!(filter_instances(list(), &none, &plugins).unwrap().len(), 3);
        let by_plugin = Inclusion {
            exclude_plugins: vec!["mod.ESP".into()],
            ..Inclusion::default()
        };
        assert_eq!(
            filter_instances(list(), &by_plugin, &plugins)
                .unwrap()
                .len(),
            1
        );
        let by_record = Inclusion {
            exclude_records: vec!["00000011".into()],
            ..Inclusion::default()
        };
        assert_eq!(
            filter_instances(list(), &by_record, &plugins)
                .unwrap()
                .len(),
            2
        );
        let by_model = Inclusion {
            exclude_models: vec!["clutter/*".into()],
            ..Inclusion::default()
        };
        let kept = filter_instances(list(), &by_model, &plugins).unwrap();
        assert!(
            kept.iter()
                .all(|i| !i.model.to_lowercase().contains("clutter"))
        );
        let only = Inclusion {
            only_plugins: vec!["Mod.esp".into()],
            ..Inclusion::default()
        };
        assert_eq!(filter_instances(list(), &only, &plugins).unwrap().len(), 2);
        let bad = Inclusion {
            exclude_plugins: vec!["Nope.esp".into()],
            ..Inclusion::default()
        };
        assert!(filter_instances(list(), &bad, &plugins).is_err());
    }

    fn rule(models: &str, plugin: Option<&str>, record_type: Option<&str>) -> IncludeRule {
        IncludeRule {
            models: models.to_owned(),
            plugin: plugin.map(str::to_owned),
            record_type: record_type.map(str::to_owned),
        }
    }

    #[test]
    fn an_include_rule_matches_by_model_pattern_plugin_and_record_type() {
        let plugins = ["A.esm".to_owned(), "Lamps.esp".to_owned()];
        let rules = IncludeRules::new(
            &[rule("Lights\\Street*.NIF", Some("lamps.ESP"), Some("STAT"))],
            &plugins,
        )
        .unwrap();
        assert!(rules.matches(&inst("meshes\\lights\\streetlamp01.nif", 1, 1, 0)));
        // the plugin of the base object counts as well as the one of the reference
        assert!(rules.matches(&inst("lights/streetlamp01.nif", 1, 0, 1)));
        assert!(!rules.matches(&inst("lights/streetlamp01.nif", 1, 0, 0)));
        assert!(!rules.matches(&inst("lights/walllamp.nif", 1, 1, 1)));
        let mut activator = inst("lights/streetlamp01.nif", 1, 1, 1);
        activator.signature = *b"ACTI";
        assert!(!rules.matches(&activator));
        assert!(rules.matches_base("Lights/StreetLamp02.nif", *b"STAT"));
        assert!(!rules.matches_base("lights/streetlamp02.nif", *b"ACTI"));
    }

    #[test]
    fn an_include_rule_naming_a_plugin_outside_the_load_order_is_refused() {
        let err = IncludeRules::new(&[rule("*", Some("Gone.esp"), None)], &["A.esm".to_owned()])
            .unwrap_err();
        assert!(err.contains("Gone.esp"), "{err}");
        assert!(IncludeRules::new(&[rule("*", None, Some("ST"))], &[]).is_err());
        assert!(IncludeRules::new(&[], &[]).unwrap().is_empty());
    }

    #[test]
    fn taking_adds_only_new_matching_references_and_counts_the_held_back_ones() {
        let rules = IncludeRules::new(&[rule("lamp*", None, None)], &["A.esm".to_owned()]).unwrap();
        let mut visible = vec![inst("lamp.nif", 1, 0, 0)];
        let candidates = vec![
            inst("lamp.nif", 1, 0, 0),
            inst("lamp.nif", 2, 0, 0),
            inst("wall.nif", 3, 0, 0),
        ];
        let held = [
            inst("lamp.nif", 4, 0, 0),
            inst("lamp.nif", 1, 0, 0),
            inst("wall.nif", 5, 0, 0),
        ];
        assert_eq!(rules.take(&mut visible, candidates, &held), 1);
        assert_eq!(
            visible.iter().map(|i| i.reference).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn an_exclusion_rule_still_wins_over_an_included_object() {
        let rules = IncludeRules::new(&[rule("lamp*", None, None)], &["A.esm".to_owned()]).unwrap();
        let mut visible = Vec::new();
        rules.take(&mut visible, vec![inst("lamp.nif", 2, 0, 0)], &[]);
        let exclude = Inclusion {
            exclude_models: vec!["lamp*".into()],
            ..Inclusion::default()
        };
        let kept = filter_instances(visible, &exclude, &["A.esm".to_owned()]).unwrap();
        assert_eq!(kept.len(), 0);
    }
}
