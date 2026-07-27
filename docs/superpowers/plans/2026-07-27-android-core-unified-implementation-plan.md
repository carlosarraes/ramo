# Android Core Extraction and Unified Default Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract Ramo's platform-neutral diff/review model into a reusable Rust crate and make unified diff the terminal default without breaking existing public paths or split mode.

**Architecture:** Keep the current `ramo` package as the terminal application and add a small `ramo-core` workspace member. Move only dependency-light models, parsing, changeset totals, and remote-review contracts; the terminal crate re-exports them so current callers and tests keep compiling. Treat the existing `Stack` renderer as unified internally, add `unified` as the user-facing alias, and default new sessions to it.

**Tech Stack:** Rust 2024, Cargo workspace resolver 2, serde 1, existing parser and test suite.

## Global Constraints

- The terminal binary remains a small native Rust binary with no Android/Kotlin dependency.
- `ramo::diff`, `ramo::remote_review`, `ramo::notes`, and `ramo::core::changeset` remain source-compatible public paths.
- Unified diff is the default; `--mode split` and `mode = "split"` continue to work.
- `auto` remains accepted for existing configuration and keeps its responsive behavior.
- Existing untracked `docs/superpowers/plans/2026-07-27-github-comment-import.md` must not be staged or modified.

---

### Task 1: Establish the Cargo workspace and core crate

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/ramo-core/Cargo.toml`
- Create: `crates/ramo-core/src/lib.rs`
- Create: `crates/ramo-core/tests/public_surface.rs`

**Interfaces:**
- Consumes: the current root package named `ramo`.
- Produces: workspace member `ramo-core` with marker constant `CORE_CRATE_NAME`.

- [ ] **Step 1: Write the failing workspace surface test**

```rust
#[test]
fn core_crate_is_linkable() {
    assert_eq!(ramo_core::CORE_CRATE_NAME, "ramo-core");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p ramo-core --test public_surface`

Expected: FAIL because package `ramo-core` does not exist.

- [ ] **Step 3: Add the workspace and crate skeleton**

Add to the root manifest before `[package]`:

```toml
[workspace]
members = [".", "crates/ramo-core"]
resolver = "2"

[workspace.package]
edition = "2024"
rust-version = "1.85"
```

Create `crates/ramo-core/Cargo.toml`:

```toml
[package]
name = "ramo-core"
version = "0.0.15"
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
```

Create `crates/ramo-core/src/lib.rs`:

```rust
pub const CORE_CRATE_NAME: &str = "ramo-core";
```

- [ ] **Step 4: Verify Cargo sees both packages**

Run: `cargo metadata --no-deps --format-version 1 | rg 'ramo-core|workspace_members'`

Expected: output names both `ramo` and `ramo-core`.

### Task 2: Move shared models and parsing behind compatibility re-exports

**Files:**
- Create: `crates/ramo-core/src/agent.rs`
- Create: `crates/ramo-core/src/changeset.rs`
- Create: `crates/ramo-core/src/diff/mod.rs`
- Create: `crates/ramo-core/src/diff/model.rs`
- Create: `crates/ramo-core/src/diff/parser.rs`
- Create: `crates/ramo-core/src/remote_review.rs`
- Modify: `src/notes/model.rs`
- Modify: `src/core/changeset.rs`
- Modify: `src/diff/mod.rs`
- Delete after move: `src/diff/model.rs`
- Delete after move: `src/diff/parser.rs`
- Modify: `src/remote_review.rs`
- Modify: `Cargo.toml`
- Test: `crates/ramo-core/tests/public_surface.rs`
- Test: `tests/library_surface.rs`

**Interfaces:**
- Consumes: current types `DiffFile`, `Changeset`, `AgentContext`, `RemoteReviewRequest`, and `RemoteReviewPublisher`.
- Produces: the same types from `ramo_core`; root modules re-export the identical definitions rather than wrappers or copies.

- [ ] **Step 1: Strengthen the failing compatibility test**

Add to `tests/library_surface.rs`:

```rust
#[test]
fn terminal_reexports_core_types_without_wrappers() {
    fn takes_core(_: ramo_core::remote_review::ReviewVerdict) {}
    takes_core(ramo::remote_review::ReviewVerdict::Approve);

    let files: Vec<ramo_core::diff::model::DiffFile> =
        ramo::diff::parser::parse_unified_diff("");
    assert!(files.is_empty());
}
```

- [ ] **Step 2: Run it to verify the missing dependency failure**

Run: `cargo test -p ramo --test library_surface`

Expected: FAIL because the root package does not yet depend on `ramo-core`.

- [ ] **Step 3: Move the dependency-light implementations**

Move the current contents without behavior changes, then update core-local paths:

```rust
// crates/ramo-core/src/lib.rs
pub const CORE_CRATE_NAME: &str = "ramo-core";
pub mod agent;
pub mod changeset;
pub mod diff;
pub mod remote_review;

// crates/ramo-core/src/diff/mod.rs
pub mod model;
pub mod parser;
```

In `diff/model.rs`, replace `crate::notes::AgentFileContext` with `crate::agent::AgentFileContext` and `crate::core::changeset::stable_file_id` with `crate::changeset::stable_file_id`.

In `diff/parser.rs`, replace its first import with:

```rust
use crate::changeset::stable_file_id;
```

In `changeset.rs`, use:

```rust
use crate::agent::AgentContext;
use crate::diff::model::DiffFile;
pub use crate::diff::model::FileStats;
```

Move `src/notes/model.rs` to `agent.rs` and make `NoteSource::from_raw` public because parsing remains in the terminal crate:

```rust
pub fn from_raw(value: Option<String>) -> Self {
    match value.as_deref() {
        None | Some("") | Some("ai") => Self::Ai,
        Some("agent" | "mcp") => Self::Agent,
        Some("user") => Self::User,
        Some(_) => Self::Named(value.expect("a named source has a value")),
    }
}
```

Move `src/remote_review.rs` unchanged into the core crate.

- [ ] **Step 4: Add serialization only to cross-platform value types**

Add `serde::Serialize` and `serde::Deserialize` to agent, diff, changeset, and remote-review value types. Do not derive serde for the `RemoteReviewPublisher` trait. For example:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RemoteLineSide {
    Left,
    Right,
}
```

Use stable snake-case field names and explicit enum rename rules so persisted mobile drafts do not depend on Rust debug names.

- [ ] **Step 5: Replace terminal definitions with re-exports**

Add `ramo-core = { path = "crates/ramo-core" }` to root dependencies, then make the compatibility modules exact re-exports:

```rust
// src/diff/mod.rs
pub use ramo_core::diff::{model, parser};
```

```rust
// src/core/changeset.rs
pub use ramo_core::changeset::*;
```

```rust
// src/notes/model.rs
pub use ramo_core::agent::*;
```

```rust
// src/remote_review.rs
pub use ramo_core::remote_review::*;
```

- [ ] **Step 6: Run the focused compatibility suite**

Run: `cargo test -p ramo-core && cargo test -p ramo --test library_surface --test input_loading --test remote_review_model --test remote_review_targets --test github_cli`

Expected: all tests PASS with no duplicate type conversions.

- [ ] **Step 7: Commit the extraction**

```bash
git add Cargo.toml Cargo.lock crates/ramo-core src/core/changeset.rs src/diff src/notes/model.rs src/remote_review.rs tests/library_surface.rs
git commit -m "refactor: extract reusable review core"
```

### Task 3: Make unified mode the terminal default and expose its name

**Files:**
- Modify: `src/core/input.rs`
- Modify: `src/config/model.rs`
- Modify: `src/config/save.rs`
- Modify: `src/cli/args.rs`
- Modify: `src/cli/normalize.rs`
- Modify: `src/review/state.rs`
- Modify: `README.md`
- Test: `tests/cli_parse.rs`
- Test: `tests/config_resolution.rs`
- Test: `tests/config_persistence.rs`
- Test: `tests/review_state.rs`

**Interfaces:**
- Consumes: internal `LayoutMode::Stack` and its existing renderer.
- Produces: default `LayoutMode::Stack`, CLI/config spelling `unified`, backward-compatible spelling `stack`, and unchanged split mode.

- [ ] **Step 1: Write failing default and alias tests**

Add these assertions:

```rust
// tests/config_resolution.rs
#[test]
fn missing_configuration_defaults_to_unified() {
    let input = patch_input(CommonOptions::default());
    let resolved = ConfigResolver::new(ConfigPaths::default()).resolve(&input).unwrap();
    assert_eq!(resolved.mode, LayoutMode::Stack);
}
```

```rust
// tests/cli_parse.rs
#[test]
fn unified_and_legacy_stack_select_the_same_layout() {
    for spelling in ["unified", "stack"] {
        let input = parse(&["ramo", "diff", "--mode", spelling]);
        assert_eq!(input.options().mode, Some(LayoutMode::Stack));
    }
}
```

- [ ] **Step 2: Run tests to verify the old auto default and missing alias**

Run: `cargo test -p ramo --test config_resolution missing_configuration_defaults_to_unified -- --exact && cargo test -p ramo --test cli_parse unified_and_legacy_stack_select_the_same_layout -- --exact`

Expected: first FAILS with `Auto`; second FAILS because `unified` is rejected.

- [ ] **Step 3: Change only the implicit defaults**

Set both default sites to stack/unified with these exact replacements:

```diff
-            mode: LayoutMode::Auto,
+            mode: LayoutMode::Stack,
```

Apply the same one-line replacement in `ReviewPreferences::default` in `src/review/state.rs`. Do not change `resolve_responsive_layout(LayoutMode::Auto, ...)`; explicit auto remains responsive.

- [ ] **Step 4: Add the user-facing alias without renaming the internal variant**

In Clap's `LayoutArg`, make `Unified` canonical and retain `Stack` as an alias that normalizes to the same internal value:

```rust
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum LayoutArg {
    Auto,
    Split,
    #[value(alias = "stack")]
    Unified,
}
```

Map `LayoutArg::Unified` to `LayoutMode::Stack`. Add a custom serde alias to the internal variant:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    Auto,
    Split,
    #[default]
    #[serde(alias = "unified")]
    Stack,
}
```

Make `layout_name(LayoutMode::Stack)` return `"unified"` so newly saved preferences use the new vocabulary.

- [ ] **Step 5: Update help and README copy**

Use the exact help string `layout mode: unified (default), split, auto` and document that `stack` remains a deprecated compatibility alias. Do not remove split-mode key bindings.

- [ ] **Step 6: Run focused terminal behavior tests**

Run: `cargo test -p ramo --test cli_parse --test config_resolution --test config_persistence --test review_state --test ui_render`

Expected: all tests PASS; snapshots or string assertions identify unified as the default.

- [ ] **Step 7: Commit the default change**

```bash
git add src/core/input.rs src/config src/cli src/review/state.rs README.md tests/cli_parse.rs tests/config_resolution.rs tests/config_persistence.rs tests/review_state.rs
git commit -m "feat: default to unified diffs"
```

### Task 4: Gate the workspace extraction

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: two-package Cargo workspace.
- Produces: CI and release commands that explicitly verify the complete workspace while still packaging only the `ramo` binary.

- [ ] **Step 1: Update CI commands**

Use these exact gates:

```yaml
- run: cargo fmt --all -- --check
- run: cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
- run: cargo test --locked --workspace --all-targets --all-features
```

Keep platform-specific PTY jobs targeted to `-p ramo`. In release builds use `cargo build --locked --release -p ramo --target ${{ matrix.target }}` so adding mobile crates cannot change desktop artifacts.

- [ ] **Step 2: Run the local workspace gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
cargo build --locked --release -p ramo
```

Expected: every command exits 0 and `target/release/ramo --help` reports unified as default.

- [ ] **Step 3: Commit CI coverage**

```bash
git add .github/workflows/ci.yml .github/workflows/release.yml
git commit -m "ci: verify the Rust workspace"
```
