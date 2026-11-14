// SPDX-License-Identifier: MPL-2.0
//! Probe for settings and `.dtl` files: the size and the leading bytes as little-endian integers.
//!
//! No format fact is used: the file is treated as a row of 32-bit little-endian integers, which is how the owner
//! reads such a file in a hex dump (`docs/spec/measurements.md`, M7 and M8).

use super::{ProbeError, Value};

/// How many leading bytes become integers.
pub const LEADING_BYTES: usize = 64;

/// Probe a file of `size` bytes whose first bytes are `head` (at most [`LEADING_BYTES`] are used).
///
/// # Errors
/// [`ProbeError`] when the file is empty.
pub fn probe(size: u64, head: &[u8]) -> Result<Value, ProbeError> {
    if size == 0 {
        return Err(ProbeError::new("the file is empty"));
    }
    let head = &head[..head.len().min(LEADING_BYTES)];
    let (words, tail) = head.as_chunks::<4>();
    let mut report = vec![
        ("size_bytes", Value::Uint(size)),
        (
            "leading_u32_le",
            Value::List(
                words
                    .iter()
                    .map(|w| Value::Uint(u64::from(u32::from_le_bytes(*w))))
                    .collect(),
            ),
        ),
    ];
    if !tail.is_empty() {
        let value = tail
            .iter()
            .rev()
            .fold(0_u64, |acc, b| (acc << 8) | u64::from(*b));
        report.push(("tail_bytes", Value::count(tail.len())));
        report.push(("tail_u_le", Value::Uint(value)));
    }
    Ok(Value::Map(report))
}

#[cfg(test)]
mod tests {
    use super::probe;

    #[test]
    fn sixteen_bytes_become_four_integers() {
        let mut bytes = vec![];
        for n in [1_u32, 2, 3, 0xFFFF_FFFF] {
            bytes.extend_from_slice(&n.to_le_bytes());
        }
        assert_eq!(
            probe(16, &bytes).unwrap().to_json(),
            r#"{"size_bytes":16,"leading_u32_le":[1,2,3,4294967295]}"#
        );
    }

    #[test]
    fn only_the_first_64_bytes_are_used_and_the_size_is_the_files() {
        let json = probe(1000, &[7; 100]).unwrap().to_json();
        assert!(json.starts_with(r#"{"size_bytes":1000,"leading_u32_le":[117901063,"#));
        assert_eq!(json.matches("117901063").count(), 16);
    }

    #[test]
    fn a_partial_word_is_one_integer_and_a_length() {
        let json = probe(7, &[1, 0, 0, 0, 2, 1, 0]).unwrap().to_json();
        assert_eq!(
            json,
            r#"{"size_bytes":7,"leading_u32_le":[1],"tail_bytes":3,"tail_u_le":258}"#
        );
    }

    #[test]
    fn an_empty_file_is_an_error() {
        assert!(probe(0, &[]).is_err());
    }
}
