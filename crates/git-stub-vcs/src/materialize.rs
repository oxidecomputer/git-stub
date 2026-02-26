// Copyright 2026 Oxide Computer Company

//! Materialization logic for git stubs.

use crate::{MaterializeError, Vcs};
use atomicwrites::AtomicFile;
use camino::{Utf8Path, Utf8PathBuf};
use fs_err as fs;
use git_stub::GitStub;
use std::io::Write;

/// Materializes git stubs into actual file content.
///
/// Reads `.gitstub` files, fetches the referenced content from Git history,
/// and writes the content to an output directory.
#[derive(Debug, Clone)]
pub struct Materializer {
    repo_root: Utf8PathBuf,
    output_dir: Utf8PathBuf,
    emit_cargo_directives: bool,
    vcs: Vcs,
}

impl Materializer {
    /// Creates a new materializer for general use.
    ///
    /// `repo_root` must be the repository root, and it is treated as relative
    /// to the current working directory. (It is also allowed to be absolute.)
    ///
    /// `output_dir` is an output directory relative to the current working
    /// directory. (It is also allowed to be absolute.)
    ///
    /// Returns an error if no VCS (`.git` or `.jj`) is detected at
    /// `repo_root`, or if the repository is a shallow clone.
    pub fn standard(
        repo_root: impl Into<Utf8PathBuf>,
        output_dir: impl Into<Utf8PathBuf>,
    ) -> Result<Self, MaterializeError> {
        let repo_root = repo_root.into();
        let vcs = Vcs::detect(&repo_root)?;
        Self::check_shallow(&vcs, &repo_root)?;
        Ok(Materializer {
            repo_root,
            output_dir: output_dir.into(),
            emit_cargo_directives: false,
            vcs,
        })
    }

    /// Creates a new materializer for use in Cargo build scripts.
    ///
    /// This constructor reads `OUT_DIR` from the environment for the output
    /// directory, and writes files to the `git-stub-vcs` directory
    /// within `OUT_DIR`. It also emits `cargo::rerun-if-changed` directives
    /// for each materialized file.
    ///
    /// `repo_root` is relative to `CARGO_MANIFEST_DIR` (the directory
    /// containing the crate's `Cargo.toml`), and is typically a relative
    /// path.
    ///
    /// # Panics
    ///
    /// Panics if the `OUT_DIR` or `CARGO_MANIFEST_DIR` environment variables
    /// are not set. Both these environment variables are expected to be set
    /// in a Cargo build script context.
    pub fn for_build_script(
        repo_root: impl Into<Utf8PathBuf>,
    ) -> Result<Self, MaterializeError> {
        let out_dir = std::env::var("OUT_DIR").expect(
            "OUT_DIR is set \
             (must be called from a Cargo build script)",
        );
        let out_dir = Utf8PathBuf::from(out_dir).join("git-stub-vcs");

        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect(
            "CARGO_MANIFEST_DIR is set \
                 (must be called from a Cargo build script)",
        );
        let manifest_dir = Utf8PathBuf::from(manifest_dir);
        let repo_root = manifest_dir.join(repo_root.into());

        let vcs = Vcs::detect(&repo_root)?;
        Self::check_shallow(&vcs, &repo_root)?;
        Ok(Materializer {
            repo_root,
            output_dir: out_dir,
            emit_cargo_directives: true,
            vcs,
        })
    }

    /// Overrides the detected VCS.
    ///
    /// Use this when you want to force a specific VCS instead of relying on
    /// automatic detection.
    ///
    /// Returns an error if shallow-clone checking fails, or if the repository
    /// is shallow under the new VCS.
    pub fn with_vcs(mut self, vcs: Vcs) -> Result<Self, MaterializeError> {
        Self::check_shallow(&vcs, &self.repo_root)?;
        self.vcs = vcs;
        Ok(self)
    }

    /// Returns the VCS that will be used for materialization.
    pub fn vcs(&self) -> &Vcs {
        &self.vcs
    }

    /// Checks whether the repository is a shallow clone and returns an
    /// error if so. Called once at construction time rather than on every
    /// `materialize()` call.
    fn check_shallow(
        vcs: &Vcs,
        repo_root: &Utf8Path,
    ) -> Result<(), MaterializeError> {
        if vcs.is_shallow_clone(repo_root).map_err(|error| {
            MaterializeError::ShallowCloneCheck {
                repo_root: repo_root.to_owned(),
                error,
            }
        })? {
            return Err(MaterializeError::ShallowClone {
                repo_root: repo_root.to_owned(),
            });
        }
        Ok(())
    }

    /// Materializes a git stub.
    ///
    /// Reads the file at `git_stub_path` (relative to the repository root),
    /// fetches the referenced content from history, and writes it to the
    /// output directory.
    ///
    /// Returns the path to the materialized file.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // In build.rs:
    /// let materializer = git_stub_vcs::Materializer::for_build_script("../..")
    ///     .expect("VCS detected at repo root");
    /// let spec_path = materializer
    ///     .materialize("openapi/my-api/my-api-1.0.0-abc123.json.gitstub")
    ///     .expect("materialized successfully");
    /// ```
    pub fn materialize(
        &self,
        git_stub_path: impl AsRef<Utf8Path>,
    ) -> Result<Utf8PathBuf, MaterializeError> {
        let git_stub_path = git_stub_path.as_ref();

        if git_stub_path.extension() != Some("gitstub") {
            return Err(MaterializeError::NotGitStub {
                path: git_stub_path.to_owned(),
            });
        }

        // Preserve directory structure, stripping only the .gitstub
        // extension.
        let output_path =
            self.output_dir.join(git_stub_path.with_extension(""));
        self.materialize_inner(git_stub_path, &output_path)?;
        Ok(output_path)
    }

    /// Materializes a git stub to a specific path.
    ///
    /// Like [`materialize`](Self::materialize), but writes to `output_path`
    /// (relative to the output directory) instead of deriving the path from
    /// the Git stub file name.
    ///
    /// `git_stub_path` is relative to the repository root.
    ///
    /// # Path handling
    ///
    /// `output_path` is joined to the output directory. If `output_path`
    /// is absolute, it replaces the output directory entirely (this is
    /// standard [`Utf8PathBuf::join`] behavior). Callers should ensure
    /// `output_path` is relative to avoid writing outside the output
    /// directory.
    pub fn materialize_to(
        &self,
        git_stub_path: impl AsRef<Utf8Path>,
        output_path: impl AsRef<Utf8Path>,
    ) -> Result<(), MaterializeError> {
        let git_stub_path = git_stub_path.as_ref();

        if git_stub_path.extension() != Some("gitstub") {
            return Err(MaterializeError::NotGitStub {
                path: git_stub_path.to_owned(),
            });
        }

        let output_path = self.output_dir.join(output_path.as_ref());
        self.materialize_inner(git_stub_path, &output_path)
    }

    /// Assumes `git_stub_path` has already been validated to have a
    /// `.gitstub` extension.
    fn materialize_inner(
        &self,
        git_stub_path: &Utf8Path,
        output_path: &Utf8Path,
    ) -> Result<(), MaterializeError> {
        let full_git_stub_path = self.repo_root.join(git_stub_path);

        if self.emit_cargo_directives {
            println!("cargo::rerun-if-changed={}", full_git_stub_path);
        }

        let git_stub_contents = fs::read_to_string(&full_git_stub_path)
            .map_err(|error| MaterializeError::ReadGitStub {
                path: full_git_stub_path.clone(),
                error,
            })?;

        let git_stub: GitStub = git_stub_contents.parse().map_err(|error| {
            MaterializeError::InvalidGitStub { path: full_git_stub_path, error }
        })?;

        let content =
            self.vcs.read_git_stub_contents(&git_stub, &self.repo_root)?;

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                MaterializeError::CreateDir { path: parent.to_owned(), error }
            })?;
        }

        AtomicFile::new(
            output_path,
            atomicwrites::OverwriteBehavior::AllowOverwrite,
        )
        .write(|f| f.write_all(&content))
        .map_err(|error| {
            use crate::errors::AtomicWriteError;
            let error = match error {
                atomicwrites::Error::Internal(e) => AtomicWriteError::Rename(e),
                atomicwrites::Error::User(e) => AtomicWriteError::Write(e),
            };
            MaterializeError::WriteOutput {
                path: output_path.to_owned(),
                error,
            }
        })?;

        Ok(())
    }
}

// Tests are in tests/integration/materialize.rs.
