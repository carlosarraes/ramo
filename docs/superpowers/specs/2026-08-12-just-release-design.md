# `just release` Design

## Goal

Add one safe command for cutting an exact Ramo release:

```text
just release 0.0.18
just release 0.1.0
just release 1.0.0
```

The caller controls the complete `X.Y.Z` version. The recipe validates semantic
version shape but does not infer or restrict whether the change is patch,
minor, or major.

## Interface

Add `release version` to the root `justfile`, following the robust release
recipes in the sibling `tmux-seer` and `grove` repositories. Accept either
`X.Y.Z` or `vX.Y.Z` input and normalize it to `X.Y.Z` for package metadata and
`vX.Y.Z` for the Git tag.

Also add a reusable `check` recipe that runs Ramo's local Rust CI gates:

```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
```

## Safety and Release Flow

Before editing or publishing anything, `just release` will:

1. reject input that is not exactly three numeric version components;
2. require a clean working tree;
3. require the current branch to be `main`;
4. reject an existing local tag with the requested version.

It will then update the package version in all six workspace manifests:

- `Cargo.toml`;
- `crates/ramo-core/Cargo.toml`;
- `crates/ramo-github/Cargo.toml`;
- `crates/ramo-mobile/Cargo.toml`;
- `crates/ramo-server/Cargo.toml`;
- `crates/uniffi-bindgen/Cargo.toml`.

It will also update the exact version assertions in
`crates/ramo-mobile/src/lib.rs` and
`android/app/src/test/kotlin/io/github/carlosarraes/ramo/data/RamoBridgeTest.kt`,
then regenerate `Cargo.lock` through Cargo.

After the version edit, the recipe runs `just check`. A failed check stops the
flow before committing, tagging, or pushing and leaves the version edits in the
working tree for inspection.

On success, it creates `chore: release version X.Y.Z`, creates annotated tag
`vX.Y.Z`, resolves the push remote from the current branch with `origin` then
`upstream` fallbacks, pushes `HEAD`, and only then pushes the tag. The tag push
triggers `.github/workflows/release.yml`, which builds the platform archives and
creates the GitHub Release.

## Implementation Shape

Keep the complete workflow inline in the root `justfile`, matching the sibling
repositories and avoiding an additional release script. Use portable Bash and
`awk` rather than platform-specific `sed -i`. Temporary files are created with
`mktemp` and removed through a trap.

## Testing and First Use

Verify recipe discovery with `just --list`, exercise invalid-version and dirty
tree guards without mutation, and inspect the recipe with `bash -n` after Just
renders it. Run the repository checks before committing the automation.

After the automation commit is clean, run `just release 0.0.18`. Confirm that:

- the release commit contains only the coordinated version bump;
- local and remote `main` point to that commit;
- annotated tag `v0.0.18` points to that commit;
- the GitHub Actions release workflow succeeds;
- the GitHub Release publishes all expected archives.
