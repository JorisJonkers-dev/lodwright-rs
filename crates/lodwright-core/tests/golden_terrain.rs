// SPDX-License-Identifier: MPL-2.0
//! Golden test: the terrain plan of a synthetic two-hill world, from plugin bytes to planned blocks.

use lodwright_core::plugin::read;
use lodwright_core::terrain::{TerrainSettings, plan_world};
use lodwright_core::worldspace::Worldspace;
use lodwright_testkit::golden::assert_golden;
use lodwright_testkit::land::Heights;
use lodwright_testkit::world::{SyntheticWorld, world_plugin};
use std::fmt::Write as _;
use std::path::Path;

fn hill(cx: f32, cy: f32) -> Heights {
    Heights::from_fn(|x, y| {
        let dx = f32::from(u8::try_from(x).unwrap()) - cx;
        let dy = f32::from(u8::try_from(y).unwrap()) - cy;
        8.0 * (30.0 - (dx * dx + dy * dy).sqrt().round()).max(0.0)
    })
}

#[test]
fn a_synthetic_world_plans_to_the_stored_blocks() {
    let world = SyntheticWorld::new(0x0100_0A00, "Testland")
        .cell(0, 0, hill(16.0, 16.0))
        .cell(1, 0, Heights::flat(0.0))
        .cell(-3, 5, hill(0.0, 32.0));
    let plugin = read(&world_plugin(&["Base.esm"], &world, true)).unwrap();
    let worlds = Worldspace::from_load_order(&[
        (
            "Base.esm".to_owned(),
            read(&world_plugin(&[], &SyntheticWorld::new(1, "x"), false)).unwrap(),
        ),
        ("Test.esp".to_owned(), plugin),
    ])
    .unwrap();
    let found = worlds.iter().find(|w| w.name() == "Testland").unwrap();
    let plan = plan_world(found, &TerrainSettings::default()).unwrap();
    let mut text = format!("world {} settings {}\n", plan.world, plan.settings_path);
    for b in &plan.blocks {
        writeln!(text, "L{} ({},{}) step {} vertices {} triangles {} error {:.2} heights {}..{} textures {}/{} mips {} {}",
            b.id.level, b.id.x, b.id.y, b.step, b.vertices, b.triangles, b.max_error,
            b.height_range.0, b.height_range.1, b.diffuse_size, b.normal_size, b.mip_levels, b.mesh_path
        ).unwrap();
    }
    assert_golden(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/terrain_plan.txt"),
        &text,
    );
}
