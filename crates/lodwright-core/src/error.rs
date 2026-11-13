// SPDX-License-Identifier: MPL-2.0
//! Error type shared by the core modules.

use std::fmt;

/// What can go wrong in the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// A block size of zero cells was requested.
    ZeroBlockSize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroBlockSize => f.write_str("a block must span at least one cell"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn zero_block_size_error_names_the_problem() {
        assert_eq!(
            Error::ZeroBlockSize.to_string(),
            "a block must span at least one cell"
        );
    }
}
