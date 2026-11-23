// SPDX-License-Identifier: GPL-3.0-only
//! The landmark presence report: for each named landmark, is it written into a generated output, where, and
//! with which texture; and a comparison with a saved report that fails when a landmark has gone.
//!
//! The landmark file is text, one landmark per line, four tab separated fields and an optional note: the name, the
//! world (editor id), the model (a path under `meshes`, or `-` for the model of a reference's base), and the place:
//! `ref:` and a reference's `FormID` in hex (as the load order numbers it), or `pos:` x, y, z and optionally the
//! rotation about x, y, z in radians and a scale, separated by commas. Blank lines and lines starting with `#` are
//! skipped.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use lodwright_contract::Request;
use lodwright_core::objects::{Candidate, Instance, references_with_model};
use lodwright_core::plugin::{Placement, ReadOptions};
use lodwright_core::worldspace::Worldspace;
use lodwright_gen::{FolderAssets, WrittenWorld, expected_geometry};
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::load::{load_plugins_with, select_worlds};

/// The contract name of the report document.
pub const LANDMARKS_CONTRACT: &str = "lodwright-landmarks";

/// Where a landmark stands.
#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    /// A reference, by its absolute `FormID`.
    Reference(u32),
    /// A position, rotation (radians) and scale.
    Position(Placement),
}

/// One line of a landmark file.
#[derive(Debug, Clone, PartialEq)]
pub struct Landmark {
    /// A name that is unique in the file.
    pub name: String,
    /// The world's editor id.
    pub world: String,
    /// The model under `meshes`, empty for the model of a reference's base object.
    pub model: String,
    /// Where it stands.
    pub place: Place,
    /// Free text.
    pub note: String,
}

fn numbers(text: &str) -> Result<Vec<f32>, String> {
    text.split(',')
        .map(|t| {
            t.trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("{t:?} is not a number"))
        })
        .collect()
}

/// Parse a landmark file.
///
/// # Errors
/// The first bad line with its number, or a name used twice.
pub fn parse_landmarks(text: &str) -> Result<Vec<Landmark>, String> {
    let mut list: Vec<Landmark> = Vec::new();
    for (number, line) in text.lines().enumerate().map(|(i, l)| (i + 1, l)) {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.splitn(5, '\t').map(str::trim).collect();
        let [name, world, model, place, ..] = fields[..] else {
            return Err(format!(
                "line {number}: needs a name, a world, a model and a place, separated by tabs"
            ));
        };
        if name.is_empty() || world.is_empty() || model.is_empty() {
            return Err(format!("line {number}: a field is empty"));
        }
        if list.iter().any(|l| l.name == name) {
            return Err(format!("line {number}: the name {name:?} is used twice"));
        }
        let place = if let Some(id) = place.strip_prefix("ref:") {
            Place::Reference(
                u32::from_str_radix(id.trim(), 16)
                    .map_err(|_| format!("line {number}: {id:?} is not a hex form id"))?,
            )
        } else if let Some(values) = place.strip_prefix("pos:") {
            let v = numbers(values).map_err(|e| format!("line {number}: {e}"))?;
            match v.len() {
                3 => Place::Position(Placement {
                    pos: [v[0], v[1], v[2]],
                    rot: [0.0; 3],
                    scale: 1.0,
                }),
                7 => Place::Position(Placement {
                    pos: [v[0], v[1], v[2]],
                    rot: [v[3], v[4], v[5]],
                    scale: v[6],
                }),
                _ => {
                    return Err(format!(
                        "line {number}: pos: takes x,y,z or x,y,z,rx,ry,rz,scale"
                    ));
                }
            }
        } else {
            return Err(format!("line {number}: the place starts with ref: or pos:"));
        };
        if matches!(place, Place::Position(_)) && model == "-" {
            return Err(format!("line {number}: a position needs a model"));
        }
        list.push(Landmark {
            name: name.to_owned(),
            world: world.to_owned(),
            model: if model == "-" {
                String::new()
            } else {
                model.to_owned()
            },
            place,
            note: fields.get(4).copied().unwrap_or("").to_owned(),
        });
    }
    Ok(list)
}

/// What the report says of one landmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Its far model's vertices are in the blocks.
    Written,
    /// Generation takes it, and the blocks do not hold it.
    Missing,
    /// A rule leaves it out on purpose (see `reason`).
    Excluded,
    /// It has no far model, or the far model cannot be read.
    NoFarModel,
    /// The reference or its base object is not in the load order, or is not what the file says.
    Unresolved,
}

impl Status {
    /// Whether the landmark is in the blocks.
    #[must_use]
    pub fn is_written(self) -> bool {
        self == Self::Written
    }

    /// The name used in the report.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Written => "written",
            Self::Missing => "missing",
            Self::Excluded => "excluded",
            Self::NoFarModel => "no_far_model",
            Self::Unresolved => "unresolved",
        }
    }
}

/// One place a landmark was found: a shape of a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundIn {
    /// The block file, relative to the output folder.
    pub block: String,
    /// The shape of the block.
    pub shape: usize,
    /// The diffuse texture the shape draws with.
    pub texture: String,
    /// Triangles of the landmark in this shape.
    pub triangles: usize,
    /// The segments that hold them.
    pub segments: Vec<usize>,
    /// The segment of the landmark's cell.
    pub expected_segment: usize,
    /// Whether the expected segment is one of `segments`.
    pub segment_ok: bool,
}

/// One landmark's line of the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandmarkReport {
    /// The landmark's name.
    pub name: String,
    /// The world.
    pub world: String,
    /// The model used.
    pub model: String,
    /// The reference, as 8 hex digits, for a reference landmark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// The position when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
    /// The status.
    pub status: Status,
    /// Why, when the status is not `written`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// Vertices of the far model once placed.
    pub expected_vertices: usize,
    /// Of those, the vertices found in the blocks.
    pub matched_vertices: usize,
    /// Triangles of the landmark in the blocks, all shapes together.
    pub triangles: usize,
    /// Where it was found.
    pub found_in: Vec<FoundIn>,
}

/// Counts by status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// Landmarks in the file.
    pub total: usize,
    /// Count per status label.
    pub by_status: BTreeMap<String, usize>,
}

/// The report document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandmarksDoc {
    /// The document version; 1.
    pub schema_version: u32,
    /// `lodwright-landmarks`.
    pub contract: String,
    /// The tool's version.
    pub tool_version: String,
    /// The counts.
    pub summary: Summary,
    /// One entry per landmark, in file order.
    pub landmarks: Vec<LandmarkReport>,
}

impl LandmarksDoc {
    fn new(landmarks: Vec<LandmarkReport>) -> Self {
        let mut summary = Summary {
            total: landmarks.len(),
            by_status: BTreeMap::new(),
        };
        for l in &landmarks {
            *summary
                .by_status
                .entry(l.status.label().to_owned())
                .or_default() += 1;
        }
        Self {
            schema_version: 1,
            contract: LANDMARKS_CONTRACT.to_owned(),
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            summary,
            landmarks,
        }
    }

    /// The report as text, one line per landmark.
    #[must_use]
    pub fn text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for l in &self.landmarks {
            let _ = write!(out, "{:<12} {}", l.status.label(), l.name);
            if l.status.is_written() {
                let shapes: Vec<String> = l
                    .found_in
                    .iter()
                    .map(|f| {
                        format!(
                            "{} shape {} {} segments {:?}{}",
                            f.block.rsplit('/').next().unwrap_or(&f.block),
                            f.shape,
                            f.texture.rsplit('\\').next().unwrap_or(&f.texture),
                            f.segments,
                            if f.segment_ok {
                                ""
                            } else {
                                " (segment differs)"
                            }
                        )
                    })
                    .collect();
                let _ = write!(out, ": {} triangles in {}", l.triangles, shapes.join("; "));
            } else if !l.reason.is_empty() {
                let _ = write!(out, ": {}", l.reason);
            }
            out.push('\n');
        }
        let counts: Vec<String> = self
            .summary
            .by_status
            .iter()
            .map(|(k, v)| format!("{v} {k}"))
            .collect();
        let _ = writeln!(
            out,
            "{} landmarks: {}",
            self.summary.total,
            counts.join(", ")
        );
        out
    }
}

struct Unresolved<'a>(&'a Landmark, String);

fn blank(l: &Landmark, status: Status, reason: String) -> LandmarkReport {
    LandmarkReport {
        name: l.name.clone(),
        world: l.world.clone(),
        model: l.model.clone(),
        reference: match l.place {
            Place::Reference(id) => Some(format!("{id:08X}")),
            Place::Position(_) => None,
        },
        position: match l.place {
            Place::Position(p) => Some(p.pos),
            Place::Reference(_) => None,
        },
        status,
        reason,
        expected_vertices: 0,
        matched_vertices: 0,
        triangles: 0,
        found_in: Vec::new(),
    }
}

fn instance_for(model: &str, placement: Placement) -> Instance {
    Instance {
        base: 0,
        signature: *b"STAT",
        model: model.to_owned(),
        placement,
        high_priority: false,
        has_tree_lod: false,
        reference: 0,
        reference_plugin: 0,
        base_plugin: 0,
        flagged_by: 0,
        part: false,
        cell: None,
    }
}

/// Look every landmark up in `output`: the plugins and models come from the request's data folder and load order,
/// the blocks from `output`.
///
/// # Errors
/// An [`Error`] when the plugins or archives cannot be read, a world is not in the load order, or a world has no
/// object blocks folder in `output`.
pub fn check_landmarks(
    request: &Request,
    output: &Path,
    landmarks: &[Landmark],
) -> Result<LandmarksDoc, Error> {
    let data = Path::new(&request.data_folder);
    let loaded = load_plugins_with(
        data,
        &request.plugins,
        ReadOptions {
            objects: true,
            ..ReadOptions::default()
        },
    )?;
    let worlds = Worldspace::from_load_order(&loaded.plugins)
        .map_err(|e| Error::new("load_order_invalid", e.to_string()))?;
    let assets = FolderAssets::open(data, &request.plugins).map_err(|e| {
        Error::new("assets_unreadable", "the game's archives cannot be read").with_detail(e)
    })?;
    let mut results: HashMap<String, LandmarkReport> = HashMap::new();
    let mut world_names: Vec<&str> = Vec::new();
    for l in landmarks {
        if !world_names.iter().any(|w| w.eq_ignore_ascii_case(&l.world)) {
            world_names.push(&l.world);
        }
    }
    for world_name in world_names {
        let chosen = select_worlds(&worlds, Some(world_name))?;
        let Some(world) = chosen.first() else {
            continue;
        };
        let mine: Vec<&Landmark> = landmarks
            .iter()
            .filter(|l| l.world.eq_ignore_ascii_case(world_name))
            .collect();
        let candidates: HashMap<u32, Candidate> =
            if mine.iter().any(|l| matches!(l.place, Place::Reference(_))) {
                references_with_model(&loaded.plugins, world.form_id())
                    .map_err(|e| Error::new("load_order_invalid", e.to_string()))?
                    .into_iter()
                    .map(|c| (c.instance.reference, c))
                    .collect()
            } else {
                HashMap::new()
            };
        let mut written = WrittenWorld::open(output, world.name()).map_err(|e| {
            Error::new(
                "blocks_unreadable",
                format!("no object blocks of {} in the output", world.name()),
            )
            .with_detail(e)
        })?;
        for l in mine {
            let report = evaluate(l, &candidates, &mut written, &assets);
            results.insert(l.name.clone(), report);
        }
    }
    let mut out = Vec::with_capacity(landmarks.len());
    for l in landmarks {
        out.push(results.remove(&l.name).unwrap_or_else(|| {
            blank(
                l,
                Status::Unresolved,
                format!("the world {} is not in the load order", l.world),
            )
        }));
    }
    Ok(LandmarksDoc::new(out))
}

fn evaluate(
    l: &Landmark,
    candidates: &HashMap<u32, Candidate>,
    written: &mut WrittenWorld,
    assets: &FolderAssets,
) -> LandmarkReport {
    let (instance, excluded) = match resolve(l, candidates) {
        Err(Unresolved(l, reason)) => return blank(l, Status::Unresolved, reason),
        Ok(found) => found,
    };
    let mut r = blank(l, Status::Written, String::new());
    r.model.clone_from(&instance.model);
    r.position = Some(instance.placement.pos);
    if let Some(why) = excluded {
        r.status = Status::Excluded;
        why.clone_into(&mut r.reason);
        return r;
    }
    let expected = match expected_geometry(&instance, assets) {
        Err(why) => {
            r.status = Status::NoFarModel;
            r.reason = why;
            return r;
        }
        Ok(expected) => expected,
    };
    let presence = written.find(&expected);
    r.expected_vertices = presence.expected_vertices;
    r.matched_vertices = presence.matched_vertices;
    r.triangles = presence.hits.iter().map(|h| h.triangles).sum();
    r.found_in = presence
        .hits
        .iter()
        .map(|h| FoundIn {
            block: h.file.clone(),
            shape: h.shape,
            texture: h.texture.clone(),
            triangles: h.triangles,
            segments: h.segments.clone(),
            expected_segment: h.expected_segment,
            segment_ok: h.segments.contains(&h.expected_segment),
        })
        .collect();
    if !presence.written() {
        r.status = Status::Missing;
        r.reason = format!(
            "{} of {} vertices of its far model are in the blocks",
            presence.matched_vertices, presence.expected_vertices
        );
    }
    r
}

fn resolve<'a>(
    l: &'a Landmark,
    candidates: &HashMap<u32, Candidate>,
) -> Result<(Instance, Option<&'static str>), Unresolved<'a>> {
    match l.place {
        Place::Position(p) => Ok((instance_for(&l.model, p), None)),
        Place::Reference(id) => {
            let Some(c) = candidates.get(&id) else {
                return Err(Unresolved(
                    l,
                    format!("reference {id:08X} is not in the world (or has no model)"),
                ));
            };
            let key = |m: &str| m.to_ascii_lowercase().replace('\\', "/");
            if !l.model.is_empty() && key(&l.model) != key(&c.instance.model) {
                return Err(Unresolved(
                    l,
                    format!(
                        "reference {id:08X} has the model {}, not {}",
                        c.instance.model, l.model
                    ),
                ));
            }
            Ok((c.instance.clone(), c.left_out_because()))
        }
    }
}

/// What changed between a saved report and the current one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comparison {
    /// Landmarks that were written and are not any more, or lost more than the allowed share of their triangles.
    pub lost: Vec<String>,
    /// Landmarks that were not written and are now.
    pub returned: Vec<String>,
    /// Landmarks in the current report that the baseline does not have.
    pub added: Vec<String>,
    /// Landmarks in the baseline that the current list no longer has.
    pub removed: Vec<String>,
}

impl Comparison {
    /// Whether the comparison fails: a landmark is lost.
    #[must_use]
    pub fn failed(&self) -> bool {
        !self.lost.is_empty()
    }

    /// The comparison as text.
    #[must_use]
    pub fn text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for (what, list) in [
            ("LOST", &self.lost),
            ("returned", &self.returned),
            ("added to the list", &self.added),
            ("no longer in the list", &self.removed),
        ] {
            for name in list {
                let _ = writeln!(out, "{what}: {name}");
            }
        }
        if out.is_empty() {
            "no landmark changed\n".clone_into(&mut out);
        }
        out
    }
}

/// Compare `current` with `baseline`. A written landmark that is not written now is lost, and so is one whose
/// triangle count fell by more than `allowed_drop` (a share from 0 to 1).
#[must_use]
pub fn compare(baseline: &LandmarksDoc, current: &LandmarksDoc, allowed_drop: f64) -> Comparison {
    let now: HashMap<&str, &LandmarkReport> = current
        .landmarks
        .iter()
        .map(|l| (l.name.as_str(), l))
        .collect();
    let before: HashMap<&str, &LandmarkReport> = baseline
        .landmarks
        .iter()
        .map(|l| (l.name.as_str(), l))
        .collect();
    let mut out = Comparison::default();
    for old in &baseline.landmarks {
        let Some(new) = now.get(old.name.as_str()) else {
            out.removed.push(old.name.clone());
            continue;
        };
        if old.status.is_written() {
            let fell = f64::from(
                u32::try_from(old.triangles.saturating_sub(new.triangles)).unwrap_or(u32::MAX),
            ) > allowed_drop
                * f64::from(u32::try_from(old.triangles).unwrap_or(u32::MAX));
            if !new.status.is_written() || fell {
                out.lost.push(old.name.clone());
            }
        } else if new.status.is_written() {
            out.returned.push(old.name.clone());
        }
    }
    for new in &current.landmarks {
        if !before.contains_key(new.name.as_str()) {
            out.added.push(new.name.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "# a comment\n\
        Tower\tMojave\tarch\\tower.nif\tref:0008D36C\tthe tower\n\
        \n\
        Gate\tMojave\tarch\\gate.nif\tpos:100,200,300\n\
        Turned\tMojave\tarch\\gate.nif\tpos:1,2,3,0,0,1.5,2\n\
        Any\tMojave\t-\tref:00000100\n";

    #[test]
    fn a_landmark_file_gives_references_and_positions_with_their_notes() {
        let list = parse_landmarks(LIST).unwrap();
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].place, Place::Reference(0x0008_D36C));
        assert_eq!(list[0].note, "the tower");
        assert_eq!(
            list[1].place,
            Place::Position(Placement {
                pos: [100.0, 200.0, 300.0],
                rot: [0.0; 3],
                scale: 1.0
            })
        );
        assert_eq!(
            list[2].place,
            Place::Position(Placement {
                pos: [1.0, 2.0, 3.0],
                rot: [0.0, 0.0, 1.5],
                scale: 2.0
            })
        );
        assert_eq!(list[3].model, "");
    }

    #[test]
    fn a_bad_landmark_line_is_refused_with_its_number() {
        for (text, wants) in [
            ("a\tW\tm.nif", "line 1: needs"),
            ("a\tW\tm.nif\tfoo", "line 1: the place starts"),
            ("a\tW\tm.nif\tref:xyz", "line 1: \"xyz\" is not a hex"),
            ("a\tW\tm.nif\tpos:1,2", "line 1: pos: takes"),
            ("a\tW\t-\tpos:1,2,3", "line 1: a position needs a model"),
            (
                "a\tW\tm.nif\tref:1\n# x\na\tW\tm.nif\tref:2",
                "line 3: the name \"a\" is used twice",
            ),
        ] {
            let err = parse_landmarks(text).unwrap_err();
            assert!(err.contains(wants), "{text:?}: {err}");
        }
    }

    fn entry(name: &str, status: Status, triangles: usize) -> LandmarkReport {
        LandmarkReport {
            triangles,
            ..blank(
                &Landmark {
                    name: name.to_owned(),
                    world: "W".to_owned(),
                    model: "m.nif".to_owned(),
                    place: Place::Reference(1),
                    note: String::new(),
                },
                status,
                String::new(),
            )
        }
    }

    #[test]
    fn a_comparison_fails_for_a_landmark_that_was_written_and_is_not_or_lost_triangles() {
        let baseline = LandmarksDoc::new(vec![
            entry("kept", Status::Written, 100),
            entry("gone", Status::Written, 100),
            entry("thin", Status::Written, 100),
            entry("later", Status::Excluded, 0),
            entry("dropped from list", Status::Written, 5),
        ]);
        let current = LandmarksDoc::new(vec![
            entry("kept", Status::Written, 90),
            entry("gone", Status::Missing, 0),
            entry("thin", Status::Written, 40),
            entry("later", Status::Written, 10),
            entry("fresh", Status::Written, 10),
        ]);
        let c = compare(&baseline, &current, 0.2);
        assert_eq!(c.lost, ["gone", "thin"]);
        assert_eq!(c.returned, ["later"]);
        assert_eq!(c.added, ["fresh"]);
        assert_eq!(c.removed, ["dropped from list"]);
        assert!(c.failed());
        let same = compare(&baseline, &baseline, 0.2);
        assert!(!same.failed());
        assert_eq!(same.text(), "no landmark changed\n");
    }

    #[test]
    fn a_report_reads_back_as_the_baseline_it_was() {
        let doc = LandmarksDoc::new(vec![
            entry("a", Status::Written, 3),
            entry("b", Status::NoFarModel, 0),
        ]);
        let text = serde_json::to_string(&doc).unwrap();
        assert!(text.contains("\"contract\":\"lodwright-landmarks\""));
        assert!(text.contains("\"status\":\"no_far_model\""));
        let back: LandmarksDoc = serde_json::from_str(&text).unwrap();
        assert_eq!(back, doc);
        assert_eq!(doc.summary.by_status["written"], 1);
    }
}
