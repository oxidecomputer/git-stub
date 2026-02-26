# AGENTS.md

This file provides guidance to LLMs when working with code in this repository.

## Build and test commands

```shell
cargo build --workspace
cargo nextest run                          # all tests
cargo clippy --workspace --all-targets     # lint
cargo xfmt                                 # check formatting (max_width=80, edition 2024)
just powerset                              # test all feature combinations via cargo-hack
SKIP_JJ_TESTS=1 cargo nextest run          # skip jj integration tests if jj not installed
```

## Architecture

Two-crate workspace:

- **`git-stub`** (1.x) — stable parsing library. Pure types (`GitStub`, `GitCommitHash`) with no process dependencies. All error types are `#[non_exhaustive]` enums via `thiserror`.

- **`git-stub-vcs`** (0.x) — VCS abstraction and materialization. `Vcs` is a Git/Jujutsu abstraction that shells out to read file contents from history. `Materializer` reads a `.gitstub` file, calls `Vcs::read_git_stub_contents`, and atomically writes the result. Has two construction modes: `standard()` for general use and `for_build_script()` which reads `OUT_DIR`/`CARGO_MANIFEST_DIR` and emits `cargo::rerun-if-changed`. Rejects shallow clones at construction time.

## Key conventions

- MSRV is 1.85 (Rust edition 2024).
- `camino::Utf8Path`/`Utf8PathBuf` everywhere instead of `std::path`.
- `fs-err` instead of `std::fs` in the VCS crate.
- Both crates enforce `#![deny(missing_docs)]`.
- VCS binary paths are configurable via `$GIT` and `$JJ` environment variables.
- Integration tests in `git-stub-vcs` create real temporary git/jj repos.
- Each crate has independent versioning and changelogs; READMEs are generated from rustdoc via `cargo-sync-rdme`.
