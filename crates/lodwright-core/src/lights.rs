// SPDX-License-Identifier: GPL-3.0-only
//! Placed lights that can stand for a lamp in the far view at night (issue 339).
//!
//! A light is a reference whose base is a light record (`LIGH`). The far view draws a small soft point where a steady
//! light stands; which lights qualify is decided here from the load order alone: the winning records, the start state
//! and the light's own flags. Nothing here knows a mod, a model or a place.

use std::collections::{HashMap, HashSet};

use crate::Error;
use crate::objects::{INITIALLY_DISABLED, StartState, disabled_at_start, start_states_elsewhere};
use crate::plugin::{LightBase, Placement, Plugin};
use crate::worldspace::{absolute, master_table};

// source: fnv-light-data
/// Light flags of a light that is not steady or does not give light: negative, flicker, off by default, flicker
/// slow, pulse and pulse slow.
pub const UNSTEADY_OR_DARK: u32 =
    0x0000_0004 | 0x0000_0008 | 0x0000_0020 | 0x0000_0040 | 0x0000_0080 | 0x0000_0100;

/// A winning reference as the selection needs it: base, flags, placement and enable parent (absolute `FormID`,
/// opposite).
type PlacedLight = (u32, u32, Placement, Option<(u32, bool)>);

/// How near (world units, every axis but height) a placed light must stand to a reference to count as that
/// reference's light when lamps are learned. Own design: a lamp head overhangs its post by up to a few hundred units.
const LAMP_PAIR_REACH: f32 = 256.0;
/// How high above or below a reference its light may stand when lamps are learned. Own design: tall posts.
const LAMP_PAIR_HEIGHT: f32 = 1024.0;
/// How far (world units, in the model's own frame) a lit copy's light may stand from the model's usual light spot and
/// still agree with it. Own design: placed by hand, the same lamp's light is seldom more than a few units off.
const LAMP_SPOT_TOLERANCE: f32 = 24.0;
/// The fewest lit copies of a model, and the smallest share of its copies that must be lit, before every copy of it
/// counts as a lamp. Own design: a mod that lights only every other post of a row still teaches the rule, while a wall
/// or a pavement tile that happens to stand beside a few lights does not.
const LAMP_MIN_LIT: usize = 3;
const LAMP_MIN_SHARE: f32 = 0.2;
/// The share of a model's lit copies whose lights must agree on one spot. Own design.
const LAMP_MIN_AGREEMENT: f32 = 0.75;
/// A copy that already has a qualifying light this near (world units) to its model's light spot gets no second point.
const LAMP_TAKEN: f32 = 64.0;

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
                        r.placement,
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
    for &id in &order {
        let Some(&(base, flags, placement, _)) = refs.get(&id) else {
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
            pos: placement.pos,
            radius: light.radius,
            color: light.color,
        });
    }
    let standing =
        |id: u32, flags: u32| flags & INITIALLY_DISABLED == 0 && !disabled_at_start(&state, id);
    let fixtures: Vec<(u32, u32, Placement)> = order
        .iter()
        .filter_map(|&id| {
            let &(base, flags, placement, _) = refs.get(&id)?;
            (!lights.contains_key(&base) && standing(id, flags)).then_some((id, base, placement))
        })
        .collect();
    let filled = unlit_lamps(&out, &fixtures);
    out.extend(filled);
    Ok(out)
}

/// The rotation of a placement as a matrix (local to world): about x, then y, then z, each turned the way the game
/// turns a reference, the same as the object blocks place a model.
fn turn_of(rot: [f32; 3]) -> [[f32; 3]; 3] {
    let (sx, cx) = (-rot[0]).sin_cos();
    let (sy, cy) = (-rot[1]).sin_cos();
    let (sz, cz) = (-rot[2]).sin_cos();
    [
        [cy * cz, -cy * sz, sy],
        [cx * sz + sx * sy * cz, cx * cz - sx * sy * sz, -sx * cy],
        [sx * sz - cx * sy * cz, sx * cz + cx * sy * sz, cx * cy],
    ]
}

/// Where `local` (in the model's own frame) stands in the world for a copy placed at `placement`.
fn to_world(placement: &Placement, local: [f32; 3]) -> [f32; 3] {
    let m = turn_of(placement.rot);
    let v = local.map(|c| c * placement.scale);
    [
        placement.pos[0] + m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        placement.pos[1] + m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        placement.pos[2] + m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// Where world point `at` stands in the own frame of a copy placed at `placement` (the inverse of [`to_world`]).
fn to_local(placement: &Placement, at: [f32; 3]) -> [f32; 3] {
    let m = turn_of(placement.rot);
    let d = [
        at[0] - placement.pos[0],
        at[1] - placement.pos[1],
        at[2] - placement.pos[2],
    ];
    let scale = if placement.scale.abs() > f32::EPSILON {
        placement.scale
    } else {
        1.0
    };
    // a rotation's inverse is its transpose
    [
        (m[0][0] * d[0] + m[1][0] * d[1] + m[2][0] * d[2]) / scale,
        (m[0][1] * d[0] + m[1][1] * d[1] + m[2][1] * d[2]) / scale,
        (m[0][2] * d[0] + m[1][2] * d[1] + m[2][2] * d[2]) / scale,
    ]
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The far light points of lamps that carry no light of their own (issue 339, lamp rows lit every other post).
///
/// A load order often lights only some copies of one lamp model: a row of posts where every second one has a light,
/// to keep the number of lights low, while every head is drawn lit near by. The far view should show every head. The
/// rule is learned from the data alone: a model (`fixtures`' base) is a lamp when at least [`LAMP_MIN_LIT`] of its
/// standing copies, and [`LAMP_MIN_SHARE`] of them, have one of `lights` near them, and [`LAMP_MIN_AGREEMENT`] of those
/// lights stand at one spot of the model's own frame (within [`LAMP_SPOT_TOLERANCE`]). Every other standing copy of
/// that model then gets a point at that spot, with the radius and colour of the lit copy nearest to it, unless a light
/// already stands there. The points come after `lights`, in the order of `fixtures`.
fn unlit_lamps(
    lights: &[FarLightSource],
    fixtures: &[(u32, u32, Placement)],
) -> Vec<FarLightSource> {
    // lights by a coarse grid cell, so pairing looks only at the cells around a reference
    let cell_of = |p: [f32; 3]| {
        #[allow(clippy::cast_possible_truncation)]
        // a world position over the cell size fits an i32
        let c = (
            (p[0] / LAMP_PAIR_REACH).floor() as i32,
            (p[1] / LAMP_PAIR_REACH).floor() as i32,
        );
        c
    };
    let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (k, l) in lights.iter().enumerate() {
        grid.entry(cell_of(l.pos)).or_default().push(k);
    }
    let near = |p: [f32; 3]| {
        let (cx, cy) = cell_of(p);
        let mut found = Vec::new();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &k in grid.get(&(cx + dx, cy + dy)).map_or(&[][..], Vec::as_slice) {
                    let l = &lights[k];
                    if (l.pos[0] - p[0]).abs() <= LAMP_PAIR_REACH
                        && (l.pos[1] - p[1]).abs() <= LAMP_PAIR_REACH
                        && (l.pos[2] - p[2]).abs() <= LAMP_PAIR_HEIGHT
                    {
                        found.push(k);
                    }
                }
            }
        }
        found
    };
    // per model: its copies, and the lights seen near each lit copy (in the model's frame)
    let mut copies: HashMap<u32, usize> = HashMap::new();
    let mut seen: HashMap<u32, Vec<([f32; 3], usize)>> = HashMap::new();
    for (_, base, placement) in fixtures {
        *copies.entry(*base).or_default() += 1;
        for k in near(placement.pos) {
            seen.entry(*base)
                .or_default()
                .push((to_local(placement, lights[k].pos), k));
        }
    }
    // the spot of each lamp model: the seen light that most others agree with
    let mut spots: HashMap<u32, ([f32; 3], usize)> = HashMap::new();
    for (base, lit) in &seen {
        let total = copies.get(base).copied().unwrap_or(0);
        let Some(&(spot, k)) = lit.iter().max_by_key(|(a, _)| {
            lit.iter()
                .filter(|(b, _)| distance(*a, *b) <= LAMP_SPOT_TOLERANCE)
                .count()
        }) else {
            continue;
        };
        let agree = lit
            .iter()
            .filter(|(b, _)| distance(spot, *b) <= LAMP_SPOT_TOLERANCE)
            .count();
        #[allow(clippy::cast_precision_loss)] // counts of copies are far below 2^24
        let (share, agreement) = (
            agree as f32 / total.max(1) as f32,
            agree as f32 / lit.len() as f32,
        );
        if agree >= LAMP_MIN_LIT && share >= LAMP_MIN_SHARE && agreement >= LAMP_MIN_AGREEMENT {
            spots.insert(*base, (spot, k));
        }
    }
    // two models of one lamp (a post and its foot) learn the same spot: a point already given is taken too
    let mut given: HashMap<(i32, i32), Vec<[f32; 3]>> = HashMap::new();
    let mut out = Vec::new();
    for &(id, base, placement) in fixtures {
        let Some(&(spot, k)) = spots.get(&base) else {
            continue;
        };
        let at = to_world(&placement, spot);
        let (cx, cy) = cell_of(at);
        let taken_by_given = (-1..=1).any(|dx| {
            (-1..=1).any(|dy| {
                given
                    .get(&(cx + dx, cy + dy))
                    .is_some_and(|ps| ps.iter().any(|p| distance(*p, at) <= LAMP_TAKEN))
            })
        });
        if taken_by_given
            || near(at)
                .iter()
                .any(|&j| distance(lights[j].pos, at) <= LAMP_TAKEN)
        {
            continue;
        }
        given.entry((cx, cy)).or_default().push(at);
        out.push(FarLightSource {
            reference: id,
            pos: at,
            radius: lights[k].radius,
            color: lights[k].color,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::far_light_sources;
    use crate::objects::INITIALLY_DISABLED;
    use crate::plugin::{ReadOptions, read_with};
    use lodwright_testkit::plugin::{Group, PluginBuilder, Record};
    use lodwright_testkit::{covers, ids};

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

    fn refr_at(id: u32, base: u32, flags: u32, pos: [f32; 3], rz: f32) -> Record {
        let mut data = Vec::new();
        for v in [pos[0], pos[1], pos[2], 0.0, 0.0, rz] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        let mut r = Record::new(*b"REFR", id)
            .with_subrecord(*b"NAME", base.to_le_bytes().to_vec())
            .with_subrecord(*b"DATA", data);
        r.flags = flags;
        r
    }

    /// Positions (rounded) of the sources of one plugin, by reference.
    fn points(p: crate::plugin::Plugin) -> Vec<(u32, [i32; 3])> {
        #[allow(clippy::cast_possible_truncation)]
        far_light_sources(&[("A.esm".to_owned(), p)], WORLD, 256)
            .unwrap()
            .iter()
            .map(|s| (s.reference, s.pos.map(|v| v.round() as i32)))
            .collect()
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

    // ---- lamps lit on every other post (issue 339): the far view shows every head ----

    const POST: u32 = 0x300;

    #[test]
    fn every_post_of_a_row_lit_every_other_post_gets_a_point_at_the_head() {
        covers(&[ids::objects::selection::FAR_LIGHTS_UNLIT_LAMPS]);
        let mut refs = Vec::new();
        for n in 0..6_u32 {
            #[allow(clippy::cast_precision_loss)]
            let x = n as f32 * 512.0;
            refs.push(refr_at(0x400 + n, POST, 0, [x, 100.0, 50.0], 0.0));
            if n % 2 == 0 {
                refs.push(refr_at(0x500 + n, 0x100, 0, [x + 4.0, 100.0, 320.0], 0.0));
            }
        }
        let p = plugin(&[], vec![light(0x100, 300, [200, 200, 160], 0)], refs);
        let got = points(p);
        assert_eq!(got.len(), 6, "{got:?}");
        assert_eq!(
            &got[3..],
            &[
                (0x401, [516, 100, 320]),
                (0x403, [1540, 100, 320]),
                (0x405, [2564, 100, 320])
            ]
        );
    }

    #[test]
    fn a_turned_post_gets_its_point_where_its_head_overhangs() {
        covers(&[ids::objects::selection::FAR_LIGHTS_UNLIT_LAMPS]);
        // the head overhangs 100 units along the post's own x; a quarter turn puts it 100 units to the south
        let quarter = std::f32::consts::FRAC_PI_2;
        let mut refs = vec![refr_at(0x410, POST, 0, [0.0, 4000.0, 0.0], quarter)];
        for n in 0..3_u32 {
            #[allow(clippy::cast_precision_loss)]
            let x = n as f32 * 600.0;
            refs.push(refr_at(0x400 + n, POST, 0, [x, 0.0, 0.0], 0.0));
            refs.push(refr_at(0x500 + n, 0x100, 0, [x + 100.0, 0.0, 300.0], 0.0));
        }
        let p = plugin(&[], vec![light(0x100, 300, [9, 9, 9], 0)], refs);
        let got = points(p);
        assert_eq!(got.last(), Some(&(0x410, [0, 3900, 300])), "{got:?}");
        assert_eq!(got.len(), 4);
    }

    #[test]
    fn a_model_that_only_sometimes_stands_beside_a_light_is_no_lamp() {
        covers(&[ids::objects::selection::FAR_LIGHTS_UNLIT_LAMPS]);
        // 20 copies of a wall, a light beside 3 of them: under the share a lamp model needs
        let mut refs = Vec::new();
        for n in 0..20_u32 {
            #[allow(clippy::cast_precision_loss)]
            let x = n as f32 * 1000.0;
            refs.push(refr_at(0x400 + n, POST, 0, [x, 0.0, 0.0], 0.0));
        }
        for n in 0..3_u32 {
            #[allow(clippy::cast_precision_loss)]
            let x = n as f32 * 1000.0;
            refs.push(refr_at(0x500 + n, 0x100, 0, [x + 50.0, 0.0, 300.0], 0.0));
        }
        let p = plugin(&[], vec![light(0x100, 300, [9, 9, 9], 0)], refs);
        assert_eq!(points(p).len(), 3);
    }

    #[test]
    fn lights_that_stand_anywhere_beside_a_model_teach_no_spot() {
        covers(&[ids::objects::selection::FAR_LIGHTS_UNLIT_LAMPS]);
        let spots = [
            [0.0, 0.0, 300.0],
            [200.0, 0.0, 40.0],
            [-150.0, 120.0, 600.0],
            [0.0, -200.0, 10.0],
        ];
        let mut refs = Vec::new();
        for (n, d) in (0_u32..).zip(spots) {
            #[allow(clippy::cast_precision_loss)]
            let x = n as f32 * 1000.0;
            refs.push(refr_at(0x400 + n, POST, 0, [x, 0.0, 0.0], 0.0));
            refs.push(refr_at(0x500 + n, 0x100, 0, [x + d[0], d[1], d[2]], 0.0));
        }
        refs.push(refr_at(0x420, POST, 0, [9000.0, 0.0, 0.0], 0.0));
        let p = plugin(&[], vec![light(0x100, 300, [9, 9, 9], 0)], refs);
        assert_eq!(points(p).len(), 4);
    }

    #[test]
    fn a_post_disabled_at_the_start_gets_no_point() {
        covers(&[ids::objects::selection::FAR_LIGHTS_UNLIT_LAMPS]);
        let mut refs = Vec::new();
        for n in 0..3_u32 {
            #[allow(clippy::cast_precision_loss)]
            let x = n as f32 * 512.0;
            refs.push(refr_at(0x400 + n, POST, 0, [x, 0.0, 0.0], 0.0));
            refs.push(refr_at(0x500 + n, 0x100, 0, [x, 0.0, 300.0], 0.0));
        }
        refs.push(refr_at(
            0x410,
            POST,
            INITIALLY_DISABLED,
            [5000.0, 0.0, 0.0],
            0.0,
        ));
        let p = plugin(&[], vec![light(0x100, 300, [9, 9, 9], 0)], refs);
        assert_eq!(points(p).len(), 3);
    }

    #[test]
    fn two_models_of_one_lamp_give_one_point() {
        // a post and its foot stand at the same place: both learn the head, the unlit post gets one point
        const FOOT: u32 = 0x301;
        covers(&[ids::objects::selection::FAR_LIGHTS_UNLIT_LAMPS]);
        let mut refs = Vec::new();
        for n in 0..4_u32 {
            #[allow(clippy::cast_precision_loss)]
            let x = n as f32 * 512.0;
            refs.push(refr_at(0x400 + n, POST, 0, [x, 0.0, 0.0], 0.0));
            refs.push(refr_at(0x600 + n, FOOT, 0, [x, 0.0, 0.0], 0.0));
            if n != 3 {
                refs.push(refr_at(0x500 + n, 0x100, 0, [x, 0.0, 300.0], 0.0));
            }
        }
        let p = plugin(&[], vec![light(0x100, 300, [9, 9, 9], 0)], refs);
        let got = points(p);
        assert_eq!(got.len(), 4, "{got:?}");
        assert_eq!(got[3].1, [1536, 0, 300]);
    }
}
