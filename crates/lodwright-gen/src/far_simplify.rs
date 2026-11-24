// SPDX-License-Identifier: GPL-3.0-only
//! Simplifies a full model into a far shape, for models that have no LOD model of their own.
//!
//! Used only when a request sets `objects.shapes_from_full_models`, which is off by default because the game's own LOD
//! is the standard. Vertices that share a position but differ in
//! texture coordinates or normal stay a seam. The result depends only on the input, on every system: the crate's SIMD
//! code is not built, and a test pins the output.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use meshoptimizer::simplify_with_attributes;

use crate::lodmodel::Part;

/// Weights of the attributes in the error: texture coordinates count fully, normals half.
const ATTRIBUTE_WEIGHTS: [f32; 5] = [1.0, 1.0, 0.5, 0.5, 0.5];

/// A triangle mesh to simplify. `positions`, `uvs` and `normals` have one entry per vertex.
#[derive(Debug, Clone, Copy)]
pub struct FarMesh<'a> {
    /// Vertex positions.
    pub positions: &'a [[f32; 3]],
    /// Texture coordinates.
    pub uvs: &'a [[f32; 2]],
    /// Vertex normals.
    pub normals: &'a [[f32; 3]],
    /// Triangles, as indices into the vertices.
    pub triangles: &'a [[u32; 3]],
    /// Vertices that must stay where they are, such as those on a block border: empty, or one entry per vertex.
    pub locked: &'a [bool],
}

/// How far to simplify: stops at whichever limit it reaches first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FarLimits {
    /// Share of the triangles to keep, above 0 and at most 1.
    pub keep: f32,
    /// Largest error allowed, as a share of the mesh's size.
    pub max_error: f32,
}

/// The simplified triangles, which index the vertices of the input mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct SimplifiedFarMesh {
    /// The triangles left.
    pub triangles: Vec<[u32; 3]>,
    /// The error reached, as a share of the mesh's size.
    pub error: f32,
}

/// Why a mesh could not be simplified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FarSimplifyError {
    /// The per-vertex lists differ in length.
    VertexListsDiffer,
    /// A triangle names a vertex that does not exist.
    IndexOutOfRange(u32),
    /// A position, texture coordinate or normal is not a finite number.
    NotFinite,
    /// A limit is outside its range.
    BadLimits,
    /// The simplifier refused the input.
    Refused(String),
}

impl fmt::Display for FarSimplifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VertexListsDiffer => f.write_str("the vertex lists of the mesh differ in length"),
            Self::IndexOutOfRange(index) => {
                write!(f, "a triangle names vertex {index}, which does not exist")
            }
            Self::NotFinite => f.write_str("the mesh has a value that is not a finite number"),
            Self::BadLimits => f.write_str("the simplification limits are out of range"),
            Self::Refused(reason) => write!(f, "the simplifier refused the mesh: {reason}"),
        }
    }
}

impl std::error::Error for FarSimplifyError {}

/// Simplifies `mesh` until `limits` stops it, keeping seams and locked vertices.
///
/// # Errors
/// When the mesh or the limits are invalid; see [`FarSimplifyError`].
pub fn simplify_far_mesh(
    mesh: FarMesh<'_>,
    limits: FarLimits,
) -> Result<SimplifiedFarMesh, FarSimplifyError> {
    validate(mesh, limits)?;
    if mesh.triangles.is_empty() {
        return Ok(SimplifiedFarMesh {
            triangles: Vec::new(),
            error: 0.0,
        });
    }

    let positions: Vec<f32> = mesh.positions.iter().flatten().copied().collect();
    let attributes: Vec<f32> = mesh
        .uvs
        .iter()
        .zip(mesh.normals)
        .flat_map(|(uv, normal)| [uv[0], uv[1], normal[0], normal[1], normal[2]])
        .collect();
    let indices: Vec<u32> = mesh.triangles.iter().flatten().copied().collect();
    let lock: Vec<u8> = mesh.locked.iter().map(|&locked| u8::from(locked)).collect();
    // in f32, the type of the share: widening it first would push an exact count such as 50 * 0.1 over the next whole number
    let target_triangles = (mesh.triangles.len() as f32 * limits.keep).ceil() as usize;

    let mut destination = vec![0_u32; indices.len()];
    let mut error = 0.0_f32;
    let count = simplify_with_attributes(
        &mut destination,
        &indices,
        indices.len(),
        &positions,
        mesh.positions.len(),
        size_of::<[f32; 3]>(),
        &attributes,
        size_of::<[f32; 5]>(),
        &ATTRIBUTE_WEIGHTS,
        ATTRIBUTE_WEIGHTS.len(),
        (!lock.is_empty()).then_some(lock.as_slice()),
        target_triangles * 3,
        limits.max_error,
        0,
        Some(&mut error),
    )
    .map_err(|e| FarSimplifyError::Refused(e.to_string()))?;

    let triangles = destination[..count].as_chunks::<3>().0.to_vec();
    Ok(SimplifiedFarMesh { triangles, error })
}

fn validate(mesh: FarMesh<'_>, limits: FarLimits) -> Result<(), FarSimplifyError> {
    let vertices = mesh.positions.len();
    if mesh.uvs.len() != vertices
        || mesh.normals.len() != vertices
        || !(mesh.locked.is_empty() || mesh.locked.len() == vertices)
    {
        return Err(FarSimplifyError::VertexListsDiffer);
    }
    let finite = mesh
        .positions
        .iter()
        .flatten()
        .chain(mesh.uvs.iter().flatten())
        .chain(mesh.normals.iter().flatten());
    if !finite.into_iter().all(|value| value.is_finite()) {
        return Err(FarSimplifyError::NotFinite);
    }
    if let Some(&index) = mesh
        .triangles
        .iter()
        .flatten()
        .find(|&&index| index as usize >= vertices)
    {
        return Err(FarSimplifyError::IndexOutOfRange(index));
    }
    if !(limits.keep > 0.0 && limits.keep <= 1.0 && limits.max_error.is_finite())
        || limits.max_error < 0.0
    {
        return Err(FarSimplifyError::BadLimits);
    }
    Ok(())
}

/// A position as exact bits, with -0 and +0 the same, to find the corners that two shapes share.
fn position_key(vertex: [f32; 3], offset: [f32; 3]) -> [u32; 3] {
    [0, 1, 2].map(|axis| (vertex[axis] + offset[axis] + 0.0).to_bits())
}

/// The far shapes of a full model, simplified. A corner that two shapes share stays where it is, so no crack opens
/// between them. A shape drawn without lighting (a light beam, a glow card) is an effect and is left out: drawn in a
/// far block, which is lit, it would show as a pale solid. `None` when no other shape is left, or when every shape
/// left has an alpha property: such a model may be an effect or glass, and the property alone does not tell, so it
/// gets no far shape. In a model with a shape without one, all those shapes are kept, because many solid models carry
/// the property on most of their shapes.
pub(crate) fn far_parts_from_full_model(parts: Vec<Part>, limits: FarLimits) -> Option<Vec<Part>> {
    let shapes: Vec<Part> = parts
        .into_iter()
        .filter(|part| !part.triangles.is_empty() && !part.unlit)
        .collect();
    if shapes.iter().all(|part| part.alpha) {
        return None;
    }
    let mut owners: HashMap<[u32; 3], usize> = HashMap::new();
    let mut shared: HashSet<[u32; 3]> = HashSet::new();
    for (index, part) in shapes.iter().enumerate() {
        for vertex in &part.vertices {
            let key = position_key(*vertex, part.offset);
            if *owners.entry(key).or_insert(index) != index {
                shared.insert(key);
            }
        }
    }
    let mut far = Vec::with_capacity(shapes.len());
    for part in shapes {
        let locked: Vec<bool> = part
            .vertices
            .iter()
            .map(|vertex| shared.contains(&position_key(*vertex, part.offset)))
            .collect();
        if let Some(simplified) = simplify_part(part, &locked, limits) {
            far.push(simplified);
        }
    }
    (!far.is_empty()).then_some(far)
}

/// One shape simplified, holding only the vertices its triangles still use. `None` when the shape cannot be simplified
/// or nothing of it is left.
fn simplify_part(part: Part, locked: &[bool], limits: FarLimits) -> Option<Part> {
    if part
        .tangents
        .as_ref()
        .is_some_and(|(t, b)| t.len() != part.vertices.len() || b.len() != part.vertices.len())
    {
        return None;
    }
    // a constant normal adds no error, for a shape that has none
    let flat = vec![[0.0, 0.0, 1.0]; part.vertices.len()];
    let triangles: Vec<[u32; 3]> = part.triangles.iter().map(|t| t.map(u32::from)).collect();
    let simplified = simplify_far_mesh(
        FarMesh {
            positions: &part.vertices,
            uvs: &part.uv,
            normals: part.normals.as_deref().unwrap_or(&flat),
            triangles: &triangles,
            locked,
        },
        limits,
    )
    .ok()?;
    if simplified.triangles.is_empty() {
        return None;
    }
    let mut kept: BTreeMap<u32, u16> = BTreeMap::new();
    for index in simplified.triangles.iter().flatten() {
        if !kept.contains_key(index) {
            kept.insert(*index, u16::try_from(kept.len()).ok()?);
        }
    }
    let mut order: Vec<(u16, usize)> = kept
        .iter()
        .map(|(&old, &new)| (new, old as usize))
        .collect();
    order.sort_unstable();
    let pick =
        |list: &[[f32; 3]]| -> Vec<[f32; 3]> { order.iter().map(|&(_, old)| list[old]).collect() };
    Some(Part {
        vertices: pick(&part.vertices),
        offset: part.offset,
        normals: part.normals.as_deref().map(pick),
        tangents: part.tangents.as_ref().map(|(t, b)| (pick(t), pick(b))),
        uv: order.iter().map(|&(_, old)| part.uv[old]).collect(),
        triangles: simplified
            .triangles
            .iter()
            .map(|t| t.map(|index| kept[&index]))
            .collect(),
        diffuse: part.diffuse,
        normal_map: part.normal_map,
        glow_map: part.glow_map,
        alpha: part.alpha,
        unlit: part.unlit,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{FarLimits, far_parts_from_full_model};
    use crate::lodmodel::Part;

    /// A flat grid of `size` squares from `x` to `x + size`, on the y axis from 0 to `size`.
    fn grid(x: u16, size: u16, alpha: bool) -> Part {
        let row = size + 1;
        let vertices: Vec<[f32; 3]> = (0..row)
            .flat_map(|j| (0..row).map(move |i| [f32::from(x + i), f32::from(j), 0.0]))
            .collect();
        let triangles = (0..size)
            .flat_map(|j| {
                (0..size).flat_map(move |i| {
                    let a = j * row + i;
                    [[a, a + 1, a + row + 1], [a, a + row + 1, a + row]]
                })
            })
            .collect();
        Part {
            uv: vec![[0.0; 2]; vertices.len()],
            normals: Some(vec![[0.0, 0.0, 1.0]; vertices.len()]),
            vertices,
            offset: [0.0; 3],
            tangents: None,
            triangles,
            diffuse: Some("a.dds".to_owned()),
            normal_map: None,
            glow_map: None,
            alpha,
            unlit: false,
        }
    }

    const LIMITS: FarLimits = FarLimits {
        keep: 0.1,
        max_error: 0.05,
    };

    fn rows_at(part: &Part, x: f32) -> BTreeSet<u32> {
        part.triangles
            .iter()
            .flatten()
            .map(|&i| part.vertices[usize::from(i)])
            .filter(|v| v[0].to_bits() == x.to_bits())
            .map(|v| v[1] as u32)
            .collect()
    }

    #[test]
    fn a_corner_two_shapes_share_stays_in_both() {
        let far = far_parts_from_full_model(vec![grid(0, 16, false), grid(16, 16, false)], LIMITS)
            .unwrap();
        assert_eq!(far.len(), 2);
        let every_row: BTreeSet<u32> = (0..=16).collect();
        assert_eq!(rows_at(&far[0], 16.0), every_row);
        assert_eq!(rows_at(&far[1], 16.0), every_row);
        assert!(
            far[0].triangles.len() < 512,
            "{} triangles",
            far[0].triangles.len()
        );
    }

    #[test]
    fn a_shape_drawn_without_lighting_is_left_out() {
        let mut bulb = grid(20, 8, false);
        bulb.unlit = true;
        let far = far_parts_from_full_model(vec![grid(0, 8, false), bulb.clone()], LIMITS).unwrap();
        assert_eq!(far.len(), 1);
        assert!(!far[0].unlit);
        // a beam: nothing but shapes drawn without lighting, one of them without an alpha property
        let mut beam = grid(0, 8, true);
        beam.unlit = true;
        assert_eq!(far_parts_from_full_model(vec![beam, bulb], LIMITS), None);
    }

    #[test]
    fn a_model_keeps_its_alpha_shapes_unless_it_has_no_other() {
        let far =
            far_parts_from_full_model(vec![grid(0, 8, false), grid(20, 8, true)], LIMITS).unwrap();
        assert_eq!(
            far.iter().map(|p| p.alpha).collect::<Vec<_>>(),
            [false, true]
        );
        assert_eq!(
            far_parts_from_full_model(vec![grid(0, 8, true)], LIMITS),
            None
        );
    }

    #[test]
    fn the_kept_vertices_are_only_those_the_triangles_use() {
        let far = far_parts_from_full_model(vec![grid(0, 16, false)], LIMITS).unwrap();
        let used: BTreeSet<u16> = far[0].triangles.iter().flatten().copied().collect();
        assert_eq!(used.len(), far[0].vertices.len());
        assert_eq!(far[0].uv.len(), far[0].vertices.len());
        assert_eq!(
            far[0].normals.as_ref().unwrap().len(),
            far[0].vertices.len()
        );
    }
}
