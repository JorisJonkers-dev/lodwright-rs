// SPDX-License-Identifier: GPL-3.0-only
//! Which objects stand in a world and can be seen from afar, after load-order overrides.
//!
//! A later plugin's record replaces an earlier one with the same absolute `FormID` as a whole. Base objects are
//! the statics and the like; references place them. A static collection expands into its parts.

use std::collections::HashMap;

use crate::Error;
use crate::plugin::{BaseObject, Placement, Plugin};
use crate::worldspace::{absolute, master_table};

/// Visible When Distant, on bases and references.
pub const VISIBLE_WHEN_DISTANT: u32 = 0x0000_8000;
/// High Priority LOD, on references.
pub const HIGH_PRIORITY_LOD: u32 = 0x0001_0000;
/// Initially Disabled, on references (such a reference is left out of LOD unless it is also persistent: the game's own
/// blocks hold the persistent ones and none of the others).
pub const INITIALLY_DISABLED: u32 = 0x0000_0800;
/// Persistent, on references.
pub const PERSISTENT: u32 = 0x0000_0400;
/// Has Tree LOD, on bases.
pub const HAS_TREE_LOD: u32 = 0x0000_0040;

/// One placed model.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// Absolute `FormID` of the base object that supplies the model.
    pub base: u32,
    /// Record signature of the base object (`STAT`, `TREE` and so on).
    pub signature: [u8; 4],
    /// Model path relative to `meshes`, as the record states it.
    pub model: String,
    /// Where it stands, in world units and radians.
    pub placement: Placement,
    /// The reference is flagged high priority.
    pub high_priority: bool,
    /// The base carries has-tree-LOD.
    pub has_tree_lod: bool,
    /// Absolute `FormID` of the reference.
    pub reference: u32,
    /// Load-order index of the plugin whose reference record won.
    pub reference_plugin: usize,
    /// Load-order index of the plugin whose base object record won.
    pub base_plugin: usize,
    /// Load-order index of the plugin that marks the object visible from far: the one whose base object record won
    /// when the base carries the flag, else the one whose reference record won.
    pub flagged_by: usize,
    /// A part of a static collection: used only when the collection's own model has no LOD model.
    pub part: bool,
    /// The grid of the exterior cell that holds a temporary reference; `None` for a persistent one. Object LOD
    /// assigns blocks by this cell when it is known and by the position otherwise (`game-object-block-cell`).
    pub cell: Option<(i32, i32)>,
}

fn compose(outer: &Placement, inner: &Placement) -> Placement {
    // A part inside a collection: the part's position is rotated and scaled by the collection's placement.
    let (sx, cx) = outer.rot[0].sin_cos();
    let (sy, cy) = outer.rot[1].sin_cos();
    let (sz, cz) = outer.rot[2].sin_cos();
    let p = inner.pos.map(|v| v * outer.scale);
    // rotate about x, then y, then z
    let (y1, z1) = (p[1] * cx - p[2] * sx, p[1] * sx + p[2] * cx);
    let (x2, z2) = (p[0] * cy + z1 * sy, -p[0] * sy + z1 * cy);
    let (x3, y3) = (x2 * cz - y1 * sz, x2 * sz + y1 * cz);
    Placement {
        pos: [outer.pos[0] + x3, outer.pos[1] + y3, outer.pos[2] + z2],
        // approximate: the parts of a collection are rarely rotated against it; the angles add
        rot: [
            outer.rot[0] + inner.rot[0],
            outer.rot[1] + inner.rot[1],
            outer.rot[2] + inner.rot[2],
        ],
        scale: outer.scale * inner.scale,
    }
}

/// A winning reference: base, flags, placement, plugin index and cell.
type Ref = (u32, u32, Placement, usize, Option<(i32, i32)>);

/// The instances in world `world` (absolute `FormID`) that are visible when distant: a reference counts when its
/// base is flagged, or when the reference itself is; initially disabled references do not count, except the persistent
/// ones (the unmodded game's own blocks hold its seven persistent disabled references and no other disabled one).
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn visible_instances(plugins: &[(String, Plugin)], world: u32) -> Result<Vec<Instance>, Error> {
    Ok(gather(plugins, world, true, false, &|base, flags| {
        base.flags & VISIBLE_WHEN_DISTANT != 0 || flags & VISIBLE_WHEN_DISTANT != 0
    })?
    .into_iter()
    .map(|(instance, _, _)| instance)
    .collect())
}

/// A reference with the flags of its winning record, for reports.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// The reference as generation sees it.
    pub instance: Instance,
    /// The flags of the reference record that won the load order.
    pub flags: u32,
    /// The flags of its base object.
    pub base_flags: u32,
}

impl Candidate {
    /// Whether generation would take this reference: something marks it visible from far, and it is not initially
    /// disabled unless it is persistent.
    #[must_use]
    pub fn is_drawn_by_rule(&self) -> bool {
        (self.base_flags | self.flags) & VISIBLE_WHEN_DISTANT != 0
            && (self.flags & INITIALLY_DISABLED == 0 || self.flags & PERSISTENT != 0)
    }

    /// Why generation leaves this reference out, or `None` when it takes it.
    #[must_use]
    pub fn left_out_because(&self) -> Option<&'static str> {
        if (self.base_flags | self.flags) & VISIBLE_WHEN_DISTANT == 0 {
            Some("not marked visible from far")
        } else if self.flags & INITIALLY_DISABLED != 0 && self.flags & PERSISTENT == 0 {
            Some("initially disabled")
        } else {
            None
        }
    }
}

/// Every reference of the world (absolute `FormID`) whose base object has a model, the winning record's flags
/// with it, whether or not anything marks it visible from far, and initially disabled ones included; a static
/// collection is not expanded into its parts. In the order the references first appear. For reports and checks,
/// not for generation.
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn references_with_model(
    plugins: &[(String, Plugin)],
    world: u32,
) -> Result<Vec<Candidate>, Error> {
    Ok(
        gather(plugins, world, false, true, &|base, _| base.model.is_some())?
            .into_iter()
            .map(|(instance, flags, base_flags)| Candidate {
                instance,
                flags,
                base_flags,
            })
            .collect(),
    )
}

#[allow(clippy::too_many_lines)] // one pass over the load order, then one over the references
fn gather(
    plugins: &[(String, Plugin)],
    world: u32,
    parts: bool,
    keep_disabled: bool,
    accept: &dyn Fn(&BaseObject, u32) -> bool,
) -> Result<Vec<(Instance, u32, u32)>, Error> {
    let mut bases: HashMap<u32, (BaseObject, usize)> = HashMap::new();
    let mut refs: HashMap<u32, Ref> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for (index, (_, plugin)) in plugins.iter().enumerate() {
        let table = master_table(plugins, plugin)?;
        let own = u32::try_from(index).map_err(|_| Error::Malformed("too many plugins"))?;
        for base in &plugin.objects {
            let mut base = base.clone();
            base.form_id = absolute(base.form_id, &table, own);
            for part in &mut base.parts {
                part.0 = absolute(part.0, &table, own);
            }
            bases.insert(base.form_id, (base, index));
        }
        for source in plugin
            .worlds
            .iter()
            .filter(|w| absolute(w.form_id, &table, own) == world)
        {
            // the plugin's own references and deletions in file order are not kept apart, so a reference it both
            // deletes and places (a deleted record is followed by its replacement elsewhere) keeps the placement
            for deleted in &source.deleted_refs {
                refs.remove(&absolute(*deleted, &table, own));
            }
            for r in &source.refs {
                let id = absolute(r.form_id, &table, own);
                refs.insert(
                    id,
                    (
                        absolute(r.base, &table, own),
                        r.flags,
                        r.placement,
                        index,
                        r.cell,
                    ),
                );
                if seen.insert(id) {
                    order.push(id);
                }
            }
        }
    }
    let mut out = Vec::new();
    for id in order {
        let Some(&(base_id, flags, placement, reference_plugin, cell)) = refs.get(&id) else {
            continue; // deleted by a later plugin
        };
        let Some((base, base_plugin)) = bases.get(&base_id) else {
            continue;
        };
        if !accept(base, flags) {
            continue;
        }
        // source: oracle-skips-initially-disabled, game-lod-object-rules-disabled-persistent
        if !keep_disabled && flags & INITIALLY_DISABLED != 0 && flags & PERSISTENT == 0 {
            continue;
        }
        let flagged_by = if base.flags & VISIBLE_WHEN_DISTANT != 0 {
            *base_plugin
        } else {
            reference_plugin
        };
        let high_priority = flags & HIGH_PRIORITY_LOD != 0;
        let has_tree_lod = base.flags & HAS_TREE_LOD != 0;
        if let Some(model) = &base.model {
            out.push((
                Instance {
                    base: base_id,
                    signature: base.signature,
                    model: model.clone(),
                    placement,
                    high_priority,
                    has_tree_lod,
                    reference: id,
                    reference_plugin,
                    base_plugin: *base_plugin,
                    flagged_by,
                    part: false,
                    cell,
                },
                flags,
                base.flags,
            ));
        }
        for (part_id, placements) in base.parts.iter().filter(|_| parts) {
            let Some((part_base, part_plugin)) = bases.get(part_id) else {
                continue;
            };
            let Some(model) = part_base.model.clone() else {
                continue;
            };
            for inner in placements {
                out.push((
                    Instance {
                        base: *part_id,
                        signature: *b"STAT",
                        model: model.clone(),
                        placement: compose(&placement, inner),
                        high_priority,
                        has_tree_lod,
                        reference: id,
                        reference_plugin,
                        base_plugin: *part_plugin,
                        flagged_by,
                        part: true,
                        cell,
                    },
                    flags,
                    base.flags,
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{
        INITIALLY_DISABLED, PERSISTENT, VISIBLE_WHEN_DISTANT, references_with_model,
        visible_instances,
    };
    use crate::plugin::{ReadOptions, read_with};
    use lodwright_testkit::plugin::{Group, PluginBuilder, Record};

    fn stat(id: u32, model: &str, flags: u32) -> Record {
        let mut r =
            Record::new(*b"STAT", id).with_subrecord(*b"MODL", format!("{model}\0").into_bytes());
        r.flags = flags;
        r
    }

    fn refr(id: u32, base: u32, flags: u32, x: f32) -> Record {
        let mut data = Vec::new();
        for v in [x, 0.0, 0.0, 0.0, 0.0, 0.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut r = Record::new(*b"REFR", id)
            .with_subrecord(*b"NAME", base.to_le_bytes().to_vec())
            .with_subrecord(*b"DATA", data);
        r.flags = flags;
        r
    }

    fn plugin(masters: &[&str], statics: Vec<Record>, refs: Vec<Record>) -> crate::plugin::Plugin {
        let mut stats = Group::new(0, *b"STAT");
        for s in statics {
            stats = stats.with(s);
        }
        let mut world = Group::new(1, 0x0000_0A00_u32.to_le_bytes());
        for r in refs {
            world = world.with(r);
        }
        let mut b = PluginBuilder::new();
        for m in masters {
            b = b.master(m);
        }
        let bytes = b.group(stats).group(world).build();
        read_with(
            &bytes,
            ReadOptions {
                objects: true,
                ..ReadOptions::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn only_flagged_statics_are_instances_and_a_later_plugin_overrides_a_reference() {
        let base = plugin(
            &[],
            vec![
                stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT),
                stat(0x101, "b.nif", 0),
            ],
            vec![refr(0x200, 0x100, 0, 1.0), refr(0x201, 0x101, 0, 2.0)],
        );
        let patch = plugin(
            &["Base.esm"],
            vec![],
            vec![refr(0x0000_0200, 0x0000_0100, 0, 9.0)],
        );
        let plugins = vec![
            ("Base.esm".to_owned(), base),
            ("Patch.esp".to_owned(), patch),
        ];
        let out = visible_instances(&plugins, 0x0000_0A00).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].model, "a.nif");
        assert_eq!(out[0].placement.pos[0], 9.0);
    }

    #[test]
    fn a_disabled_reference_is_left_out_and_a_flagged_reference_counts_for_an_unflagged_base() {
        let p = plugin(
            &[],
            vec![stat(0x100, "a.nif", 0)],
            vec![
                refr(0x200, 0x100, INITIALLY_DISABLED | VISIBLE_WHEN_DISTANT, 1.0),
                refr(0x201, 0x100, VISIBLE_WHEN_DISTANT, 2.0),
            ],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].reference, 0x201);
    }

    #[test]
    fn a_persistent_disabled_reference_still_counts_as_in_the_games_own_blocks() {
        let p = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT)],
            vec![
                refr(0x200, 0x100, INITIALLY_DISABLED | PERSISTENT, 1.0),
                refr(0x201, 0x100, INITIALLY_DISABLED, 2.0),
            ],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].reference, 0x200);
    }

    #[test]
    fn a_later_plugin_that_enables_a_disabled_reference_brings_it_back() {
        let base = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT)],
            vec![refr(0x200, 0x100, INITIALLY_DISABLED, 1.0)],
        );
        let patch = plugin(&["Base.esm"], vec![], vec![refr(0x200, 0x100, 0, 5.0)]);
        let plugins = vec![
            ("Base.esm".to_owned(), base),
            ("Patch.esp".to_owned(), patch),
        ];
        assert_eq!(visible_instances(&plugins, 0x0000_0A00).unwrap().len(), 1);
        let alone = visible_instances(&plugins[..1], 0x0000_0A00).unwrap();
        assert_eq!(alone.len(), 0);
    }

    /// The reference with an enable parent (`XESP`: the parent's form id and a flags word).
    fn with_enable_parent(mut r: Record) -> Record {
        r = r.with_subrecord(*b"XESP", [0x34, 0x12, 0, 0, 0, 0, 0, 0].to_vec());
        r
    }

    #[test]
    fn an_initially_disabled_reference_with_an_enable_parent_is_left_out_like_one_without() {
        let p = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT)],
            vec![
                with_enable_parent(refr(0x200, 0x100, INITIALLY_DISABLED, 1.0)),
                refr(0x201, 0x100, INITIALLY_DISABLED, 2.0),
                refr(0x202, 0x100, 0, 3.0),
            ],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].reference, 0x202);
    }

    #[test]
    fn a_persistent_disabled_reference_with_an_enable_parent_is_kept() {
        let p = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT)],
            vec![with_enable_parent(refr(
                0x200,
                0x100,
                INITIALLY_DISABLED | PERSISTENT,
                1.0,
            ))],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_later_plugin_that_disables_an_enabled_reference_takes_it_out() {
        let base = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT)],
            vec![refr(0x200, 0x100, 0, 1.0)],
        );
        let patch = plugin(
            &["Base.esm"],
            vec![],
            vec![refr(0x200, 0x100, INITIALLY_DISABLED, 1.0)],
        );
        let plugins = vec![
            ("Base.esm".to_owned(), base),
            ("Patch.esp".to_owned(), patch),
        ];
        assert_eq!(visible_instances(&plugins, 0x0000_0A00).unwrap().len(), 0);
        assert_eq!(
            visible_instances(&plugins[..1], 0x0000_0A00).unwrap().len(),
            1
        );
    }

    #[test]
    fn every_reference_with_a_model_is_listed_with_the_reason_it_is_left_out_or_none() {
        let p = plugin(
            &[],
            vec![
                stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT),
                stat(0x101, "b.nif", 0),
            ],
            vec![
                refr(0x200, 0x100, 0, 1.0),
                refr(0x201, 0x100, INITIALLY_DISABLED, 2.0),
                refr(0x202, 0x100, INITIALLY_DISABLED | PERSISTENT, 3.0),
                refr(0x203, 0x101, 0, 4.0),
                refr(0x204, 0x101, VISIBLE_WHEN_DISTANT, 5.0),
            ],
        );
        let listed = references_with_model(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        let why: Vec<(u32, Option<&str>)> = listed
            .iter()
            .map(|c| (c.instance.reference, c.left_out_because()))
            .collect();
        assert_eq!(
            why,
            [
                (0x200, None),
                (0x201, Some("initially disabled")),
                (0x202, None),
                (0x203, Some("not marked visible from far")),
                (0x204, None),
            ]
        );
        // the report's view agrees with what generation takes
        let taken: Vec<u32> = listed
            .iter()
            .filter(|c| c.is_drawn_by_rule())
            .map(|c| c.instance.reference)
            .collect();
        let generated = visible_instances(
            &[(
                "A.esm".to_owned(),
                plugin(
                    &[],
                    vec![
                        stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT),
                        stat(0x101, "b.nif", 0),
                    ],
                    vec![
                        refr(0x200, 0x100, 0, 1.0),
                        refr(0x201, 0x100, INITIALLY_DISABLED, 2.0),
                        refr(0x202, 0x100, INITIALLY_DISABLED | PERSISTENT, 3.0),
                        refr(0x203, 0x101, 0, 4.0),
                        refr(0x204, 0x101, VISIBLE_WHEN_DISTANT, 5.0),
                    ],
                ),
            )],
            0x0000_0A00,
        )
        .unwrap();
        assert_eq!(
            taken,
            generated.iter().map(|i| i.reference).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_reference_deleted_by_a_later_plugin_drops_out_and_a_third_plugin_can_place_it_again() {
        let base = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT)],
            vec![refr(0x200, 0x100, 0, 1.0), refr(0x201, 0x100, 0, 2.0)],
        );
        // the patch deletes reference 0x200 (a deleted record has no subrecords)
        let mut deleted = Record::new(*b"REFR", 0x0000_0200);
        deleted.flags = 0x0000_0020;
        let patch = plugin(&["Base.esm"], vec![], vec![deleted]);
        let mut plugins = vec![
            ("Base.esm".to_owned(), base),
            ("Patch.esp".to_owned(), patch),
        ];
        let out = visible_instances(&plugins, 0x0000_0A00).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].reference, 0x201);
        // a plugin after the deleting one places the reference again, at a new position
        let again = plugin(
            &["Base.esm"],
            vec![],
            vec![refr(0x0000_0200, 0x0000_0100, 0, 7.0)],
        );
        plugins.push(("Again.esp".to_owned(), again));
        let out = visible_instances(&plugins, 0x0000_0A00).unwrap();
        let mut at: Vec<(u32, f32)> = out
            .iter()
            .map(|i| (i.reference, i.placement.pos[0]))
            .collect();
        at.sort_by_key(|a| a.0);
        assert_eq!(
            at,
            [(0x200, 7.0), (0x201, 2.0)],
            "no reference appears twice"
        );
    }

    #[test]
    fn the_plugin_that_marks_an_object_visible_from_far_is_the_one_whose_flag_counts() {
        // the base is flagged in Base.esm; Patch.esp flags a reference to an unflagged base of its own
        let base = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT)],
            vec![refr(0x200, 0x100, 0, 1.0)],
        );
        let patch = plugin(
            &["Base.esm"],
            vec![stat(0x0100_0100, "b.nif", 0)],
            vec![refr(0x0100_0200, 0x0100_0100, VISIBLE_WHEN_DISTANT, 2.0)],
        );
        let plugins = vec![
            ("Base.esm".to_owned(), base),
            ("Patch.esp".to_owned(), patch),
        ];
        let mut out = visible_instances(&plugins, 0x0000_0A00).unwrap();
        out.sort_by_key(|i| i.reference);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].model.as_str(), out[0].flagged_by), ("a.nif", 0));
        assert_eq!((out[1].model.as_str(), out[1].flagged_by), ("b.nif", 1));
    }
}
