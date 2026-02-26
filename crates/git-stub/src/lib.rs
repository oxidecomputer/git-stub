// Copyright 2026 Oxide Computer Company

//! Parsing types for Git stubs.
//!
//! A *Git stub* (e.g., `foo.json.gitstub`) contains a reference to a file
//! stored in Git history, in the format `commit:path`. This allows storing a
//! pointer to a file's contents without duplicating the actual data in the
//! working tree.
//!
//! Git stubs are useful in case you have several different versions of a file
//! that must be stored side by side, but the files aren't large enough to be
//! stored through a mechanism like [Git LFS](https://git-lfs.com/). Git stubs
//! are similar to [LFS pointer
//! files](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md#the-pointer),
//! with the difference being that the canonical versions are stored in old
//! versions of Git history rather than on an external server.
//!
//! For more about the motivation and design decisions behind Git stubs, see
//! [RFD 634 Git stub files for Dropshot versioned
//! APIs](https://rfd.shared.oxide.computer/rfd/0634).
//!
//! The main entry point is [`GitStub`].
//!
//! # Examples
//!
//! ```
//! use git_stub::{GitCommitHash, GitStub};
//!
//! // A Git stub contains a single line in the format "commit:path\n".
//! let file_contents =
//!     "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2:openapi/api-v1.json\n";
//! let stub: GitStub = file_contents.parse().unwrap();
//!
//! // Inspect the parsed commit hash and path.
//! assert!(matches!(stub.commit(), GitCommitHash::Sha1(_)));
//! assert_eq!(stub.path().as_str(), "openapi/api-v1.json");
//!
//! // Canonical input: needs_rewrite is false.
//! assert!(!stub.needs_rewrite());
//!
//! // Round-trip back to canonical file contents.
//! assert_eq!(stub.to_file_contents(), file_contents);
//!
//! // Parsing from non-canonical input (e.g., missing trailing newline)
//! // sets needs_rewrite to true.
//! let stub2: GitStub =
//!     "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2:openapi/api-v1.json"
//!         .parse()
//!         .unwrap();
//! assert!(stub2.needs_rewrite());
//!
//! // To retrieve the actual file from Git history, use
//! // `git_stub_vcs::Vcs` to read the contents.
//! ```
//!
//! # Related crates
//!
//! For materializing files from version control systems like Git or Jujutsu,
//! see [`git-stub-vcs`](https://crates.io/crates/git-stub-vcs) ([source
//! tree](https://github.com/oxidecomputer/git-stub/tree/main/crates/git-stub-vcs)).

#![deny(missing_docs)]

mod errors;
mod git_stub;
mod hash;

pub use errors::{CommitHashParseError, GitStubParseError};
pub use git_stub::GitStub;
pub use hash::GitCommitHash;
