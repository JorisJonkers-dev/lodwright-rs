// SPDX-License-Identifier: GPL-3.0-only
//! Far glow (`objects.far_glow`): a glow atlas beside the object atlas, named in the glow slot of the far blocks with an
//! emissive material, on synthetic models. Off, the output is the same bytes as for models without glow maps.

use std::collections::HashMap;

use lodwright_core::objects::Instance;
use lodwright_core::plugin::Placement;
use lodwright_dds::{EncodeOptions, Filter, Format, Image, Quality, decode_image, encode};
use lodwright_gen::{Assets, Surfaces, generate_objects_with_progress};
use lodwright_nif::{
    AvObject, Block, Body, ExportInfo, GeometryData, Header, NiNode, NiTriShape, NiTriStripsData,
    NifFile, ObjectNet, read_object_shapes,
};

const BLOCK: &str = "meshes/landscape/lod/mojave/blocks/mojave.level4.x0.y0.nif";
const GLOW: &str = "textures/landscape/lod/mojave/blocks/mojave.buildings_g.dds";

struct Memory(HashMap<String, Vec<u8>>);

impl Assets for Memory {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0
            .get(&path.to_ascii_lowercase().replace('\\', "/"))
            .cloned()
    }
}

fn av(properties: Vec<i32>) -> AvObject {
    AvObject {
        net: ObjectNet {
            name: -1,
            extra_data: vec![],
            controller: -1,
        },
        flags: 14,
        translation: [0.0; 3],
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        properties,
        collision_object: -1,
    }
}

/// A one-triangle LOD model whose texture set names `diffuse` and, in slot 2, `glow` (empty for none).
fn model(diffuse: &str, glow: &str) -> Vec<u8> {
    let mut lighting = Vec::new();
    for v in [u32::MAX, 0, u32::MAX] {
        lighting.extend_from_slice(&v.to_le_bytes());
    }
    lighting.extend_from_slice(&[0; 42]);
    lighting[34..38].copy_from_slice(&4_u32.to_le_bytes());
    let mut set = 6_u32.to_le_bytes().to_vec();
    for p in [diffuse, "", glow] {
        set.extend_from_slice(&u32::try_from(p.len()).unwrap().to_le_bytes());
        set.extend_from_slice(p.as_bytes());
    }
    set.extend_from_slice(&[0; 12]);
    one_shape(
        &[b"BSShaderPPLightingProperty", b"BSShaderTextureSet"],
        vec![Body::Opaque(lighting), Body::Opaque(set)],
    )
}

/// A one-triangle model drawn without lighting and blended by its alpha, as the light strips on a casino front are:
/// its `BSShaderNoLightingProperty` names `texture`.
fn card_model(texture: &str) -> Vec<u8> {
    let mut unlit = Vec::new();
    for v in [u32::MAX, 0, u32::MAX] {
        unlit.extend_from_slice(&v.to_le_bytes());
    }
    unlit.extend_from_slice(&[0; 22]);
    unlit.extend_from_slice(&u32::try_from(texture.len()).unwrap().to_le_bytes());
    unlit.extend_from_slice(texture.as_bytes());
    unlit.extend_from_slice(&[0; 16]);
    // name, extra data, controller, blend flags and threshold
    let mut alpha = Vec::new();
    for v in [u32::MAX, 0, u32::MAX] {
        alpha.extend_from_slice(&v.to_le_bytes());
    }
    alpha.extend_from_slice(&[0xed, 0x00, 0x80]);
    one_shape(
        &[b"BSShaderNoLightingProperty", b"NiAlphaProperty"],
        vec![Body::Opaque(unlit), Body::Opaque(alpha)],
    )
}

/// A model of one triangle whose shape has the two property blocks `properties` (types and bodies).
fn one_shape(property_types: &[&[u8]; 2], properties: Vec<Body>) -> Vec<u8> {
    let shape = NiTriShape {
        av: av(vec![3, 4]),
        data: 2,
        skin_instance: -1,
        material_names: vec![],
        material_extra_data: vec![],
        active_material: 0,
        material_needs_update: 0,
        shader_property: -1,
        alpha_property: -1,
    };
    let data = NiTriStripsData {
        geometry: GeometryData {
            group_id: 0,
            num_vertices: 3,
            keep_flags: 0,
            compress_flags: 0,
            vertices: Some(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]]),
            vector_flags: 1,
            material_crc: 0,
            normals: None,
            tangents: None,
            center: [0.0; 3],
            radius: 0.0,
            vertex_colors: None,
            uv_sets: vec![vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]],
            consistency: 0,
            additional_data: -1,
        },
        num_triangles: 1,
        strips: Some(vec![vec![0, 1, 2]]),
    };
    let types: Vec<Vec<u8>> = [&b"NiNode"[..], b"NiTriStrips", b"NiTriStripsData"]
        .iter()
        .chain(property_types)
        .map(|t| t.to_vec())
        .collect();
    let mut bodies = vec![
        Body::Node(NiNode {
            av: av(vec![]),
            children: vec![1],
            effects: vec![],
        }),
        Body::TriStrips(shape),
        Body::TriStripsData(data),
    ];
    bodies.extend(properties);
    NifFile {
        header: Header {
            user_version: 11,
            bethesda: Some((
                34,
                ExportInfo {
                    author: vec![0],
                    unknown_int: 0,
                    process_script: vec![0],
                    export_script: vec![0],
                    max_filepath: vec![],
                },
            )),
            block_types: types,
            strings: vec![],
            groups: vec![],
        },
        blocks: bodies
            .into_iter()
            .enumerate()
            .map(|(i, body)| Block {
                type_index: u16::try_from(i).unwrap(),
                body,
            })
            .collect(),
        roots: vec![0],
    }
    .write()
    .unwrap()
}

fn texture(size: u32, rgb: [u8; 3]) -> Vec<u8> {
    let px: Vec<u8> = (0..size * size)
        .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
        .collect();
    encode(
        &Image::new(size, size, px).unwrap(),
        &EncodeOptions {
            format: Format::Bc1,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )
    .unwrap()
}

/// A lit wall (with a glow map unless `wall_glow` is empty) and a rock without one.
fn assets(wall_glow: &str) -> Memory {
    let mut m = HashMap::new();
    m.insert(
        "meshes/a/wall_lod.nif".to_owned(),
        model("Data\\Textures\\a\\wall.dds", wall_glow),
    );
    m.insert(
        "meshes/a/rock_lod.nif".to_owned(),
        model("Data\\Textures\\a\\rock.dds", ""),
    );
    m.insert("textures/a/wall.dds".to_owned(), texture(128, [90, 90, 90]));
    m.insert(
        "textures/a/wall_g.dds".to_owned(),
        texture(128, [255, 200, 40]),
    );
    m.insert(
        "textures/a/rock.dds".to_owned(),
        texture(64, [120, 100, 80]),
    );
    Memory(m)
}

fn instance(model: &str, x: f32, reference: u32) -> Instance {
    Instance {
        base: reference,
        signature: *b"STAT",
        model: model.to_owned(),
        placement: Placement {
            pos: [x, 100.0, 10.0],
            rot: [0.0; 3],
            scale: 1.0,
        },
        high_priority: false,
        has_tree_lod: false,
        reference,
        reference_plugin: 0,
        base_plugin: 0,
        flagged_by: 0,
        part: false,
        cell: None,
    }
}

fn generate(name: &str, assets: &Memory, glow: bool) -> (std::path::PathBuf, Vec<String>) {
    let out = std::env::temp_dir().join(format!("lodwright-glow-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let list = [
        instance("a\\wall.nif", 100.0, 1),
        instance("a\\rock.nif", 300.0, 2),
    ];
    let report = generate_objects_with_progress(
        "Mojave",
        &list,
        assets,
        &out,
        Surfaces {
            glow,
            ..Surfaces::default()
        },
        &mut |_, _| {},
    )
    .unwrap();
    let files = report.files.iter().map(|f| f.path.clone()).collect();
    (out, files)
}

fn shapes(out: &std::path::Path) -> Vec<lodwright_nif::ObjectShape> {
    read_object_shapes(&NifFile::read(&std::fs::read(out.join(BLOCK)).unwrap()).unwrap()).unwrap()
}

#[test]
fn with_far_glow_the_blocks_name_a_glow_atlas_and_an_emissive_material() {
    let (out, files) = generate("on", &assets("Data\\Textures\\a\\wall_g.dds"), true);
    assert!(files.iter().any(|f| f == GLOW), "{files:?}");
    let shapes = shapes(&out);
    assert_eq!(shapes.len(), 1);
    assert_eq!(
        shapes[0].glow_texture,
        "Data\\Textures\\Landscape\\LOD\\Mojave\\Blocks\\Mojave.Buildings_g.dds"
    );
    assert!(shapes[0].emissive);
    // the glow atlas is half the atlas, as the normal atlas is, and black but for the wall's glow map
    let glow = decode_image(&std::fs::read(out.join(GLOW)).unwrap()).unwrap();
    assert_eq!((glow.width(), glow.height()), (1024, 512));
    let lit = glow
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 200 && p[1] > 150)
        .count();
    // the wall's 128 square texture is 64 square at half size; everything else stays black
    assert_eq!(lit, 64 * 64);
    assert!(
        glow.rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| (p[0] > 200 && p[1] > 150) || p[..3] == [0, 0, 0])
    );
}

#[test]
fn without_far_glow_the_output_is_the_same_bytes_as_for_models_without_glow_maps() {
    let (with_maps, files) = generate("off-maps", &assets("Data\\Textures\\a\\wall_g.dds"), false);
    let (without, other) = generate("off-none", &assets(""), false);
    assert_eq!(files, other);
    assert!(!files.iter().any(|f| f.ends_with("_g.dds")), "{files:?}");
    for f in &files {
        assert_eq!(
            std::fs::read(with_maps.join(f)).unwrap(),
            std::fs::read(without.join(f)).unwrap(),
            "{f}"
        );
    }
    let shapes = shapes(&with_maps);
    assert_eq!(
        (shapes[0].glow_texture.as_str(), shapes[0].emissive),
        ("", false)
    );
}

#[test]
fn far_glow_on_models_without_glow_maps_writes_no_glow_atlas_and_the_same_blocks() {
    let (on, files) = generate("on-none", &assets(""), true);
    let (off, other) = generate("off-none-2", &assets(""), false);
    assert_eq!(files, other);
    for f in &files {
        assert_eq!(
            std::fs::read(on.join(f)).unwrap(),
            std::fs::read(off.join(f)).unwrap(),
            "{f}"
        );
    }
}

#[test]
fn a_glow_map_that_cannot_be_read_leaves_its_page_without_a_glow_atlas() {
    let (out, files) = generate("missing", &assets("Data\\Textures\\a\\gone_g.dds"), true);
    assert!(!files.iter().any(|f| f.ends_with("_g.dds")), "{files:?}");
    assert_eq!(shapes(&out)[0].glow_texture, "");
}

/// The wall with an authored LOD model without a glow map and a full model with one, as lit casinos have.
fn assets_with_full_model() -> Memory {
    let mut m = assets("").0;
    m.insert(
        "meshes/a/wall.nif".to_owned(),
        model(
            "Data\\Textures\\a\\wall.dds",
            "Data\\Textures\\a\\wall_g.dds",
        ),
    );
    Memory(m)
}

fn generate_replacing(
    name: &str,
    list: &[Instance],
    replaces: lodwright_gen::RuleReplaces<'_>,
) -> (std::path::PathBuf, Vec<String>) {
    let out = std::env::temp_dir().join(format!("lodwright-glow-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let limits = |i: &Instance| {
        i.model
            .contains("wall")
            .then_some(lodwright_gen::FarLimits {
                keep: 1.0,
                max_error: 0.05,
            })
    };
    let report = generate_objects_with_progress(
        "Mojave",
        list,
        &assets_with_full_model(),
        &out,
        Surfaces {
            glow: true,
            rule_shapes: Some(&limits),
            rule_replaces: Some(replaces),
            ..Surfaces::default()
        },
        &mut |_, _| {},
    )
    .unwrap();
    let files = report.files.iter().map(|f| f.path.clone()).collect();
    (out, files)
}

#[test]
fn a_rule_that_replaces_the_lod_model_draws_the_full_model_with_its_glow_map() {
    let list = [
        instance("a\\wall.nif", 100.0, 1),
        instance("a\\rock.nif", 300.0, 2),
    ];
    let (out, files) = generate_replacing("replace", &list, &|i| i.model.contains("wall"));
    assert!(files.iter().any(|f| f == GLOW), "{files:?}");
    assert!(shapes(&out)[0].emissive);
    // without the rule's word the authored LOD model, which has no glow map, is drawn
    let (out, files) = generate_replacing("keep", &list, &|_| false);
    assert!(!files.iter().any(|f| f == GLOW), "{files:?}");
    assert_eq!(shapes(&out)[0].glow_texture, "");
}

#[test]
fn a_lod_model_stays_when_the_rule_does_not_match_every_object_that_uses_it() {
    let list = [
        instance("a\\wall.nif", 100.0, 1),
        instance("a\\wall.nif", 200.0, 3),
        instance("a\\rock.nif", 300.0, 2),
    ];
    let (out, files) = generate_replacing("partial", &list, &|i| i.reference == 1);
    assert!(!files.iter().any(|f| f == GLOW), "{files:?}");
    // both walls are still drawn, from the authored LOD model
    assert_eq!(shapes(&out)[0].triangles.len(), 3);
}

/// A texture of one colour and alpha, BC3.
fn texture_rgba(size: u32, rgba: [u8; 4]) -> Vec<u8> {
    let px: Vec<u8> = (0..size * size).flat_map(|_| rgba).collect();
    encode(
        &Image::new(size, size, px).unwrap(),
        &EncodeOptions {
            format: Format::Bc3,
            mips: true,
            filter: Filter::Box,
            quality: Quality::Fast,
        },
    )
    .unwrap()
}

/// A rock with a LOD model and casino lights with a full model only: one shape drawn without lighting, blended by its
/// alpha, made far by a rule.
fn generate_lights(name: &str, glow: bool) -> (std::path::PathBuf, Vec<String>) {
    let mut m = assets("").0;
    m.insert(
        "meshes/a/lights.nif".to_owned(),
        card_model("textures\\fx\\strip.dds"),
    );
    m.insert(
        "textures/fx/strip.dds".to_owned(),
        texture_rgba(64, [255, 255, 255, 128]),
    );
    let out = std::env::temp_dir().join(format!("lodwright-glow-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let limits = |i: &Instance| {
        i.model
            .contains("lights")
            .then_some(lodwright_gen::FarLimits {
                keep: 1.0,
                max_error: 0.05,
            })
    };
    let list = [
        instance("a\\lights.nif", 100.0, 1),
        instance("a\\rock.nif", 300.0, 2),
    ];
    let report = generate_objects_with_progress(
        "Mojave",
        &list,
        &Memory(m),
        &out,
        Surfaces {
            glow,
            rule_shapes: Some(&limits),
            ..Surfaces::default()
        },
        &mut |_, _| {},
    )
    .unwrap();
    let files = report.files.iter().map(|f| f.path.clone()).collect();
    (out, files)
}

#[test]
fn with_far_glow_a_shape_drawn_without_lighting_is_a_black_card_that_glows_with_its_texture() {
    let (out, files) = generate_lights("card", true);
    assert!(files.iter().any(|f| f == GLOW), "{files:?}");
    let shapes = shapes(&out);
    assert!(shapes[0].emissive);
    // the rock's triangle and the card's
    assert_eq!(shapes[0].triangles.len(), 2);
    // the card glows with its texture weighted by its alpha: white at half alpha is half grey
    let glow = decode_image(&std::fs::read(out.join(GLOW)).unwrap()).unwrap();
    let lit: Vec<&[u8; 4]> = glow
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] > 0)
        .collect();
    assert_eq!(lit.len(), 32 * 32);
    assert!(
        lit.iter().all(|p| (118..=138).contains(&p[0])),
        "{:?}",
        lit[0]
    );
    // and by day it is black: the card's place in the atlas is black, the rock keeps its colour
    let atlas = decode_image(
        &std::fs::read(out.join("textures/landscape/lod/mojave/blocks/mojave.buildings.dds"))
            .unwrap(),
    )
    .unwrap();
    let px = atlas.rgba().as_chunks::<4>().0;
    assert!(px.iter().filter(|p| p[..3] == [0, 0, 0]).count() >= 64 * 64);
    assert!(px.iter().any(|p| p[0] > 100 && p[1] > 80));
}

#[test]
fn without_far_glow_a_model_of_shapes_drawn_without_lighting_gets_no_far_shape() {
    let (out, files) = generate_lights("no-card", false);
    assert!(!files.iter().any(|f| f.ends_with("_g.dds")), "{files:?}");
    // only the rock
    assert_eq!(shapes(&out)[0].triangles.len(), 1);
}
