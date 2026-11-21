// SPDX-License-Identifier: GPL-3.0-only
//! The stub plugin of a packed archive reads back as a plugin with no records, no masters, not a master.

use std::io::Cursor;
use std::path::Path;

use esplugin::{GameId, ParseOptions, Plugin};
use lodwright_assets::stub::{AUTHOR, PackedNames, stub_plugin};
use lodwright_plugin_meta::read_header;

#[test]
fn the_stub_has_the_one_master_it_was_given_and_is_not_a_master() {
    let header = read_header(
        "Lodwright LOD.esp",
        &stub_plugin("FalloutNV.esm", 14_000_000),
    )
    .unwrap();
    assert_eq!(header.masters, ["FalloutNV.esm"]);
    assert!(!header.is_master);
}

#[test]
fn the_stub_holds_no_record_and_names_its_author() {
    let mut plugin = Plugin::new(GameId::FalloutNV, Path::new("Lodwright LOD.esp"));
    plugin
        .parse_reader(
            Cursor::new(stub_plugin("FalloutNV.esm", 14_000_000)),
            ParseOptions::whole_plugin(),
        )
        .unwrap();
    assert_eq!(plugin.record_and_group_count(), Some(0));
    assert_eq!(plugin.header_version(), Some(1.34));
    assert_eq!(
        plugin
            .description()
            .unwrap()
            .as_deref()
            .map(|d| d.starts_with("Registers")),
        Some(true)
    );
    assert!(!plugin.is_light_plugin());
    assert_eq!(AUTHOR, "lodwright");
}

#[test]
fn the_stub_file_name_is_the_archive_base_name() {
    let names = PackedNames::new("Lodwright LOD").unwrap();
    assert_eq!(
        Path::new(&names.plugin).file_stem(),
        Path::new(&names.archive).file_stem()
    );
    assert_eq!(names.plugin, "Lodwright LOD.esp");
}
