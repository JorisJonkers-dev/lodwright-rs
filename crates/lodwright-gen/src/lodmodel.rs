// SPDX-License-Identifier: MPL-2.0
//! Reading the small LOD models that sit beside the full models: geometry with the node transforms applied, plus
//! the texture paths and whether the shape is alpha tested.

use lodwright_nif::{Body, NiNode, NiTriShape, NifFile};

/// One shape of a LOD model, in the model's own space.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Positions.
    pub vertices: Vec<[f32; 3]>,
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
    /// The shape has an alpha property.
    pub alpha: bool,
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

    fn point(&self, v: [f32; 3]) -> [f32; 3] {
        let p = apply(&self.rot, v.map(|c| c * self.scale));
        [self.pos[0] + p[0], self.pos[1] + p[1], self.pos[2] + p[2]]
    }
}

fn node_xf(node: &NiNode) -> Xf {
    Xf {
        rot: mat_of(&node.av.rotation),
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

/// The texture paths of a `BSShaderTextureSet` block: the diffuse and the normal slot.
fn texture_set(bytes: &[u8]) -> (Option<String>, Option<String>) {
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
    (it.next().flatten(), it.next().flatten())
}

/// Where a `BSShaderPPLightingProperty` links its texture set (see `game-lod-terrain-block-nif`).
fn lighting_texture_set(bytes: &[u8]) -> Option<usize> {
    let b = bytes.get(34..38)?;
    usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok()
}

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

    fn textures(&self, shape: &NiTriShape) -> (Option<String>, Option<String>, bool) {
        let (mut diffuse, mut normal, mut alpha) = (None, None, shape.alpha_property >= 0);
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
                (diffuse, normal) = texture_set(set_bytes);
            }
        }
        (diffuse, normal, alpha)
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
        let (diffuse, normal_map, alpha) = self.textures(shape);
        self.parts.push(Part {
            vertices: vertices.iter().map(|v| xf.point(*v)).collect(),
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
            alpha,
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

/// Parse a LOD model into shapes with every transform applied.
///
/// # Errors
/// A message when the bytes are not a NIF this reader understands or have no geometry.
pub fn parse_lod_model(bytes: &[u8]) -> Result<Vec<Part>, String> {
    let file = NifFile::read(bytes).map_err(|e| e.to_string())?;
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
