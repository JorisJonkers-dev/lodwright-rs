// SPDX-License-Identifier: GPL-3.0-only
//! A whole `generate` run over a synthetic load of sixty worlds, in its own process, stays under its memory budget.
//!
//! The peak (`peak_memory_mib`, from `VmHWM`) is reported on Linux only; elsewhere the run must still finish and say
//! nothing about its peak.

use std::path::PathBuf;
use std::process::Command;

use lodwright_contract::Request;
use lodwright_testkit::land::Heights;
use lodwright_testkit::plugin::PluginBuilder;
use lodwright_testkit::world::{SyntheticWorld, world_group};

const WORLDS: u32 = 60;
const BUDGET_MIB: u64 = 256;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lodwright-budget-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_sixty_world_run_stays_under_its_memory_budget() {
    let data = temp("data");
    std::fs::create_dir_all(&data).unwrap();
    let mut builder = PluginBuilder::new();
    for index in 0..WORLDS {
        let mut world = SyntheticWorld::new(0x1000 + index * 0x2000, &format!("World{index}"));
        for cell in 0..4_i32 {
            let height = f32::from(u16::try_from((cell + 1) * 8).unwrap());
            world = world.cell(cell % 2, cell / 2, Heights::flat(height));
        }
        builder = builder.group(world_group(&world, false));
    }
    std::fs::write(data.join("Worlds.esm"), builder.build()).unwrap();
    let out = temp("out");
    let mut request = Request::new(
        data.to_str().unwrap(),
        &["Worlds.esm"],
        out.to_str().unwrap(),
    );
    request.objects.enabled = false;
    request.trees.enabled = false;
    request.terrain.land_textures = false;
    request.limits.memory_mib = Some(BUDGET_MIB);
    let file = temp("request.json");
    std::fs::write(&file, serde_json::to_string(&request).unwrap()).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_lodwright"))
        .args(["generate", "--request", file.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    let text = String::from_utf8(run.stdout).unwrap();
    assert_eq!(run.status.code(), Some(0), "{text}");
    let doc: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    let settings = std::fs::read_dir(out.join("lodsettings")).unwrap().count();
    assert_eq!(settings, WORLDS as usize, "every world is built");
    if cfg!(target_os = "linux") {
        let peak = doc["peak_memory_mib"]
            .as_u64()
            .expect("Linux reports the peak");
        assert!(
            peak <= BUDGET_MIB,
            "peak {peak} MiB over the {BUDGET_MIB} MiB budget"
        );
    } else {
        assert!(doc.get("peak_memory_mib").is_none());
    }
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&data);
}
