// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic NIF files for `lodwright probe`, in the layout of version 20.2.0.7 (Fallout 3 and New Vegas): the header with its block table
//! and a few block types. Block contents are zeros or counters; nothing here is, or is derived from, a game file.

/// One block: its type name and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The block type name, for example `NiTriShapeData`.
    pub type_name: String,
    /// The block's bytes.
    pub bytes: Vec<u8>,
}

/// Builds a NIF with the given blocks.
#[derive(Debug, Clone)]
pub struct NifBuilder {
    user_version: u32,
    user_version_2: u32,
    author: String,
    strings: Vec<String>,
    blocks: Vec<Block>,
}

impl Default for NifBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl NifBuilder {
    /// A builder for user version 11 and user version 2 of 34, with no blocks.
    #[must_use]
    pub fn new() -> Self {
        Self {
            user_version: 11,
            user_version_2: 34,
            author: "author".to_owned(),
            strings: vec![],
            blocks: vec![],
        }
    }

    /// Set the author text of the export info (any text; the probe must never echo it).
    #[must_use]
    pub fn author(mut self, text: &str) -> Self {
        text.clone_into(&mut self.author);
        self
    }

    /// Add an entry to the header's string table.
    #[must_use]
    pub fn string(mut self, text: &str) -> Self {
        self.strings.push(text.to_owned());
        self
    }

    /// Append a block.
    #[must_use]
    pub fn block(mut self, type_name: &str, bytes: Vec<u8>) -> Self {
        self.blocks.push(Block {
            type_name: type_name.to_owned(),
            bytes,
        });
        self
    }

    // source: nif-header-layout, probe-nif-block-table
    /// The file: header, block data and an empty footer (zero roots).
    ///
    /// # Panics
    /// When a count or a length does not fit its field (never for test-sized input).
    #[must_use]
    pub fn build(&self) -> Vec<u8> {
        let mut b = b"Gamebryo File Format, Version 20.2.0.7\n".to_vec();
        b.extend_from_slice(&0x1402_0007_u32.to_le_bytes());
        b.push(1); // little endian
        b.extend_from_slice(&self.user_version.to_le_bytes());
        b.extend_from_slice(&u32::try_from(self.blocks.len()).unwrap().to_le_bytes());
        b.extend_from_slice(&self.user_version_2.to_le_bytes());
        short_string(&mut b, &self.author);
        if self.user_version_2 < 131 {
            short_string(&mut b, "process"); // process script
        }
        short_string(&mut b, "export"); // export script
        if self.user_version_2 >= 103 {
            short_string(&mut b, "maxpath");
        }
        let mut types: Vec<&str> = vec![];
        for block in &self.blocks {
            if !types.contains(&block.type_name.as_str()) {
                types.push(&block.type_name);
            }
        }
        b.extend_from_slice(&u16::try_from(types.len()).unwrap().to_le_bytes());
        for name in &types {
            sized_string(&mut b, name);
        }
        for block in &self.blocks {
            let index = types.iter().position(|t| *t == block.type_name).unwrap();
            b.extend_from_slice(&u16::try_from(index).unwrap().to_le_bytes());
        }
        for block in &self.blocks {
            b.extend_from_slice(&u32::try_from(block.bytes.len()).unwrap().to_le_bytes());
        }
        b.extend_from_slice(&u32::try_from(self.strings.len()).unwrap().to_le_bytes());
        let longest = self.strings.iter().map(String::len).max().unwrap_or(0);
        b.extend_from_slice(&u32::try_from(longest).unwrap().to_le_bytes());
        for text in &self.strings {
            sized_string(&mut b, text);
        }
        b.extend_from_slice(&0_u32.to_le_bytes()); // groups
        for block in &self.blocks {
            b.extend_from_slice(&block.bytes);
        }
        b.extend_from_slice(&0_u32.to_le_bytes()); // footer: zero roots
        b
    }
}

fn short_string(b: &mut Vec<u8>, text: &str) {
    b.push(u8::try_from(text.len()).unwrap());
    b.extend_from_slice(text.as_bytes());
}

fn sized_string(b: &mut Vec<u8>, text: &str) {
    b.extend_from_slice(&u32::try_from(text.len()).unwrap().to_le_bytes());
    b.extend_from_slice(text.as_bytes());
}

/// What the geometry data of a synthetic shape holds besides its counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryOptions {
    /// Vector flags: bit 0 UV present, bit 12 tangents.
    pub vector_flags: u16,
    /// Whether normals (and, with the tangents flag, tangents) are stored.
    pub has_normals: bool,
    /// Whether vertex colours are stored.
    pub has_vertex_colors: bool,
}

impl Default for GeometryOptions {
    fn default() -> Self {
        Self {
            vector_flags: 1,
            has_normals: true,
            has_vertex_colors: false,
        }
    }
}

// source: probe-nif-geometry-data-layout
fn geometry_prefix(vertices: u16, options: GeometryOptions) -> Vec<u8> {
    let n = usize::from(vertices);
    let mut b = vec![];
    b.extend_from_slice(&0_i32.to_le_bytes()); // group id
    b.extend_from_slice(&vertices.to_le_bytes());
    b.extend_from_slice(&[0, 0]); // keep and compress flags
    b.push(1); // has vertices
    b.extend(std::iter::repeat_n(0_u8, n * 12));
    b.extend_from_slice(&options.vector_flags.to_le_bytes());
    b.push(u8::from(options.has_normals));
    if options.has_normals {
        b.extend(std::iter::repeat_n(0_u8, n * 12));
        if options.vector_flags & 4096 != 0 {
            b.extend(std::iter::repeat_n(0_u8, n * 24));
        }
    }
    b.extend_from_slice(&[0; 16]); // bounding sphere
    b.push(u8::from(options.has_vertex_colors));
    if options.has_vertex_colors {
        b.extend(std::iter::repeat_n(0_u8, n * 16));
    }
    if options.vector_flags & 1 != 0 {
        b.extend(std::iter::repeat_n(0_u8, n * 8));
    }
    b.extend_from_slice(&0_u16.to_le_bytes()); // consistency flags
    b.extend_from_slice(&(-1_i32).to_le_bytes()); // additional data
    b
}

// source: nif-trishape-strips-data
/// An `NiTriShapeData` block with `vertices` vertices and `triangles` triangles (indices count up, wrapping).
///
/// # Panics
/// Never for u16-sized input.
#[must_use]
pub fn tri_shape_data(vertices: u16, triangles: u16, options: GeometryOptions) -> Vec<u8> {
    let mut b = geometry_prefix(vertices, options);
    b.extend_from_slice(&triangles.to_le_bytes());
    b.extend_from_slice(&(u32::from(triangles) * 3).to_le_bytes());
    b.push(1); // has triangles
    for t in 0..triangles {
        for corner in 0..3_u16 {
            let index = t.wrapping_add(corner) % vertices.max(1);
            b.extend_from_slice(&index.to_le_bytes());
        }
    }
    b.extend_from_slice(&0_u16.to_le_bytes()); // match groups
    b
}

// source: nif-trishape-strips-data
/// An `NiTriStripsData` block with `vertices` vertices and one strip per entry of `strip_lengths`.
///
/// # Panics
/// Never for u16-sized input.
#[must_use]
pub fn tri_strips_data(vertices: u16, strip_lengths: &[u16], options: GeometryOptions) -> Vec<u8> {
    let mut b = geometry_prefix(vertices, options);
    let triangles: u16 = strip_lengths.iter().map(|l| l.saturating_sub(2)).sum();
    b.extend_from_slice(&triangles.to_le_bytes());
    b.extend_from_slice(&u16::try_from(strip_lengths.len()).unwrap().to_le_bytes());
    for length in strip_lengths {
        b.extend_from_slice(&length.to_le_bytes());
    }
    b.push(1); // has points
    for length in strip_lengths {
        for i in 0..*length {
            b.extend_from_slice(&(i % vertices.max(1)).to_le_bytes());
        }
    }
    b
}

// source: nif-texture-set
/// A `BSShaderTextureSet` block holding `paths`.
///
/// # Panics
/// Never for test-sized input.
#[must_use]
pub fn texture_set(paths: &[&str]) -> Vec<u8> {
    let mut b = u32::try_from(paths.len()).unwrap().to_le_bytes().to_vec();
    for path in paths {
        sized_string(&mut b, path);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::{GeometryOptions, NifBuilder, texture_set, tri_shape_data, tri_strips_data};

    #[test]
    fn the_header_starts_with_the_text_line_and_the_version() {
        let bytes = NifBuilder::new().build();
        assert!(bytes.starts_with(b"Gamebryo File Format, Version 20.2.0.7\n"));
        assert_eq!(&bytes[39..43], &0x1402_0007_u32.to_le_bytes());
    }

    #[test]
    fn a_built_file_is_as_long_as_its_parts() {
        let shape = tri_shape_data(3, 1, GeometryOptions::default());
        let with = NifBuilder::new()
            .block("NiTriShapeData", shape.clone())
            .build();
        let without = NifBuilder::new().build();
        // one block adds its bytes, a type name (4 + 14), an index (2) and a size (4)
        assert_eq!(with.len(), without.len() + shape.len() + 18 + 2 + 4);
    }

    #[test]
    fn geometry_sizes_follow_the_options() {
        let plain = tri_shape_data(4, 2, GeometryOptions::default()).len();
        let colours = tri_shape_data(
            4,
            2,
            GeometryOptions {
                has_vertex_colors: true,
                ..GeometryOptions::default()
            },
        )
        .len();
        assert_eq!(colours, plain + 4 * 16);
        let tangents = tri_shape_data(
            4,
            2,
            GeometryOptions {
                vector_flags: 1 | 4096,
                ..GeometryOptions::default()
            },
        )
        .len();
        assert_eq!(tangents, plain + 4 * 24);
    }

    #[test]
    fn strips_and_texture_sets_have_their_counts() {
        let strips = tri_strips_data(5, &[4, 3], GeometryOptions::default());
        assert!(strips.len() > 50);
        let set = texture_set(&["a", "bc"]);
        assert_eq!(set.len(), 4 + 4 + 1 + 4 + 2);
    }
}
