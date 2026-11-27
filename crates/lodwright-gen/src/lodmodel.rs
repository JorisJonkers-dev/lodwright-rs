// SPDX-License-Identifier: GPL-3.0-only
//! Reading the small LOD models that sit beside the full models: geometry with the shape rotations and the scales
//! applied, the translations kept apart as an offset the placement scale does not touch, node rotations ignored
//! (`game-object-shape-offset`), plus the texture paths and whether the shape is alpha tested.

use lodwright_nif::{Body, NiNode, NiTriShape, NifFile};

/// One shape of a LOD model, in the model's own space.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Positions with the node rotations and scales applied, but not the translations.
    pub vertices: Vec<[f32; 3]>,
    /// The sum of the node translations: the game's own blocks apply the placement scale to the positions but
    /// not to this offset.
    pub offset: [f32; 3],
    /// Normals, when the model has them.
    pub normals: Option<Vec<[f32; 3]>>,
    /// Tangents and bitangents, when the model has them.
    pub tangents: Option<(Vec<[f32; 3]>, Vec<[f32; 3]>)>,
    /// The first UV set (zeros when the model has none).
    pub uv: Vec<[f32; 2]>,
    /// Triangles.
    pub triangles: Vec<[u16; 3]>,
    /// Diffuse texture path from the shape's texture set.
    pub diffuse: Option<String>,
    /// Normal map path from the shape's texture set.
    pub normal_map: Option<String>,
    /// Glow map path from the shape's texture set (slot 2), as lit windows and signs have. For a shape drawn
    /// without lighting, the texture its `BSShaderNoLightingProperty` names: what such a shape shows is light it
    /// gives off.
    pub glow_map: Option<String>,
    /// The shape has an alpha property.
    pub alpha: bool,
    /// The shape is drawn without lighting (`BSShaderNoLightingProperty`), as light beams, glows and other effects
    /// are.
    pub unlit: bool,
}

type Mat = [[f32; 3]; 3];

fn mat_of(r: &[f32; 9]) -> Mat {
    [[r[0], r[1], r[2]], [r[3], r[4], r[5]], [r[6], r[7], r[8]]]
}

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut o = [[0.0; 3]; 3];
    for (i, row) in o.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

fn apply(m: &Mat, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// A rigid transform with uniform scale.
#[derive(Clone, Copy)]
struct Xf {
    rot: Mat,
    pos: [f32; 3],
    scale: f32,
}

impl Xf {
    const IDENTITY: Self = Self {
        rot: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        pos: [0.0; 3],
        scale: 1.0,
    };

    fn then(&self, child: &Self) -> Self {
        // child applied first, then self
        let p = apply(&self.rot, child.pos.map(|v| v * self.scale));
        Self {
            rot: mul(&self.rot, &child.rot),
            pos: [self.pos[0] + p[0], self.pos[1] + p[1], self.pos[2] + p[2]],
            scale: self.scale * child.scale,
        }
    }
}

// source: game-object-shape-offset
/// A node's transform without its rotation: the game's own blocks ignore the rotation of the model's nodes (a node
/// rotation in a model changes no vertex of the blocks measured) and keep only the shapes' own transforms.
fn node_xf(node: &NiNode) -> Xf {
    Xf {
        rot: Xf::IDENTITY.rot,
        pos: node.av.translation,
        scale: node.av.scale,
    }
}

fn shape_xf(shape: &NiTriShape) -> Xf {
    Xf {
        rot: mat_of(&shape.av.rotation),
        pos: shape.av.translation,
        scale: shape.av.scale,
    }
}

/// The texture paths of a `BSShaderTextureSet` block: the diffuse, the normal and the glow slot.
pub(crate) fn texture_set(bytes: &[u8]) -> (Option<String>, Option<String>, Option<String>) {
    let mut at = 4;
    let count = bytes
        .get(..4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let mut paths = Vec::new();
    for _ in 0..count.min(8) {
        let Some(len) = bytes.get(at..at + 4) else {
            break;
        };
        let len = u32::from_le_bytes([len[0], len[1], len[2], len[3]]) as usize;
        at += 4;
        let Some(text) = bytes.get(at..at + len) else {
            break;
        };
        at += len;
        paths.push(Some(String::from_utf8_lossy(text).into_owned()).filter(|s| !s.is_empty()));
    }
    let mut it = paths.into_iter();
    (
        it.next().flatten(),
        it.next().flatten(),
        it.next().flatten(),
    )
}

/// Where a `BSLightingShaderProperty` of a Skyrim-stream model (user version 12, stream 83: the shader's type, the
/// name, the extra data links, the controller, two flag words and the UV offset and scale come first) links its texture
/// set. Some Fallout models that mods ship (the `DynDOLOD` rock models of Tale of Two Wastelands) are written this way.
fn skyrim_lighting_texture_set(bytes: &[u8]) -> Option<usize> {
    let word = |at: usize| -> Option<u32> {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let extra = usize::try_from(word(8)?).ok()?;
    if extra > 64 {
        return None;
    }
    // type, name, extra count, extra links, controller, flags 1 and 2, UV offset and scale
    let at = 12 + 4 * extra + 4 + 8 + 16;
    usize::try_from(word(at)?).ok()
}

/// The texture a `BSShaderNoLightingProperty` names: after the object name, its extra data links and controller, the
/// property flags, the shader type, both shader flag words, the environment map scale and the clamp mode.
pub(crate) fn no_lighting_texture(bytes: &[u8]) -> Option<String> {
    let word = |at: usize| -> Option<usize> {
        bytes
            .get(at..at + 4)
            .and_then(|b| usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok())
    };
    let extra = word(4)?;
    if extra > 64 {
        return None;
    }
    let at = 8 + 4 * extra + 4 + 2 + 4 + 4 + 4 + 4 + 4;
    let len = word(at)?;
    let text = bytes.get(at + 4..at + 4 + len)?;
    Some(String::from_utf8_lossy(text).into_owned()).filter(|s| !s.is_empty())
}

/// Where a `BSShaderPPLightingProperty` links its texture set (see `game-lod-terrain-block-nif`).
pub(crate) fn lighting_texture_set(bytes: &[u8]) -> Option<usize> {
    let b = bytes.get(34..38)?;
    usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok()
}

/// The diffuse, normal and glow paths of a shape.
type Textures = (Option<String>, Option<String>, Option<String>);

struct Ctx<'a> {
    file: &'a NifFile,
    parts: Vec<Part>,
}

impl Ctx<'_> {
    fn name(&self, i: usize) -> &[u8] {
        self.file
            .blocks
            .get(i)
            .and_then(|b| self.file.header.block_types.get(usize::from(b.type_index)))
            .map_or(&[][..], Vec::as_slice)
    }

    fn node(&self, i: usize) -> Option<NiNode> {
        match &self.file.blocks.get(i)?.body {
            Body::Node(n) => Some(n.clone()),
            Body::Opaque(bytes) if self.name(i).ends_with(b"Node") => {
                match Body::parse(b"NiNode", bytes, self.file.header.profile()) {
                    Ok((Body::Node(n), _)) => Some(n),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn textures(&self, shape: &NiTriShape) -> (Textures, bool, bool) {
        let mut alpha = shape.alpha_property >= 0;
        let (mut diffuse, mut normal, mut glow) = (None, None, None);
        let unlit = shape
            .av
            .properties
            .iter()
            .chain(std::iter::once(&shape.shader_property))
            .filter_map(|p| usize::try_from(*p).ok())
            .any(|p| self.name(p) == b"BSShaderNoLightingProperty");
        for p in &shape.av.properties {
            let Ok(p) = usize::try_from(*p) else { continue };
            if self.name(p) == b"NiAlphaProperty" {
                alpha = true;
            }
            if let Some(Body::Opaque(bytes)) = self.file.blocks.get(p).map(|b| &b.body)
                && self.name(p) == b"BSShaderPPLightingProperty"
                && let Some(set) = lighting_texture_set(bytes)
                && let Some(Body::Opaque(set_bytes)) = self.file.blocks.get(set).map(|b| &b.body)
            {
                (diffuse, normal, glow) = texture_set(set_bytes);
            }
        }
        if diffuse.is_none()
            && let Ok(p) = usize::try_from(shape.shader_property)
            && self.name(p) == b"BSLightingShaderProperty"
            && let Some(Body::Opaque(bytes)) = self.file.blocks.get(p).map(|b| &b.body)
            && let Some(set) = skyrim_lighting_texture_set(bytes)
            && self.name(set) == b"BSShaderTextureSet"
            && let Some(Body::Opaque(set_bytes)) = self.file.blocks.get(set).map(|b| &b.body)
        {
            (diffuse, normal, glow) = texture_set(set_bytes);
        }
        // a shape drawn without lighting shows the light it gives off: its texture is its glow
        if unlit && glow.is_none() {
            glow = shape
                .av
                .properties
                .iter()
                .chain(std::iter::once(&shape.shader_property))
                .filter_map(|p| usize::try_from(*p).ok())
                .filter(|p| self.name(*p) == b"BSShaderNoLightingProperty")
                .find_map(|p| match self.file.blocks.get(p).map(|b| &b.body) {
                    Some(Body::Opaque(bytes)) => no_lighting_texture(bytes),
                    _ => None,
                });
        }
        ((diffuse, normal, glow), alpha, unlit)
    }

    fn shape(&mut self, shape: &NiTriShape, xf: &Xf) {
        let Ok(data) = usize::try_from(shape.data) else {
            return;
        };
        let (geometry, triangles) = match self.file.blocks.get(data).map(|b| &b.body) {
            Some(Body::TriStripsData(d)) => (d.geometry.clone(), d.to_triangles()),
            Some(Body::TriShapeData(d)) => {
                (d.geometry.clone(), d.triangles.clone().unwrap_or_default())
            }
            _ => return,
        };
        let Some(vertices) = geometry.vertices else {
            return;
        };
        let n = vertices.len();
        let turn = |v: &[f32; 3]| apply(&xf.rot, *v);
        let ((diffuse, normal_map, glow_map), alpha, unlit) = self.textures(shape);
        self.parts.push(Part {
            // source: game-object-shape-offset
            vertices: vertices
                .iter()
                .map(|v| apply(&xf.rot, v.map(|c| c * xf.scale)))
                .collect(),
            offset: xf.pos,
            normals: geometry.normals.map(|ns| ns.iter().map(turn).collect()),
            tangents: geometry
                .tangents
                .map(|(t, b)| (t.iter().map(turn).collect(), b.iter().map(turn).collect())),
            uv: geometry
                .uv_sets
                .into_iter()
                .next()
                .unwrap_or_else(|| vec![[0.0; 2]; n]),
            triangles,
            diffuse,
            normal_map,
            glow_map,
            alpha,
            unlit,
        });
    }

    fn visit(&mut self, i: usize, parent: &Xf, depth: usize) {
        if depth > 16 {
            return;
        }
        if let Some(node) = self.node(i) {
            let xf = parent.then(&node_xf(&node));
            for child in &node.children {
                if let Ok(c) = usize::try_from(*child) {
                    self.visit(c, &xf, depth + 1);
                }
            }
            return;
        }
        let shape = match self.file.blocks.get(i).map(|b| &b.body) {
            Some(Body::TriStrips(s) | Body::TriShape(s)) => s.clone(),
            _ => return,
        };
        let xf = parent.then(&shape_xf(&shape));
        self.shape(&shape, &xf);
    }
}

/// Parse a LOD model into shapes in the model's own space (see [`Part`] for what is applied).
///
/// # Errors
/// A message when the bytes are not a NIF this reader understands or have no geometry.
pub fn parse_lod_model(bytes: &[u8]) -> Result<Vec<Part>, String> {
    // bytes after the footer are ignored, as the game does: some mods ship LOD models with a stale tail
    let file = NifFile::read_ignoring_trailing_bytes(bytes).map_err(|e| e.to_string())?;
    let mut ctx = Ctx {
        file: &file,
        parts: Vec::new(),
    };
    for root in &file.roots {
        if let Ok(r) = usize::try_from(*root) {
            ctx.visit(r, &Xf::IDENTITY, 0);
        }
    }
    if ctx.parts.is_empty() {
        return Err("the model has no triangle geometry".to_owned());
    }
    Ok(ctx.parts)
}

#[cfg(test)]
mod tests {
    use super::skyrim_lighting_texture_set;

    /// The block layout measured on the `DynDOLOD` rock models of Tale of Two Wastelands (numbers only): shader type,
    /// name, extra data count and links, controller, two flag words, UV offset and scale, then the texture set link.
    fn block(extra: &[u32], set: u32) -> Vec<u8> {
        let mut b = Vec::new();
        for w in [1_u32, u32::MAX, u32::try_from(extra.len()).unwrap()] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        for e in extra {
            b.extend_from_slice(&e.to_le_bytes());
        }
        for w in [u32::MAX, 0x8000_0000, 0x0000_0001] {
            b.extend_from_slice(&w.to_le_bytes());
        }
        for f in [0.0_f32, 0.0, 1.0, 1.0] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&set.to_le_bytes());
        b
    }

    #[test]
    fn a_texture_set_gives_its_diffuse_normal_and_glow_slots() {
        let mut set = 6_u32.to_le_bytes().to_vec();
        for p in ["a.dds", "", "a_g.dds", "h.dds", "", ""] {
            set.extend_from_slice(&u32::try_from(p.len()).unwrap().to_le_bytes());
            set.extend_from_slice(p.as_bytes());
        }
        assert_eq!(
            super::texture_set(&set),
            (Some("a.dds".to_owned()), None, Some("a_g.dds".to_owned()))
        );
    }

    /// A `BSShaderNoLightingProperty` body with `extra` extra data links and the texture `file`.
    fn no_lighting(extra: u32, file: &str) -> Vec<u8> {
        let mut b = vec![0_u8; 4];
        b.extend_from_slice(&extra.to_le_bytes());
        b.extend(std::iter::repeat_n(
            0_u8,
            4 * extra as usize + 4 + 2 + 4 * 5,
        ));
        b.extend_from_slice(&u32::try_from(file.len()).unwrap().to_le_bytes());
        b.extend_from_slice(file.as_bytes());
        b.extend_from_slice(&[0_u8; 16]);
        b
    }

    #[test]
    fn a_no_lighting_property_names_its_texture() {
        assert_eq!(
            super::no_lighting_texture(&no_lighting(0, "fx\\strip.dds")),
            Some("fx\\strip.dds".to_owned())
        );
        assert_eq!(
            super::no_lighting_texture(&no_lighting(2, "fx\\b.dds")),
            Some("fx\\b.dds".to_owned())
        );
        assert_eq!(super::no_lighting_texture(&no_lighting(0, "")), None);
        assert_eq!(
            super::no_lighting_texture(&no_lighting(0, "x.dds")[..20]),
            None
        );
    }

    #[test]
    fn a_skyrim_stream_lighting_property_names_its_texture_set() {
        assert_eq!(skyrim_lighting_texture_set(&block(&[], 2)), Some(2));
        assert_eq!(skyrim_lighting_texture_set(&block(&[7, 8], 3)), Some(3));
    }

    #[test]
    fn a_short_or_absurd_block_names_none() {
        assert_eq!(skyrim_lighting_texture_set(&block(&[], 2)[..20]), None);
        let mut absurd = block(&[], 2);
        absurd[8..12].copy_from_slice(&1000_u32.to_le_bytes());
        assert_eq!(skyrim_lighting_texture_set(&absurd), None);
    }
}
