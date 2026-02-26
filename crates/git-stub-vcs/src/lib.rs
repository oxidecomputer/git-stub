// Copyright 2026 Oxide Computer Company

//! VCS abstraction and materialization for git stubs.
//!
//! A *git stub* (e.g., `foo.json.gitstub`) contains a reference to a file
//! stored in Git history, in the format `commit:path`. This crate provides
//! a VCS abstraction for reading file contents from history, and helpers to
//! materialize these references into actual files.
//!
//! # Usage in build scripts
//!
//! ```no_run
//! // build.rs
//! use git_stub_vcs::Materializer;
//!
//! fn main() {
//!     // repo_root is relative to CARGO_MANIFEST_DIR (the directory containing
//!     // this crate's Cargo.toml). Typically "." or some number of "..".
//!     let repo_root = "../..";
//!     let materializer = Materializer::for_build_script(repo_root)
//!         .expect("VCS detected at repo root");
//!
//!     // git_stub_path is relative to repo_root.
//!     let spec_path = materializer
//!         .materialize("openapi/my-api/my-api-1.0.0-abc123.json.gitstub")
//!         .expect("materialized successfully");
//!
//!     // spec_path is a path in OUT_DIR with the materialized content.
//!     // The materializer also emits cargo::rerun-if-changed for the git stub.
//! }
//! ```
//!
//! # Usage outside build scripts
//!
//! ```no_run
//! use git_stub_vcs::Materializer;
//!
//! // repo_root is relative to the current working directory.
//! let materializer = Materializer::standard("../..", "/tmp/output")
//!     .expect("VCS detected at repo root");
//!
//! // git_stub_path is relative to repo_root.
//! let spec_path = materializer
//!     .materialize("openapi/my-api/my-api-1.0.0-abc123.json.gitstub")
//!     .expect("materialized successfully");
//! ```

#![deny(missing_docs)]

mod errors;
mod materialize;
mod vcs;

pub use errors::{
    AtomicWriteError, MaterializeError, ReadContentsError, ShallowCloneError,
    VcsDetectError, VcsEnvError,
};
pub use materialize::Materializer;
pub use vcs::{Vcs, VcsName};
