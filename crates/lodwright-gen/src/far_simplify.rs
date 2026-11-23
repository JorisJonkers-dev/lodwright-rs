// SPDX-License-Identifier: GPL-3.0-only
//! Simplifies a full model into a far shape, for models that have no LOD model of their own.
//!
//! Nothing in a generation run calls this yet: the switch that turns it on comes with the step that reads full models,
//! and stays off by default because the game's own LOD is the standard. Vertices that share a position but differ in
//! texture coordinates or normal stay a seam. The result depends only on the input, on every system: the crate's SIMD
//! code is not built, and a test pins the output.

use std::fmt;

use meshoptimizer::simplify_with_attributes;

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
    let target_triangles = (mesh.triangles.len() as f64 * f64::from(limits.keep)).ceil() as usize;

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
