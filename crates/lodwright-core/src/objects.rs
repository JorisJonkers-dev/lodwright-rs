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
/// Initially Disabled, on references (such references still enter LOD in the game's own blocks).
pub const INITIALLY_DISABLED: u32 = 0x0000_0800;
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
/// base is flagged, or when the reference itself is; initially disabled references count too.
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn visible_instances(plugins: &[(String, Plugin)], world: u32) -> Result<Vec<Instance>, Error> {
    let mut bases: HashMap<u32, (BaseObject, usize)> = HashMap::new();
    let mut refs: HashMap<u32, Ref> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
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
            for r in &source.refs {
                let id = absolute(r.form_id, &table, own);
                if refs
                    .insert(
                        id,
                        (
                            absolute(r.base, &table, own),
                            r.flags,
                            r.placement,
                            index,
                            r.cell,
                        ),
                    )
                    .is_none()
                {
                    order.push(id);
                }
            }
        }
    }
    let mut out = Vec::new();
    for id in order {
        let (base_id, flags, placement, reference_plugin, cell) = refs[&id];
        let Some((base, base_plugin)) = bases.get(&base_id) else {
            continue;
        };
        if base.flags & VISIBLE_WHEN_DISTANT == 0 && flags & VISIBLE_WHEN_DISTANT == 0 {
            continue;
        }
        let high_priority = flags & HIGH_PRIORITY_LOD != 0;
        let has_tree_lod = base.flags & HAS_TREE_LOD != 0;
        if let Some(model) = &base.model {
            out.push(Instance {
                base: base_id,
                signature: base.signature,
                model: model.clone(),
                placement,
                high_priority,
                has_tree_lod,
                reference: id,
                reference_plugin,
                base_plugin: *base_plugin,
                part: false,
                cell,
            });
        }
        for (part_id, placements) in &base.parts {
            let Some((part_base, part_plugin)) = bases.get(part_id) else {
                continue;
            };
            let Some(model) = part_base.model.clone() else {
                continue;
            };
            for inner in placements {
                out.push(Instance {
                    base: *part_id,
                    signature: *b"STAT",
                    model: model.clone(),
                    placement: compose(&placement, inner),
                    high_priority,
                    has_tree_lod,
                    reference: id,
                    reference_plugin,
                    base_plugin: *part_plugin,
                    part: true,
                    cell,
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{INITIALLY_DISABLED, VISIBLE_WHEN_DISTANT, visible_instances};
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
    fn a_disabled_reference_still_counts_and_a_flagged_reference_counts_for_an_unflagged_base() {
        let p = plugin(
            &[],
            vec![stat(0x100, "a.nif", 0)],
            vec![
                refr(0x200, 0x100, INITIALLY_DISABLED | VISIBLE_WHEN_DISTANT, 1.0),
                refr(0x201, 0x100, VISIBLE_WHEN_DISTANT, 2.0),
            ],
        );
        let out = visible_instances(&[("A.esm".to_owned(), p)], 0x0000_0A00).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].reference, 0x201);
    }
}
