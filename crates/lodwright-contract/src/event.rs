// SPDX-License-Identifier: GPL-3.0-only
//! Progress events: newline-delimited JSON on stderr or in the file named by `--events`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a finished block holds. An open enum: a reader treats a kind it does not know as "other".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(extend("x-open-enum" = true))]
pub enum BlockKind {
    /// A terrain block (mesh, diffuse and normal map) of a level.
    Terrain,
    /// An object LOD block file.
    Objects,
    /// A tree LOD block file.
    Trees,
    /// A kind this build does not know, written by a later version. Never written by this build.
    #[serde(other)]
    Other,
}

/// One progress line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A block finished.
    Block {
        /// What the block holds; a line without it (written before the kind existed) is a terrain block.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<BlockKind>,
        /// World editor id.
        world: String,
        /// Cells per block edge: the terrain level, 4 for an object block, 8 for a tree block.
        level: u32,
        /// Blocks of this kind and level finished so far, in this world.
        done: u64,
        /// Blocks of this kind and level in total, in this world.
        of: u64,
        /// The block's files were kept from an earlier run (its inputs did not change).
        #[serde(default)]
        up_to_date: bool,
    },
}

impl Event {
    /// The line (without the newline) to print.
    ///
    /// # Panics
    /// Never: the type serialises to JSON.
    #[must_use]
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("an event serialises")
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockKind, Event};

    #[test]
    fn a_block_event_is_one_line_with_the_event_tag_first() {
        let line = Event::Block {
            kind: Some(BlockKind::Terrain),
            world: "W".into(),
            level: 4,
            done: 1,
            of: 9,
            up_to_date: false,
        }
        .to_line();
        assert_eq!(
            line,
            r#"{"event":"block","kind":"terrain","world":"W","level":4,"done":1,"of":9,"up_to_date":false}"#
        );
        assert!(!line.contains('\n'));
    }

    #[test]
    fn a_line_without_a_kind_reads_as_before_and_a_line_with_one_reads_its_kind() {
        let old: Event = serde_json::from_str(
            r#"{"event":"block","world":"W","level":4,"done":1,"of":9,"up_to_date":true}"#,
        )
        .unwrap();
        assert!(matches!(
            old,
            Event::Block {
                kind: None,
                level: 4,
                up_to_date: true,
                ..
            }
        ));
        let new: Event = serde_json::from_str(
            r#"{"event":"block","kind":"trees","world":"W","level":8,"done":2,"of":3,"up_to_date":false}"#,
        )
        .unwrap();
        assert_eq!(
            new,
            Event::Block {
                kind: Some(BlockKind::Trees),
                world: "W".into(),
                level: 8,
                done: 2,
                of: 3,
                up_to_date: false
            }
        );
    }

    #[test]
    fn a_kind_this_build_does_not_know_reads_as_other() {
        let event: Event = serde_json::from_str(
            r#"{"event":"block","kind":"water","world":"W","level":4,"done":1,"of":2}"#,
        )
        .unwrap();
        assert!(matches!(
            event,
            Event::Block {
                kind: Some(BlockKind::Other),
                ..
            }
        ));
    }

    #[test]
    fn an_event_line_reads_back_and_ignores_unknown_fields() {
        let event: Event = serde_json::from_str(
            r#"{"event":"block","world":"W","level":4,"done":1,"of":9,"eta":3}"#,
        )
        .unwrap();
        assert_eq!(
            event,
            Event::Block {
                kind: None,
                world: "W".into(),
                level: 4,
                done: 1,
                of: 9,
                up_to_date: false
            }
        );
    }
}
