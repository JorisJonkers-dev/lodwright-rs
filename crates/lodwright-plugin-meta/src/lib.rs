// SPDX-License-Identifier: GPL-3.0-only
//! Plugin header facts read with the `esplugin` crate (GPL-3.0): masters and the master flag. Kept in its own
//! crate so the MPL-2.0 crates keep a permissive dependency tree; see `docs/LICENSING.md`.

use std::io::Cursor;
use std::path::Path;

use esplugin::{GameId, ParseOptions, Plugin};

/// What a plugin's header says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Master file names, in the order the header lists them.
    pub masters: Vec<String>,
    /// Whether the header carries the master flag.
    pub is_master: bool,
}

/// Read the header of a Fallout: New Vegas plugin from its bytes.
///
/// # Errors
/// A message when the bytes are not a plugin esplugin can read.
pub fn read_header(name: &str, bytes: &[u8]) -> Result<Header, String> {
    let mut plugin = Plugin::new(GameId::FalloutNV, Path::new(name));
    plugin
        .parse_reader(Cursor::new(bytes), ParseOptions::header_only())
        .map_err(|e| e.to_string())?;
    Ok(Header {
        masters: plugin.masters().map_err(|e| e.to_string())?,
        is_master: plugin.is_master_file(),
    })
}

/// Check that every master named by a plugin that is also in `order` loads before it (names compared without
/// regard to case). `headers` is in load order, as `(name, header)`.
///
/// # Errors
/// A message naming the plugin and the master that loads too late.
pub fn check_master_order(headers: &[(String, Header)]) -> Result<(), String> {
    for (i, (name, header)) in headers.iter().enumerate() {
        for master in &header.masters {
            let later = headers[i..]
                .iter()
                .any(|(other, _)| other.eq_ignore_ascii_case(master));
            if later {
                return Err(format!(
                    "plugin {name} loads before its master {master}: masters must come first"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Header, check_master_order, read_header};
    use lodwright_testkit::plugin::PluginBuilder;

    fn header(masters: &[&str]) -> Header {
        Header {
            masters: masters.iter().map(|m| (*m).to_owned()).collect(),
            is_master: false,
        }
    }

    #[test]
    fn masters_are_read_in_header_order() {
        let bytes = PluginBuilder::new().master("A.esm").master("B.esm").build();
        let h = read_header("x.esp", &bytes).unwrap();
        assert_eq!(h.masters, ["A.esm", "B.esm"]);
    }

    #[test]
    fn text_is_not_a_plugin() {
        assert!(read_header("x.esp", b"just some text, not a plugin").is_err());
    }

    #[test]
    fn a_master_after_its_dependent_is_an_error() {
        let list = vec![
            ("Patch.esp".to_owned(), header(&["base.esm"])),
            ("Base.esm".to_owned(), header(&[])),
        ];
        let err = check_master_order(&list).unwrap_err();
        assert!(
            err.contains("Patch.esp") && err.contains("base.esm"),
            "{err}"
        );
    }

    #[test]
    fn masters_first_is_accepted_and_an_absent_master_is_left_to_the_caller() {
        let list = vec![
            ("Base.esm".to_owned(), header(&[])),
            ("Patch.esp".to_owned(), header(&["Base.esm", "Gone.esm"])),
        ];
        assert!(check_master_order(&list).is_ok());
    }
}
