<!-- cargo-sync-rdme title [[ -->
# git-stub-vcs
<!-- cargo-sync-rdme ]] -->
<!-- cargo-sync-rdme badge [[ -->
![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/git-stub-vcs.svg?)
[![crates.io](https://img.shields.io/crates/v/git-stub-vcs.svg?logo=rust)](https://crates.io/crates/git-stub-vcs)
[![docs.rs](https://img.shields.io/docsrs/git-stub-vcs.svg?logo=docs.rs)](https://docs.rs/git-stub-vcs)
[![Rust: ^1.85.0](https://img.shields.io/badge/rust-^1.85.0-93450a.svg?logo=rust)](https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field)
<!-- cargo-sync-rdme ]] -->
<!-- cargo-sync-rdme rustdoc [[ -->
VCS abstraction and materialization for git stubs.

A *git stub* (e.g., `foo.json.gitstub`) contains a reference to a file
stored in Git history, in the format `commit:path`. This crate provides
a VCS abstraction for reading file contents from history, and helpers to
materialize these references into actual files.

## Usage in build scripts

````rust,no_run
// build.rs
use git_stub_vcs::Materializer;

fn main() {
    // repo_root is relative to CARGO_MANIFEST_DIR (the directory containing
    // this crate's Cargo.toml). Typically "." or some number of "..".
    let repo_root = "../..";
    let materializer = Materializer::for_build_script(repo_root)
        .expect("VCS detected at repo root");

    // git_stub_path is relative to repo_root.
    let spec_path = materializer
        .materialize("openapi/my-api/my-api-1.0.0-abc123.json.gitstub")
        .expect("materialized successfully");

    // spec_path is a path in OUT_DIR with the materialized content.
    // The materializer also emits cargo::rerun-if-changed for the git stub.
}
````

## Usage outside build scripts

````rust,no_run
use git_stub_vcs::Materializer;

// repo_root is relative to the current working directory.
let materializer = Materializer::standard("../..", "/tmp/output")
    .expect("VCS detected at repo root");

// git_stub_path is relative to repo_root.
let spec_path = materializer
    .materialize("openapi/my-api/my-api-1.0.0-abc123.json.gitstub")
    .expect("materialized successfully");
````
<!-- cargo-sync-rdme ]] -->

## License

This project is available under the terms of either the [Apache 2.0 license](LICENSE-APACHE) or the [MIT license](LICENSE-MIT).
