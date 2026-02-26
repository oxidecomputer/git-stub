// Copyright 2026 Oxide Computer Company

//! Error types for git stub operations.

use camino::Utf8PathBuf;
use thiserror::Error;

/// An error that occurs while parsing a
/// [`GitCommitHash`](crate::GitCommitHash).
#[derive(Clone, Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum CommitHashParseError {
    /// The commit hash has an invalid length.
    #[error(
        "invalid length: expected 40 (SHA-1) or 64 (SHA-256) hex characters, \
         got {0}"
    )]
    InvalidLength(usize),

    /// The commit hash is not valid hexadecimal.
    #[error("invalid hexadecimal")]
    InvalidHex(hex::FromHexError),
}

/// An error that occurs while parsing a [`GitStub`](crate::GitStub).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GitStubParseError {
    /// The input was empty or contained only whitespace.
    #[error("git stub is empty")]
    EmptyInput,

    /// The git stub string did not contain the expected 'commit:path'
    /// format.
    #[error(
        "invalid git stub format: expected 'commit:path', got {0:?} \
         (missing ':' separator)"
    )]
    InvalidFormat(String),

    /// The commit hash in the git stub was invalid.
    #[error("invalid commit hash in git stub")]
    InvalidCommitHash(#[from] CommitHashParseError),

    /// The path component was empty.
    #[error("git stub has empty path (nothing after ':')")]
    EmptyPath,

    /// The path contains a non-normal component (e.g., `..`, `.`, `/`, or a
    /// Windows prefix). Only plain file and directory names are allowed.
    #[error(
        "git stub path {path:?} contains non-normal component {component:?} \
         (only plain file/directory names are allowed)"
    )]
    InvalidPathComponent {
        /// The full path that failed validation.
        path: Utf8PathBuf,
        /// The non-normal component that was found (e.g., `..`, `.`, `/`).
        component: String,
    },

    /// The path contains a newline character.
    #[error("git stub path contains a newline character")]
    NewlineInPath,
}
