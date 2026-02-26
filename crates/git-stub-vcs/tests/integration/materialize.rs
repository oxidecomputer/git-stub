// Copyright 2026 Oxide Computer Company

//! Integration tests for git-stub-vcs.

use anyhow::Result;
use atomicwrites::{AtomicFile, OverwriteBehavior};
use camino::Utf8Path;
use camino_tempfile::Utf8TempDir;
use git_stub::GitStub;
use git_stub_vcs::{
    MaterializeError, Materializer, ReadContentsError, Vcs, VcsName,
};
use std::{fs, io::Write, process::Command};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Returns a `Command` for git, respecting the `$GIT` environment variable.
fn git_command() -> Command {
    let bin = std::env::var("GIT").unwrap_or_else(|_| "git".to_string());
    Command::new(bin)
}

/// Returns a `Command` for jj, respecting the `$JJ` environment variable.
fn jj_command() -> Command {
    let bin = std::env::var("JJ").unwrap_or_else(|_| "jj".to_string());
    Command::new(bin)
}

/// Writes content to a file atomically.
fn write_file(
    path: impl AsRef<Utf8Path>,
    content: impl AsRef<[u8]>,
) -> std::io::Result<()> {
    let path = path.as_ref();
    AtomicFile::new(path, OverwriteBehavior::AllowOverwrite)
        .write(|f| f.write_all(content.as_ref()))
        .map_err(|e| e.into())
}

/// Returns `Ok(true)` if jj is available, `Ok(false)` if `SKIP_JJ_TESTS`
/// is set, or `Err` if jj is not found.
fn check_jj_available() -> Result<bool> {
    if std::env::var("SKIP_JJ_TESTS").is_ok() {
        return Ok(false);
    }

    match jj_command().arg("--version").output() {
        Ok(o) if o.status.success() => Ok(true),
        Ok(o) => Err(anyhow::anyhow!(
            "jj --version failed ({}): {}. \
             Set SKIP_JJ_TESTS=1 to skip these tests",
            o.status,
            String::from_utf8_lossy(&o.stderr).trim(),
        )),
        Err(e) => Err(anyhow::anyhow!(
            "jj not found ({e}). Install jj \
             (https://jj-vcs.dev/) or set SKIP_JJ_TESTS=1 to \
             skip these tests"
        )),
    }
}

// ---------------------------------------------------------------------------
// Repository setup helpers
// ---------------------------------------------------------------------------

/// Initializes a git repository and configures the user.
fn init_git_repo(repo_root: &Utf8Path) -> Result<()> {
    let status =
        git_command().args(["init"]).current_dir(repo_root).status()?;
    assert!(status.success(), "git init failed");

    let status = git_command()
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_root)
        .status()?;
    assert!(status.success(), "git config user.email failed");

    let status = git_command()
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_root)
        .status()?;
    assert!(status.success(), "git config user.name failed");

    Ok(())
}

/// Creates a JSON file and commits it via git.
/// Returns the commit hash.
fn commit_json_via_git(repo_root: &Utf8Path, contents: &str) -> Result<String> {
    let json_path = repo_root.join("openapi").join("api.json");
    fs::create_dir_all(json_path.parent().unwrap())?;
    write_file(&json_path, contents)?;

    let status =
        git_command().args(["add", "."]).current_dir(repo_root).status()?;
    assert!(status.success(), "git add failed");

    let status = git_command()
        .args(["commit", "-m", "Add API spec"])
        .current_dir(repo_root)
        .status()?;
    assert!(status.success(), "git commit failed");

    let output = git_command()
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()?;
    assert!(
        output.status.success(),
        "git rev-parse HEAD failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Creates a JSON file and commits it via jj.
/// Returns the commit hash.
fn commit_json_via_jj(repo_root: &Utf8Path, contents: &str) -> Result<String> {
    let json_path = repo_root.join("openapi").join("api.json");
    fs::create_dir_all(json_path.parent().unwrap())?;
    write_file(&json_path, contents)?;

    let status = jj_command()
        .args(["commit", "-m", "Add API spec"])
        .current_dir(repo_root)
        .status()?;
    assert!(status.success(), "jj commit failed");

    let output = jj_command()
        .args(["log", "-r", "@-", "--no-graph", "-T", "commit_id"])
        .current_dir(repo_root)
        .output()?;
    assert!(
        output.status.success(),
        "jj log failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Sets up a temporary git repository with a committed JSON file.
/// Returns (temp_dir, commit_hash).
fn setup_git_repo() -> Result<(Utf8TempDir, String)> {
    let temp = Utf8TempDir::with_prefix("git-stub-materialize-")?;
    let repo_root = temp.path();

    init_git_repo(repo_root)?;
    let commit_hash = commit_json_via_git(
        repo_root,
        r#"{"name": "test-api", "version": "1.0.0"}"#,
    )?;

    Ok((temp, commit_hash))
}

/// Sets up a temporary jj-colocated repository with a committed JSON
/// file. Returns (temp_dir, commit_hash).
fn setup_jj_colocated_repo() -> Result<(Utf8TempDir, String)> {
    let temp = Utf8TempDir::with_prefix("git-stub-materialize-jj-")?;
    let repo_root = temp.path();

    let status = jj_command()
        .args(["git", "init", "--colocate"])
        .current_dir(repo_root)
        .status()?;
    assert!(status.success(), "jj git init --colocate failed");

    let commit_hash = commit_json_via_jj(
        repo_root,
        r#"{"name": "test-api", "version": "2.0.0"}"#,
    )?;

    Ok((temp, commit_hash))
}

/// Sets up a non-colocated jj repo (`.jj` but no `.git`).
fn setup_jj_non_colocated_repo() -> Result<(Utf8TempDir, String)> {
    let temp = Utf8TempDir::with_prefix("git-stub-materialize-jj-noncoloc-")?;
    let repo_root = temp.path();

    let status = jj_command()
        .args(["git", "init", "--no-colocate"])
        .current_dir(repo_root)
        .status()?;
    assert!(status.success(), "jj git init --no-colocate failed");

    let commit_hash = commit_json_via_jj(
        repo_root,
        r#"{"name": "test-api", "version": "3.0.0"}"#,
    )?;

    Ok((temp, commit_hash))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_materialize_git_stub() -> Result<()> {
    let (temp, commit_hash) = setup_git_repo()?;
    let repo_root = temp.path();

    let git_stub_content = format!("{}:openapi/api.json\n", commit_hash);
    let git_stub_path = repo_root.join("openapi").join("api.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = repo_root.join("out");
    fs::create_dir_all(&output_dir)?;

    let materializer = Materializer::standard(repo_root, &output_dir)?;
    let result = materializer.materialize("openapi/api.json.gitstub")?;

    assert!(result.exists(), "materialized file should exist");
    let materialized_content = fs::read_to_string(&result)?;
    assert_eq!(
        materialized_content, r#"{"name": "test-api", "version": "1.0.0"}"#,
        "materialized content should match original"
    );

    assert_eq!(
        result.file_name().unwrap(),
        "api.json",
        "filename should match original (without .gitstub extension)"
    );

    Ok(())
}

#[test]
fn test_materialize_missing_git_stub() -> Result<()> {
    let (temp, _) = setup_git_repo()?;

    let materializer = Materializer::standard(temp.path(), temp.path())?;
    let result = materializer.materialize("nonexistent.json.gitstub");

    assert!(
        matches!(result, Err(MaterializeError::ReadGitStub { .. })),
        "should fail with ReadGitStub error"
    );

    Ok(())
}

#[test]
fn test_materialize_invalid_git_stub() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let git_stub_path = temp.path().join("invalid.json.gitstub");
    write_file(&git_stub_path, "not a valid gitstub\n")?;

    let materializer = Materializer::standard(temp.path(), temp.path())?;
    let result = materializer.materialize("invalid.json.gitstub");

    assert!(
        matches!(result, Err(MaterializeError::InvalidGitStub { .. })),
        "should fail with InvalidGitStub error"
    );

    Ok(())
}

#[test]
fn test_vcs_detection_git_only() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let materializer = Materializer::standard(temp.path(), temp.path())?;
    assert_eq!(materializer.vcs().name(), VcsName::Git, "should detect git");

    Ok(())
}

#[test]
fn test_vcs_detection_with_jj_dir() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    // Initialize a real colocated jj repo so `.jj` is valid.
    let status = jj_command()
        .args(["git", "init", "--git-repo", ".", "."])
        .current_dir(temp.path())
        .status()?;
    assert!(status.success(), "jj git init --git-repo . . failed");

    let materializer = Materializer::standard(temp.path(), temp.path())?;
    assert_eq!(
        materializer.vcs().name(),
        VcsName::Jj,
        "should detect jj when .jj exists"
    );

    Ok(())
}

#[test]
fn test_vcs_detection_no_repo() -> Result<()> {
    let temp = Utf8TempDir::with_prefix("git-stub-materialize-")?;
    // No .git or .jj directory.

    let result = Materializer::standard(temp.path(), temp.path());
    assert!(
        matches!(result, Err(MaterializeError::VcsDetect(_))),
        "should fail with VcsDetect error when no repo exists"
    );

    Ok(())
}

#[test]
fn test_with_vcs_override() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let materializer = Materializer::standard(temp.path(), temp.path())?;
    let result = materializer.with_vcs(Vcs::jj()?);
    assert!(
        matches!(result, Err(MaterializeError::ShallowCloneCheck { .. })),
        "forcing jj in a non-jj repo should fail shallow-check setup"
    );

    Ok(())
}

/// Test materialization in jj colocated mode.
#[test]
fn test_materialize_git_stub_with_jj_colocated() -> Result<()> {
    if !check_jj_available()? {
        eprintln!("jj tests skipped (SKIP_JJ_TESTS set)");
        return Ok(());
    }

    let (temp, commit_hash) = setup_jj_colocated_repo()?;
    let repo_root = temp.path();

    let git_stub_content = format!("{}:openapi/api.json\n", commit_hash);
    let git_stub_path = repo_root.join("openapi").join("api.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = repo_root.join("out");
    fs::create_dir_all(&output_dir)?;

    let materializer = Materializer::standard(repo_root, &output_dir)?;
    assert_eq!(materializer.vcs().name(), VcsName::Jj, "should detect jj");

    let result = materializer.materialize("openapi/api.json.gitstub")?;

    assert!(result.exists(), "materialized file should exist");
    let materialized_content = fs::read_to_string(&result)?;
    assert_eq!(
        materialized_content, r#"{"name": "test-api", "version": "2.0.0"}"#,
        "materialized content should match original"
    );

    Ok(())
}

/// Test that forcing git works in colocated mode (git is available).
#[test]
fn test_vcs_override_git_in_jj_colocated() -> Result<()> {
    if !check_jj_available()? {
        eprintln!("jj tests skipped (SKIP_JJ_TESTS set)");
        return Ok(());
    }

    let (temp, commit_hash) = setup_jj_colocated_repo()?;
    let repo_root = temp.path();

    let git_stub_content = format!("{}:openapi/api.json\n", commit_hash);
    let git_stub_path = repo_root.join("openapi").join("api.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = repo_root.join("out");
    fs::create_dir_all(&output_dir)?;

    // Force using git — this works in colocated mode because .git
    // exists.
    let materializer =
        Materializer::standard(repo_root, &output_dir)?.with_vcs(Vcs::git()?)?;
    assert!(
        materializer.vcs().name() == VcsName::Git,
        "with_vcs should override jj detection"
    );

    let result = materializer.materialize("openapi/api.json.gitstub")?;
    assert!(
        result.exists(),
        "git materialization in colocated repo should work"
    );

    Ok(())
}

/// Test materialization in jj non-colocated mode (no `.git`, must use
/// jj).
#[test]
fn test_materialize_git_stub_with_jj_non_colocated() -> Result<()> {
    if !check_jj_available()? {
        eprintln!("jj tests skipped (SKIP_JJ_TESTS set)");
        return Ok(());
    }

    let (temp, commit_hash) = setup_jj_non_colocated_repo()?;
    let repo_root = temp.path();

    // Verify there's no .git directory.
    assert!(
        !repo_root.join(".git").exists(),
        "non-colocated repo should not have .git"
    );
    assert!(
        repo_root.join(".jj").exists(),
        "non-colocated repo should have .jj"
    );

    let git_stub_content = format!("{}:openapi/api.json\n", commit_hash);
    let git_stub_path = repo_root.join("openapi").join("api.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = repo_root.join("out");
    fs::create_dir_all(&output_dir)?;

    let materializer = Materializer::standard(repo_root, &output_dir)?;
    assert_eq!(materializer.vcs().name(), VcsName::Jj, "should detect jj");

    let result = materializer.materialize("openapi/api.json.gitstub")?;

    assert!(result.exists(), "materialized file should exist");
    let materialized_content = fs::read_to_string(&result)?;
    assert_eq!(
        materialized_content, r#"{"name": "test-api", "version": "3.0.0"}"#,
        "materialized content should match original"
    );

    Ok(())
}

#[test]
fn test_materialize_to_custom_path() -> Result<()> {
    let (temp, commit_hash) = setup_git_repo()?;
    let repo_root = temp.path();

    let git_stub_content = format!("{}:openapi/api.json\n", commit_hash);
    let git_stub_path = repo_root.join("openapi").join("api.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = repo_root.join("out");
    fs::create_dir_all(&output_dir)?;

    let materializer = Materializer::standard(repo_root, &output_dir)?;

    // Use a nested custom path unrelated to the stub file name.
    materializer
        .materialize_to("openapi/api.json.gitstub", "custom/output.json")?;

    let output_path = output_dir.join("custom").join("output.json");
    assert!(output_path.exists(), "output at custom path should exist");
    let content = fs::read_to_string(&output_path)?;
    assert_eq!(
        content, r#"{"name": "test-api", "version": "1.0.0"}"#,
        "content at custom path should match original"
    );

    Ok(())
}

#[test]
fn test_materialize_rejects_non_gitstub_extension() -> Result<()> {
    let (temp, _) = setup_git_repo()?;

    let materializer = Materializer::standard(temp.path(), temp.path())?;
    let result = materializer.materialize("openapi/api.json");

    assert!(
        matches!(result, Err(MaterializeError::NotGitStub { .. })),
        "should fail with NotGitStub error, got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_materialize_to_rejects_non_gitstub_extension() -> Result<()> {
    let (temp, _) = setup_git_repo()?;

    let materializer = Materializer::standard(temp.path(), temp.path())?;
    let result = materializer.materialize_to("openapi/api.json", "output.json");

    assert!(
        matches!(result, Err(MaterializeError::NotGitStub { .. })),
        "should fail with NotGitStub error, got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_materialize_shallow_clone_rejected() -> Result<()> {
    // Create a source repository with a committed file.
    let (source_temp, _commit_hash) = setup_git_repo()?;
    let source_root = source_temp.path();

    // Shallow-clone the source repository.
    let clone_temp = Utf8TempDir::with_prefix("git-stub-materialize-shallow-")?;
    let clone_root = clone_temp.path();

    let status = git_command()
        .args([
            "clone",
            "--depth=1",
            &format!("file://{}", source_root),
            clone_root.as_str(),
        ])
        .status()?;
    assert!(status.success(), "git clone --depth=1 failed");

    let output_dir = clone_root.join("out");
    fs::create_dir_all(&output_dir)?;

    let result = Materializer::standard(clone_root, &output_dir);

    assert!(
        matches!(result, Err(MaterializeError::ShallowClone { .. })),
        "should fail with ShallowClone error, got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_materialize_shallow_jj_clone_rejected() -> Result<()> {
    if !check_jj_available()? {
        eprintln!("jj tests skipped (SKIP_JJ_TESTS set)");
        return Ok(());
    }

    // Use a temp jj config dir in tests to avoid depending on host-level
    // config path permissions.
    let jj_config = Utf8TempDir::with_prefix("git-stub-jj-config-")?;
    // SAFETY: nextest runs each test in a separate process.
    // See https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", jj_config.path());
    }

    // Create a source repository with two commits.
    let (source_temp, old_commit_hash) = setup_git_repo()?;
    let source_root = source_temp.path();
    let _new_commit_hash = commit_json_via_git(
        source_root,
        r#"{"name": "test-api", "version": "1.1.0"}"#,
    )?;

    // Shallow-clone only the latest commit.
    let clone_temp =
        Utf8TempDir::with_prefix("git-stub-materialize-jj-shallow-")?;
    let clone_root = clone_temp.path();

    let status = git_command()
        .args([
            "clone",
            "--depth=1",
            &format!("file://{}", source_root),
            clone_root.as_str(),
        ])
        .status()?;
    assert!(status.success(), "git clone --depth=1 failed");

    // Initialize jj in the shallow clone so VCS detection prefers jj.
    let status = jj_command()
        .args(["git", "init", "--git-repo", ".", "."])
        .current_dir(clone_root)
        .status()?;
    assert!(status.success(), "jj git init --git-repo . . failed");

    let git_stub_content = format!("{old_commit_hash}:openapi/api.json\n");
    let git_stub_path = clone_root.join("openapi").join("api.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = clone_root.join("out");
    fs::create_dir_all(&output_dir)?;

    let result = Materializer::standard(clone_root, &output_dir);
    assert!(
        matches!(result, Err(MaterializeError::ShallowClone { .. })),
        "jj-backed shallow clone should be rejected at construction, got: {result:?}"
    );

    // SAFETY: nextest runs each test in a separate process.
    unsafe {
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    Ok(())
}

#[test]
fn test_materialize_idempotent() -> Result<()> {
    let (temp, commit_hash) = setup_git_repo()?;
    let repo_root = temp.path();

    let git_stub_content = format!("{}:openapi/api.json\n", commit_hash);
    let git_stub_path = repo_root.join("openapi").join("api.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = repo_root.join("out");
    fs::create_dir_all(&output_dir)?;

    let materializer = Materializer::standard(repo_root, &output_dir)?;

    let first = materializer.materialize("openapi/api.json.gitstub")?;
    let second = materializer.materialize("openapi/api.json.gitstub")?;

    assert_eq!(first, second, "both calls should return the same path");

    let content = fs::read_to_string(&second)?;
    assert_eq!(
        content, r#"{"name": "test-api", "version": "1.0.0"}"#,
        "content should be correct after second materialization"
    );

    Ok(())
}

#[test]
fn test_materialize_git_stub_with_jj_dash_prefixed_path() -> Result<()> {
    if !check_jj_available()? {
        eprintln!("jj tests skipped (SKIP_JJ_TESTS set)");
        return Ok(());
    }

    let (temp, _) = setup_jj_colocated_repo()?;
    let repo_root = temp.path();

    let dash_path = repo_root.join("-dash.json");
    write_file(&dash_path, r#"{"name": "dash-file", "version": "1.0.0"}"#)?;

    let status = jj_command()
        .args(["commit", "-m", "Add dash-prefixed file"])
        .current_dir(repo_root)
        .status()?;
    assert!(status.success(), "jj commit failed");

    let output = jj_command()
        .args(["log", "-r", "@-", "--no-graph", "-T", "commit_id"])
        .current_dir(repo_root)
        .output()?;
    assert!(
        output.status.success(),
        "jj log failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let commit_hash = String::from_utf8(output.stdout)?.trim().to_string();

    let git_stub_content = format!("{commit_hash}:-dash.json\n");
    let git_stub_path = repo_root.join("dash.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = repo_root.join("out");
    fs::create_dir_all(&output_dir)?;

    let materializer = Materializer::standard(repo_root, &output_dir)?;
    assert_eq!(materializer.vcs().name(), VcsName::Jj, "should detect jj");

    let result = materializer.materialize("dash.json.gitstub")?;
    assert!(result.exists(), "materialized file should exist");

    let materialized_content = fs::read_to_string(&result)?;
    assert_eq!(
        materialized_content, r#"{"name": "dash-file", "version": "1.0.0"}"#,
        "materialized content should match original"
    );

    Ok(())
}

/// Test that jj non-colocated repos don't trigger shallow clone
/// detection.
#[test]
fn test_jj_non_colocated_not_shallow() -> Result<()> {
    if !check_jj_available()? {
        eprintln!("jj tests skipped (SKIP_JJ_TESTS set)");
        return Ok(());
    }

    let (temp, commit_hash) = setup_jj_non_colocated_repo()?;
    let repo_root = temp.path();

    let git_stub_content = format!("{}:openapi/api.json\n", commit_hash);
    let git_stub_path = repo_root.join("openapi").join("api.json.gitstub");
    write_file(&git_stub_path, &git_stub_content)?;

    let output_dir = repo_root.join("out");
    fs::create_dir_all(&output_dir)?;

    // This should succeed for this non-shallow jj repo.
    let materializer = Materializer::standard(repo_root, &output_dir)?;
    let result = materializer.materialize("openapi/api.json.gitstub");
    assert!(
        result.is_ok(),
        "jj materialization should not fail with shallow clone error"
    );

    Ok(())
}

#[test]
fn test_read_contents_nonexistent_commit() -> Result<()> {
    let (temp, _commit_hash) = setup_git_repo()?;
    let repo_root = temp.path();

    // Construct a stub with a valid-format but nonexistent commit hash.
    let fake_hash = "dead".repeat(10);
    let stub: GitStub = format!("{fake_hash}:openapi/api.json").parse()?;

    let vcs = Vcs::git()?;
    let result = vcs.read_git_stub_contents(&stub, repo_root);
    assert!(
        matches!(result, Err(ReadContentsError::VcsFailed { .. })),
        "nonexistent commit should produce VcsFailed, got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_read_contents_nonexistent_path() -> Result<()> {
    let (temp, commit_hash) = setup_git_repo()?;
    let repo_root = temp.path();

    // Valid commit but nonexistent path.
    let stub: GitStub =
        format!("{commit_hash}:nonexistent/file.json").parse()?;

    let vcs = Vcs::git()?;
    let result = vcs.read_git_stub_contents(&stub, repo_root);
    assert!(
        matches!(result, Err(ReadContentsError::VcsFailed { .. })),
        "nonexistent path should produce VcsFailed, \
         got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_read_contents_spawn_failure() -> Result<()> {
    let (temp, commit_hash) = setup_git_repo()?;
    let repo_root = temp.path();

    let stub: GitStub = format!("{commit_hash}:openapi/api.json").parse()?;

    // Construct a Vcs with a nonexistent binary.
    // SAFETY: nextest runs each test in a separate process.
    // See https://nexte.st/docs/configuration/env-vars/#altering-the-environment-within-tests
    unsafe {
        std::env::set_var("GIT", "/nonexistent/git-binary");
    }
    let vcs = Vcs::git()?;
    unsafe {
        std::env::remove_var("GIT");
    }

    let result = vcs.read_git_stub_contents(&stub, repo_root);
    assert!(
        matches!(result, Err(ReadContentsError::SpawnFailed { .. })),
        "nonexistent binary should produce SpawnFailed, \
         got: {result:?}"
    );

    Ok(())
}

// --- git_stub_path validation tests ---

#[test]
fn test_materialize_rejects_absolute_git_stub_path() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let materializer = Materializer::standard(temp.path(), temp.path())?;

    let result = materializer.materialize("/etc/api.json.gitstub");
    assert!(
        matches!(result, Err(MaterializeError::InvalidPathComponent { .. })),
        "should reject absolute git_stub_path, got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_materialize_rejects_parent_dir_git_stub_path() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let materializer = Materializer::standard(temp.path(), temp.path())?;

    let result = materializer.materialize("../escape/api.json.gitstub");
    assert!(
        matches!(result, Err(MaterializeError::InvalidPathComponent { .. })),
        "should reject git_stub_path with .., got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_materialize_rejects_curdir_git_stub_path() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let materializer = Materializer::standard(temp.path(), temp.path())?;

    let result = materializer.materialize("./openapi/api.json.gitstub");
    assert!(
        matches!(result, Err(MaterializeError::InvalidPathComponent { .. })),
        "should reject git_stub_path with ., got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_materialize_to_rejects_absolute_git_stub_path() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let materializer = Materializer::standard(temp.path(), temp.path())?;

    let result =
        materializer.materialize_to("/etc/api.json.gitstub", "output.json");
    assert!(
        matches!(result, Err(MaterializeError::InvalidPathComponent { .. })),
        "should reject absolute git_stub_path, got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_materialize_to_rejects_parent_dir_git_stub_path() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let materializer = Materializer::standard(temp.path(), temp.path())?;

    let result =
        materializer.materialize_to("../../api.json.gitstub", "output.json");
    assert!(
        matches!(result, Err(MaterializeError::InvalidPathComponent { .. })),
        "should reject git_stub_path with .., got: {result:?}"
    );

    Ok(())
}

#[test]
fn test_materialize_rejects_embedded_parent_dir() -> Result<()> {
    let (temp, _) = setup_git_repo()?;
    let materializer = Materializer::standard(temp.path(), temp.path())?;

    let result =
        materializer.materialize("openapi/../../escape/api.json.gitstub");
    assert!(
        matches!(result, Err(MaterializeError::InvalidPathComponent { .. })),
        "should reject git_stub_path with embedded .., \
         got: {result:?}"
    );

    Ok(())
}
