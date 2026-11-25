// SPDX-License-Identifier: GPL-3.0-only
//! Which objects stand in a world and can be seen from afar, after load-order overrides.
//!
//! A later plugin's record replaces an earlier one with the same absolute `FormID` as a whole. Base objects are
//! the statics and the like; references place them. A static collection expands into its parts.

use std::collections::{HashMap, HashSet};

use crate::Error;
use crate::plugin::{BaseObject, Placement, Plugin};
use crate::worldspace::{absolute, master_table};

/// Visible When Distant, on bases and references.
pub const VISIBLE_WHEN_DISTANT: u32 = 0x0000_8000;
/// High Priority LOD, on references and on statics (bit 16; on activators and doors the same bit means Random Anim
/// Start, so only a static's bit counts).
pub const HIGH_PRIORITY_LOD: u32 = 0x0001_0000;
// source: game-lod-object-rules-disabled-persistent
/// Initially Disabled, on references. Such a reference is not in the world when the game starts and is left out of LOD,
/// unless it is also persistent: the game's own blocks hold its seven persistent disabled references (the Helios One
/// solar reflectors among them), so the far view's own objects keep them (see
/// `docs/decisions/2026-11-24-far-view-draws-what-stands-at-the-start.md`).
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
    /// The reference or its static base is flagged high priority: the object also goes to the block's `high` file,
    /// which the game keeps drawn much farther away than the normal one.
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

/// A winning reference: base, flags, placement, plugin index, cell and enable parent (absolute `FormID`, opposite).
type Ref = (
    u32,
    u32,
    Placement,
    usize,
    Option<(i32, i32)>,
    Option<(u32, bool)>,
);

/// The start state a reference needs: its flags and its enable parent (absolute `FormID`, opposite).
pub(crate) type StartState = (u32, Option<(u32, bool)>);

/// Whether reference `id` is disabled when the game starts: initially disabled itself, or enabled through an enable
/// parent that is disabled then (or, with the opposite flag, one that is enabled then). `state` gives a reference's
/// winning flags and enable parent, in any world of the load order. A parent that is not a reference of any world (an
/// actor, or a reference in an interior) counts as enabled. A chain is followed to its end however long it is; a loop
/// counts as enabled.
pub(crate) fn disabled_at_start(state: &dyn Fn(u32) -> Option<StartState>, id: u32) -> bool {
    let mut at = id;
    let mut flips = false;
    let mut seen: HashSet<u32> = HashSet::new();
    while seen.insert(at) {
        let Some((flags, parent)) = state(at) else {
            return flips;
        };
        if flags & INITIALLY_DISABLED != 0 {
            return !flips;
        }
        let Some((parent, opposite)) = parent else {
            return flips;
        };
        flips ^= opposite;
        at = parent;
    }
    false
}

/// The winning flags and enable parent of every reference in every world but `world`, for enable parents that stand
/// in another world than their children (a marker in the Strip that turns on lights seen from the Mojave).
/// A plugin whose masters are not in the load order is passed over: [`gather`] has refused the load order already.
pub(crate) fn start_states_elsewhere(
    plugins: &[(String, Plugin)],
    world: u32,
) -> HashMap<u32, StartState> {
    let mut out: HashMap<u32, StartState> = HashMap::new();
    for (index, (_, plugin)) in plugins.iter().enumerate() {
        let (Ok(table), Ok(own)) = (master_table(plugins, plugin), u32::try_from(index)) else {
            continue;
        };
        for source in plugin
            .worlds
            .iter()
            .filter(|w| absolute(w.form_id, &table, own) != world)
        {
            for deleted in &source.deleted_refs {
                out.remove(&absolute(*deleted, &table, own));
            }
            for r in &source.refs {
                out.insert(
                    absolute(r.form_id, &table, own),
                    (
                        r.flags,
                        r.enable_parent
                            .map(|p| (absolute(p.form_id, &table, own), p.opposite)),
                    ),
                );
            }
        }
    }
    out
}

/// The instances in world `world` (absolute `FormID`) that are visible when distant: a reference counts when its
/// base is flagged, or when the reference itself is, and it stands in the world when the game starts. Initially
/// disabled references and references whose enable parent keeps them disabled at the start do not count, except
/// initially disabled persistent references, which the game's own blocks hold whatever their enable parent; the
/// winning record of each reference decides.
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn visible_instances(plugins: &[(String, Plugin)], world: u32) -> Result<Vec<Instance>, Error> {
    Ok(gather(plugins, world, true, false, &|base, flags| {
        base.flags & VISIBLE_WHEN_DISTANT != 0 || flags & VISIBLE_WHEN_DISTANT != 0
    })?
    .into_iter()
    .map(|(instance, _, _, _)| instance)
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
    /// Its enable parent keeps it disabled when the game starts.
    pub hidden_by_parent: bool,
}

impl Candidate {
    /// Whether generation would take this reference: something marks it visible from far, and it stands in the world
    /// when the game starts or is an initially disabled persistent reference (the game's own blocks hold those).
    #[must_use]
    pub fn is_drawn_by_rule(&self) -> bool {
        self.left_out_because().is_none()
    }

    /// Why generation leaves this reference out, or `None` when it takes it.
    #[must_use]
    pub fn left_out_because(&self) -> Option<&'static str> {
        if (self.base_flags | self.flags) & VISIBLE_WHEN_DISTANT == 0 {
            Some("not marked visible from far")
        } else if self.flags & INITIALLY_DISABLED != 0 && self.flags & PERSISTENT == 0 {
            Some("initially disabled")
        } else if self.hidden_by_parent {
            Some("disabled at the start by its enable parent")
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
            .map(
                |(instance, flags, base_flags, hidden_by_parent)| Candidate {
                    instance,
                    flags,
                    base_flags,
                    hidden_by_parent,
                },
            )
            .collect(),
    )
}

/// The references in world `world` (absolute `FormID`) whose base object's model is one of `models` (paths relative to
/// `meshes`, any case and either separator), whether or not anything marks them visible from far. Deleted references,
/// every initially disabled reference (persistent ones too) and every reference its enable parent keeps disabled at the
/// start are left out, as for the far view's own objects. Static collections are not expanded.
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn references_of_models(
    plugins: &[(String, Plugin)],
    world: u32,
    models: &[&str],
) -> Result<Vec<Instance>, Error> {
    if models.is_empty() {
        return Ok(Vec::new());
    }
    let key = |m: &str| {
        let m = m.to_ascii_lowercase().replace('\\', "/");
        m.strip_prefix("meshes/").map_or(m.clone(), str::to_owned)
    };
    let wanted: std::collections::HashSet<String> = models.iter().map(|m| key(m)).collect();
    Ok(references_where(plugins, world, &|base| {
        base.model
            .as_deref()
            .is_some_and(|m| wanted.contains(&key(m)))
    })?
    .taken)
}

/// What [`references_where`] found: the references that stand in the world when the game starts, and those left out
/// because they are disabled then (themselves, or through their enable parent chain).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Matched {
    /// Enabled at the start, through their enable parent chain too.
    pub taken: Vec<Instance>,
    /// Initially disabled (persistent ones too) or kept disabled at the start by their enable parent chain: a far block
    /// shows what stands when the game starts.
    pub held_back: Vec<Instance>,
}

/// The references in world `world` (absolute `FormID`) whose winning base object passes `accept`, whether or not
/// anything marks them visible from far, in load order. Deleted references are left out; static collections are not
/// expanded.
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn references_where(
    plugins: &[(String, Plugin)],
    world: u32,
    accept: &dyn Fn(&BaseObject) -> bool,
) -> Result<Matched, Error> {
    let mut out = Matched::default();
    for (instance, flags, _, hidden_by_parent) in
        gather(plugins, world, false, true, &|base, _| accept(base))?
    {
        if flags & INITIALLY_DISABLED != 0 || hidden_by_parent {
            out.held_back.push(instance);
        } else {
            out.taken.push(instance);
        }
    }
    Ok(out)
}

/// The object bounds (`OBND`) of every base object by absolute `FormID`, as the winning record states them.
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn base_bounds(plugins: &[(String, Plugin)]) -> Result<HashMap<u32, [i16; 6]>, Error> {
    let mut out = HashMap::new();
    for (index, (_, plugin)) in plugins.iter().enumerate() {
        let table = master_table(plugins, plugin)?;
        let own = u32::try_from(index).map_err(|_| Error::Malformed("too many plugins"))?;
        for base in &plugin.objects {
            let id = absolute(base.form_id, &table, own);
            match base.bounds {
                Some(b) => {
                    out.insert(id, b);
                }
                None => {
                    out.remove(&id);
                }
            }
        }
    }
    Ok(out)
}

/// High Priority LOD set on a static base (on other base types bit 16 means something else).
fn static_high_priority(base: &BaseObject) -> bool {
    base.signature == *b"STAT" && base.flags & HIGH_PRIORITY_LOD != 0
}

#[allow(clippy::too_many_lines)] // one pass over the load order, then one over the references
fn gather(
    plugins: &[(String, Plugin)],
    world: u32,
    parts: bool,
    keep_disabled: bool,
    accept: &dyn Fn(&BaseObject, u32) -> bool,
) -> Result<Vec<(Instance, u32, u32, bool)>, Error> {
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
                        r.enable_parent
                            .map(|p| (absolute(p.form_id, &table, own), p.opposite)),
                    ),
                );
                if seen.insert(id) {
                    order.push(id);
                }
            }
        }
    }
    // a parent in another world is looked up there; those references are read only when a parent is not found here
    let elsewhere: std::cell::OnceCell<HashMap<u32, StartState>> = std::cell::OnceCell::new();
    let state = |at: u32| -> Option<StartState> {
        if let Some(&(_, flags, _, _, _, parent)) = refs.get(&at) {
            return Some((flags, parent));
        }
        elsewhere
            .get_or_init(|| start_states_elsewhere(plugins, world))
            .get(&at)
            .copied()
    };
    let mut out = Vec::new();
    for id in order {
        let Some(&(base_id, flags, placement, reference_plugin, cell, _)) = refs.get(&id) else {
            continue; // deleted by a later plugin
        };
        let Some((base, base_plugin)) = bases.get(&base_id) else {
            continue;
        };
        if !accept(base, flags) {
            continue;
        }
        // source: oracle-skips-initially-disabled, game-lod-object-rules-disabled-persistent
        // what stands at the start: initially disabled references and references their enable parent keeps disabled
        // are left out; initially disabled persistent references stay, as the game's own blocks hold them
        let hidden_by_parent = flags & INITIALLY_DISABLED == 0 && disabled_at_start(&state, id);
        let disabled = flags & INITIALLY_DISABLED != 0 && flags & PERSISTENT == 0;
        if !keep_disabled && (disabled || hidden_by_parent) {
            continue;
        }
        let flagged_by = if base.flags & VISIBLE_WHEN_DISTANT != 0 {
            *base_plugin
        } else {
            reference_plugin
        };
        // source: oracle-high-priority-on-static
        let high_priority = flags & HIGH_PRIORITY_LOD != 0 || static_high_priority(base);
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
                hidden_by_parent,
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
                        // a part is a static of its own: its flag counts as well as the collection's reference's
                        high_priority: high_priority || static_high_priority(part_base),
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
                    hidden_by_parent,
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{
        HIGH_PRIORITY_LOD, INITIALLY_DISABLED, PERSISTENT, VISIBLE_WHEN_DISTANT,
        references_with_model, visible_instances,
    };
    use crate::plugin::{ReadOptions, read_with};
    use lodwright_testkit::plugin::{Group, PluginBuilder, Record};
    use lodwright_testkit::{covers, ids};

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
    fn high_priority_on_a_static_base_counts_as_on_the_reference() {
        covers(&[ids::objects::selection::HIGH_PRIORITY]);
        let p = plugin(
            &[],
            vec![
                stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT | HIGH_PRIORITY_LOD),
                stat(0x101, "b.nif", VISIBLE_WHEN_DISTANT),
            ],
            vec![
                refr(0x200, 0x100, 0, 1.0),
                refr(0x201, 0x101, HIGH_PRIORITY_LOD, 2.0),
                refr(0x202, 0x101, 0, 3.0),
            ],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        let high: Vec<(u32, bool)> = out.iter().map(|i| (i.reference, i.high_priority)).collect();
        assert_eq!(high, vec![(0x200, true), (0x201, true), (0x202, false)]);
    }

    #[test]
    fn a_part_of_a_collection_counts_its_own_static_flag() {
        let mut data = Vec::new();
        for v in [0.0_f32, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut scol = Record::new(*b"SCOL", 0x102)
            .with_subrecord(*b"MODL", b"set.nif\0".to_vec())
            .with_subrecord(*b"ONAM", 0x100_u32.to_le_bytes().to_vec())
            .with_subrecord(*b"DATA", data.clone())
            .with_subrecord(*b"ONAM", 0x101_u32.to_le_bytes().to_vec())
            .with_subrecord(*b"DATA", data);
        scol.flags = VISIBLE_WHEN_DISTANT | HIGH_PRIORITY_LOD;
        let mut world = Group::new(1, 0x0000_0A00_u32.to_le_bytes());
        world = world.with(refr(0x200, 0x102, 0, 1.0));
        let bytes = PluginBuilder::new()
            .group(
                Group::new(0, *b"STAT")
                    .with(stat(0x100, "high.nif", HIGH_PRIORITY_LOD))
                    .with(stat(0x101, "low.nif", 0)),
            )
            .group(Group::new(0, *b"SCOL").with(scol))
            .group(world)
            .build();
        let p = read_with(
            &bytes,
            ReadOptions {
                objects: true,
                ..ReadOptions::default()
            },
        )
        .unwrap();
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        let parts: Vec<(&str, bool)> = out
            .iter()
            .filter(|i| i.part)
            .map(|i| (i.model.as_str(), i.high_priority))
            .collect();
        // the part whose static is flagged is high priority; the collection's own bit 16 is not read
        assert_eq!(parts, vec![("high.nif", true), ("low.nif", false)]);
        assert!(out.iter().filter(|i| !i.part).all(|i| !i.high_priority));
    }

    #[test]
    fn bit_16_on_an_activator_is_not_high_priority() {
        covers(&[ids::objects::selection::HIGH_PRIORITY]);
        let mut acti = Record::new(*b"ACTI", 0x100).with_subrecord(*b"MODL", b"a.nif\0".to_vec());
        acti.flags = VISIBLE_WHEN_DISTANT | HIGH_PRIORITY_LOD;
        let mut world = Group::new(1, 0x0000_0A00_u32.to_le_bytes());
        world = world.with(refr(0x200, 0x100, 0, 1.0));
        let bytes = PluginBuilder::new()
            .group(Group::new(0, *b"ACTI").with(acti))
            .group(world)
            .build();
        let p = read_with(
            &bytes,
            ReadOptions {
                objects: true,
                ..ReadOptions::default()
            },
        )
        .unwrap();
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        assert_eq!(out.len(), 1);
        assert!(!out[0].high_priority);
    }

    #[test]
    fn only_flagged_statics_are_instances_and_a_later_plugin_overrides_a_reference() {
        covers(&[ids::objects::selection::FLAGGED_STATICS]);
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
        covers(&[ids::objects::selection::DISABLED_REFERENCES]);
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
        covers(&[ids::objects::selection::DISABLED_REFERENCES]);
        let p = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT)],
            vec![
                refr(0x200, 0x100, INITIALLY_DISABLED | PERSISTENT, 1.0),
                refr(0x201, 0x100, INITIALLY_DISABLED, 2.0),
                refr(0x202, 0x100, PERSISTENT, 3.0),
            ],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        assert_eq!(
            out.iter().map(|i| i.reference).collect::<Vec<_>>(),
            vec![0x200, 0x202]
        );
    }

    #[test]
    fn a_later_plugin_that_enables_a_disabled_reference_brings_it_back() {
        covers(&[ids::objects::selection::DISABLED_REFERENCES]);
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
        // one parent outside the world, one disabled parent in it, one disabled parent with the opposite flag
        let p = plugin(
            &[],
            vec![
                stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT),
                stat(0x101, "m.nif", 0),
            ],
            vec![
                with_enable_parent(refr(0x200, 0x100, INITIALLY_DISABLED | PERSISTENT, 1.0)),
                with_parent(
                    refr(0x201, 0x100, INITIALLY_DISABLED | PERSISTENT, 2.0),
                    0x300,
                    false,
                ),
                with_parent(
                    refr(0x202, 0x100, INITIALLY_DISABLED | PERSISTENT, 3.0),
                    0x300,
                    true,
                ),
                refr(0x300, 0x101, INITIALLY_DISABLED | PERSISTENT, 0.0),
            ],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        assert_eq!(
            out.iter().map(|i| i.reference).collect::<Vec<_>>(),
            vec![0x200, 0x201, 0x202]
        );
    }

    /// The reference with enable parent `parent` (as stored), taking the opposite state when `opposite` is set.
    fn with_parent(r: Record, parent: u32, opposite: bool) -> Record {
        let mut xesp = parent.to_le_bytes().to_vec();
        xesp.extend_from_slice(&u32::from(opposite).to_le_bytes());
        r.with_subrecord(*b"XESP", xesp)
    }

    #[test]
    fn a_reference_follows_the_start_state_of_its_enable_parent_chain() {
        covers(&[ids::objects::selection::ENABLE_PARENTS]);
        let marker = stat(0x101, "marker.nif", 0);
        let p = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT), marker],
            vec![
                // parents: one disabled, one enabled, and a disabled one behind an enabled link
                refr(0x300, 0x101, INITIALLY_DISABLED | PERSISTENT, 0.0),
                refr(0x301, 0x101, PERSISTENT, 0.0),
                with_parent(refr(0x302, 0x101, PERSISTENT, 0.0), 0x300, false),
                with_parent(refr(0x200, 0x100, 0, 1.0), 0x300, false),
                with_parent(refr(0x201, 0x100, 0, 2.0), 0x300, true),
                with_parent(refr(0x202, 0x100, 0, 3.0), 0x301, false),
                with_parent(refr(0x203, 0x100, 0, 4.0), 0x301, true),
                with_parent(refr(0x204, 0x100, 0, 5.0), 0x302, false),
                with_parent(refr(0x205, 0x100, 0, 6.0), 0x302, true),
                // a parent that is not a reference of this world counts as enabled
                with_parent(refr(0x206, 0x100, 0, 7.0), 0x999, false),
            ],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        let taken: Vec<u32> = out.iter().map(|i| i.reference).collect();
        assert_eq!(taken, [0x201, 0x202, 0x205, 0x206]);
    }

    #[test]
    fn a_long_enable_parent_chain_is_followed_to_its_end_and_a_loop_counts_as_enabled() {
        covers(&[ids::objects::selection::ENABLE_PARENTS]);
        let marker = stat(0x101, "marker.nif", 0);
        // 0x200 hangs from a chain of 40 links whose last reference is disabled at the start
        let mut refs = vec![refr(0x340, 0x101, INITIALLY_DISABLED | PERSISTENT, 0.0)];
        for link in 0x300..0x340 {
            refs.push(with_parent(
                refr(link, 0x101, PERSISTENT, 0.0),
                link + 1,
                false,
            ));
        }
        refs.push(with_parent(refr(0x200, 0x100, 0, 1.0), 0x300, false));
        // 0x201 hangs from a loop of 20 references, none of them disabled
        for link in 0x400..0x414 {
            let next = if link == 0x413 { 0x400 } else { link + 1 };
            refs.push(with_parent(refr(link, 0x101, PERSISTENT, 0.0), next, false));
        }
        refs.push(with_parent(refr(0x201, 0x100, 0, 2.0), 0x400, false));
        let p = plugin(
            &[],
            vec![stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT), marker],
            refs,
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        let taken: Vec<u32> = out.iter().map(|i| i.reference).collect();
        assert_eq!(taken, [0x201]);
    }

    #[test]
    fn a_later_plugin_that_disables_an_enable_parent_takes_its_children_out() {
        covers(&[ids::objects::selection::ENABLE_PARENTS]);
        let base = plugin(
            &[],
            vec![
                stat(0x100, "a.nif", VISIBLE_WHEN_DISTANT),
                stat(0x101, "marker.nif", 0),
            ],
            vec![
                refr(0x300, 0x101, PERSISTENT, 0.0),
                with_parent(refr(0x200, 0x100, 0, 1.0), 0x300, false),
                refr(0x201, 0x100, 0, 2.0),
            ],
        );
        let patch = plugin(
            &["Base.esm"],
            vec![],
            vec![refr(0x300, 0x101, INITIALLY_DISABLED | PERSISTENT, 0.0)],
        );
        let plugins = vec![
            ("Base.esm".to_owned(), base),
            ("Patch.esp".to_owned(), patch),
        ];
        let all: Vec<u32> = visible_instances(&plugins[..1], 0x0000_0A00)
            .unwrap()
            .iter()
            .map(|i| i.reference)
            .collect();
        assert_eq!(all, [0x200, 0x201]);
        let patched: Vec<u32> = visible_instances(&plugins, 0x0000_0A00)
            .unwrap()
            .iter()
            .map(|i| i.reference)
            .collect();
        assert_eq!(patched, [0x201]);
        let listed = references_with_model(&plugins, 0x0000_0A00).unwrap();
        let child = listed
            .iter()
            .find(|c| c.instance.reference == 0x200)
            .unwrap();
        assert_eq!(
            child.left_out_because(),
            Some("disabled at the start by its enable parent")
        );
    }

    #[test]
    fn references_of_models_match_any_spelling_and_leave_every_disabled_reference_out() {
        let plugins = vec![(
            "a.esm".to_owned(),
            plugin(
                &[],
                vec![
                    stat(0x100, "Meshes\\A\\Wall.NIF", 0),
                    stat(0x101, "a\\other.nif", 0),
                ],
                vec![
                    refr(0x200, 0x100, 0, 1.0),
                    refr(0x201, 0x100, INITIALLY_DISABLED, 2.0),
                    refr(0x202, 0x100, INITIALLY_DISABLED | PERSISTENT, 3.0),
                    refr(0x203, 0x101, 0, 4.0),
                ],
            ),
        )];
        let world = plugins[0].1.worlds[0].form_id;
        let found = super::references_of_models(&plugins, world, &["a/wall.nif"]).unwrap();
        assert_eq!(
            found.iter().map(|i| i.reference).collect::<Vec<_>>(),
            vec![0x200]
        );
    }

    #[test]
    fn references_of_models_leave_a_reference_its_enable_parent_keeps_disabled_out() {
        let p = plugin(
            &[],
            vec![stat(0x100, "a/wall.nif", 0), stat(0x101, "marker.nif", 0)],
            vec![
                refr(0x200, 0x100, 0, 1.0),
                refr(0x300, 0x101, INITIALLY_DISABLED, 0.0),
                with_parent(refr(0x201, 0x100, 0, 2.0), 0x300, false),
            ],
        );
        let found =
            super::references_of_models(&[("A.esm".to_owned(), p)], 0x0000_0A00, &["a/wall.nif"])
                .unwrap();
        assert_eq!(
            found.iter().map(|i| i.reference).collect::<Vec<_>>(),
            vec![0x200]
        );
    }

    #[test]
    fn references_where_splits_enabled_references_from_disabled_and_parented_ones() {
        let p = plugin(
            &[],
            vec![stat(0x100, "a/lamp.nif", 0), stat(0x101, "a/wall.nif", 0)],
            vec![
                refr(0x200, 0x100, 0, 1.0),
                refr(0x201, 0x100, INITIALLY_DISABLED, 2.0),
                refr(0x202, 0x100, INITIALLY_DISABLED | PERSISTENT, 3.0),
                // a parent that is not a reference of the load order counts as enabled
                with_enable_parent(refr(0x203, 0x100, 0, 4.0)),
                refr(0x204, 0x101, 0, 5.0),
                // a parent disabled at the start keeps its child out, and lets its opposite child in
                refr(0x300, 0x101, INITIALLY_DISABLED | PERSISTENT, 0.0),
                with_parent(refr(0x205, 0x100, 0, 6.0), 0x300, false),
                with_parent(refr(0x206, 0x100, 0, 7.0), 0x300, true),
            ],
        );
        let found = super::references_where(&[("A.esm".to_owned(), p)], 0x0000_0A00, &|b| {
            b.model.as_deref() == Some("a/lamp.nif")
        })
        .unwrap();
        let ids = |v: &[super::Instance]| v.iter().map(|i| i.reference).collect::<Vec<_>>();
        assert_eq!(ids(&found.taken), vec![0x200, 0x203, 0x206]);
        assert_eq!(ids(&found.held_back), vec![0x201, 0x202, 0x205]);
    }

    /// A plugin with the statics in world 0xA00 and a second world 0xB00 that holds `markers`.
    fn plugin_two_worlds(
        statics: Vec<Record>,
        refs: Vec<Record>,
        markers: Vec<Record>,
    ) -> crate::plugin::Plugin {
        let mut stats = Group::new(0, *b"STAT");
        for s in statics {
            stats = stats.with(s);
        }
        let mut world = Group::new(1, 0x0000_0A00_u32.to_le_bytes());
        for r in refs {
            world = world.with(r);
        }
        let mut other = Group::new(1, 0x0000_0B00_u32.to_le_bytes());
        for r in markers {
            other = other.with(r);
        }
        let bytes = PluginBuilder::new()
            .group(stats)
            .group(world)
            .group(other)
            .build();
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
    fn an_enable_parent_in_another_world_decides_the_start_state() {
        covers(&[ids::objects::selection::ENABLE_PARENTS]);
        // lights seen from far whose enable parent is a disabled marker in another world (the Strip's lights): the
        // child is disabled at the start, its opposite twin stands
        let p = plugin_two_worlds(
            vec![
                stat(0x100, "a/lights.nif", VISIBLE_WHEN_DISTANT),
                stat(0x101, "marker.nif", 0),
            ],
            vec![
                with_parent(refr(0x200, 0x100, PERSISTENT, 1.0), 0x300, false),
                with_parent(refr(0x201, 0x100, PERSISTENT, 2.0), 0x300, true),
                with_parent(refr(0x202, 0x100, PERSISTENT, 3.0), 0x301, false),
            ],
            vec![
                refr(0x300, 0x101, INITIALLY_DISABLED | PERSISTENT, 0.0),
                refr(0x301, 0x101, PERSISTENT, 0.0),
            ],
        );
        let plugins = [("A.esm".to_owned(), p)];
        let ids = |v: &[super::Instance]| v.iter().map(|i| i.reference).collect::<Vec<_>>();
        assert_eq!(
            ids(&visible_instances(&plugins, 0x0000_0A00).unwrap()),
            vec![0x201, 0x202]
        );
        let found = super::references_where(&plugins, 0x0000_0A00, &|b| {
            b.model.as_deref() == Some("a/lights.nif")
        })
        .unwrap();
        assert_eq!(ids(&found.taken), vec![0x201, 0x202]);
        assert_eq!(ids(&found.held_back), vec![0x200]);
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
        covers(&[ids::objects::selection::DELETED_REFERENCES]);
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
        covers(&[ids::objects::selection::FLAGGED_STATICS]);
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
