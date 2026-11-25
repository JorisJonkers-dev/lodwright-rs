// SPDX-License-Identifier: GPL-3.0-only
//! Digests that tie a document to what produced it. A digest is a hash, never content.

use sha2::{Digest, Sha256};

/// `sha256:<hex>` of `bytes`.
#[must_use]
pub fn sha256_digest(bytes: &[u8]) -> String {
    let mut stream = Sha256Stream::new();
    stream.update(bytes);
    stream.finish()
}

/// An incremental SHA-256 that prefixes every part with its length, so that two different splits of the same
/// bytes into parts never collide.
#[derive(Debug, Default)]
pub struct Sha256Stream(Sha256);

impl Sha256Stream {
    /// An empty stream.
    #[must_use]
    pub fn new() -> Self {
        Self(Sha256::new())
    }

    /// Add `bytes` as the next part (length first).
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
    }

    /// The digest as `sha256:<hex>`.
    #[must_use]
    pub fn finish(self) -> String {
        let mut text = String::from("sha256:");
        for byte in self.0.finalize() {
            text.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
            text.push(char::from(b"0123456789abcdef"[usize::from(byte & 15)]));
        }
        text
    }
}

/// `sha256:<hex>` of the `len` bytes `reader` gives, read a part at a time: the same as [`sha256_digest`] of those
/// bytes as one slice.
///
/// # Errors
/// When `reader` fails or ends before `len` bytes.
pub fn sha256_digest_of_reader(
    len: u64,
    mut reader: impl std::io::Read,
) -> std::io::Result<String> {
    let mut stream = Sha256Stream::new();
    stream.0.update(len.to_le_bytes());
    let mut buf = vec![0_u8; 1 << 20];
    let mut left = len;
    while left > 0 {
        let want = usize::try_from(left.min(buf.len() as u64)).unwrap_or(buf.len());
        reader.read_exact(&mut buf[..want])?;
        stream.0.update(&buf[..want]);
        left -= want as u64;
    }
    Ok(stream.finish())
}

/// The digest of a normalised argument set: the version tag of the normal form, then each `key=value` line.
/// Callers pass the lines already normalised (case folded, order fixed by the caller where order is meaning,
/// for example the load order).
#[must_use]
pub fn arguments_digest(lines: &[String]) -> String {
    let mut stream = Sha256Stream::new();
    stream.update(b"lodwright-arguments-v1");
    for line in lines {
        stream.update(line.as_bytes());
    }
    stream.finish()
}

#[cfg(test)]
mod tests {
    use super::{Sha256Stream, arguments_digest, sha256_digest};
    use lodwright_testkit::{covers, ids};

    #[test]
    fn digest_of_nothing_is_the_known_sha256_with_its_length_prefix() {
        covers(&[ids::contract::documents::DIGEST_FUNCTION]);
        // The prefix is eight zero bytes, so this is SHA-256 of eight zero bytes.
        assert_eq!(
            sha256_digest(b""),
            "sha256:af5570f5a1810b7af78caf4bc70a660f0df51e42baf91d4de5b2328de0e83dfc"
        );
    }

    #[test]
    fn a_reader_read_in_parts_gives_the_digest_of_the_whole_slice() {
        covers(&[ids::contract::documents::DIGEST_FUNCTION]);
        let bytes: Vec<u8> = (0..3_000_000_u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(
            super::sha256_digest_of_reader(bytes.len() as u64, bytes.as_slice()).unwrap(),
            sha256_digest(&bytes)
        );
        assert!(super::sha256_digest_of_reader(10, &b"short"[..]).is_err());
    }

    #[test]
    fn parts_are_length_prefixed_so_splits_differ() {
        covers(&[ids::contract::documents::DIGEST_FUNCTION]);
        let mut a = Sha256Stream::new();
        a.update(b"ab");
        a.update(b"c");
        let mut b = Sha256Stream::new();
        b.update(b"a");
        b.update(b"bc");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn argument_digest_depends_on_every_line_and_their_order() {
        covers(&[ids::contract::documents::DIGEST_FUNCTION]);
        let one = arguments_digest(&["a=1".into(), "b=2".into()]);
        assert_eq!(one, arguments_digest(&["a=1".into(), "b=2".into()]));
        assert_ne!(one, arguments_digest(&["b=2".into(), "a=1".into()]));
        assert_ne!(one, arguments_digest(&["a=1".into()]));
        assert!(one.starts_with("sha256:") && one.len() == 71);
    }
}
