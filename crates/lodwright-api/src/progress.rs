// SPDX-License-Identifier: GPL-3.0-only
//! Progress events and the cancel token of a call.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    /// Writing the manifest.
    Manifest,
}

impl Step {
    /// A stable key a caller words in its own voice, for example `lod.terrain`.
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            Self::ReadPlugins => "lod.read_plugins",
            Self::Terrain => "lod.terrain",
            Self::Objects => "lod.objects",
            Self::Trees => "lod.trees",
            Self::Manifest => "lod.manifest",
        }
    }
}

/// One progress event. A step starts with an event without counts; terrain then reports each finished block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// The step.
    pub step: Step,
    /// The world editor id, for the per-world steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// The terrain level (cells per block edge), for a finished block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// Blocks of this level finished so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done: Option<u64>,
    /// Blocks of this level in total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// The block's files were kept from an earlier run.
    #[serde(default)]
    pub up_to_date: bool,
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
        }
    }

    /// The stable label key of the event's step.
    #[must_use]
    pub fn label_key(&self) -> &'static str {
        self.step.label_key()
    }
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
    }
}
