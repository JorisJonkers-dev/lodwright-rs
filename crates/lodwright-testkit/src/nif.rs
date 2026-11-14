// SPDX-License-Identifier: MPL-2.0
//! Synthetic shape sets for NIF tests: plain data (no NIF types), built from a seed.
//!
//! The NIF crate turns these into files; the testkit stays free of format code so that the crate under test
//! does not depend on its own fixtures. Values are multiples of 1/16 so every float is exact.

use crate::rng::Rng;

/// One synthetic triangle mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSpec {
    /// Shape name.
    pub name: String,
    /// Translation of the shape node.
    pub translation: [f32; 3],
    /// Vertex positions.
    pub vertices: Vec<[f32; 3]>,
    /// Per-vertex normals, when present.
    pub normals: Option<Vec<[f32; 3]>>,
    /// Per-vertex texture coordinates, when present.
    pub uvs: Option<Vec<[f32; 2]>>,
    /// Triangles as vertex indices.
    pub triangles: Vec<[u16; 3]>,
    /// Alpha property flags and threshold, when the shape has one.
    pub alpha: Option<(u16, u8)>,
}

fn unit(rng: &mut Rng) -> f32 {
    let v = i16::try_from(rng.below(512)).unwrap_or(0) - 256;
    f32::from(v) / 16.0
}

fn vec3(rng: &mut Rng) -> [f32; 3] {
    [unit(rng), unit(rng), unit(rng)]
}

fn small(rng: &mut Rng, bound: u32) -> u16 {
    u16::try_from(rng.below(bound)).unwrap_or(0)
}

/// A set of between zero and `max_shapes` shapes, each with up to 24 vertices.
#[must_use]
pub fn shape_set(rng: &mut Rng, max_shapes: u32) -> Vec<ShapeSpec> {
    let count = rng.below(max_shapes.saturating_add(1));
    (0..count)
        .map(|i| {
            let n = small(rng, 24) + 3;
            let vertices = (0..n).map(|_| vec3(rng)).collect();
            let normals = (rng.below(2) == 1).then(|| (0..n).map(|_| vec3(rng)).collect());
            let uvs = (rng.below(2) == 1).then(|| (0..n).map(|_| [unit(rng), unit(rng)]).collect());
            let triangles = (0..small(rng, 16))
                .map(|_| {
                    [
                        small(rng, u32::from(n)),
                        small(rng, u32::from(n)),
                        small(rng, u32::from(n)),
                    ]
                })
                .collect();
            let alpha = (rng.below(2) == 1).then(|| {
                (
                    small(rng, u32::from(u16::MAX)),
                    u8::try_from(small(rng, 256)).unwrap_or(0),
                )
            });
            ShapeSpec {
                name: format!("shape{i}"),
                translation: vec3(rng),
                vertices,
                normals,
                uvs,
                triangles,
                alpha,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::shape_set;
    use crate::rng::Rng;

    #[test]
    fn the_same_seed_gives_the_same_shape_set() {
        assert_eq!(
            shape_set(&mut Rng::new(7), 5),
            shape_set(&mut Rng::new(7), 5)
        );
    }

    #[test]
    fn triangle_indices_stay_inside_the_vertex_list() {
        let mut rng = Rng::new(3);
        for _ in 0..50 {
            for shape in shape_set(&mut rng, 4) {
                let n = u16::try_from(shape.vertices.len()).unwrap();
                assert!(shape.triangles.iter().flatten().all(|i| *i < n));
            }
        }
    }
}
