// SPDX-License-Identifier: GPL-3.0-only
//! Progress events and the cancel token of a call.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lodwright_contract::BlockKind;
use serde::{Deserialize, Serialize};

/// A step of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// Reading the plugins of the load order.
    ReadPlugins,
    /// Building the terrain blocks of a world.
    Terrain,
    /// Building the object LOD of a world.
    Objects,
    /// Building the tree LOD of a world.
    Trees,
    /// Packing the generated files into an archive, when the request asks for it. After the last world, before the
    /// manifest.
    Pack,
    /// Writing the manifest.
    Manifest,
}

impl Step {
    /// The step's name as it is written in events and the library's serialised events.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::ReadPlugins => "read_plugins",
            Self::Terrain => "terrain",
            Self::Objects => "objects",
            Self::Trees => "trees",
            Self::Pack => "pack",
            Self::Manifest => "manifest",
        }
    }

    /// A stable key a caller words in its own voice, for example `lod.terrain`.
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            Self::ReadPlugins => "lod.read_plugins",
            Self::Terrain => "lod.terrain",
            Self::Objects => "lod.objects",
            Self::Trees => "lod.trees",
            Self::Pack => "lod.pack",
            Self::Manifest => "lod.manifest",
        }
    }
}

/// One progress event. A step starts with an event without counts; terrain, objects and trees then report each
/// finished block with `level`, `done` and `total`; the pack step reports the files it has read with `done` and `total`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// The step.
    pub step: Step,
    /// The world editor id, for the per-world steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// Cells per block edge, for a finished block: the terrain level, 4 for an object block, 8 for a tree block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Blocks of this step and level finished so far, in this world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<u64>,
    /// Blocks of this step and level in total, in this world.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// The block's files were kept from an earlier run.
    #[serde(default)]
    pub up_to_date: bool,
    /// Which phase of the run this is, from 1. A phase is one step of one world, `read_plugins` first and
    /// `manifest` last. Absent on the first event of a run, before the plugins are read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_index: Option<u32>,
    /// How many phases the run has. Absent on the first event of a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_count: Option<u32>,
    /// Progress inside the phase, in units that never change their total during the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_done: Option<u64>,
    /// The phase's total, in the same units as `phase_done`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_total: Option<u64>,
    /// Run-wide progress. Never decreases during a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_done: Option<u64>,
    /// The run's total, in the same units as `run_done`. The same number on every event of a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_total: Option<u64>,
}

impl ProgressEvent {
    /// The start of `step`, for `world` when the step is per world.
    #[must_use]
    pub fn start(step: Step, world: Option<&str>) -> Self {
        Self {
            step,
            world: world.map(str::to_owned),
            level: None,
            done: None,
            total: None,
            up_to_date: false,
            phase_index: None,
            phase_count: None,
            phase_done: None,
            phase_total: None,
            run_done: None,
            run_total: None,
        }
    }

    /// The events-file line (`lodwright generate --events`) of a finished block; `None` for the start of a step and
    /// for the steps that have no blocks. The line is made from this event, so the file and the callback cannot
    /// tell different stories.
    #[must_use]
    pub fn block_line(&self) -> Option<lodwright_contract::Event> {
        let kind = match self.step {
            Step::Terrain => BlockKind::Terrain,
            Step::Objects => BlockKind::Objects,
            Step::Trees => BlockKind::Trees,
            Step::ReadPlugins | Step::Pack | Step::Manifest => return None,
        };
        Some(lodwright_contract::Event::Block {
            kind: Some(kind),
            world: self.world.clone()?,
            level: self.level?,
            done: self.done?,
            of: self.total?,
            up_to_date: self.up_to_date,
            phase: self.run_total.map(|_| self.step.name().to_owned()),
            phase_index: self.phase_index,
            phase_count: self.phase_count,
            phase_done: self.phase_done,
            phase_total: self.phase_total,
            run_done: self.run_done,
            run_total: self.run_total,
        })
    }

    /// The stable label key of the event's step.
    #[must_use]
    pub fn label_key(&self) -> &'static str {
        self.step.label_key()
    }
}

/// The run-wide total of every run, in units of a ten-thousandth of the run.
pub const RUN_TOTAL: u64 = 10_000;
/// Share of the run given to reading the plugins and to writing the manifest.
const READ_SHARE: u64 = 300;
const MANIFEST_SHARE: u64 = 200;
/// Share of the run given to packing, when the request asks for it.
const PACK_SHARE: u64 = 500;
/// Relative weight of a terrain, object and tree phase of one world. An estimate from a run of ten plugins; only
/// the shape of the bar depends on it, never its direction.
const TERRAIN_WEIGHT: u64 = 3;
const OBJECTS_WEIGHT: u64 = 4;
const TREES_WEIGHT: u64 = 3;

#[derive(Debug, Clone)]
struct Phase {
    step: Step,
    world: Option<String>,
    start: u64,
    width: u64,
}

/// Turns the finished blocks of a run into events that carry a run-wide progress that only moves forward.
///
/// The phases of a run are known once the plugins are read: one per enabled step per world, between reading the
/// plugins and writing the manifest. Each phase owns a fixed slice of [`RUN_TOTAL`]; inside a terrain phase the
/// levels share the slice by the area of their blocks (a level of 4 cells has sixty-four times the blocks of a level
/// of 32), and objects and trees count blocks. A number that would go back (terrain blocks finish on several
/// workers, out of order) is held at the highest value seen.
#[derive(Debug, Clone)]
pub(crate) struct RunTracker {
    phases: Vec<Phase>,
    current: usize,
    levels: Vec<u32>,
    run_done: u64,
    phase_done: u64,
}

impl RunTracker {
    /// A tracker before the plugins are read: it knows the run total and that reading comes first.
    pub(crate) fn new() -> Self {
        Self {
            phases: vec![Phase {
                step: Step::ReadPlugins,
                world: None,
                start: 0,
                width: READ_SHARE,
            }],
            current: 0,
            levels: Vec::new(),
            run_done: 0,
            phase_done: 0,
        }
    }

    /// Fix the phases once the worlds are chosen. `terrain_levels` is the terrain levels in the order they are
    /// built, or `None` when terrain is off.
    pub(crate) fn plan(
        &mut self,
        worlds: &[&str],
        terrain_levels: Option<&[u32]>,
        objects: bool,
        trees: bool,
        pack: bool,
    ) {
        self.levels = terrain_levels.map_or_else(Vec::new, <[u32]>::to_vec);
        let mut wanted: Vec<(Step, Option<String>, u64)> = Vec::new();
        for w in worlds {
            for (step, on, weight) in [
                (Step::Terrain, terrain_levels.is_some(), TERRAIN_WEIGHT),
                (Step::Objects, objects, OBJECTS_WEIGHT),
                (Step::Trees, trees, TREES_WEIGHT),
            ] {
                if on {
                    wanted.push((step, Some((*w).to_owned()), weight));
                }
            }
        }
        let sum: u64 = wanted.iter().map(|w| w.2).sum();
        let pack_share = if pack { PACK_SHARE } else { 0 };
        let room = RUN_TOTAL - READ_SHARE - MANIFEST_SHARE - pack_share;
        let mut phases = vec![self.phases[0].clone()];
        let mut cumulative = 0;
        let mut start = READ_SHARE;
        for (step, world, weight) in wanted {
            cumulative += weight;
            let end = READ_SHARE + room * cumulative / sum.max(1);
            phases.push(Phase {
                step,
                world,
                start,
                width: end - start,
            });
            start = end;
        }
        if pack {
            phases.push(Phase {
                step: Step::Pack,
                world: None,
                start,
                width: PACK_SHARE,
            });
            start += PACK_SHARE;
        }
        phases.push(Phase {
            step: Step::Manifest,
            world: None,
            start,
            width: RUN_TOTAL - start,
        });
        self.phases = phases;
        self.current = 0;
    }

    fn fill(&mut self, mut event: ProgressEvent, within: u64) -> ProgressEvent {
        let phase = &self.phases[self.current];
        let within = within.min(phase.width);
        self.phase_done = self.phase_done.max(within);
        self.run_done = self.run_done.max(phase.start + self.phase_done);
        event.phase_index = u32::try_from(self.current + 1).ok();
        // before the plugins are read the run has one known phase and the count is not final
        event.phase_count = (self.phases.len() > 1)
            .then(|| u32::try_from(self.phases.len()).ok())
            .flatten();
        event.phase_done = Some(self.phase_done);
        event.phase_total = Some(phase.width);
        event.run_done = Some(self.run_done);
        event.run_total = Some(RUN_TOTAL);
        event
    }

    fn enter(&mut self, step: Step, world: Option<&str>) {
        let found = (self.current..self.phases.len())
            .find(|&i| self.phases[i].step == step && self.phases[i].world.as_deref() == world);
        if let Some(i) = found
            && i != self.current
        {
            self.current = i;
            self.phase_done = 0;
        }
    }

    /// The start event of a step, with the run's numbers.
    pub(crate) fn start(&mut self, step: Step, world: Option<&str>) -> ProgressEvent {
        self.enter(step, world);
        self.fill(ProgressEvent::start(step, world), 0)
    }

    /// The event of a finished block, with the run's numbers.
    pub(crate) fn block(
        &mut self,
        step: Step,
        world: &str,
        level: u32,
        done: u64,
        total: u64,
        up_to_date: bool,
    ) -> ProgressEvent {
        self.enter(step, Some(world));
        let width = self.phases[self.current].width;
        let within = if step == Step::Terrain {
            self.terrain_within(level, done, total, width)
        } else {
            scaled(width, done, total)
        };
        let event = ProgressEvent {
            level: Some(level),
            done: Some(done),
            total: Some(total),
            up_to_date,
            ..ProgressEvent::start(step, Some(world))
        };
        self.fill(event, within)
    }

    /// The event of the pack step with `done` of `total` files read, with the run's numbers.
    pub(crate) fn files(&mut self, step: Step, done: u64, total: u64) -> ProgressEvent {
        self.enter(step, None);
        let width = self.phases[self.current].width;
        let event = ProgressEvent {
            done: Some(done),
            total: Some(total),
            ..ProgressEvent::start(step, None)
        };
        self.fill(event, scaled(width, done, total))
    }

    fn terrain_within(&self, level: u32, done: u64, total: u64, width: u64) -> u64 {
        let weight = |l: u32| u128::from(1u32 << 20) / u128::from(l.max(1)).pow(2);
        let sum: u128 = self.levels.iter().map(|l| weight(*l)).sum();
        let Some(at) = self.levels.iter().position(|l| *l == level) else {
            return scaled(width, done, total);
        };
        let before: u128 = self.levels[..at].iter().map(|l| weight(*l)).sum();
        let now = weight(level) * u128::from(done.min(total)) / u128::from(total.max(1));
        u64::try_from(u128::from(width) * (before + now) / sum.max(1)).unwrap_or(width)
    }
}

fn scaled(width: u64, done: u64, total: u64) -> u64 {
    u64::try_from(u128::from(width) * u128::from(done.min(total)) / u128::from(total.max(1)))
        .unwrap_or(width)
}

/// Receives the progress of a call. Terrain blocks finish on worker threads, so a receiver is `Send`; the call
/// hands it one event at a time.
pub trait Progress: Send {
    /// One event.
    fn report(&mut self, event: &ProgressEvent);
}

impl<F: FnMut(&ProgressEvent) + Send> Progress for F {
    fn report(&mut self, event: &ProgressEvent) {
        self(event);
    }
}

/// A receiver that drops every event.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProgress;

impl Progress for NoProgress {
    fn report(&mut self, _: &ProgressEvent) {}
}

/// Asks a running call to stop. Clones share one flag: cancel from any thread, and the call stops at its next safe
/// point (before the next terrain block, world or step).
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that is not cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the call to stop.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether [`CancelToken::cancel`] was called on this token or a clone.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::{CancelToken, ProgressEvent, Step};

    #[test]
    fn a_clone_of_a_token_sees_the_cancel() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled());
    }

    #[test]
    fn a_start_event_has_no_counts_and_serialises_without_them() {
        let e = ProgressEvent::start(Step::Terrain, Some("W"));
        assert_eq!(e.label_key(), "lod.terrain");
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"step":"terrain","world":"W","up_to_date":false}"#
        );
        assert_eq!(e.block_line(), None);
    }

    fn finished(step: Step, level: u32) -> ProgressEvent {
        ProgressEvent {
            level: Some(level),
            done: Some(2),
            total: Some(5),
            ..ProgressEvent::start(step, Some("W"))
        }
    }

    #[test]
    fn a_finished_block_of_each_step_renders_the_events_line_with_its_kind() {
        let lines: Vec<String> = [
            finished(Step::Terrain, 16),
            finished(Step::Objects, 4),
            finished(Step::Trees, 8),
        ]
        .iter()
        .map(|e| e.block_line().unwrap().to_line())
        .collect();
        assert_eq!(
            lines,
            [
                r#"{"event":"block","kind":"terrain","world":"W","level":16,"done":2,"of":5,"up_to_date":false}"#,
                r#"{"event":"block","kind":"objects","world":"W","level":4,"done":2,"of":5,"up_to_date":false}"#,
                r#"{"event":"block","kind":"trees","world":"W","level":8,"done":2,"of":5,"up_to_date":false}"#,
            ]
        );
    }

    #[test]
    fn a_step_without_blocks_has_no_events_line_even_with_counts() {
        assert_eq!(finished(Step::Manifest, 4).block_line(), None);
        assert_eq!(finished(Step::ReadPlugins, 4).block_line(), None);
    }
}
