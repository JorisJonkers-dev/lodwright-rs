// SPDX-License-Identifier: MPL-2.0
//! Little-endian cursor reader and byte writer with bounds-checked counts.

use crate::error::{NifError, Result};

/// A cursor over a byte slice. Every read is bounds-checked and every count is checked against the bytes left,
/// so a hostile count cannot make the reader allocate more than the input could possibly hold.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn peek(&self, ahead: usize) -> Option<u8> {
        self.data.get(self.pos.checked_add(ahead)?).copied()
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(NifError::Truncated {
                at: self.pos,
                needed: n,
            });
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut out = [0_u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(crate) fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub(crate) fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    /// Check that `count` elements of at least `elem` bytes each can still be read.
    pub(crate) fn check_count(&self, what: &'static str, count: u32, elem: usize) -> Result<usize> {
        let n = usize::try_from(count).map_err(|_| NifError::BadCount { what, count })?;
        match n.checked_mul(elem.max(1)) {
            Some(bytes) if bytes <= self.remaining() => Ok(n),
            _ => Err(NifError::BadCount { what, count }),
        }
    }

    /// A `u32` count followed by that many elements of `elem` bytes (as a lower bound).
    pub(crate) fn count32(&mut self, what: &'static str, elem: usize) -> Result<usize> {
        let c = self.u32()?;
        self.check_count(what, c, elem)
    }

    /// A `u16` count.
    pub(crate) fn count16(&mut self, what: &'static str, elem: usize) -> Result<usize> {
        let c = self.u16()?;
        self.check_count(what, u32::from(c), elem)
    }

    pub(crate) fn i32s(&mut self, n: usize) -> Result<Vec<i32>> {
        (0..n).map(|_| self.i32()).collect()
    }

    pub(crate) fn f32s<const N: usize>(&mut self) -> Result<[f32; N]> {
        let mut out = [0.0_f32; N];
        for v in &mut out {
            *v = self.f32()?;
        }
        Ok(out)
    }

    pub(crate) fn vecs<const N: usize>(&mut self, n: usize) -> Result<Vec<[f32; N]>> {
        (0..n).map(|_| self.f32s::<N>()).collect()
    }

    // source: nif-primitive-encodings, nif-block-table
    /// A `u32` length and that many bytes.
    pub(crate) fn sized_string(&mut self, what: &'static str) -> Result<Vec<u8>> {
        let n = self.count32(what, 1)?;
        Ok(self.take(n)?.to_vec())
    }

    // source: nif-primitive-encodings, nif-header-fields
    /// A `u8` length and that many bytes.
    pub(crate) fn short_string(&mut self) -> Result<Vec<u8>> {
        let n = usize::from(self.u8()?);
        Ok(self.take(n)?.to_vec())
    }
}

/// Convert a length to the `u32` the format stores.
pub(crate) fn len32(n: usize, what: &'static str) -> Result<u32> {
    u32::try_from(n).map_err(|_| NifError::Inconsistent(what))
}

/// Convert a length to the `u16` the format stores.
pub(crate) fn len16(n: usize, what: &'static str) -> Result<u16> {
    u16::try_from(n).map_err(|_| NifError::Inconsistent(what))
}

pub(crate) fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn put_i32(out: &mut Vec<u8>, v: i32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn put_f32s(out: &mut Vec<u8>, vs: &[f32]) {
    for v in vs {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

pub(crate) fn put_i32s_counted(out: &mut Vec<u8>, vs: &[i32], what: &'static str) -> Result<()> {
    put_u32(out, len32(vs.len(), what)?);
    for v in vs {
        put_i32(out, *v);
    }
    Ok(())
}

pub(crate) fn put_sized_string(out: &mut Vec<u8>, s: &[u8], what: &'static str) -> Result<()> {
    put_u32(out, len32(s.len(), what)?);
    out.extend_from_slice(s);
    Ok(())
}

pub(crate) fn put_short_string(out: &mut Vec<u8>, s: &[u8], what: &'static str) -> Result<()> {
    let n = u8::try_from(s.len()).map_err(|_| NifError::Inconsistent(what))?;
    out.push(n);
    out.extend_from_slice(s);
    Ok(())
}
