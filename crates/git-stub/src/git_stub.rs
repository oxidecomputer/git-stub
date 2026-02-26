// Copyright 2026 Oxide Computer Company

//! Git stub types and operations.

use crate::{GitCommitHash, GitStubParseError};
use camino::{Utf8Component, Utf8Path, Utf8PathBuf};
use std::{fmt, str::FromStr};

/// Represents a git stub: a reference to a file at a specific commit.
///
/// A git stub is stored as a string in the format `commit:path`, and can be
/// used to retrieve file contents via `git cat-file blob commit:path`.
///
/// Construct via [`FromStr`] (parsing) or [`GitStub::new`].
///
/// # Invariants
///
/// - The path is non-empty.
/// - The path uses forward slashes (backslashes are normalized on
///   construction).
/// - Every path component is a normal file or directory name (no `..`,
///   `.`, root `/`, or Windows prefixes).
///
/// # Examples
///
/// ```
/// use git_stub::GitStub;
///
/// let git_stub: GitStub =
///     "0123456789abcdef0123456789abcdef01234567:openapi/api.json"
///         .parse()
///         .unwrap();
///
/// assert_eq!(git_stub.path().as_str(), "openapi/api.json");
/// ```
#[derive(Clone, Debug)]
pub struct GitStub {
    commit: GitCommitHash,
    path: Utf8PathBuf,
    /// Whether the input used to construct this `GitStub` was not in canonical
    /// form (e.g., had backslashes, extra whitespace, or a missing trailing
    /// newline).
    needs_rewrite: bool,
}

impl PartialEq for GitStub {
    fn eq(&self, other: &Self) -> bool {
        self.commit == other.commit && self.path == other.path
    }
}

impl Eq for GitStub {}

// Hash must be consistent with the custom PartialEq above: exclude
// `needs_rewrite` so that two stubs with the same commit:path hash
// identically regardless of how they were parsed.
impl core::hash::Hash for GitStub {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.commit.hash(state);
        self.path.hash(state);
    }
}

impl GitStub {
    /// Creates a new `GitStub` with the given commit hash and path.
    ///
    /// The path is normalized: backslashes are converted to forward slashes.
    ///
    /// Returns an error if:
    /// - The path is empty.
    /// - The path contains a newline character.
    /// - Any path component is not a normal file or directory name (e.g.,
    ///   `..`, `.`, root `/`, or a Windows prefix).
    pub fn new(
        commit: GitCommitHash,
        path: Utf8PathBuf,
    ) -> Result<Self, GitStubParseError> {
        let raw = path.as_str();
        let needs_rewrite = raw.contains('\\');
        let normalized = raw.replace('\\', "/");
        if normalized.is_empty() {
            return Err(GitStubParseError::EmptyPath);
        }
        if normalized.contains('\n') {
            return Err(GitStubParseError::NewlineInPath);
        }
        let path = Utf8PathBuf::from(normalized);

        // Reject paths that contain anything other than plain file/directory
        // names. This prevents path traversal (e.g., `../escape`) and
        // absolute paths (e.g., `/etc/passwd`).
        if let Some(component) = find_non_normal_component(&path) {
            return Err(GitStubParseError::InvalidPathComponent {
                path,
                component,
            });
        }

        Ok(GitStub { commit, path, needs_rewrite })
    }

    /// Returns the commit hash.
    pub fn commit(&self) -> GitCommitHash {
        self.commit
    }

    /// Returns the path within the repository.
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    /// Returns the canonical file contents for this git stub.
    ///
    /// The canonical format is `commit:path\n` where:
    /// - The path uses forward slashes (even on Windows).
    /// - The file ends with a single newline.
    pub fn to_file_contents(&self) -> String {
        format!("{}\n", self)
    }

    /// Returns whether the input used to construct this `GitStub` was not in
    /// canonical form.
    ///
    /// A Git stub needs rewriting if it doesn't match the canonical
    /// format:
    ///
    /// - Missing trailing newline.
    /// - Contains backslashes in the path.
    /// - Has extra whitespace.
    pub fn needs_rewrite(&self) -> bool {
        self.needs_rewrite
    }
}

impl fmt::Display for GitStub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.commit, self.path)
    }
}

impl FromStr for GitStub {
    type Err = GitStubParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Check for non-canonical formatting before trimming. Canonical
        // form is exactly `commit:path\n`: a single trailing newline and no
        // other surrounding whitespace. (Backslash detection is handled
        // separately by `new()`.)
        let needs_rewrite = !s.ends_with('\n') || s.trim().len() + 1 != s.len();

        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(GitStubParseError::EmptyInput);
        }
        let (commit_str, path) = trimmed.split_once(':').ok_or_else(|| {
            GitStubParseError::InvalidFormat(trimmed.to_owned())
        })?;
        let commit: GitCommitHash = commit_str.parse()?;
        // Uppercase hex is accepted by the parser but Display emits
        // lowercase, so the round-trip would differ. Flag it.
        let has_uppercase_hex =
            commit_str.bytes().any(|b| b.is_ascii_uppercase());
        // GitStub::new handles backslash normalization and empty-path
        // rejection.
        let mut stub = GitStub::new(commit, Utf8PathBuf::from(path))?;
        // Merge in the whitespace/newline/case canonicality check with
        // whatever new() detected (e.g., backslashes in path).
        stub.needs_rewrite =
            stub.needs_rewrite || needs_rewrite || has_uppercase_hex;
        Ok(stub)
    }
}

/// Returns the first non-normal component in the path, if any.
///
/// A normal component is a plain file or directory name (not `..`, `.`,
/// root `/`, or a Windows prefix).
fn find_non_normal_component(path: &Utf8Path) -> Option<String> {
    path.components().find_map(|component| match component {
        Utf8Component::Normal(_) => None,
        Utf8Component::Prefix(_)
        | Utf8Component::RootDir
        | Utf8Component::CurDir
        | Utf8Component::ParentDir => Some(component.as_str().to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SHA1: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn test_git_stub_parse() {
        let input = format!("{}:openapi/api/api-1.0.0-def456.json", VALID_SHA1);
        let git_stub = input.parse::<GitStub>().unwrap();
        assert_eq!(git_stub.commit().to_string(), VALID_SHA1);
        assert_eq!(
            git_stub.path().as_str(),
            "openapi/api/api-1.0.0-def456.json"
        );
    }

    #[test]
    fn test_git_stub_parse_with_whitespace() {
        let input = format!("  {}:path/file.json\n", VALID_SHA1);
        let git_stub = input.parse::<GitStub>().unwrap();
        assert_eq!(git_stub.commit().to_string(), VALID_SHA1);
        assert_eq!(git_stub.path().as_str(), "path/file.json");
    }

    #[test]
    fn test_git_stub_parse_invalid_no_colon() {
        let result = "no-colon".parse::<GitStub>();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GitStubParseError::InvalidFormat(_)
        ));
    }

    #[test]
    fn test_git_stub_parse_invalid_empty() {
        let result = "".parse::<GitStub>();
        assert!(result.is_err());
    }

    #[test]
    fn test_git_stub_parse_invalid_commit_hash() {
        // Valid format but invalid commit hash (too short).
        let result = "abc123:path/file.json".parse::<GitStub>();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GitStubParseError::InvalidCommitHash(_)
        ));
    }

    #[test]
    fn test_git_stub_roundtrip() {
        let git_stub = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("path/to/file.json"),
        )
        .unwrap();
        let s = git_stub.to_string();
        let expected = format!("{}:path/to/file.json", VALID_SHA1);
        assert_eq!(s, expected);
        let parsed = s.parse::<GitStub>().unwrap();
        assert_eq!(git_stub, parsed);
    }

    #[test]
    fn test_git_stub_to_file_contents() {
        let git_stub = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("path/to/file.json"),
        )
        .unwrap();
        let contents = git_stub.to_file_contents();
        let expected = format!("{}:path/to/file.json\n", VALID_SHA1);
        assert_eq!(contents, expected, "should have trailing newline");
    }

    #[test]
    fn test_git_stub_new_normalizes_backslashes() {
        // The constructor should normalize backslashes to forward slashes.
        let git_stub = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("path\\to\\file.json"),
        )
        .unwrap();
        assert_eq!(
            git_stub.path().as_str(),
            "path/to/file.json",
            "constructor should normalize backslashes"
        );
        // Display should also reflect the normalization.
        let s = git_stub.to_string();
        assert!(!s.contains('\\'), "display should not contain backslashes");
        assert!(s.contains("path/to/file.json"));
    }

    #[test]
    fn test_git_stub_new_rejects_empty_path() {
        let result =
            GitStub::new(VALID_SHA1.parse().unwrap(), Utf8PathBuf::from(""));
        assert!(
            matches!(result, Err(GitStubParseError::EmptyPath)),
            "should reject empty path"
        );
    }

    #[test]
    fn test_git_stub_parse_normalizes_backslashes() {
        // Parsing should normalize backslashes to forward slashes.
        let input = format!("{}:path\\to\\file.json", VALID_SHA1);
        let git_stub = input.parse::<GitStub>().unwrap();
        assert_eq!(
            git_stub.path().as_str(),
            "path/to/file.json",
            "backslashes should be normalized to forward slashes"
        );
    }

    #[test]
    fn test_git_stub_parse_error_variants() {
        // Empty input.
        let result = "".parse::<GitStub>();
        assert!(matches!(result, Err(GitStubParseError::EmptyInput)));

        // Whitespace-only input.
        let result = "   \n  ".parse::<GitStub>();
        assert!(matches!(result, Err(GitStubParseError::EmptyInput)));

        // Empty path (valid commit hash but nothing after colon).
        let input = format!("{}:", VALID_SHA1);
        let result = input.parse::<GitStub>();
        assert!(matches!(result, Err(GitStubParseError::EmptyPath)));
    }

    #[test]
    fn test_git_stub_needs_rewrite() {
        // Canonical format: forward slashes, single trailing newline.
        let canonical = format!("{}:path/to/file.json\n", VALID_SHA1);
        let stub = canonical.parse::<GitStub>().unwrap();
        assert!(
            !stub.needs_rewrite(),
            "canonical format should not need rewrite"
        );

        // Missing trailing newline.
        let missing_newline = format!("{}:path/to/file.json", VALID_SHA1);
        let stub = missing_newline.parse::<GitStub>().unwrap();
        assert!(
            stub.needs_rewrite(),
            "missing trailing newline should need rewrite"
        );

        // Extra trailing newlines.
        let extra_newlines = format!("{}:path/to/file.json\n\n", VALID_SHA1);
        let stub = extra_newlines.parse::<GitStub>().unwrap();
        assert!(
            stub.needs_rewrite(),
            "extra trailing newlines should need rewrite"
        );

        // Leading whitespace.
        let leading_whitespace =
            format!("  {}:path/to/file.json\n", VALID_SHA1);
        let stub = leading_whitespace.parse::<GitStub>().unwrap();
        assert!(stub.needs_rewrite(), "leading whitespace should need rewrite");

        // Backslashes in path.
        let backslashes = format!("{}:path\\to\\file.json\n", VALID_SHA1);
        let stub = backslashes.parse::<GitStub>().unwrap();
        assert!(
            stub.needs_rewrite(),
            "backslashes in path should need rewrite"
        );

        // CRLF line ending.
        let crlf = format!("{}:path/to/file.json\r\n", VALID_SHA1);
        let stub = crlf.parse::<GitStub>().unwrap();
        assert!(stub.needs_rewrite(), "CRLF should need rewrite");
        assert_eq!(
            stub.path().as_str(),
            "path/to/file.json",
            "CRLF should not leave \\r in the path"
        );
    }

    #[test]
    fn test_git_stub_new_needs_rewrite() {
        // new() with a clean path should not need rewrite.
        let stub = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("path/to/file.json"),
        )
        .unwrap();
        assert!(
            !stub.needs_rewrite(),
            "new() with canonical path should not need rewrite"
        );

        // new() with backslashes should need rewrite.
        let stub = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("path\\to\\file.json"),
        )
        .unwrap();
        assert!(
            stub.needs_rewrite(),
            "new() with backslashes should need rewrite"
        );
    }

    #[test]
    fn test_git_stub_needs_rewrite_uppercase_hex() {
        // Uppercase hex is valid but non-canonical; Display emits
        // lowercase, so the round-trip would differ.
        let upper = "0123456789ABCDEF0123456789ABCDEF01234567";
        let input = format!("{}:path/to/file.json\n", upper);
        let stub = input.parse::<GitStub>().unwrap();
        assert!(
            stub.needs_rewrite(),
            "uppercase hex in commit hash should need rewrite"
        );

        // The canonical form uses lowercase.
        let canonical = stub.to_file_contents();
        assert_ne!(
            canonical, input,
            "canonical output should differ from uppercase input"
        );
        assert_eq!(
            canonical,
            format!("{}:path/to/file.json\n", upper.to_ascii_lowercase()),
        );

        // Lowercase hex should not need rewrite.
        let lower_input = format!("{}:path/to/file.json\n", VALID_SHA1);
        let stub2 = lower_input.parse::<GitStub>().unwrap();
        assert!(
            !stub2.needs_rewrite(),
            "lowercase hex should not need rewrite"
        );
    }

    #[test]
    fn test_git_stub_needs_rewrite_equality() {
        // Two stubs with the same commit:path should be equal even if one
        // needs rewriting.
        let canonical = format!("{}:path/to/file.json\n", VALID_SHA1);
        let non_canonical = format!("  {}:path/to/file.json", VALID_SHA1);
        let a = canonical.parse::<GitStub>().unwrap();
        let b = non_canonical.parse::<GitStub>().unwrap();
        assert!(!a.needs_rewrite());
        assert!(b.needs_rewrite());
        assert_eq!(a, b, "equality should ignore needs_rewrite");
    }

    #[test]
    fn test_git_stub_sha256_roundtrip() {
        let sha256 = "0123456789abcdef0123456789abcdef\
             0123456789abcdef0123456789abcdef";
        let input = format!("{}:openapi/api.json\n", sha256);
        let stub = input.parse::<GitStub>().unwrap();

        assert!(
            matches!(stub.commit(), crate::GitCommitHash::Sha256(_)),
            "64-char hex should parse as SHA-256"
        );
        assert_eq!(stub.path().as_str(), "openapi/api.json");
        assert!(!stub.needs_rewrite());

        // Round-trip through Display and back.
        let reparsed = stub.to_string().parse::<GitStub>().unwrap();
        assert_eq!(stub, reparsed);
    }

    #[test]
    fn test_git_stub_path_containing_colon() {
        // Colons after the first are part of the path. The parser uses
        // split_once(':'), so only the first colon is the separator.
        let input = format!("{}:path/to/file:v2.json\n", VALID_SHA1);
        let stub = input.parse::<GitStub>().unwrap();
        assert_eq!(
            stub.path().as_str(),
            "path/to/file:v2.json",
            "colons after the first should be part of the path"
        );
        assert!(!stub.needs_rewrite());
    }

    #[test]
    fn test_git_stub_hash_consistency_with_eq() {
        use std::collections::HashSet;

        // Two stubs that are equal (same commit:path) but differ in
        // needs_rewrite must produce the same Hash.
        let canonical = format!("{}:path/to/file.json\n", VALID_SHA1);
        let non_canonical = format!("  {}:path/to/file.json", VALID_SHA1);
        let a = canonical.parse::<GitStub>().unwrap();
        let b = non_canonical.parse::<GitStub>().unwrap();
        assert_eq!(a, b);
        assert!(!a.needs_rewrite());
        assert!(b.needs_rewrite());

        let mut set = HashSet::new();
        set.insert(a);
        set.insert(b);
        assert_eq!(set.len(), 1, "equal stubs must hash identically");
    }

    // --- Path component validation tests ---

    #[test]
    fn test_git_stub_rejects_parent_dir() {
        let result = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("../escape/file.json"),
        );
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "should reject path with .. component"
        );
    }

    #[test]
    fn test_git_stub_rejects_current_dir() {
        let result = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("./path/file.json"),
        );
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "should reject path with . component"
        );
    }

    #[test]
    fn test_git_stub_rejects_absolute_path() {
        let result = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("/absolute/path/file.json"),
        );
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "should reject absolute path"
        );
    }

    #[test]
    fn test_git_stub_rejects_embedded_parent_dir() {
        let result = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("path/../../escape/file.json"),
        );
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "should reject path with embedded .. components"
        );
    }

    #[test]
    fn test_git_stub_rejects_dot_only_path() {
        let result =
            GitStub::new(VALID_SHA1.parse().unwrap(), Utf8PathBuf::from("."));
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "should reject path that is just '.'"
        );
    }

    #[test]
    fn test_git_stub_rejects_dotdot_only_path() {
        let result =
            GitStub::new(VALID_SHA1.parse().unwrap(), Utf8PathBuf::from(".."));
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "should reject path that is just '..'"
        );
    }

    #[test]
    fn test_git_stub_rejects_backslash_parent_dir() {
        // After backslash normalization, this becomes "../escape/file.json".
        let result = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("..\\escape\\file.json"),
        );
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "should reject backslash-normalized path with .. component"
        );
    }

    #[test]
    fn test_git_stub_parse_rejects_parent_dir() {
        // Validation also applies when parsing from string.
        let input = format!("{}:../escape/file.json", VALID_SHA1);
        let result = input.parse::<GitStub>();
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "parsing should reject path with .. component"
        );
    }

    #[test]
    fn test_git_stub_parse_rejects_absolute_path() {
        let input = format!("{}:/etc/passwd", VALID_SHA1);
        let result = input.parse::<GitStub>();
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "parsing should reject absolute path"
        );
    }

    #[test]
    fn test_git_stub_parse_rejects_current_dir() {
        let input = format!("{}:./path/file.json", VALID_SHA1);
        let result = input.parse::<GitStub>();
        assert!(
            matches!(
                result,
                Err(GitStubParseError::InvalidPathComponent { .. })
            ),
            "parsing should reject path with . component"
        );
    }

    #[test]
    fn test_git_stub_rejects_newline_in_path() {
        // Multi-line input (e.g., from a merge conflict or
        // accidental concatenation) is rejected because the
        // path would contain a newline.
        let input =
            format!("{}:path/a.json\n{}:path/b.json\n", VALID_SHA1, VALID_SHA1);
        let result = input.parse::<GitStub>();
        assert!(
            matches!(result, Err(GitStubParseError::NewlineInPath)),
            "multi-line input should be rejected"
        );

        // Direct construction also rejects newlines.
        let result = GitStub::new(
            VALID_SHA1.parse().unwrap(),
            Utf8PathBuf::from("path/\n/file.json"),
        );
        assert!(
            matches!(result, Err(GitStubParseError::NewlineInPath)),
            "path with embedded newline should be rejected"
        );
    }
}
