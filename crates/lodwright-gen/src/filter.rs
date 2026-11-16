// SPDX-License-Identifier: MPL-2.0
//! Inclusion rules for object LOD: which of the game's visible objects the caller wants left out or kept.

use lodwright_contract::Inclusion;
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
    use super::{filter_instances, glob};
    use lodwright_contract::Inclusion;
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
            part: false,
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
}
