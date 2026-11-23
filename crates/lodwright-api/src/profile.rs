// SPDX-License-Identifier: GPL-3.0-only
//! The step profile of a run: wall time, CPU time, peak memory and files written per step, in the order the steps ran.

use std::cell::RefCell;
use std::time::Instant;

use lodwright_contract::{
    ManifestFile, StepProfile, peak_memory_mib, process_cpu_ms, reset_peak_memory,
};

struct Open {
    step: String,
    world: Option<String>,
    started: Instant,
    cpu: Option<u64>,
    files: u64,
    bytes: u64,
}

/// Times the steps of one run. [`StepClock::begin`] ends the step that is open and starts the next; the peak
/// memory of a step is the highest resident size between its start and its end (on Linux).
#[derive(Default)]
struct State {
    open: Option<Open>,
    done: Vec<StepProfile>,
}

/// The clock takes `&self`, so a callback that a step hands to the code it runs can start the next phase.
#[derive(Default)]
pub(crate) struct StepClock {
    state: RefCell<State>,
}

impl StepClock {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// End the open step, if any, and start `step` (a name such as `terrain` or `terrain.blocks`).
    pub(crate) fn begin(&self, step: &str, world: Option<&str>) {
        self.end();
        reset_peak_memory();
        self.state.borrow_mut().open = Some(Open {
            step: step.to_owned(),
            world: world.map(str::to_owned),
            started: Instant::now(),
            cpu: process_cpu_ms(),
            files: 0,
            bytes: 0,
        });
    }

    /// Count files the open step wrote.
    pub(crate) fn wrote<'a>(&self, files: impl IntoIterator<Item = &'a ManifestFile>) {
        if let Some(open) = &mut self.state.borrow_mut().open {
            for f in files {
                open.files += 1;
                open.bytes += f.bytes;
            }
        }
    }

    /// End the open step.
    pub(crate) fn end(&self) {
        let Some(open) = self.state.borrow_mut().open.take() else {
            return;
        };
        let wall = open.started.elapsed();
        let cpu = open
            .cpu
            .zip(process_cpu_ms())
            .map(|(from, to)| to.saturating_sub(from));
        self.state.borrow_mut().done.push(StepProfile {
            step: open.step,
            world: open.world,
            wall_ms: u64::try_from(wall.as_millis()).unwrap_or(u64::MAX),
            cpu_ms: cpu,
            peak_memory_mib: peak_memory_mib(),
            files: open.files,
            bytes: open.bytes,
        });
    }

    /// End the open step and hand over every step.
    pub(crate) fn finish(self) -> Vec<StepProfile> {
        self.end();
        self.state.into_inner().done
    }
}

#[cfg(test)]
mod tests {
    use super::StepClock;
    use lodwright_contract::ManifestFile;

    #[test]
    fn steps_are_reported_in_order_with_their_world_and_the_files_they_wrote() {
        let clock = StepClock::new();
        clock.begin("read_plugins", None);
        clock.begin("terrain", Some("Island"));
        let file = |bytes| ManifestFile {
            path: "a".to_owned(),
            bytes,
            digest: String::new(),
        };
        clock.wrote(&[file(10), file(5)]);
        clock.begin("objects", Some("Island"));
        let steps = clock.finish();
        let names: Vec<(&str, Option<&str>)> = steps
            .iter()
            .map(|s| (s.step.as_str(), s.world.as_deref()))
            .collect();
        assert_eq!(
            names,
            [
                ("read_plugins", None),
                ("terrain", Some("Island")),
                ("objects", Some("Island"))
            ]
        );
        assert_eq!((steps[1].files, steps[1].bytes), (2, 15));
        assert_eq!((steps[0].files, steps[2].bytes), (0, 0));
    }

    #[test]
    fn a_step_that_burns_cpu_reports_cpu_time_where_the_platform_says() {
        let clock = StepClock::new();
        clock.begin("busy", None);
        let start = std::time::Instant::now();
        let mut x = 1_u64;
        while start.elapsed().as_millis() < 120 {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        }
        std::hint::black_box(x);
        let steps = clock.finish();
        assert!(steps[0].wall_ms >= 100, "{}", steps[0].wall_ms);
        if cfg!(target_os = "linux") {
            assert!(steps[0].cpu_ms.unwrap() >= 50, "{:?}", steps[0].cpu_ms);
            assert!(steps[0].peak_memory_mib.unwrap() > 0);
        }
    }
}
