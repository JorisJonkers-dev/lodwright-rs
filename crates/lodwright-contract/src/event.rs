// SPDX-License-Identifier: MPL-2.0
//! Progress events: newline-delimited JSON on stderr or in the file named by `--events`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One progress line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A block of a level finished.
    Block {
        /// World editor id.
        world: String,
        /// Level (cells per block edge).
        level: u32,
        /// Blocks of this level finished so far.
        done: u64,
        /// Blocks of this level in total.
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
    use super::Event;

    #[test]
    fn a_block_event_is_one_line_with_the_event_tag_first() {
        let line = Event::Block {
            world: "W".into(),
            level: 4,
            done: 1,
            of: 9,
            up_to_date: false,
        }
        .to_line();
        assert_eq!(
            line,
            r#"{"event":"block","world":"W","level":4,"done":1,"of":9,"up_to_date":false}"#
        );
        assert!(!line.contains('\n'));
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
                world: "W".into(),
                level: 4,
                done: 1,
                of: 9,
                up_to_date: false
            }
        );
    }
}
