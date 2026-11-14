// SPDX-License-Identifier: MPL-2.0
//! A bounds-checked little-endian cursor. Every read returns `None` past the end; nothing indexes the slice.

/// A cursor over a byte slice.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// A cursor at the start of `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Bytes not yet read.
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// The next `n` bytes.
    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let bytes = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    /// Skip `n` bytes.
    pub fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(drop)
    }

    /// Skip `count` items of `size` bytes each, checking the product.
    pub fn skip_items(&mut self, count: usize, size: usize) -> Option<()> {
        self.skip(count.checked_mul(size)?)
    }

    /// A byte.
    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    /// A little-endian `u16`.
    pub fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    /// A little-endian `u32`.
    pub fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// A `u32` widened to `usize`.
    pub fn len32(&mut self) -> Option<usize> {
        usize::try_from(self.u32()?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::Reader;

    #[test]
    fn reads_little_endian_and_stops_at_the_end() {
        let mut r = Reader::new(&[1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(r.u8(), Some(1));
        assert_eq!(r.u16(), Some(0x0302));
        assert_eq!(r.u32(), Some(0x0706_0504));
        assert_eq!(r.remaining(), 0);
        assert_eq!(r.u8(), None);
    }

    #[test]
    fn a_failed_read_does_not_move_the_cursor() {
        let mut r = Reader::new(&[1, 2, 3]);
        assert_eq!(r.u32(), None);
        assert_eq!(r.take(4), None);
        assert_eq!(r.take(3), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn huge_counts_do_not_overflow() {
        let mut r = Reader::new(&[0; 8]);
        assert_eq!(r.skip_items(usize::MAX, 12), None);
        assert_eq!(r.skip(usize::MAX), None);
        assert_eq!(r.skip_items(2, 4), Some(()));
        assert_eq!(r.len32(), None);
    }
}
