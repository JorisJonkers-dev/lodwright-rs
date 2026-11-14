// SPDX-License-Identifier: MPL-2.0
//! LAND layers: vertex normals (VNML), vertex colours (VCLR), base textures (BTXT) and additional texture layers
//! (ATXT with its VTXT opacity points).
//!
//! A cell has four quadrants of 17 x 17 points; each quadrant has one base texture and any number of alpha layers.
//! Quadrants share their middle row and column of the 33 x 33 vertex grid.
//!
//! Derived from: the UESP LAND page and xEdit's definitions (`provenance/sources.toml`: `land-vnml-layout`,
//! `land-vclr-layout`, `land-layer-header`, `land-vtxt-layout`, `land-quadrant-grid`).

use crate::Error;
use crate::land::SIDE;

/// Bytes of a VNML or VCLR payload.
pub const GRID_LEN: usize = SIDE * SIDE * 3;
/// Points along one quadrant edge.
pub const QUADRANT_SIDE: usize = 17;
/// Points in a quadrant.
pub const QUADRANT_POINTS: usize = QUADRANT_SIDE * QUADRANT_SIDE;
/// Bytes of a BTXT or ATXT payload.
pub const HEADER_LEN: usize = 8;
/// Bytes of one VTXT point.
pub const POINT_LEN: usize = 8;

/// A 33 x 33 grid of three bytes per vertex (normals or colours), row-major from the south-west vertex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexGrid {
    data: Vec<[u8; 3]>,
}

impl VertexGrid {
    /// The three bytes at vertex (`x`, `y`).
    ///
    /// # Panics
    /// If `x` or `y` is [`SIDE`] or more.
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> [u8; 3] {
        assert!(x < SIDE && y < SIDE, "vertex outside the cell");
        self.data[y * SIDE + x]
    }

    /// All vertices, row-major.
    #[must_use]
    pub fn as_slice(&self) -> &[[u8; 3]] {
        &self.data
    }
}

fn decode_grid(payload: &[u8], what: &'static str) -> Result<VertexGrid, Error> {
    if payload.len() != GRID_LEN {
        return Err(Error::SubrecordLength {
            what,
            found: payload.len(),
        });
    }
    Ok(VertexGrid {
        data: payload.as_chunks::<3>().0.to_vec(),
    })
}

/// Decode a VNML payload (the bytes of each normal component are kept raw).
///
/// # Errors
/// [`Error::SubrecordLength`] unless the payload is exactly [`GRID_LEN`] bytes.
pub fn decode_vnml(payload: &[u8]) -> Result<VertexGrid, Error> {
    // source: land-vnml-layout
    decode_grid(payload, "VNML")
}

/// Decode a VCLR payload: red, green, blue per vertex.
///
/// # Errors
/// [`Error::SubrecordLength`] unless the payload is exactly [`GRID_LEN`] bytes.
pub fn decode_vclr(payload: &[u8]) -> Result<VertexGrid, Error> {
    // source: land-vclr-layout
    decode_grid(payload, "VCLR")
}

/// The texture, quadrant and layer number of a BTXT or ATXT subrecord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerHeader {
    /// LTEX `FormID` as stored; 0 is no texture.
    pub texture: u32,
    /// Quadrant 0 to 3: south-west, south-east, north-west, north-east.
    pub quadrant: u8,
    /// Layer number.
    pub layer: i16,
}

/// Decode a BTXT or ATXT payload; `what` names the subrecord in errors.
///
/// # Errors
/// [`Error::SubrecordLength`] unless the payload is [`HEADER_LEN`] bytes, [`Error::LayerQuadrant`] above 3.
pub fn decode_layer_header(payload: &[u8], what: &'static str) -> Result<LayerHeader, Error> {
    // source: land-layer-header
    let [a, b, c, d, quadrant, _, l0, l1] =
        <[u8; HEADER_LEN]>::try_from(payload).map_err(|_| Error::SubrecordLength {
            what,
            found: payload.len(),
        })?;
    if quadrant > 3 {
        return Err(Error::LayerQuadrant(quadrant));
    }
    Ok(LayerHeader {
        texture: u32::from_le_bytes([a, b, c, d]),
        quadrant,
        layer: i16::from_le_bytes([l0, l1]),
    })
}

/// Decode a VTXT payload into the opacity of every point of a quadrant (zero where no point is listed).
///
/// # Errors
/// [`Error::SubrecordLength`] unless the length is a multiple of [`POINT_LEN`], [`Error::LayerPosition`] for a
/// position above 288 and [`Error::LayerOpacity`] for a NaN or infinite opacity.
pub fn decode_vtxt(payload: &[u8]) -> Result<Vec<f32>, Error> {
    // source: land-vtxt-layout
    let (points, rest) = payload.as_chunks::<POINT_LEN>();
    if !rest.is_empty() {
        return Err(Error::SubrecordLength {
            what: "VTXT",
            found: payload.len(),
        });
    }
    let mut grid = vec![0.0_f32; QUADRANT_POINTS];
    for [p0, p1, _, _, o0, o1, o2, o3] in points {
        let position = u16::from_le_bytes([*p0, *p1]);
        let opacity = f32::from_le_bytes([*o0, *o1, *o2, *o3]);
        if usize::from(position) >= QUADRANT_POINTS {
            return Err(Error::LayerPosition(position));
        }
        if !opacity.is_finite() {
            return Err(Error::LayerOpacity);
        }
        grid[usize::from(position)] = opacity;
    }
    Ok(grid)
}

/// One additional texture layer of one quadrant with its opacity points.
#[derive(Debug, Clone, PartialEq)]
pub struct AlphaLayer {
    /// LTEX `FormID` (as stored in a plugin, absolute after [`crate::worldspace`] resolves it).
    pub texture: u32,
    /// Quadrant 0 to 3.
    pub quadrant: u8,
    /// Layer number.
    pub layer: i16,
    opacity: Vec<f32>,
}

impl AlphaLayer {
    /// A layer from its header and the 17 x 17 opacity grid (row-major from the quadrant's south-west point).
    ///
    /// # Panics
    /// If `opacity` is not [`QUADRANT_POINTS`] long.
    #[must_use]
    pub fn new(header: LayerHeader, opacity: Vec<f32>) -> Self {
        assert_eq!(
            opacity.len(),
            QUADRANT_POINTS,
            "a quadrant has 17 x 17 points"
        );
        Self {
            texture: header.texture,
            quadrant: header.quadrant,
            layer: header.layer,
            opacity,
        }
    }

    /// The quadrant's 17 x 17 opacity grid, row-major from its south-west point.
    #[must_use]
    pub fn opacity_grid(&self) -> &[f32] {
        &self.opacity
    }

    /// Opacity at point (`col`, `row`) of the quadrant, both below [`QUADRANT_SIDE`].
    ///
    /// # Panics
    /// If `col` or `row` is [`QUADRANT_SIDE`] or more.
    #[must_use]
    pub fn point(&self, col: usize, row: usize) -> f32 {
        assert!(
            col < QUADRANT_SIDE && row < QUADRANT_SIDE,
            "point outside the quadrant"
        );
        self.opacity[row * QUADRANT_SIDE + col]
    }

    /// Opacity at cell vertex (`x`, `y`), or `None` when the vertex is outside this layer's quadrant. Vertices on
    /// the middle row or column belong to two quadrants.
    #[must_use]
    pub fn vertex_opacity(&self, x: usize, y: usize) -> Option<f32> {
        // source: land-quadrant-grid
        let (origin_x, origin_y) = quadrant_origin(self.quadrant);
        let col = x.checked_sub(origin_x).filter(|c| *c < QUADRANT_SIDE)?;
        let row = y.checked_sub(origin_y).filter(|r| *r < QUADRANT_SIDE)?;
        Some(self.opacity[row * QUADRANT_SIDE + col])
    }
}

/// The cell vertex of a quadrant's south-west point.
#[must_use]
pub fn quadrant_origin(quadrant: u8) -> (usize, usize) {
    // source: land-quadrant-grid
    (
        usize::from(quadrant & 1) * (SIDE - 1) / 2,
        usize::from(quadrant >> 1) * (SIDE - 1) / 2,
    )
}

/// The decoded layers of one LAND record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CellLayers {
    /// Vertex normals, when the record has VNML.
    pub normals: Option<VertexGrid>,
    /// Vertex colours, when the record has VCLR.
    pub colors: Option<VertexGrid>,
    /// Base texture (LTEX `FormID`) per quadrant; `None` when the quadrant has no BTXT or a null texture.
    pub base: [Option<u32>; 4],
    /// Additional layers, ordered by layer number then quadrant.
    pub alpha: Vec<AlphaLayer>,
}

impl CellLayers {
    /// Apply `map` to every texture `FormID` (used to make them absolute across a load order).
    pub fn map_textures(&mut self, mut map: impl FnMut(u32) -> u32) {
        for texture in self.base.iter_mut().flatten() {
            *texture = map(*texture);
        }
        for layer in &mut self.alpha {
            layer.texture = map(layer.texture);
        }
    }
}

/// Collects the layer subrecords of one LAND record in file order.
#[derive(Debug, Default)]
pub struct LayerReader {
    layers: CellLayers,
    pending: Option<LayerHeader>,
}

impl LayerReader {
    /// Feed one subrecord. Signatures other than the layer ones are ignored (returns `Ok`).
    ///
    /// # Errors
    /// The decode errors of the subrecord.
    pub fn subrecord(&mut self, signature: [u8; 4], payload: &[u8]) -> Result<(), Error> {
        match &signature {
            b"VNML" => self.layers.normals = Some(decode_vnml(payload)?),
            b"VCLR" => self.layers.colors = Some(decode_vclr(payload)?),
            b"BTXT" => {
                self.flush();
                let header = decode_layer_header(payload, "BTXT")?;
                self.layers.base[usize::from(header.quadrant)] =
                    (header.texture != 0).then_some(header.texture);
            }
            b"ATXT" => {
                self.flush();
                self.pending = Some(decode_layer_header(payload, "ATXT")?);
            }
            b"VTXT" => {
                // a VTXT without a leading ATXT has nothing to attach to and is skipped
                let opacity = decode_vtxt(payload)?;
                if let Some(header) = self.pending.take() {
                    self.push(header, opacity);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// An ATXT with no VTXT after it is a layer with no opacity points.
    fn flush(&mut self) {
        if let Some(header) = self.pending.take() {
            self.push(header, vec![0.0; QUADRANT_POINTS]);
        }
    }

    fn push(&mut self, header: LayerHeader, opacity: Vec<f32>) {
        if header.texture != 0 {
            self.layers.alpha.push(AlphaLayer::new(header, opacity));
        }
    }

    /// The decoded layers.
    #[must_use]
    pub fn finish(mut self) -> CellLayers {
        self.flush();
        self.layers.alpha.sort_by_key(|l| (l.layer, l.quadrant));
        self.layers
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AlphaLayer, CellLayers, GRID_LEN, LayerHeader, LayerReader, QUADRANT_POINTS,
        decode_layer_header, decode_vclr, decode_vnml, decode_vtxt, quadrant_origin,
    };
    use crate::Error;
    use lodwright_testkit::land::{
        encode_atxt, encode_btxt, encode_vclr, encode_vnml, encode_vtxt,
    };
    use lodwright_testkit::rng::Rng;

    fn pattern(seed: u8) -> Vec<[u8; 3]> {
        (0..33 * 33_u32)
            .map(|i| {
                let b = u8::try_from(i % 251).unwrap();
                [b, b.wrapping_add(seed), b.wrapping_mul(3)]
            })
            .collect()
    }

    #[test]
    fn normals_and_colours_round_trip_in_row_major_order() {
        let normals = pattern(1);
        let decoded = decode_vnml(&encode_vnml(&normals)).unwrap();
        assert_eq!(decoded.as_slice(), normals.as_slice());
        assert_eq!(decoded.get(2, 1), normals[33 + 2]);
        let colours = pattern(9);
        assert_eq!(
            decode_vclr(&encode_vclr(&colours)).unwrap().as_slice(),
            colours
        );
    }

    #[test]
    fn a_grid_of_the_wrong_length_names_its_subrecord() {
        assert_eq!(
            decode_vnml(&[0; GRID_LEN - 1]),
            Err(Error::SubrecordLength {
                what: "VNML",
                found: GRID_LEN - 1
            })
        );
        assert_eq!(
            decode_vclr(&[]),
            Err(Error::SubrecordLength {
                what: "VCLR",
                found: 0
            })
        );
    }

    #[test]
    fn a_layer_header_round_trips() {
        let header = LayerHeader {
            texture: 0x0100_0ABC,
            quadrant: 3,
            layer: -2,
        };
        let bytes = encode_btxt(header.texture, header.quadrant, header.layer);
        assert_eq!(decode_layer_header(&bytes, "BTXT"), Ok(header));
    }

    #[test]
    fn a_layer_header_with_a_bad_quadrant_or_length_is_refused() {
        assert_eq!(
            decode_layer_header(&encode_atxt(1, 4, 0), "ATXT"),
            Err(Error::LayerQuadrant(4))
        );
        assert_eq!(
            decode_layer_header(&[0; 7], "ATXT"),
            Err(Error::SubrecordLength {
                what: "ATXT",
                found: 7
            })
        );
    }

    #[test]
    fn vtxt_points_land_at_their_positions_and_the_rest_is_zero() {
        let grid = decode_vtxt(&encode_vtxt(&[(0, 1.0), (18, 0.5), (288, 0.25)])).unwrap();
        assert_eq!(grid.len(), QUADRANT_POINTS);
        assert_eq!((grid[0], grid[18], grid[288]), (1.0, 0.5, 0.25));
        assert_eq!(grid.iter().filter(|v| **v != 0.0).count(), 3);
        assert_eq!(decode_vtxt(&[]).unwrap(), vec![0.0; QUADRANT_POINTS]);
    }

    #[test]
    fn vtxt_rejects_ragged_lengths_bad_positions_and_non_finite_opacities() {
        assert_eq!(
            decode_vtxt(&[0; 9]),
            Err(Error::SubrecordLength {
                what: "VTXT",
                found: 9
            })
        );
        assert_eq!(
            decode_vtxt(&encode_vtxt(&[(289, 1.0)])),
            Err(Error::LayerPosition(289))
        );
        assert_eq!(
            decode_vtxt(&encode_vtxt(&[(1, f32::NAN)])),
            Err(Error::LayerOpacity)
        );
    }

    #[test]
    fn quadrants_start_where_the_halves_of_the_cell_start() {
        assert_eq!(quadrant_origin(0), (0, 0));
        assert_eq!(quadrant_origin(1), (16, 0));
        assert_eq!(quadrant_origin(2), (0, 16));
        assert_eq!(quadrant_origin(3), (16, 16));
    }

    fn layer(quadrant: u8, points: &[(u16, f32)]) -> AlphaLayer {
        let mut reader = LayerReader::default();
        reader
            .subrecord(*b"ATXT", &encode_atxt(0x10, quadrant, 0))
            .unwrap();
        reader.subrecord(*b"VTXT", &encode_vtxt(points)).unwrap();
        reader.finish().alpha.remove(0)
    }

    #[test]
    fn an_asymmetric_pattern_lands_on_the_right_quadrant_and_vertex() {
        // position 1 is column 1 of row 0; position 17 is column 0 of row 1; position 19 is column 2 of row 1
        let points = [(1, 0.2), (17, 0.4), (19, 0.6)];
        for quadrant in 0..4_u8 {
            let alpha = layer(quadrant, &points);
            let (ox, oy) = quadrant_origin(quadrant);
            assert_eq!(alpha.vertex_opacity(ox + 1, oy), Some(0.2));
            assert_eq!(alpha.vertex_opacity(ox, oy + 1), Some(0.4));
            assert_eq!(alpha.vertex_opacity(ox + 2, oy + 1), Some(0.6));
            assert_eq!(alpha.vertex_opacity(ox + 1, oy + 1), Some(0.0));
            assert_eq!(alpha.vertex_opacity(ox, oy + 2), Some(0.0));
        }
        // a south-east layer does not claim the south-west corner vertex
        assert_eq!(layer(1, &points).vertex_opacity(1, 0), None);
        assert_eq!(layer(2, &points).vertex_opacity(1, 0), None);
    }

    #[test]
    fn the_shared_middle_vertex_belongs_to_both_neighbouring_quadrants() {
        let south_west = layer(0, &[(16, 0.7)]);
        let south_east = layer(1, &[(0, 0.9)]);
        assert_eq!(south_west.vertex_opacity(16, 0), Some(0.7));
        assert_eq!(south_east.vertex_opacity(16, 0), Some(0.9));
    }

    #[test]
    fn the_reader_collects_layers_in_order_and_skips_unknown_subrecords() {
        let mut reader = LayerReader::default();
        let steps: Vec<([u8; 4], Vec<u8>)> = vec![
            (*b"DATA", vec![7; 4]),
            (*b"BTXT", encode_btxt(0x11, 2, 0)),
            (*b"BTXT", encode_btxt(0, 1, 0)),
            (*b"ATXT", encode_atxt(0x21, 1, 2)),
            (*b"VTXT", encode_vtxt(&[(5, 1.0)])),
            (*b"ATXT", encode_atxt(0x22, 0, 1)),
            (*b"VTXT", encode_vtxt(&[(6, 0.5)])),
            (*b"ATXT", encode_atxt(0x23, 3, 1)),
            (*b"ZZZZ", vec![1, 2, 3]),
            (*b"VTXT", encode_vtxt(&[])),
            (*b"VNML", encode_vnml(&pattern(0))),
            (*b"VCLR", encode_vclr(&pattern(1))),
            (*b"VTEX", vec![0; 8]),
        ];
        for (signature, payload) in &steps {
            reader.subrecord(*signature, payload).unwrap();
        }
        let layers = reader.finish();
        assert_eq!(layers.base, [None, None, Some(0x11), None]);
        let order: Vec<_> = layers
            .alpha
            .iter()
            .map(|l| (l.layer, l.quadrant, l.texture))
            .collect();
        assert_eq!(order, [(1, 0, 0x22), (1, 3, 0x23), (2, 1, 0x21)]);
        assert_eq!(layers.alpha[0].point(6, 0), 0.5);
        assert!(layers.normals.is_some() && layers.colors.is_some());
    }

    #[test]
    fn an_atxt_without_vtxt_is_a_layer_without_points_and_a_stray_vtxt_is_skipped() {
        let mut reader = LayerReader::default();
        reader
            .subrecord(*b"VTXT", &encode_vtxt(&[(1, 1.0)]))
            .unwrap();
        reader
            .subrecord(*b"ATXT", &encode_atxt(0x30, 0, 0))
            .unwrap();
        let layers = reader.finish();
        assert_eq!(layers.alpha.len(), 1);
        assert!(layers.alpha[0].opacity_grid().iter().all(|v| *v == 0.0));
    }

    #[test]
    fn a_null_texture_makes_no_layer() {
        let mut reader = LayerReader::default();
        reader.subrecord(*b"ATXT", &encode_atxt(0, 0, 0)).unwrap();
        reader
            .subrecord(*b"VTXT", &encode_vtxt(&[(1, 1.0)]))
            .unwrap();
        assert_eq!(reader.finish().alpha.len(), 0);
    }

    #[test]
    fn textures_can_be_remapped() {
        let mut layers = CellLayers::default();
        layers.base[0] = Some(1);
        layers.alpha.push(AlphaLayer::new(
            LayerHeader {
                texture: 2,
                quadrant: 0,
                layer: 0,
            },
            vec![0.0; QUADRANT_POINTS],
        ));
        layers.map_textures(|id| id + 0x0100_0000);
        assert_eq!(layers.base[0], Some(0x0100_0001));
        assert_eq!(layers.alpha[0].texture, 0x0100_0002);
    }

    #[test]
    fn random_bytes_never_panic_any_decoder() {
        let mut rng = Rng::new(0x001A_7E85);
        for _ in 0..500 {
            let len = usize::try_from(rng.below(80)).unwrap();
            let bytes: Vec<u8> = (0..len)
                .map(|_| u8::try_from(rng.below(256)).unwrap())
                .collect();
            let mut reader = LayerReader::default();
            for signature in [*b"VNML", *b"VCLR", *b"BTXT", *b"ATXT", *b"VTXT"] {
                let _ = reader.subrecord(signature, &bytes);
            }
            let _ = reader.finish();
        }
    }
}
