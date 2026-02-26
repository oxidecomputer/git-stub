// Copyright 2026 Oxide Computer Company

//! Error types for git stub VCS operations and materialization.

use crate::VcsName;
use camino::Utf8PathBuf;
use git_stub::{GitStub, GitStubParseError};
use std::{ffi::OsString, io};
use thiserror::Error;

// ---- VCS errors ----

/// An error from reading a VCS binary path from the environment.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VcsEnvError {
    /// The environment variable is set but is not valid UTF-8.
    #[error(
        "${var} environment variable is not valid \
         UTF-8: {value:?}"
    )]
    NonUtf8 {
        /// The environment variable name.
        var: &'static str,
        /// The non-UTF-8 value.
        value: OsString,
    },
}

/// An error that occurs during VCS detection.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VcsDetectError {
    /// The provided repository root does not exist.
    #[error(
        "{repo_root} does not exist \
         (expected a repository root with .git or .jj)"
    )]
    PathNotFound {
        /// The path that was provided.
        repo_root: Utf8PathBuf,
    },

    /// The provided repository root is not a directory.
    #[error(
        "{repo_root} is not a directory \
         (expected a repository root with .git or .jj)"
    )]
    NotADirectory {
        /// The path that was provided.
        repo_root: Utf8PathBuf,
    },

    /// An I/O error occurred while probing the repository root.
    #[error("I/O error while checking for VCS at {path}")]
    Io {
        /// The path being checked when the error occurred.
        path: Utf8PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// Neither `.git` nor `.jj` was found at the repository root.
    #[error("no VCS found at {repo_root} (expected .git or .jj)")]
    NotFound {
        /// The repository root that was searched.
        repo_root: Utf8PathBuf,
    },

    /// A VCS environment variable is not valid UTF-8.
    #[error(transparent)]
    Env(#[from] VcsEnvError),
}

/// An error that occurs while checking for a shallow clone.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ShallowCloneError {
    /// Failed to spawn the VCS process.
    #[error("failed to run {vcs_name} at {binary_path:?} in {repo_root}")]
    SpawnFailed {
        /// The name of the VCS.
        vcs_name: VcsName,
        /// The path to the VCS executable.
        binary_path: String,
        /// The working directory where the command was run.
        repo_root: Utf8PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The VCS command to check for a shallow repository failed.
    #[error(
        "{vcs_name} failed to check for shallow clone \
         ({exit_status}): {stderr}"
    )]
    VcsFailed {
        /// The name of the VCS.
        vcs_name: VcsName,
        /// A human-readable description of the exit status (e.g.,
        /// "exit code 128" or "killed by signal").
        exit_status: String,
        /// The stderr output from the VCS.
        stderr: String,
    },

    /// The VCS command succeeded but returned unexpected output.
    #[error(
        "{vcs_name} returned unexpected output for shallow clone \
         check: expected \"true\" or \"false\", got {stdout:?}"
    )]
    UnexpectedOutput {
        /// The name of the VCS.
        vcs_name: VcsName,
        /// The stdout content that could not be interpreted.
        stdout: String,
    },
}

/// An error that occurs while reading the contents of a
/// [`GitStub`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReadContentsError {
    /// Failed to spawn the VCS process.
    #[error("failed to run {vcs_name} at {binary_path:?} in {repo_root}")]
    SpawnFailed {
        /// The name of the VCS.
        vcs_name: VcsName,
        /// The path to the VCS executable.
        binary_path: String,
        /// The working directory where the command was run.
        repo_root: Utf8PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// The VCS command failed.
    #[error("{vcs_name} failed to read {stub} ({exit_status}): {stderr}")]
    VcsFailed {
        /// The name of the VCS.
        vcs_name: VcsName,
        /// The stub that was requested.
        stub: GitStub,
        /// A human-readable description of the exit status (e.g.,
        /// "exit code 128" or "killed by signal").
        exit_status: String,
        /// The stderr output from the VCS.
        stderr: String,
    },
}

// ---- Materialization errors ----

/// Errors that can occur during git stub materialization.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MaterializeError {
    /// The path does not have a `.gitstub` extension.
    #[error("path does not end with .gitstub: {path}")]
    NotGitStub {
        /// The path that was provided.
        path: Utf8PathBuf,
    },

    /// Failed to read the Git stub.
    #[error("failed to read Git stub {path}")]
    ReadGitStub {
        /// The path to the Git stub.
        path: Utf8PathBuf,
        /// The underlying I/O error.
        #[source]
        error: io::Error,
    },

    /// The git stub has an invalid format.
    #[error("invalid Git stub format in {path}")]
    InvalidGitStub {
        /// The path to the Git stub.
        path: Utf8PathBuf,
        /// Details about the parsing error.
        #[source]
        error: GitStubParseError,
    },

    /// VCS detection failed.
    #[error("VCS detection failed")]
    VcsDetect(#[from] VcsDetectError),

    /// Failed to read contents from Git.
    #[error("failed to read git stub contents")]
    ReadContents(#[from] ReadContentsError),

    /// Failed to check whether the repository is a shallow clone.
    #[error("failed to check for shallow clone at {repo_root}")]
    ShallowCloneCheck {
        /// The repository root.
        repo_root: Utf8PathBuf,
        /// The underlying error.
        #[source]
        error: ShallowCloneError,
    },

    /// The repository is a shallow clone.
    #[error(
        "shallow clone detected at {repo_root}: cannot dereference \
         git stubs without full history \
         (run `git fetch --unshallow`)"
    )]
    ShallowClone {
        /// The repository root.
        repo_root: Utf8PathBuf,
    },

    /// Failed to create output directory.
    #[error("failed to create output directory {path}")]
    CreateDir {
        /// The directory path.
        path: Utf8PathBuf,
        /// The underlying I/O error.
        #[source]
        error: io::Error,
    },

    /// Failed to write the materialized file.
    #[error("failed to write materialized spec to {path}")]
    WriteOutput {
        /// The path where the write failed.
        path: Utf8PathBuf,
        /// The underlying write error.
        #[source]
        error: AtomicWriteError,
    },
}

/// An error that occurred during an atomic file write.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AtomicWriteError {
    /// Writing contents to the temporary file failed.
    #[error("writing file contents failed")]
    Write(#[source] io::Error),

    /// The atomic write infrastructure failed (e.g., creating the
    /// temporary file, or renaming it into place).
    #[error("atomic create or rename failed")]
    Rename(#[source] io::Error),
}
