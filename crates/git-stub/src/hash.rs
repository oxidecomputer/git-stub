// Copyright 2026 Oxide Computer Company

//! Git commit hash types.

use crate::CommitHashParseError;
use std::{fmt, str::FromStr};

/// A Git commit hash.
///
/// This type guarantees the contained value is either:
///
/// - 20 bytes (SHA-1, displayed as 40 lowercase hex characters)
/// - 32 bytes (SHA-256, displayed as 64 lowercase hex characters)
///
/// # Parsing
///
/// Parse from a hex string using [`FromStr`]:
///
/// ```
/// use git_stub::GitCommitHash;
///
/// let hash: GitCommitHash =
///     "0123456789abcdef0123456789abcdef01234567".parse().unwrap();
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GitCommitHash {
    /// A SHA-1 hash: the one traditionally used in Git.
    Sha1([u8; 20]),
    /// A SHA-256 hash, supported by newer versions of Git.
    Sha256([u8; 32]),
}

impl FromStr for GitCommitHash {
    type Err = CommitHashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let len = s.len();
        match len {
            40 => {
                let mut bytes = [0; 20];
                hex::decode_to_slice(s, &mut bytes)
                    .map_err(CommitHashParseError::InvalidHex)?;
                Ok(GitCommitHash::Sha1(bytes))
            }
            64 => {
                let mut bytes = [0; 32];
                hex::decode_to_slice(s, &mut bytes)
                    .map_err(CommitHashParseError::InvalidHex)?;
                Ok(GitCommitHash::Sha256(bytes))
            }
            _ => Err(CommitHashParseError::InvalidLength(len)),
        }
    }
}

impl fmt::Display for GitCommitHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitCommitHash::Sha1(bytes) => hex::encode(bytes).fmt(f),
            GitCommitHash::Sha256(bytes) => hex::encode(bytes).fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SHA1: &str = "0123456789abcdef0123456789abcdef01234567";
    const VALID_SHA256: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn test_commit_hash_valid() {
        let hash: GitCommitHash = VALID_SHA1.parse().unwrap();
        assert_eq!(hash.to_string(), VALID_SHA1);

        let hash: GitCommitHash = VALID_SHA256.parse().unwrap();
        assert_eq!(hash.to_string(), VALID_SHA256);
    }

    #[test]
    fn test_commit_hash_invalid() {
        assert_eq!(
            "abc123".parse::<GitCommitHash>(),
            Err(CommitHashParseError::InvalidLength(6)),
            "too short"
        );

        assert_eq!(
            VALID_SHA1[..39].parse::<GitCommitHash>(),
            Err(CommitHashParseError::InvalidLength(39)),
            "39 chars (one short of SHA-1)"
        );

        let input = format!("{}0", VALID_SHA1);
        assert_eq!(
            input.parse::<GitCommitHash>(),
            Err(CommitHashParseError::InvalidLength(41)),
            "41 chars (one over SHA-1)"
        );

        assert!(
            matches!(
                "0123456789abcdefg123456789abcdef01234567"
                    .parse::<GitCommitHash>(),
                Err(CommitHashParseError::InvalidHex(_))
            ),
            "non-hex character 'g'"
        );

        let input = format!(" {}", VALID_SHA1);
        assert_eq!(
            input.parse::<GitCommitHash>(),
            Err(CommitHashParseError::InvalidLength(41)),
            "leading whitespace (the CommitHash parser doesn't do trimming)"
        );
    }

    #[test]
    fn test_commit_hash_empty_string() {
        assert_eq!(
            "".parse::<GitCommitHash>(),
            Err(CommitHashParseError::InvalidLength(0)),
            "empty string"
        );
    }
}
