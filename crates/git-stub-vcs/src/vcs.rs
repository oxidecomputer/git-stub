// Copyright 2026 Oxide Computer Company

//! Version control system abstraction for reading file contents from history.

use crate::{
    ReadContentsError, ShallowCloneError, VcsDetectError, VcsEnvError,
};
use camino::Utf8Path;
use fs_err as fs;
use git_stub::GitStub;
use std::{fmt, io, process::Command};

/// Reads a VCS binary path from an environment variable, falling back
/// to `default` if the variable is unset or empty.
///
/// The value is trimmed of leading and trailing whitespace.
///
/// Returns an error if the variable is set but is not valid UTF-8.
fn read_vcs_env(
    var: &'static str,
    default: &str,
) -> Result<String, VcsEnvError> {
    match std::env::var(var) {
        Ok(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(default.to_string())
            } else {
                Ok(trimmed.to_string())
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(default.to_string()),
        Err(std::env::VarError::NotUnicode(value)) => {
            Err(VcsEnvError::NonUtf8 { var, value })
        }
    }
}

/// The name of a version control system.
///
/// Used in error messages and for identifying which VCS is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VcsName {
    /// Git version control.
    Git,
    /// Jujutsu (jj) version control.
    Jj,
}

impl fmt::Display for VcsName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VcsName::Git => write!(f, "git"),
            VcsName::Jj => write!(f, "jj"),
        }
    }
}

/// The version control system used to read file contents from history.
///
/// Supports Git and Jujutsu (jj). Use [`Vcs::git()`], [`Vcs::jj()`], or
/// [`Vcs::detect()`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vcs(VcsKind);

/// The internal representation of a VCS.
#[derive(Debug, Clone, PartialEq, Eq)]
enum VcsKind {
    /// Git version control.
    Git {
        /// Path to the git binary.
        binary: String,
    },
    /// Jujutsu (jj) version control.
    Jj {
        /// Path to the jj binary.
        binary: String,
    },
}

impl Vcs {
    /// Creates a Git VCS using the `$GIT` environment variable or
    /// `"git"`.
    ///
    /// Returns an error if the `$GIT` environment variable is set
    /// but is not valid UTF-8.
    pub fn git() -> Result<Self, VcsEnvError> {
        let binary = read_vcs_env("GIT", "git")?;
        Ok(Vcs(VcsKind::Git { binary }))
    }

    /// Creates a Jujutsu VCS using the `$JJ` environment variable
    /// or `"jj"`.
    ///
    /// Returns an error if the `$JJ` environment variable is set
    /// but is not valid UTF-8.
    pub fn jj() -> Result<Self, VcsEnvError> {
        let binary = read_vcs_env("JJ", "jj")?;
        Ok(Vcs(VcsKind::Jj { binary }))
    }

    /// Detects the appropriate VCS for a repository.
    ///
    /// `repo_root` must be the repository root.
    ///
    /// Detection order:
    /// 1. If a `.jj` path exists, returns jj (including colocated
    ///    mode where both `.jj` and `.git` exist).
    /// 2. If a `.git` path exists, returns git. (`.git` may be a
    ///    directory or a file, as in worktrees and submodules.)
    /// 3. Otherwise, returns an error.
    pub fn detect(repo_root: &Utf8Path) -> Result<Self, VcsDetectError> {
        // Use metadata() to distinguish "not a directory" from I/O
        // errors (e.g., permission denied).
        match fs::metadata(repo_root) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                return Err(VcsDetectError::NotADirectory {
                    repo_root: repo_root.to_owned(),
                });
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(VcsDetectError::PathNotFound {
                    repo_root: repo_root.to_owned(),
                });
            }
            Err(err) => {
                return Err(VcsDetectError::Io {
                    path: repo_root.to_owned(),
                    source: err,
                });
            }
        }

        let jj_path = repo_root.join(".jj");
        match jj_path.try_exists() {
            Ok(true) => return Ok(Self::jj()?),
            Ok(false) => {}
            Err(source) => {
                return Err(VcsDetectError::Io { path: jj_path, source });
            }
        }

        let git_path = repo_root.join(".git");
        match git_path.try_exists() {
            Ok(true) => return Ok(Self::git()?),
            Ok(false) => {}
            Err(source) => {
                return Err(VcsDetectError::Io { path: git_path, source });
            }
        }

        Err(VcsDetectError::NotFound { repo_root: repo_root.to_owned() })
    }

    /// Returns the path to the VCS binary.
    pub fn binary(&self) -> &str {
        match &self.0 {
            VcsKind::Git { binary } | VcsKind::Jj { binary } => binary,
        }
    }

    /// Returns the name of the VCS.
    pub fn name(&self) -> VcsName {
        match &self.0 {
            VcsKind::Git { .. } => VcsName::Git,
            VcsKind::Jj { .. } => VcsName::Jj,
        }
    }

    /// Checks if the repository at `repo_root` is a shallow clone.
    ///
    /// For Git, runs `git rev-parse --is-shallow-repository`.
    /// For Jujutsu, resolves the underlying Git store using
    /// `jj git root --ignore-working-copy` and checks for a `shallow`
    /// marker file there.
    pub fn is_shallow_clone(
        &self,
        repo_root: &Utf8Path,
    ) -> Result<bool, ShallowCloneError> {
        match &self.0 {
            VcsKind::Git { binary } => {
                let output = Command::new(binary)
                    .current_dir(repo_root)
                    .args(["rev-parse", "--is-shallow-repository"])
                    .output()
                    .map_err(|source| ShallowCloneError::SpawnFailed {
                        vcs_name: VcsName::Git,
                        binary_path: binary.clone(),
                        repo_root: repo_root.to_owned(),
                        source,
                    })?;

                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    match stdout.trim() {
                        "true" => Ok(true),
                        "false" => Ok(false),
                        other => Err(ShallowCloneError::UnexpectedOutput {
                            vcs_name: VcsName::Git,
                            stdout: other.to_owned(),
                        }),
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(ShallowCloneError::VcsFailed {
                        vcs_name: VcsName::Git,
                        exit_status: output.status.to_string(),
                        stderr: stderr.trim().to_string(),
                    })
                }
            }
            VcsKind::Jj { binary } => {
                let output = Command::new(binary)
                    .current_dir(repo_root)
                    .args(["git", "root", "--ignore-working-copy"])
                    .output()
                    .map_err(|source| ShallowCloneError::SpawnFailed {
                        vcs_name: VcsName::Jj,
                        binary_path: binary.clone(),
                        repo_root: repo_root.to_owned(),
                        source,
                    })?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(ShallowCloneError::VcsFailed {
                        vcs_name: VcsName::Jj,
                        exit_status: output.status.to_string(),
                        stderr: stderr.trim().to_string(),
                    });
                }

                let git_root = String::from_utf8_lossy(&output.stdout);
                let git_root = git_root.trim();
                if git_root.is_empty() {
                    return Err(ShallowCloneError::UnexpectedOutput {
                        vcs_name: VcsName::Jj,
                        stdout: git_root.to_string(),
                    });
                }

                let shallow_path =
                    camino::Utf8PathBuf::from(git_root).join("shallow");
                shallow_path.try_exists().map_err(|source| {
                    ShallowCloneError::Io { path: shallow_path.clone(), source }
                })
            }
        }
    }

    /// Reads the contents of the file referenced by a git stub.
    ///
    /// For Git, runs `git cat-file blob <commit>:<path>`.
    /// For Jujutsu, runs `jj file show --revision <commit> <path>`.
    pub fn read_git_stub_contents(
        &self,
        stub: &GitStub,
        repo_root: &Utf8Path,
    ) -> Result<Vec<u8>, ReadContentsError> {
        let vcs_name = self.name();
        let binary_path = self.binary().to_string();

        let mut cmd = Command::new(self.binary());
        cmd.current_dir(repo_root);

        match &self.0 {
            VcsKind::Git { .. } => {
                // git cat-file blob <commit>:<path>
                cmd.args(["cat-file", "blob"]).arg(stub.to_string());
            }
            VcsKind::Jj { .. } => {
                // Skip the working-copy snapshot: this is a read-only
                // operation and snapshotting can modify repo state or
                // slow things down in build scripts.
                cmd.args([
                    "file",
                    "show",
                    "--ignore-working-copy",
                    "--revision",
                    &stub.commit().to_string(),
                ]);
                // `--` is required so filenames beginning with `-` are
                // treated as paths rather than options.
                cmd.arg("--").arg(stub.path().as_str());
            }
        }

        let output =
            cmd.output().map_err(|source| ReadContentsError::SpawnFailed {
                vcs_name,
                binary_path,
                repo_root: repo_root.to_owned(),
                source,
            })?;

        if output.status.success() {
            Ok(output.stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ReadContentsError::VcsFailed {
                vcs_name,
                stub: stub.clone(),
                exit_status: output.status.to_string(),
                stderr: stderr.trim().to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Vcs, VcsName};
    use crate::VcsDetectError;
    use camino_tempfile::Utf8TempDir;
    use std::fs;

    #[test]
    fn test_vcs_git_default() {
        // SAFETY:
        // https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
        unsafe {
            std::env::remove_var("GIT");
        }
        let vcs = Vcs::git().unwrap();
        assert_eq!(vcs.name(), VcsName::Git);
        assert_eq!(vcs.binary(), "git");
    }

    #[test]
    fn test_vcs_git_from_env() {
        // SAFETY:
        // https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
        unsafe {
            std::env::set_var("GIT", "/custom/git");
        }
        let vcs = Vcs::git().unwrap();
        // SAFETY:
        // https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
        unsafe {
            std::env::remove_var("GIT");
        }
        assert_eq!(vcs.name(), VcsName::Git);
        assert_eq!(vcs.binary(), "/custom/git");
    }

    #[test]
    fn test_vcs_jj_default() {
        // SAFETY:
        // https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
        unsafe {
            std::env::remove_var("JJ");
        }
        let vcs = Vcs::jj().unwrap();
        assert_eq!(vcs.name(), VcsName::Jj);
        assert_eq!(vcs.binary(), "jj");
    }

    #[test]
    fn test_vcs_jj_from_env() {
        // SAFETY:
        // https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
        unsafe {
            std::env::set_var("JJ", "/custom/jj");
        }
        let vcs = Vcs::jj().unwrap();
        // SAFETY:
        // https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
        unsafe {
            std::env::remove_var("JJ");
        }
        assert_eq!(vcs.name(), VcsName::Jj);
        assert_eq!(vcs.binary(), "/custom/jj");
    }

    #[test]
    fn test_vcs_git_empty_env_falls_back() {
        // SAFETY: nextest runs each test in a separate process, so
        // no other threads are reading the environment concurrently.
        // See https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
        unsafe {
            std::env::set_var("GIT", "");
        }
        assert_eq!(Vcs::git().unwrap().binary(), "git", "empty string");
        unsafe {
            std::env::set_var("GIT", "   ");
        }
        assert_eq!(Vcs::git().unwrap().binary(), "git", "whitespace only");
        unsafe {
            std::env::remove_var("GIT");
        }
    }

    #[test]
    fn test_vcs_jj_empty_env_falls_back() {
        // SAFETY: nextest runs each test in a separate process, so
        // no other threads are reading the environment concurrently.
        // See https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
        unsafe {
            std::env::set_var("JJ", "");
        }
        assert_eq!(Vcs::jj().unwrap().binary(), "jj", "empty string");
        unsafe {
            std::env::set_var("JJ", "   ");
        }
        assert_eq!(Vcs::jj().unwrap().binary(), "jj", "whitespace only");
        unsafe {
            std::env::remove_var("JJ");
        }
    }

    #[test]
    fn test_vcs_detect_git_only() {
        let temp = Utf8TempDir::with_prefix("git-stub-vcs-").unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();

        let vcs = Vcs::detect(temp.path()).unwrap();
        assert_eq!(vcs.name(), VcsName::Git);
    }

    #[test]
    fn test_vcs_detect_jj_only() {
        let temp = Utf8TempDir::with_prefix("git-stub-vcs-").unwrap();
        fs::create_dir(temp.path().join(".jj")).unwrap();

        let vcs = Vcs::detect(temp.path()).unwrap();
        assert_eq!(vcs.name(), VcsName::Jj);
    }

    #[test]
    fn test_vcs_detect_colocated_prefers_jj() {
        let temp = Utf8TempDir::with_prefix("git-stub-vcs-").unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::create_dir(temp.path().join(".jj")).unwrap();

        let vcs = Vcs::detect(temp.path()).unwrap();
        assert_eq!(vcs.name(), VcsName::Jj, "colocated mode should prefer jj");
    }

    #[test]
    fn test_vcs_detect_neither_returns_error() {
        let temp = Utf8TempDir::with_prefix("git-stub-vcs-").unwrap();
        // No .git or .jj directory.

        let err = Vcs::detect(temp.path()).unwrap_err();
        assert!(
            matches!(err, VcsDetectError::NotFound { .. }),
            "should return NotFound when neither .git nor .jj exists"
        );
    }

    #[test]
    fn test_vcs_detect_not_a_directory() {
        let temp = Utf8TempDir::with_prefix("git-stub-vcs-").unwrap();
        let file_path = temp.path().join("not-a-dir");
        fs::write(&file_path, "").unwrap();

        let err = Vcs::detect(&file_path).unwrap_err();
        assert!(
            matches!(err, VcsDetectError::NotADirectory { .. }),
            "should return NotADirectory for a file path"
        );
    }

    #[test]
    fn test_vcs_detect_nonexistent_path() {
        let temp = Utf8TempDir::with_prefix("git-stub-vcs-").unwrap();
        let gone = temp.path().join("nonexistent");

        let err = Vcs::detect(&gone).unwrap_err();
        assert!(
            matches!(err, VcsDetectError::PathNotFound { .. }),
            "should return PathNotFound for a nonexistent path"
        );
    }

    #[test]
    fn test_vcs_binary() {
        let git = Vcs::git().unwrap();
        assert_eq!(git.name(), VcsName::Git);
        // Binary is "git" by default (unless $GIT is set).

        let jj = Vcs::jj().unwrap();
        assert_eq!(jj.name(), VcsName::Jj);
        // Binary is "jj" by default (unless $JJ is set).
    }

    #[test]
    fn test_vcs_name() {
        let git = Vcs::git().unwrap();
        assert_eq!(git.name(), VcsName::Git);
        assert_eq!(git.name().to_string(), "git");

        let jj = Vcs::jj().unwrap();
        assert_eq!(jj.name(), VcsName::Jj);
        assert_eq!(jj.name().to_string(), "jj");
    }
}
