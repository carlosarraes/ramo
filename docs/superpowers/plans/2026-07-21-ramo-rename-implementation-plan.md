# Ramo Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the complete product identity to Ramo and publish the existing history as `carlosarraes/ramo`.

**Architecture:** Apply one consistent identity at compile time, runtime, installation, configuration, documentation, and integration boundaries. Preserve behavior and Git history; do not introduce a compatibility wrapper or second binary.

**Tech Stack:** Rust 2024, Cargo, shell/PowerShell installers, GitHub Actions, Git, GitHub CLI.

## Global Constraints

- The shipped artifact remains one native Rust executable with no JavaScript or TypeScript runtime.
- The executable, Cargo package, library crate, runtime identity, config namespace, environment prefix, archives, skill, and prompt are named `ramo`.
- Hunk compatibility aliases remain; old pdiff branding and aliases do not.
- Existing commits and tags must be pushed without history rewriting.

---

### Task 1: Rename compile-time and runtime identity

**Files:**
- Modify: `Cargo.toml`, `Cargo.lock`, `src/**/*.rs`, `tests/**/*.rs`, `benches/parity.rs`
- Rename: `src/session/pdiff-review-SKILL.md` to `src/session/ramo-review-SKILL.md`

**Interfaces:**
- Consumes: the existing `pdiff` crate and executable contract.
- Produces: the same public Rust modules and CLI behavior under the `ramo` crate and executable.

- [ ] Replace crate imports, binary lookups, diagnostics, service identity, paths, environment keys, socket/buffer names, and embedded skill references with Ramo equivalents.
- [ ] Rename the embedded skill file and update `include_str!` references.
- [ ] Run `cargo check --locked` and expect success with a `ramo` binary.
- [ ] Commit with `refactor: rename runtime identity to ramo`.

### Task 2: Rename distribution and documentation

**Files:**
- Modify: `README.md`, `install.sh`, `install.ps1`, `.github/workflows/*.yml`, `justfile`, `docs/**/*.md`, `src/markup/guide.md`, `src/pi_prompt.md`

**Interfaces:**
- Consumes: the `ramo` binary and repository name from Task 1.
- Produces: install archives named `ramo-<target>`, Ramo commands and URLs, `.ramo` configuration examples, and a `ramo.md` Pi prompt.

- [ ] Rewrite README introduction, installation, review workflows, integrations, configuration, and guarantees around Ramo.
- [ ] Replace installer paths, variables, downloads, artifacts, workflow commands, benchmark copy, and generated Pi prompt names.
- [ ] Replace old branding in maintained design/history documents so the checked-out codebase presents one current identity.
- [ ] Run a case-insensitive tracked-file search and expect no old-brand matches.
- [ ] Commit with `docs: rebrand project as ramo`.

### Task 3: Verify the renamed product

**Files:**
- Test: all Rust tests and installer scripts.

**Interfaces:**
- Consumes: Tasks 1 and 2.
- Produces: evidence that the rename is complete and behavior-preserving.

- [ ] Run `cargo fmt --check` and expect success.
- [ ] Run `cargo clippy --locked --all-targets --all-features -- -D warnings` and expect success.
- [ ] Run `cargo test --locked` and expect all tests to pass.
- [ ] Run `cargo run --locked -- --version` and `cargo run --locked -- diff --help`; expect the Ramo identity and successful exits.
- [ ] Run the installer integration tests and `cargo build --release --locked`; expect `target/release/ramo` and no `target/release/pdiff`.
- [ ] Commit any verification-only corrections with `fix: complete ramo rename`.

### Task 4: Integrate and publish

**Files:**
- Modify: Git refs and remote configuration only.

**Interfaces:**
- Consumes: the verified rename commits.
- Produces: local `main` and public `carlosarraes/ramo` at the same commit with all historical tags.

- [ ] Fast-forward local `main` to `feat/rename-ramo` and rerun the rename smoke checks from `main`.
- [ ] Remove the owned rename worktree and delete its merged branch.
- [ ] Create `carlosarraes/ramo` with `gh repo create carlosarraes/ramo --public`.
- [ ] Set `upstream` to `git@github.com:carlosarraes/ramo.git`, then run `git push -u upstream main --follow-tags`.
- [ ] Verify public visibility, matching local/remote commit IDs, tag presence, one worktree, and a clean local status.
