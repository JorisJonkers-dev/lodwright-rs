// SPDX-License-Identifier: GPL-3.0-only
//! Placed lights that can stand for a lamp in the far view at night (issue 339).
//!
//! A light is a reference whose base is a light record (`LIGH`). The far view draws a small soft point where a steady
//! light stands; which lights qualify is decided here from the load order alone: the winning records, the start state
//! and the light's own flags. Nothing here knows a mod, a model or a place.

use std::collections::{HashMap, HashSet};

use crate::Error;
use crate::objects::{INITIALLY_DISABLED, StartState, disabled_at_start, start_states_elsewhere};
use crate::plugin::{LightBase, Plugin};
use crate::worldspace::{absolute, master_table};

// source: fnv-light-data
/// Light flags of a light that is not steady or does not give light: negative, flicker, off by default, flicker
/// slow, pulse and pulse slow.
pub const UNSTEADY_OR_DARK: u32 =
    0x0000_0004 | 0x0000_0008 | 0x0000_0020 | 0x0000_0040 | 0x0000_0080 | 0x0000_0100;

/// A winning reference as the selection needs it: base, flags, position and enable parent (absolute `FormID`,
/// opposite).
type PlacedLight = (u32, u32, [f32; 3], Option<(u32, bool)>);

/// A placed light that qualifies for a far light point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FarLightSource {
    /// The reference (absolute `FormID`).
    pub reference: u32,
    /// Where it stands, in world units.
    pub pos: [f32; 3],
    /// The light's radius in game units.
    pub radius: u32,
    /// The light's colour, red, green, blue.
    pub color: [u8; 3],
}

/// The steady lights of world `world` (absolute `FormID`) that stand when the game starts and reach at least
/// `min_radius` game units, in the order their references first appear. The winning record of each reference and
/// of each light base decides. Left out: references that are initially disabled or kept disabled at the start by
/// their enable parent chain, and lights that are negative, flicker, pulse or are off by default.
///
/// # Errors
/// [`Error::MissingMaster`] when a plugin names a master that is not in the load order.
pub fn far_light_sources(
    plugins: &[(String, Plugin)],
    world: u32,
    min_radius: u32,
) -> Result<Vec<FarLightSource>, Error> {
    let mut lights: HashMap<u32, LightBase> = HashMap::new();
    let mut refs: HashMap<u32, PlacedLight> = HashMap::new();
    let mut order: Vec<u32> = Vec::new();
    let mut seen: HashSet<u32> = HashSet::new();
    for (index, (_, plugin)) in plugins.iter().enumerate() {
        let table = master_table(plugins, plugin)?;
        let own = u32::try_from(index).map_err(|_| Error::Malformed("too many plugins"))?;
        for light in &plugin.lights {
            let id = absolute(light.form_id, &table, own);
            lights.insert(
                id,
                LightBase {
                    form_id: id,
                    ..*light
                },
            );
        }
        for source in plugin
            .worlds
            .iter()
            .filter(|w| absolute(w.form_id, &table, own) == world)
        {
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
                        r.placement.pos,
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
    let elsewhere: std::cell::OnceCell<HashMap<u32, StartState>> = std::cell::OnceCell::new();
    let state = |at: u32| -> Option<StartState> {
        if let Some(&(_, flags, _, parent)) = refs.get(&at) {
            return Some((flags, parent));
        }
        elsewhere
            .get_or_init(|| start_states_elsewhere(plugins, world))
            .get(&at)
            .copied()
    };
    let mut out = Vec::new();
    for id in order {
        let Some(&(base, flags, pos, _)) = refs.get(&id) else {
            continue; // deleted by a later plugin
        };
        let Some(light) = lights.get(&base) else {
            continue;
        };
        // source: fnv-light-data
        if light.flags & UNSTEADY_OR_DARK != 0 || light.radius < min_radius {
            continue;
        }
        if flags & INITIALLY_DISABLED != 0 || disabled_at_start(&state, id) {
            continue;
        }
        out.push(FarLightSource {
            reference: id,
            pos,
            radius: light.radius,
            color: light.color,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::far_light_sources;
    use crate::objects::INITIALLY_DISABLED;
    use crate::plugin::{ReadOptions, read_with};
    use lodwright_testkit::plugin::{Group, PluginBuilder, Record};

    const WORLD: u32 = 0x0000_0A00;

    fn light(id: u32, radius: u32, color: [u8; 3], flags: u32) -> Record {
        let mut data = Vec::new();
        data.extend_from_slice(&(-1_i32).to_le_bytes());
        data.extend_from_slice(&radius.to_le_bytes());
        data.extend_from_slice(&[color[0], color[1], color[2], 0]);
        data.extend_from_slice(&flags.to_le_bytes());
        for v in [1.0_f32, 90.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0.0_f32.to_le_bytes());
        Record::new(*b"LIGH", id).with_subrecord(*b"DATA", data)
    }

    fn refr(id: u32, base: u32, flags: u32, x: f32) -> Record {
        let mut data = Vec::new();
        for v in [x, 2.0, 3.0, 0.0, 0.0, 0.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut r = Record::new(*b"REFR", id)
            .with_subrecord(*b"NAME", base.to_le_bytes().to_vec())
            .with_subrecord(*b"DATA", data);
        r.flags = flags;
        r
    }

    fn plugin(masters: &[&str], lights: Vec<Record>, refs: Vec<Record>) -> crate::plugin::Plugin {
        let mut group = Group::new(0, *b"LIGH");
        for l in lights {
            group = group.with(l);
        }
        let mut world = Group::new(1, WORLD.to_le_bytes());
        for r in refs {
            world = world.with(r);
        }
        let mut b = PluginBuilder::new();
        for m in masters {
            b = b.master(m);
        }
        read_with(
            &b.group(group).group(world).build(),
            ReadOptions {
                objects: true,
                ..ReadOptions::default()
            },
        )
        .unwrap()
    }

    fn sources(plugins: &[(String, crate::plugin::Plugin)]) -> Vec<(u32, u32, [u8; 3], f32)> {
        far_light_sources(plugins, WORLD, 256)
            .unwrap()
            .iter()
            .map(|s| (s.reference, s.radius, s.color, s.pos[0]))
            .collect()
    }

    #[test]
    fn a_steady_light_gives_its_radius_colour_and_position() {
        let p = plugin(
            &[],
            vec![light(0x100, 512, [255, 200, 90], 0)],
            vec![refr(0x200, 0x100, 0, 7.0)],
        );
        assert_eq!(
            sources(&[("A.esm".to_owned(), p)]),
            vec![(0x200, 512, [255, 200, 90], 7.0)]
        );
    }

    #[test]
    fn a_light_under_the_minimum_radius_is_left_out() {
        let p = plugin(
            &[],
            vec![light(0x100, 255, [1, 2, 3], 0)],
            vec![refr(0x200, 0x100, 0, 1.0)],
        );
        assert_eq!(sources(&[("A.esm".to_owned(), p)]), Vec::new());
    }

    #[test]
    fn flickering_pulsing_negative_and_off_by_default_lights_are_left_out() {
        let flags = [0x4, 0x8, 0x20, 0x40, 0x80, 0x100];
        let lights = flags
            .iter()
            .zip(0x100_u32..)
            .map(|(&f, id)| light(id, 512, [9, 9, 9], f))
            .collect();
        let refs = (0..6_u32)
            .map(|n| refr(0x200 + n, 0x100 + n, 0, 1.0))
            .collect();
        let p = plugin(&[], lights, refs);
        assert_eq!(sources(&[("A.esm".to_owned(), p)]), Vec::new());
    }

    #[test]
    fn a_spot_light_counts_as_steady() {
        let p = plugin(
            &[],
            vec![light(0x100, 512, [9, 9, 9], 0x200)],
            vec![refr(0x200, 0x100, 0, 1.0)],
        );
        assert_eq!(sources(&[("A.esm".to_owned(), p)]).len(), 1);
    }

    #[test]
    fn an_initially_disabled_light_is_left_out() {
        let p = plugin(
            &[],
            vec![light(0x100, 512, [9, 9, 9], 0)],
            vec![refr(0x200, 0x100, INITIALLY_DISABLED, 1.0)],
        );
        assert_eq!(sources(&[("A.esm".to_owned(), p)]), Vec::new());
    }

    #[test]
    fn a_reference_that_is_not_a_light_is_not_a_source() {
        let p = plugin(
            &[],
            vec![light(0x100, 512, [9, 9, 9], 0)],
            vec![refr(0x200, 0x101, 0, 1.0)],
        );
        assert_eq!(sources(&[("A.esm".to_owned(), p)]), Vec::new());
    }

    #[test]
    fn a_later_plugin_that_makes_a_light_flicker_takes_it_out() {
        let game = plugin(
            &[],
            vec![light(0x100, 512, [9, 9, 9], 0)],
            vec![refr(0x200, 0x100, 0, 1.0)],
        );
        let patch = plugin(
            &["A.esm"],
            vec![light(0x100, 512, [9, 9, 9], 0x8)],
            Vec::new(),
        );
        assert_eq!(
            sources(&[("A.esm".to_owned(), game), ("B.esp".to_owned(), patch)]),
            Vec::new()
        );
    }
}
