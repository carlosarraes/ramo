# Native VCS and Pager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every Git, Jujutsu, Sapling, difftool, and pager input in delivery slice 2 execute through the Rust binary with Hunk-compatible normalized output, diagnostics, large-file policy, untracked files, source-side metadata, and plain-text pager fallback.

**Architecture:** Add a small VCS domain under `src/vcs/`: neutral operation contracts, repository detection, safe process execution, and one adapter per native VCS. `ReviewLoader` receives an explicit load context and returns either a normalized review or sanitized plain text; runtime remains the only owner of terminal and child-process behavior. Git-only source endpoints are represented as inert Rust data and read lazily, so the model stays cloneable and no long-lived subprocess or JavaScript runtime enters the binary.

**Tech Stack:** Rust 2024, `std::process::Command`, Clap, Serde/TOML, `similar`, Ratatui/Crossterm, `shell-words` for argv parsing without a shell, `tempfile`/`assert_cmd` for fixtures, and `portable-pty` for Unix PTY contract tests.

## Global Constraints

- The shipped product is 100% Rust and one native `ramo` executable; do not add Node.js, Bun, TypeScript, JavaScript, WebView, or an embedded JS engine.
- Git, Jujutsu, and Sapling are optional external executables. Only a workflow backed by that VCS may require its executable.
- Preserve existing `ramo` patch, file-pair, Vim-selection, comment, tmux, clipboard, and Pi integration behavior.
- Hunk's top menu bar and dropdown menus remain intentionally excluded.
- Match Hunk at `/home/carraes/github/hunk` commit `53fcb2c`; do not import or execute its TypeScript.
- All process invocations use argv with `shell(false)` semantics, bounded captured output, ignored stdin, and explicit accepted exit codes.
- A failed VCS or pager command must identify the operation and corrective action before Ratatui initializes.
- Large diff thresholds are exactly 1,000,000 bytes or 20,000 changed/text lines; untracked line sniffing is capped at 256 KiB unless the byte threshold is crossed.
- Slice 4 owns observation/debounce/reload execution. This slice produces complete `ReloadPlan::Vcs` data but does not start watchers.

## File and module map

- `src/vcs/types.rs`: `VcsId`, neutral operations, command results, source endpoints, and adapter output.
- `src/vcs/command.rs`: bounded, shell-free command execution and typed spawn/exit failures.
- `src/vcs/detect.rs`: nearest-checkout detection and explicit-config selection.
- `src/vcs/git.rs`: Git argv construction, error translation, loading, untracked/large-file handling, and exact old/new endpoints.
- `src/vcs/jj.rs`: Jujutsu argv construction, detection, loading, and diagnostics.
- `src/vcs/sl.rs`: Sapling detection, loading, untracked files, and diagnostics.
- `src/vcs/source.rs`: bounded lazy reads from the filesystem, Git refs, and the Git index.
- `src/vcs/mod.rs`: public exports and adapter dispatch.
- `src/input/vcs.rs`: adapter output to `Changeset` normalization and reload plans.
- `src/input/pager.rs`: patch detection and safe terminal-text sanitization.
- `src/pager.rs`: text-pager command resolution and child lifecycle.
- `src/core/input.rs`, `src/config/model.rs`, `src/config/load.rs`: typed VCS selection and loader context inputs.
- `src/diff/model.rs`, `src/diff/parser.rs`: explicit stats, moved-line classes, and source-side specifications.
- `src/input/mod.rs`, `src/runtime.rs`, `src/error.rs`: review/plain-text outcomes and terminal-safe dispatch.
- `tests/vcs_contract.rs`: pure adapter selection, command construction, and diagnostic contracts.
- `tests/git_loading.rs`: real temporary Git repositories for diff/show/stash/pathspec/untracked/large/source behavior.
- `tests/jj_loading.rs`, `tests/sl_loading.rs`: fake native executables plus repository markers for deterministic adapter integration.
- `tests/pager.rs`, `tests/pty_pager.rs`: safe pager parsing, text fallback, exit status, recursion prevention, and PTY behavior.

---

### Task 1: Introduce typed VCS contracts and explicit load context

**Files:**
- Create: `src/vcs/types.rs`
- Create: `src/vcs/command.rs`
- Create: `src/vcs/mod.rs`
- Modify: `src/lib.rs`
- Modify: `src/core/input.rs`
- Modify: `src/config/model.rs`
- Modify: `src/config/load.rs`
- Modify: `src/input/mod.rs`
- Modify: `src/runtime.rs`
- Test: `tests/vcs_contract.rs`
- Test: `tests/config_resolution.rs`
- Test: `tests/input_loading.rs`

**Interfaces:**
- Consumes: `ReviewInput`, `ResolvedConfig`, `Changeset`, and the existing `ReviewLoader` dispatch seam.
- Produces: `VcsId`, `VcsOperation`, `VcsLoadContext<'a>`, `VcsPatch`, `CommandSpec`, `CommandRunner`, `SystemCommandRunner`, `LoadContext<'a>`, and `ReloadPlan::Vcs`.

- [x] **Step 1: Write failing typed-selection and context tests**

```rust
// tests/vcs_contract.rs
use std::path::Path;

use ramo::core::input::VcsId;
use ramo::vcs::{CommandSpec, VcsOperation};

#[test]
fn vcs_ids_are_closed_and_display_native_executable_names() {
    assert_eq!(VcsId::Git.executable(), "git");
    assert_eq!(VcsId::Jj.executable(), "jj");
    assert_eq!(VcsId::Sl.executable(), "sl");
}

#[test]
fn command_specs_are_argv_not_shell_strings() {
    let spec = CommandSpec::new("git", Path::new("/repo"))
        .args(["diff", "--", "file; touch /tmp/not-run"]);
    assert_eq!(spec.program, "git");
    assert_eq!(spec.args[2], "file; touch /tmp/not-run");
    assert_eq!(spec.accepted_exit_codes, vec![0]);
}

#[test]
fn neutral_operations_do_not_leak_cli_command_names() {
    assert_eq!(VcsOperation::WorkingTree.kind_name(), "working-tree diff");
    assert_eq!(VcsOperation::RevisionShow.kind_name(), "revision show");
    assert_eq!(VcsOperation::StashShow.kind_name(), "stash show");
}
```

Add to `tests/config_resolution.rs`:

```rust
#[test]
fn vcs_config_is_typed_and_rejects_unknown_providers() {
    let temp = tempfile::tempdir().unwrap();
    let valid = temp.path().join("valid.toml");
    std::fs::write(&valid, "vcs = \"jj\"\n").unwrap();
    let resolved = ConfigResolver::new(ConfigPaths { user: Some(valid), repo: None })
        .resolve(&patch_input(CommonOptions::default()))
        .unwrap();
    assert_eq!(resolved.vcs, Some(ramo::core::input::VcsId::Jj));

    let invalid = temp.path().join("invalid.toml");
    std::fs::write(&invalid, "vcs = \"mercurial\"\n").unwrap();
    let error = ConfigResolver::new(ConfigPaths { user: Some(invalid), repo: None })
        .resolve(&patch_input(CommonOptions::default()))
        .unwrap_err();
    assert!(error.to_string().contains("unknown variant `mercurial`"));
}
```

- [x] **Step 2: Run the focused tests and confirm the new API is absent**

Run: `cargo test --test vcs_contract && cargo test --test config_resolution vcs_config_is_typed`

Expected: compilation fails because `ramo::vcs`, `VcsId`, and the typed `ResolvedConfig::vcs` do not exist.

- [x] **Step 3: Add the contracts and bounded command runner**

```rust
// src/vcs/types.rs
use std::path::PathBuf;

use crate::core::input::{ReviewInput, VcsId};
use crate::diff::model::DiffFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcsOperation { WorkingTree, RevisionShow, StashShow }

impl VcsOperation {
    pub fn kind_name(self) -> &'static str {
        match self {
            Self::WorkingTree => "working-tree diff",
            Self::RevisionShow => "revision show",
            Self::StashShow => "stash show",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcsPatch {
    pub vcs: VcsId,
    pub repo_root: PathBuf,
    pub source_label: String,
    pub title: String,
    pub patch_text: String,
    pub extra_files: Vec<DiffFile>,
}

pub struct VcsLoadContext<'a> {
    pub cwd: &'a std::path::Path,
    pub config: &'a crate::config::ResolvedConfig,
    pub runner: &'a dyn super::command::CommandRunner,
    pub git_executable: &'a str,
    pub jj_executable: &'a str,
    pub sl_executable: &'a str,
}

pub trait VcsAdapter {
    fn id(&self) -> VcsId;
    fn detect(&self, cwd: &std::path::Path) -> Option<PathBuf>;
    fn load(
        &self,
        input: &ReviewInput,
        context: &VcsLoadContext<'_>,
    ) -> Result<VcsPatch, super::VcsError>;
}
```

Define the errors needed by the runner in `src/vcs/mod.rs`; Task 2 adds operation-specific translation variants without changing these field names:

```rust
#[derive(Debug)]
pub enum VcsError {
    Spawn { program: String, source: std::io::Error },
    Exit { program: String, args: Vec<String>, code: i32, stderr: String },
    OutputTooLarge { program: String, limit: usize },
    UnsupportedOperation { vcs: VcsId, operation: InputKind },
}

impl std::fmt::Display for VcsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn { program, source } => write!(formatter, "could not run {program}: {source}"),
            Self::Exit { program, args, code, stderr } => write!(formatter, "{program} {} exited with {code}: {}", args.join(" "), stderr.trim()),
            Self::OutputTooLarge { program, limit } => write!(formatter, "{program} output exceeded {limit} bytes"),
            Self::UnsupportedOperation { vcs, operation } => write!(formatter, "{vcs:?} does not support {operation:?}"),
        }
    }
}

impl std::error::Error for VcsError {}
```

```rust
// src/vcs/command.rs
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub accepted_exit_codes: Vec<i32>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, cwd: &Path) -> Self {
        Self { program: program.into(), args: Vec::new(), cwd: cwd.into(), env: BTreeMap::new(), accepted_exit_codes: vec![0] }
    }
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
    pub fn accepted_exit_codes(mut self, codes: impl IntoIterator<Item = i32>) -> Self {
        self.accepted_exit_codes = codes.into_iter().collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput { pub code: i32, pub stdout: Vec<u8>, pub stderr: Vec<u8> }

pub trait CommandRunner: Send + Sync {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, super::VcsError>;
}

#[derive(Debug, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, super::VcsError> {
        let output = Command::new(&spec.program).args(&spec.args).current_dir(&spec.cwd)
            .envs(&spec.env).stdin(std::process::Stdio::null()).output()
            .map_err(|source| super::VcsError::Spawn { program: spec.program.clone(), source })?;
        if output.stdout.len() > MAX_CAPTURE_BYTES || output.stderr.len() > MAX_CAPTURE_BYTES {
            return Err(super::VcsError::OutputTooLarge { program: spec.program.clone(), limit: MAX_CAPTURE_BYTES });
        }
        let code = output.status.code().unwrap_or(128);
        if !spec.accepted_exit_codes.contains(&code) {
            return Err(super::VcsError::Exit { program: spec.program.clone(), args: spec.args.clone(), code, stderr: String::from_utf8_lossy(&output.stderr).into_owned() });
        }
        Ok(CommandOutput { code, stdout: output.stdout, stderr: output.stderr })
    }
}
```

Add `VcsId` to `src/core/input.rs` and use it in config:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VcsId { Git, Jj, Sl }

impl VcsId {
    pub fn executable(self) -> &'static str {
        match self { Self::Git => "git", Self::Jj => "jj", Self::Sl => "sl" }
    }
}
```

Change both `ConfigLayer::vcs` and `ResolvedConfig::vcs` to `Option<VcsId>`. Add `LoadContext<'a> { cwd: &'a Path, config: &'a ResolvedConfig, runner: &'a dyn CommandRunner }` to `src/input/mod.rs`, require it in `ReviewLoader::load`, and extend reload data:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadPlan {
    None,
    Files { left: PathBuf, right: PathBuf },
    Vcs { input: ReviewInput, repo_root: PathBuf },
}
```

Update runtime to resolve config once, construct `SystemCommandRunner`, build `LoadContext`, and pass it to every load. Update existing input tests with a helper returning a default config, temp cwd, and system runner.

- [x] **Step 4: Run contract, config, and existing loader tests**

Run: `cargo test --test vcs_contract && cargo test --test config_resolution && cargo test --test input_loading`

Expected: all tests pass, including the existing patch and file-pair contracts.

- [x] **Step 5: Commit the contracts**

```bash
git add src/lib.rs src/core/input.rs src/config/model.rs src/config/load.rs src/input/mod.rs src/runtime.rs src/vcs tests/vcs_contract.rs tests/config_resolution.rs tests/input_loading.rs
git commit -m "refactor: define native vcs loading contracts"
```

### Task 2: Implement Git command construction, detection, and diagnostics

**Files:**
- Create: `src/vcs/detect.rs`
- Create: `src/vcs/git.rs`
- Modify: `src/vcs/mod.rs`
- Test: `tests/vcs_contract.rs`

**Interfaces:**
- Consumes: `VcsAdapter`, `CommandSpec`, `VcsError`, and neutral `ReviewInput` variants.
- Produces: `GitAdapter`, `build_git_diff_args`, `build_git_show_args`, `build_git_stash_args`, `select_vcs`, and actionable Git error translation.

- [x] **Step 1: Add failing Git argv, detection, and error tests**

```rust
#[test]
fn git_args_force_parseable_prefixes_and_preserve_pathspec_boundaries() {
    let args = ramo::vcs::git::build_git_diff_args(
        Some("main...HEAD"), true, &["src/lib.rs".into()], &[], false,
    );
    assert_eq!(&args[..8], ["-c", "diff.noprefix=false", "-c", "diff.mnemonicPrefix=false", "-c", "diff.srcPrefix=a/", "-c", "diff.dstPrefix=b/"]);
    assert!(args.windows(2).any(|pair| pair == ["--staged", "main...HEAD"]));
    assert_eq!(&args[args.len() - 2..], ["--", "src/lib.rs"]);
}

#[test]
fn nearest_checkout_wins_and_same_root_prefers_jj_then_sl_then_git() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".git")).unwrap();
    std::fs::create_dir_all(temp.path().join("nested/.jj")).unwrap();
    std::fs::create_dir_all(temp.path().join("nested/src")).unwrap();
    let selected = ramo::vcs::detect::select_vcs(temp.path().join("nested/src").as_path(), None).unwrap();
    assert_eq!(selected.id, VcsId::Jj);
    assert_eq!(selected.repo_root, temp.path().join("nested"));
}
```

Use a `FakeRunner` implementing `CommandRunner` to return `VcsError::Exit` with `not a git repository`, `bad revision`, and `No stash entries found.`; assert the formatted messages contain, respectively, `inside a Git repository`, the supplied range/ref, and `git stash list`.

- [x] **Step 2: Run the Git contract tests and verify red**

Run: `cargo test --test vcs_contract git_`

Expected: compilation fails because `GitAdapter`, selection, and builders are absent.

- [x] **Step 3: Add exact Git builders and nearest-marker selection**

In `src/vcs/git.rs`, define the canonical prefix constant and builders:

```rust
const PREFIX_ARGS: &[&str] = &[
    "-c", "diff.noprefix=false", "-c", "diff.mnemonicPrefix=false",
    "-c", "diff.srcPrefix=a/", "-c", "diff.dstPrefix=b/",
];

pub fn build_git_diff_args(
    range: Option<&str>, staged: bool, pathspecs: &[String], excluded: &[String], color_moved: bool,
) -> Vec<String> {
    let mut args = PREFIX_ARGS.iter().map(|value| (*value).into()).collect::<Vec<_>>();
    args.extend(["diff", "--no-ext-diff", "--find-renames", if color_moved { "--color=always" } else { "--no-color" }].map(String::from));
    if color_moved { args.push("--color-moved=zebra".into()); }
    if staged { args.push("--staged".into()); }
    if let Some(range) = range { args.push(range.into()); }
    if !pathspecs.is_empty() || !excluded.is_empty() {
        args.push("--".into());
        args.extend(pathspecs.iter().cloned());
        args.extend(excluded.iter().map(|path| format!(":(exclude){path}")));
    }
    args
}
```

`build_git_show_args` uses `show --format= --no-ext-diff --find-renames`; `build_git_stash_args` uses `stash show -p --no-ext-diff --find-renames`; both prepend `PREFIX_ARGS`, add `--no-color` or deterministic moved-line colors, and append refs/pathspecs without shell parsing.

In `src/vcs/detect.rs`, walk `Path::ancestors()`, recognize `.git` as either a directory or worktree file, `.jj`, `.sl`, and Sapling `.hg/requires` containing a line equal to `treestate`. Calculate ancestor distance and select the lowest distance; break equal-root ties in `[Jj, Sl, Git]` order. An explicit `VcsId` chooses that adapter but still lets its loader produce the missing-repository diagnostic.

Add `VcsError` to `src/vcs/mod.rs` with `Spawn`, `Exit`, `OutputTooLarge`, `UnsupportedOperation`, `NotRepository`, `InvalidRevision`, `MissingStash`, `InvalidUtf8`, and `Io` variants. Its `Display` text names `ramo`, the native VCS, the input value, and one concrete corrective action. `GitAdapter::load` initially returns `UnsupportedOperation` after selecting the correct builder; Task 3 replaces that branch with repository loading.

- [x] **Step 4: Run the complete contract suite**

Run: `cargo test --test vcs_contract`

Expected: all selection, argv, and translated-diagnostic tests pass.

- [x] **Step 5: Commit Git contracts**

```bash
git add src/vcs tests/vcs_contract.rs
git commit -m "feat: add native git command contracts"
```

### Task 3: Load Git working trees, ranges, staged changes, revisions, and stashes

**Files:**
- Create: `src/input/vcs.rs`
- Modify: `src/input/mod.rs`
- Modify: `src/vcs/git.rs`
- Modify: `src/vcs/mod.rs`
- Test: `tests/git_loading.rs`

**Interfaces:**
- Consumes: Git builders, `CommandRunner`, shared patch parser, `ResolvedConfig`, and `ReviewInput`.
- Produces: executable Git review inputs, normalized titles/source labels, untracked `DiffFile`s, and `ReloadPlan::Vcs`.

- [x] **Step 1: Write failing real-repository integration tests**

Create this fixture in `tests/git_loading.rs`; it keeps every repository operation as argv and centralizes the new load context:

```rust
struct GitFixture { temp: tempfile::TempDir }

impl GitFixture {
    fn new() -> Self {
        let fixture = Self::non_repository();
        fixture.git(["init", "-q"]);
        fixture.git(["config", "user.name", "Ramo Test"]);
        fixture.git(["config", "user.email", "ramo@example.invalid"]);
        fixture
    }

    fn non_repository() -> Self { Self { temp: tempfile::tempdir().unwrap() } }
    fn path(&self) -> &std::path::Path { self.temp.path() }

    fn git<const N: usize>(&self, args: [&str; N]) -> String {
        let output = std::process::Command::new("git").args(args).current_dir(self.path()).output().unwrap();
        assert!(output.status.success(), "git failed: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    }

    fn write(&self, path: &str, contents: &str) {
        let absolute = self.path().join(path);
        if let Some(parent) = absolute.parent() { std::fs::create_dir_all(parent).unwrap(); }
        std::fs::write(absolute, contents).unwrap();
    }

    fn commit_file(&self, path: &str, contents: &str) {
        self.write(path, contents);
        self.git(["add", path]);
        self.git(["commit", "-q", "-m", path]);
    }

    fn commit_all(&self, message: &str) {
        self.git(["add", "-A"]);
        self.git(["commit", "-q", "-m", message]);
    }

    fn load(&self, input: ReviewInput) -> LoadedReview {
        let config = ResolvedConfig::default();
        let runner = SystemCommandRunner;
        let context = LoadContext { cwd: self.path(), config: &config, runner: &runner };
        ReviewLoader.load(&input, &mut std::io::Cursor::new([]), &context).unwrap()
    }

    fn load_error(&self, input: ReviewInput) -> LoadError {
        let config = ResolvedConfig::default();
        let runner = SystemCommandRunner;
        let context = LoadContext { cwd: self.path(), config: &config, runner: &runner };
        ReviewLoader.load(&input, &mut std::io::Cursor::new([]), &context).unwrap_err()
    }
}
```

Use paths containing spaces and semicolons to prove argv safety. Add these tests:

```rust
#[test]
fn working_tree_includes_tracked_and_untracked_files() {
    let repo = GitFixture::new();
    repo.commit_file("tracked.txt", "before\n");
    repo.write("tracked.txt", "after\n");
    repo.write("new file;safe.txt", "new\n");
    let loaded = repo.load(ReviewInput::VcsDiff { range: None, staged: false, pathspecs: vec![], options: CommonOptions::default() });
    assert_eq!(loaded.changeset.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["tracked.txt", "new file;safe.txt"]);
    assert!(loaded.changeset.files[1].is_untracked);
}

#[test]
fn staged_diff_excludes_untracked_and_unstaged_changes() {
    let repo = GitFixture::new();
    repo.commit_file("staged.txt", "base\n");
    repo.commit_file("unstaged.txt", "base\n");
    repo.write("staged.txt", "index\n");
    repo.write("unstaged.txt", "worktree\n");
    repo.write("unknown.txt", "unknown\n");
    repo.git(["add", "staged.txt"]);
    let loaded = repo.load(ReviewInput::VcsDiff { range: None, staged: true, pathspecs: vec![], options: CommonOptions::default() });
    assert_eq!(loaded.changeset.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["staged.txt"]);
}

#[test]
fn range_and_pathspec_review_only_the_requested_history() {
    let repo = GitFixture::new();
    repo.commit_file("src/lib.rs", "one\n");
    repo.commit_file("docs/readme.md", "one\n");
    repo.write("src/lib.rs", "two\n");
    repo.write("docs/readme.md", "two\n");
    repo.commit_all("change both");
    let loaded = repo.load(ReviewInput::VcsDiff { range: Some("HEAD^..HEAD".into()), staged: false, pathspecs: vec!["src".into()], options: CommonOptions::default() });
    assert_eq!(loaded.changeset.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["src/lib.rs"]);
}

#[test]
fn show_defaults_to_head_and_accepts_an_explicit_ref() {
    let repo = GitFixture::new();
    repo.commit_file("file.txt", "one\n");
    repo.write("file.txt", "two\n");
    repo.commit_all("second");
    let head = repo.load(ReviewInput::Show { reference: None, pathspecs: vec![], options: CommonOptions::default() });
    assert!(head.changeset.files[0].patch.contains("+two"));
    let parent = repo.load(ReviewInput::Show { reference: Some("HEAD^".into()), pathspecs: vec![], options: CommonOptions::default() });
    assert!(parent.changeset.files[0].patch.contains("+one"));
}

#[test]
fn stash_show_defaults_to_latest_stash_and_accepts_a_ref() {
    let repo = GitFixture::new();
    repo.commit_file("file.txt", "base\n");
    repo.write("file.txt", "first\n");
    repo.git(["stash", "push", "-m", "first"]);
    repo.write("file.txt", "second\n");
    repo.git(["stash", "push", "-m", "second"]);
    let latest = repo.load(ReviewInput::StashShow { reference: None, options: CommonOptions::default() });
    assert!(latest.changeset.files[0].patch.contains("+second"));
    let first = repo.load(ReviewInput::StashShow { reference: Some("stash@{1}".into()), options: CommonOptions::default() });
    assert!(first.changeset.files[0].patch.contains("+first"));
}

#[test]
fn exclude_untracked_removes_only_synthetic_files() {
    let repo = GitFixture::new();
    repo.commit_file("tracked.txt", "base\n");
    repo.write("tracked.txt", "changed\n");
    repo.write("unknown.txt", "unknown\n");
    let loaded = repo.load(ReviewInput::VcsDiff { range: None, staged: false, pathspecs: vec![], options: CommonOptions { exclude_untracked: Some(true), ..Default::default() } });
    assert_eq!(loaded.changeset.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["tracked.txt"]);
}

#[test]
fn invalid_repo_revision_and_empty_stash_are_actionable() {
    let outside = GitFixture::non_repository();
    assert!(outside.load_error(working_tree_input()).to_string().contains("inside a Git repository"));
    let repo = GitFixture::new();
    repo.commit_file("file.txt", "base\n");
    assert!(repo.load_error(ReviewInput::Show { reference: Some("missing".into()), pathspecs: vec![], options: CommonOptions::default() }).to_string().contains("missing"));
    assert!(repo.load_error(ReviewInput::StashShow { reference: None, options: CommonOptions::default() }).to_string().contains("git stash push"));
}
```

- [x] **Step 2: Run the Git loading tests and verify the unsupported seam fails**

Run: `cargo test --test git_loading`

Expected: all cases fail with the Task 2 unsupported-load error.

- [x] **Step 3: Implement Git loading and untracked synthesis**

Implement `GitAdapter::load` as follows:

```rust
let repo_root = run_text(context.runner, CommandSpec::new(executable, context.cwd)
    .args(["rev-parse", "--show-toplevel"]))?;
let (title, patch_args) = match input {
    ReviewInput::VcsDiff { range, staged, pathspecs, .. } => (
        git_diff_title(&repo_root, range.as_deref(), *staged),
        build_git_diff_args(range.as_deref(), *staged, pathspecs, &[], context.config.color_moved),
    ),
    ReviewInput::Show { reference, pathspecs, .. } => (
        git_show_title(&repo_root, reference.as_deref()),
        build_git_show_args(reference.as_deref(), pathspecs, context.config.color_moved),
    ),
    ReviewInput::StashShow { reference, .. } => (
        git_stash_title(&repo_root, reference.as_deref()),
        build_git_stash_args(reference.as_deref(), context.config.color_moved),
    ),
    _ => return Err(VcsError::UnsupportedOperation { vcs: VcsId::Git, operation: input.kind() }),
};
```

Run in `repo_root`, decode UTF-8, translate process failures, and return `VcsPatch`. For a working-tree-shaped diff (`!staged` and no explicit multi-revision range), run `git --no-optional-locks status --porcelain=v1 -z --untracked-files=all [-- pathspecs]`. Parse only `?? ` records. Reject directory symlinks; synthesize text files with `similar::TextDiff`, binary placeholders using the existing 8 KiB NUL sniff, and stable `FileChangeKind::Added` metadata. A single revision still has the worktree on the right; `A..B`, `A...B`, and revision-set forms remain commit-to-commit and exclude untracked files.

In `src/input/vcs.rs`, select the adapter from explicit config or detection, call it, normalize `patch_text`, parse tracked files, append `extra_files`, and return:

```rust
LoadedReview {
    changeset: Changeset::new(patch.source_label, patch.title, files),
    reload_plan: ReloadPlan::Vcs { input: input.clone(), repo_root: patch.repo_root },
}
```

Update `ReviewLoader::load` so `VcsDiff`, `Show`, and `StashShow` dispatch to this module. Remove `UnsupportedInput` expectations for those variants.

- [x] **Step 4: Run Git and regression tests**

Run: `cargo test --test git_loading && cargo test --test input_loading && cargo test --test cli_contract`

Expected: all tests pass; malformed patches remain distinct from valid empty Git diffs.

- [x] **Step 5: Commit executable Git workflows**

```bash
git add src/input src/vcs tests/git_loading.rs tests/input_loading.rs
git commit -m "feat: load native git review inputs"
```

### Task 4: Add large-file policy, moved-line classes, and lazy source endpoints

**Files:**
- Create: `src/vcs/source.rs`
- Modify: `src/core/changeset.rs`
- Modify: `src/diff/model.rs`
- Modify: `src/diff/parser.rs`
- Modify: `src/input/file_pair.rs`
- Modify: `src/input/patch.rs`
- Modify: `src/vcs/git.rs`
- Test: `tests/git_loading.rs`
- Test: `tests/input_loading.rs`

**Interfaces:**
- Consumes: normalized `DiffFile`, Git review inputs, Git patch output, and file-pair paths.
- Produces: explicit `FileStats`, `MovedLineKind`, `SourceSpec`, `SourceReader`, skipped large-file placeholders, and exact Git source endpoints.

- [x] **Step 1: Write failing stats, source, moved-line, and threshold tests**

```rust
#[test]
fn large_tracked_and_untracked_files_are_placeholders_with_stats() {
    let repo = GitFixture::new();
    repo.commit_file("tracked.txt", "base\n");
    repo.write("tracked.txt", &"changed\n".repeat(20_001));
    repo.write("untracked.txt", &"new\n".repeat(20_001));
    let loaded = repo.load(working_tree_input());
    assert!(loaded.changeset.files.iter().all(|file| file.is_too_large));
    assert!(loaded.changeset.files.iter().all(|file| file.hunks.is_empty()));
    assert!(loaded.changeset.files.iter().any(|file| file.stats_truncated));
}

#[test]
fn source_specs_match_staged_worktree_show_and_rename_endpoints() {
    let repo = GitFixture::new();
    repo.commit_file("old.txt", "base\n");
    repo.write("old.txt", "worktree\n");
    let worktree = repo.load(working_tree_input());
    assert!(matches!(worktree.changeset.files[0].old_source, SourceSpec::GitIndex { .. }));
    assert!(matches!(worktree.changeset.files[0].new_source, SourceSpec::File(_)));
    repo.git(["add", "old.txt"]);
    let staged = repo.load(staged_input());
    assert!(matches!(staged.changeset.files[0].old_source, SourceSpec::GitBlob { .. }));
    assert!(matches!(staged.changeset.files[0].new_source, SourceSpec::GitIndex { .. }));
    repo.commit_all("update");
    repo.git(["mv", "old.txt", "new.txt"]);
    let renamed = repo.load(staged_input());
    assert_eq!(renamed.changeset.files[0].previous_path.as_deref(), Some("old.txt"));
    assert!(matches!(&renamed.changeset.files[0].old_source, SourceSpec::GitBlob { path, .. } if path == "old.txt"));
    assert!(matches!(&renamed.changeset.files[0].new_source, SourceSpec::GitIndex { path, .. } if path == "new.txt"));
    let shown = repo.load(ReviewInput::Show { reference: Some("HEAD".into()), pathspecs: vec![], options: CommonOptions::default() });
    assert!(matches!(shown.changeset.files[0].old_source, SourceSpec::GitBlob { .. }));
    assert!(matches!(shown.changeset.files[0].new_source, SourceSpec::GitBlob { .. }));
}

#[test]
fn source_reader_bounds_text_and_returns_none_for_expected_missing_sides() {
    let repo = GitFixture::new();
    repo.commit_file("file.txt", "12345\n");
    let mut reader = SourceReader::new(&SystemCommandRunner, "git", 4);
    assert_eq!(reader.read(&SourceSpec::None).unwrap(), None);
    let spec = SourceSpec::GitBlob { repo_root: repo.path().into(), reference: "HEAD".into(), path: "file.txt".into() };
    assert!(matches!(reader.read(&spec), Err(SourceError::TooLarge { limit: 4 })));
}

#[test]
fn deterministic_git_ansi_colors_become_moved_line_classes() {
    let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n\x1b[1;35m-old\x1b[m\n\x1b[1;36m+new\x1b[m\n";
    let files = ramo::vcs::git::parse_git_patch(patch);
    assert_eq!(files[0].hunks[0].lines[0].moved, Some(MovedLineKind::OldMoved));
    assert_eq!(files[0].hunks[0].lines[1].moved, Some(MovedLineKind::NewMoved));
    assert_eq!(files[0].hunks[0].lines[0].content, "old");
}

#[cfg(unix)]
#[test]
fn difftool_dev_null_side_is_an_added_or_deleted_file() {
    let temp = tempfile::tempdir().unwrap();
    let added = temp.path().join("added.txt");
    std::fs::write(&added, "new\n").unwrap();
    let loaded = load_file_pair("/dev/null", &added, Some("src/added.txt"));
    assert_eq!(loaded.changeset.files[0].change_kind, FileChangeKind::Added);
    assert_eq!(loaded.changeset.files[0].old_source, SourceSpec::None);
    assert_eq!(loaded.changeset.files[0].new_source, SourceSpec::File(added));
}
```

- [x] **Step 2: Run threshold and source tests to verify red**

Run: `cargo test --test git_loading large_ && cargo test --test git_loading source_ && cargo test --test input_loading moved_`

Expected: compilation fails because stats, source specs, and moved classes are absent.

- [x] **Step 3: Extend the normalized model without dynamic runtime objects**

Move `FileStats` into `src/diff/model.rs` and add:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileStats { pub additions: usize, pub deletions: usize }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovedLineKind { OldMoved, OldMovedDimmed, NewMoved, NewMovedDimmed }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SourceSpec {
    None,
    File(PathBuf),
    GitBlob { repo_root: PathBuf, reference: String, path: String },
    GitIndex { repo_root: PathBuf, path: String },
}
```

Add `moved: Option<MovedLineKind>` to `DiffLine`, and `stats: FileStats`, `old_source: SourceSpec`, `new_source: SourceSpec` to `DiffFile`. Parser-created patch inputs default both sources to `None`; direct files use `File(left/right)`. In difftool mode only, `/dev/null` is an absent side, uses `SourceSpec::None`, and produces added/deleted metadata; a misspelled ordinary direct-file path remains an I/O error. `Changeset::stats()` sums `file.stats`, so placeholders contribute without fake hunks.

Implement `SourceReader` with a `HashMap<SourceSpec, Option<String>>` cache. `File` uses a bounded `Read::take(max + 1)`; `GitBlob` runs `git show <ref>:<path>`; `GitIndex` runs `git show :<path>`; `None` returns `Ok(None)`. Exceeding 1,000,000 bytes returns `SourceError::TooLarge`; expected absent Git sides return `Ok(None)` while other failures retain diagnostics.

- [x] **Step 4: Implement Git preflight thresholds and source endpoint resolution**

Run `git diff --no-ext-diff --find-renames --no-color --numstat -z` before the tracked patch. Parse NUL records; skip a tracked file when additions + deletions exceeds 20,000 or its current filesystem path exceeds 1,000,000 bytes. Exclude skipped paths from the patch with `:(exclude)<path>` and append placeholder files with exact stats.

For untracked files, read at most 256 KiB for line counting unless size already exceeds 1,000,000 bytes. Mark incomplete counts with `stats_truncated = true`. Preserve binary placeholders before UTF-8 decoding.

Resolve sources exactly:

- unstaged working tree: old `GitIndex`, new `File`;
- staged: old `HEAD` blob or `None` for an unborn repository, new `GitIndex`;
- single ref versus worktree: old resolved Git blob, new `File`;
- `A..B`: old negative ref blob, new positive ref blob;
- `A...B`: old merge-base blob, new right ref blob;
- `show REF` and `stash show REF`: old `<resolved>^`, new `<resolved>`;
- added/deleted files replace the absent side with `SourceSpec::None`;
- renames use `previous_path` for the old source and current path for the new source.

Before terminal-control stripping, recognize the deterministic Git SGR configuration: magenta old lines and cyan new lines, with `dim` producing the dimmed variants. Strip all control bytes from stored content after attaching the class.

- [x] **Step 5: Run all model and Git tests**

Run: `cargo test --lib && cargo test --test input_loading && cargo test --test git_loading`

Expected: all tests pass, including exact boundary cases at 1,000,000 bytes and 20,000 lines.

- [x] **Step 6: Commit source and large-file behavior**

```bash
git add src/core src/diff src/input src/vcs tests/git_loading.rs tests/input_loading.rs
git commit -m "feat: preserve vcs sources and large diff metadata"
```

### Task 5: Implement Jujutsu detection, review commands, and diagnostics

**Files:**
- Create: `src/vcs/jj.rs`
- Modify: `src/vcs/mod.rs`
- Modify: `src/vcs/detect.rs`
- Modify: `src/input/vcs.rs`
- Test: `tests/jj_loading.rs`
- Test: `tests/vcs_contract.rs`

**Interfaces:**
- Consumes: neutral VCS operations, the command runner, nearest-checkout selection, and shared patch normalization.
- Produces: `JjAdapter`, Jujutsu diff/show command builders, polling-compatible reload data, and Jujutsu-specific failures.

- [x] **Step 1: Write failing Jujutsu builder and fake-executable integration tests**

```rust
#[test]
fn jj_builders_use_git_format_and_fileset_boundary() {
    assert_eq!(
        ramo::vcs::jj::build_jj_diff_args(Some("main..@"), &["src/lib.rs".into()]),
        ["diff", "--git", "-r", "main..@", "--", "src/lib.rs"],
    );
    assert_eq!(
        ramo::vcs::jj::build_jj_show_args(None, &[]),
        ["diff", "--git", "-r", "@"],
    );
}

#[test]
fn jj_staged_and_stash_operations_fail_explicitly() {
    let error = fixture().load_error(staged_input());
    assert!(error.to_string().contains("Jujutsu has no staging area"));
    assert!(error.to_string().contains("Remove `--staged`"));
}

#[test]
fn jj_diff_and_show_load_git_patches_from_the_native_executable() {
    let fixture = FakeVcsFixture::jj();
    fixture.reply(["root"], fixture.repo_root().to_string_lossy());
    fixture.reply(["diff", "--git"], include_str!("fixtures/simple.patch"));
    let loaded = fixture.load(working_tree_input());
    assert_eq!(loaded.changeset.files[0].path, "src/main.rs");
    assert_eq!(loaded.changeset.title, format!("{} working copy", fixture.repo_name()));
    assert!(matches!(loaded.reload_plan, ReloadPlan::Vcs { .. }));
}
```

`FakeVcsFixture` writes a temporary executable using Rust test code, marks it executable on Unix, records each NUL-delimited argv vector to a log, and selects canned stdout/stderr/exit code by the exact args. It also creates `.jj` so adapter selection does not depend on a real `jj` installation.

Add cases for explicit/config-selected JJ outside a repo, missing executable, invalid revset messages (`Failed to parse revset`, `Revision not found`, `Revset expression resolved to no revisions`), a nonzero generic diagnostic, and a path containing shell metacharacters.

- [x] **Step 2: Run Jujutsu tests and verify red**

Run: `cargo test --test jj_loading && cargo test --test vcs_contract jj_`

Expected: compilation fails because `JjAdapter` and the builders are absent.

- [x] **Step 3: Implement the Jujutsu adapter**

```rust
pub fn build_jj_diff_args(range: Option<&str>, pathspecs: &[String]) -> Vec<String> {
    let mut args = vec!["diff".into(), "--git".into()];
    if let Some(range) = range { args.extend(["-r".into(), range.into()]); }
    if !pathspecs.is_empty() { args.push("--".into()); args.extend(pathspecs.iter().cloned()); }
    args
}

pub fn build_jj_show_args(reference: Option<&str>, pathspecs: &[String]) -> Vec<String> {
    let mut args = vec!["diff".into(), "--git".into(), "-r".into(), reference.unwrap_or("@").into()];
    if !pathspecs.is_empty() { args.push("--".into()); args.extend(pathspecs.iter().cloned()); }
    args
}
```

Every command prepends `--no-pager --color never`. Resolve the repo with `jj root`. Working-tree reviews use `jj diff --git`; ranges add `-r`; shows use `jj diff --git -r <ref-or-@>`. Reject staged and stash operations before spawning. Translate missing executable, missing workspace, invalid revset, and generic exit failures into `VcsError` messages naming Jujutsu and the exact user input. Return no Git source specs because JJ does not expose the same stable index/blob endpoints through this contract.

- [x] **Step 4: Run Jujutsu, Git, and config regression tests**

Run: `cargo test --test jj_loading && cargo test --test vcs_contract && cargo test --test git_loading && cargo test --test config_resolution`

Expected: all tests pass and explicit `vcs = "jj"` never silently falls back to Git.

- [x] **Step 5: Commit Jujutsu support**

```bash
git add src/vcs src/input/vcs.rs tests/jj_loading.rs tests/vcs_contract.rs
git commit -m "feat: add native jujutsu reviews"
```

### Task 6: Implement Sapling detection, review commands, and unknown files

**Files:**
- Create: `src/vcs/sl.rs`
- Modify: `src/vcs/mod.rs`
- Modify: `src/vcs/detect.rs`
- Modify: `src/input/vcs.rs`
- Test: `tests/sl_loading.rs`
- Test: `tests/vcs_contract.rs`

**Interfaces:**
- Consumes: adapter dispatch, untracked-file synthesis and thresholds, fake native executable harness, and shared patch normalization.
- Produces: `SaplingAdapter`, `.sl`/Sapling-`.hg` detection, diff/show loading, unknown-file inclusion, and Sapling diagnostics.

- [x] **Step 1: Add failing Sapling contracts**

```rust
#[test]
fn sl_builders_use_git_format_and_show_change() {
    assert_eq!(
        ramo::vcs::sl::build_sl_diff_args(Some("main::."), &["src".into()]),
        ["diff", "--git", "-r", "main::.", "--", "src"],
    );
    assert_eq!(
        ramo::vcs::sl::build_sl_show_args(None, &[]),
        ["diff", "--git", "--change", "."],
    );
}

#[test]
fn upstream_mercurial_marker_is_not_misdetected_as_sapling() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".hg")).unwrap();
    std::fs::write(temp.path().join(".hg/requires"), "revlogv1\n").unwrap();
    assert_ne!(select_vcs(temp.path(), None).map(|value| value.id), Some(VcsId::Sl));
    std::fs::write(temp.path().join(".hg/requires"), "revlogv1\ntreestate\n").unwrap();
    assert_eq!(select_vcs(temp.path(), None).unwrap().id, VcsId::Sl);
}
```

Add fake-executable cases for working copy, `-r` range, `show --change`, `sl status --unknown --print0 --root-relative`, excluded untracked files, binary unknown files, 20,001-line unknown files, staged rejection, missing repo/executable, invalid revset, and generic failure.

- [x] **Step 2: Run Sapling tests and verify red**

Run: `cargo test --test sl_loading && cargo test --test vcs_contract sl_`

Expected: compilation fails because the Sapling adapter/builders are absent.

- [x] **Step 3: Implement Sapling behavior**

All spawned commands begin `sl --noninteractive --color never`. Builders are:

```rust
pub fn build_sl_diff_args(range: Option<&str>, pathspecs: &[String]) -> Vec<String> {
    let mut args = vec!["diff".into(), "--git".into()];
    if let Some(range) = range { args.extend(["-r".into(), range.into()]); }
    append_pathspecs(&mut args, pathspecs);
    args
}

pub fn build_sl_show_args(reference: Option<&str>, pathspecs: &[String]) -> Vec<String> {
    let mut args = vec!["diff".into(), "--git".into(), "--change".into(), reference.unwrap_or(".").into()];
    append_pathspecs(&mut args, pathspecs);
    args
}
```

Resolve the root with `sl root`. Reject staged and stash operations. For working-copy reviews with untracked inclusion enabled, run `sl status --unknown --print0 --root-relative [-- pathspecs]`, parse only `? ` NUL records, filter directory symlinks, and reuse Task 4's filesystem untracked builder and thresholds. `.sl` always identifies Sapling; `.hg` identifies it only when `.hg/requires` contains the complete line `treestate`.

Translate missing repo phrases case-insensitively, invalid revision/revset phrases, missing executable, and generic exit errors without Git fallback.

- [x] **Step 4: Run all three adapter suites**

Run: `cargo test --test sl_loading && cargo test --test jj_loading && cargo test --test git_loading && cargo test --test vcs_contract`

Expected: all tests pass, including same-root selection order and unknown-file policy.

- [x] **Step 5: Commit Sapling support**

```bash
git add src/vcs src/input/vcs.rs tests/sl_loading.rs tests/vcs_contract.rs
git commit -m "feat: add native sapling reviews"
```

### Task 7: Implement diff-aware pager input and safe plain-text fallback

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `src/input/pager.rs`
- Create: `src/pager.rs`
- Modify: `src/input/mod.rs`
- Modify: `src/lib.rs`
- Modify: `src/runtime.rs`
- Modify: `src/error.rs`
- Test: `tests/pager.rs`

**Interfaces:**
- Consumes: `ReviewInput::Pager`, stdin, patch normalization, `ResolvedConfig`, stdout terminal state, and process exit status.
- Produces: `LoadOutcome::{Review, PlainText}`, `looks_like_patch`, `sanitize_terminal_text`, `PagerSpec`, and `page_plain_text`.

- [x] **Step 1: Write failing pager detection, sanitizer, and command-policy tests**

```rust
#[test]
fn patch_detection_accepts_git_unified_and_hunk_only_inputs_after_ansi_removal() {
    assert!(looks_like_patch("\x1b[31mdiff --git a/a b/a\x1b[0m\n"));
    assert!(looks_like_patch("--- a/a\n+++ b/a\n@@ -1 +1 @@\n"));
    assert!(looks_like_patch("heading\n@@ -1 +1 @@\n-old\n+new\n"));
    assert!(!looks_like_patch("ordinary compiler output\n"));
}

#[test]
fn pager_resolution_never_invokes_a_shell_or_recurses_into_ramo() {
    let env = Env::from_iter([("RAMO_TEXT_PAGER", "env LESS=-FRX 'less' -R")]);
    let spec = resolve_text_pager(&env).unwrap();
    assert_eq!(spec.program, "less");
    assert_eq!(spec.args, ["-R"]);
    assert_eq!(spec.env.get("LESS").map(String::as_str), Some("-FRX"));

    let recursive = Env::from_iter([("RAMO_TEXT_PAGER", "/usr/bin/ramo pager")]);
    assert_eq!(resolve_text_pager(&recursive).unwrap().display, "less -R");
}

#[test]
fn sanitizer_removes_osc_controls_but_can_preserve_sgr_styles() {
    let text = "safe\x1b]8;;https://bad\x1b\\link\x1b]8;;\x1b\\\x1b[31m red\x1b[0m\r\n";
    assert_eq!(sanitize_terminal_text(text, false), "safelink red\n");
    assert_eq!(sanitize_terminal_text(text, true), "safelink\x1b[31m red\x1b[0m\n");
}
```

Add loader cases asserting patch-like pager stdin becomes a normal `LoadedReview`, ordinary text becomes `LoadOutcome::PlainText`, and empty ordinary text is valid plain output. Add command-resolution cases for quotes, backslashes, leading `NAME=value`, `env NAME=value`, `RAMO_TEXT_PAGER` precedence over `PAGER`, invalid quoting, `.exe`/`.cmd` recursion checks, and strings containing `;`, `|`, `$()`, and redirects as literal argv rather than operators.

- [x] **Step 2: Run pager unit tests and verify red**

Run: `cargo test --test pager`

Expected: compilation fails because pager modules and `LoadOutcome` do not exist.

- [x] **Step 3: Add the safe argv parser and pager outcome**

Add `shell-words = "1"` to normal dependencies. Define:

```rust
#[derive(Debug)]
pub enum LoadOutcome { Review(LoadedReview), PlainText(String) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub display: String,
}
```

`ReviewLoader::load` returns `LoadOutcome` for all inputs. Existing callers unwrap `Review`; pager reads stdin once, calls `looks_like_patch`, delegates patch input to the shared parser, or returns sanitized source text as `PlainText`.

`resolve_text_pager` reads `RAMO_TEXT_PAGER`, then `PAGER`, then `less -R`. Use `shell_words::split`; consume leading `NAME=value`; support one leading `env` followed by assignments; never invoke a shell. Normalize executable basename by `/` and `\\`, strip `.exe`/`.cmd`, lowercase, and fall back when it equals `ramo` or the parsed command is empty. An invalid explicit pager produces `PagerError::InvalidCommand` naming the source variable.

`sanitize_terminal_text(text, preserve_sgr)` normalizes CRLF, removes OSC sequences, all non-SGR CSI, C0 controls except newline/tab, and DEL. When `preserve_sgr` is true, retain only CSI sequences ending in `m` whose parameters contain digits and semicolons.

- [x] **Step 4: Execute plain text with inherited terminal ownership**

```rust
pub fn page_plain_text(
    text: &str,
    spec: &PagerSpec,
    stdout_is_terminal: bool,
) -> Result<ExitCode, PagerError> {
    if !stdout_is_terminal {
        print!("{}", sanitize_terminal_text(text, false));
        std::io::stdout().flush()?;
        return Ok(ExitCode::SUCCESS);
    }
    let mut child = Command::new(&spec.program).args(&spec.args).envs(&spec.env)
        .stdin(Stdio::piped()).stdout(Stdio::inherit()).stderr(Stdio::inherit())
        .spawn().map_err(|source| PagerError::Spawn { display: spec.display.clone(), source })?;
    child.stdin.take().expect("piped stdin").write_all(sanitize_terminal_text(text, true).as_bytes())?;
    let status = child.wait()?;
    Ok(exit_code_from_status(status))
}
```

On Unix, `exit_code_from_status` maps signals to `128 + signal`; otherwise it uses the child code or 1. Runtime branches on `LoadOutcome` before TTY replacement or Ratatui initialization. Pager failures become `AppError::Pager` with exit code 1; a pager child nonzero/signal status is returned unchanged.

- [x] **Step 5: Run pager and runtime regressions**

Run: `cargo test --test pager && cargo test --test runtime_resolution && cargo test --all-targets`

Expected: all tests pass and plain text never emits an alternate-screen control sequence.

- [x] **Step 6: Commit pager fallback**

```bash
git add Cargo.toml Cargo.lock src/input src/pager.rs src/lib.rs src/runtime.rs src/error.rs tests/pager.rs
git commit -m "feat: add safe diff-aware pager fallback"
```

### Task 8: Verify black-box VCS and pager terminal behavior

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `tests/pty_pager.rs`
- Create: `tests/cli_vcs.rs`
- Modify: `tests/cli_contract.rs`

**Interfaces:**
- Consumes: the compiled `ramo` executable, native temporary Git fixtures, and pager child dispatch.
- Produces: end-to-end proof that failures occur before terminal startup, diff pager launches Ratatui, text pager owns the terminal, and exit codes propagate.

- [x] **Step 1: Add failing CLI and PTY tests**

Add `portable-pty = "0.9"` to dev-dependencies. In `tests/cli_vcs.rs`, add:

```rust
#[test]
fn invalid_git_ref_fails_before_terminal_startup() {
    let repo = GitFixture::with_commit();
    Command::cargo_bin("ramo").unwrap().current_dir(repo.path())
        .args(["show", "does-not-exist"])
        .assert().failure()
        .stderr(predicate::str::contains("does-not-exist").and(predicate::str::contains("Check the ref")))
        .stdout(predicate::str::contains("\x1b[?1049h").not());
}

#[test]
fn git_show_reaches_the_native_loader_without_terminal_startup_for_an_empty_pathspec() {
    let repo = GitFixture::with_commit();
    Command::cargo_bin("ramo").unwrap().current_dir(repo.path())
        .args(["show", "HEAD", "--", "absent-path"])
        .assert().success()
        .stderr(predicate::str::contains("No changes to review."))
        .stdout(predicate::str::contains("\x1b[?1049h").not());
}
```

In `tests/pty_pager.rs`, create an 80x24 PTY, spawn the built binary, and enforce five-second read deadlines. The tests use `PtyProcess::spawn(cwd, args, env)`, `send`, `send_eof`, `read_until`, and `wait`, defined in Step 3. Add:

```rust
#[test]
fn patch_pager_enters_review_ui_and_quits_cleanly() {
    let cwd = std::env::current_dir().unwrap();
    let mut process = PtyProcess::spawn(&cwd, &["pager"], &[]);
    process.send(include_str!("fixtures/simple.patch"));
    process.send_eof();
    let rendered = process.read_until("NORMAL");
    assert!(rendered.contains("src/main.rs"));
    process.send("q");
    assert_eq!(process.wait(), 0);
    assert!(process.raw().windows(8).any(|bytes| bytes == b"\x1b[?1049h"));
    assert!(process.raw().windows(8).any(|bytes| bytes == b"\x1b[?1049l"));
}

#[test]
fn plain_text_pager_does_not_enter_alternate_screen() {
    let temp = tempfile::tempdir().unwrap();
    let helper = write_helper(temp.path(), "capture", "#!/bin/sh\ncat\n");
    let mut process = PtyProcess::spawn(temp.path(), &["pager"], &[("RAMO_TEXT_PAGER", helper.to_str().unwrap())]);
    process.send("safe\x1b]8;;https://bad\x1b\\text\x1b]8;;\x1b\\\n");
    process.send_eof();
    let output = process.read_until("safetext");
    assert_eq!(process.wait(), 0);
    assert!(!output.contains("https://bad"));
    assert!(!process.raw().windows(8).any(|bytes| bytes == b"\x1b[?1049h"));
}

#[test]
fn pager_nonzero_exit_code_is_propagated() {
    let temp = tempfile::tempdir().unwrap();
    let helper = write_helper(temp.path(), "fail", "#!/bin/sh\ncat >/dev/null\nexit 23\n");
    let mut process = PtyProcess::spawn(temp.path(), &["pager"], &[("RAMO_TEXT_PAGER", helper.to_str().unwrap())]);
    process.send("ordinary text\n");
    process.send_eof();
    assert_eq!(process.wait(), 23);
}

#[test]
fn recursive_pager_setting_uses_fallback_without_spawning_ramo_again() {
    let temp = tempfile::tempdir().unwrap();
    write_helper(temp.path(), "less", "#!/bin/sh\ncat\nprintf 'LESS_CALLED\\n'\n");
    let path = format!("{}:{}", temp.path().display(), std::env::var("PATH").unwrap());
    let mut process = PtyProcess::spawn(temp.path(), &["pager"], &[("RAMO_TEXT_PAGER", "ramo pager"), ("PATH", &path)]);
    process.send("ordinary text\n");
    process.send_eof();
    assert!(process.read_until("LESS_CALLED").contains("ordinary text"));
    assert_eq!(process.wait(), 0);
}

#[cfg(unix)]
#[test]
fn ctrl_c_terminated_pager_maps_to_130() {
    let temp = tempfile::tempdir().unwrap();
    let helper = write_helper(temp.path(), "interrupt", "#!/bin/sh\ncat >/dev/null\nkill -INT $$\n");
    let mut process = PtyProcess::spawn(temp.path(), &["pager"], &[("RAMO_TEXT_PAGER", helper.to_str().unwrap())]);
    process.send("ordinary text\n");
    process.send_eof();
    assert_eq!(process.wait(), 130);
}
```

- [x] **Step 2: Run black-box tests and confirm missing PTY coverage**

Run: `cargo test --test cli_vcs && cargo test --test pty_pager -- --nocapture`

Expected: compilation fails until the PTY harness and portable-pty dev dependency are added.

- [x] **Step 3: Implement deterministic PTY helpers and close runtime gaps**

The PTY helper must use this concrete shape:

```rust
struct PtyProcess {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn std::io::Write + Send>,
    chunks: std::sync::mpsc::Receiver<Vec<u8>>,
    raw: Vec<u8>,
}

impl PtyProcess {
    fn spawn(cwd: &std::path::Path, args: &[&str], env: &[(&str, &str)]) -> Self {
        let pair = portable_pty::native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin("ramo"));
        command.cwd(cwd);
        args.iter().for_each(|arg| { command.arg(arg); });
        env.iter().for_each(|(key, value)| { command.env(key, value); });
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let writer = pair.master.take_writer().unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let (sender, chunks) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 { break; }
                if sender.send(buffer[..count].to_vec()).is_err() { break; }
            }
        });
        Self { child, writer, chunks, raw: Vec::new() }
    }

    fn send(&mut self, text: &str) { self.writer.write_all(text.as_bytes()).unwrap(); self.writer.flush().unwrap(); }
    fn send_eof(&mut self) { self.send("\u{4}"); }
    fn raw(&self) -> &[u8] { &self.raw }
    fn read_until(&mut self, needle: &str) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let chunk = self.chunks.recv_timeout(remaining).expect("PTY output deadline");
            self.raw.extend(chunk);
            let clean = String::from_utf8_lossy(&self.raw).replace(|character: char| character == '\u{1b}', "");
            if clean.contains(needle) { return clean; }
        }
    }
    fn wait(&mut self) -> u32 { self.child.wait().unwrap().exit_code() }
}

#[cfg(unix)]
fn write_helper(directory: &std::path::Path, name: &str, source: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = directory.join(name);
    std::fs::write(&path, source).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}
```

Use a reader thread plus `recv_timeout(Duration::from_secs(5))`; do not sleep. Strip ANSI only for assertions, retain raw bytes to detect alternate-screen entry/exit. Always close the writer and wait/kill the child on assertion failure so tests cannot hang. Helper pager executables are created by the test in a temp directory and made executable with `PermissionsExt`; Windows-only cases use `.cmd` and skip Unix signal assertions with `#[cfg(unix)]`.

Fix only behavior exposed by these tests: no Ratatui initialization on CLI/VCS/pager errors, exactly one terminal restoration sequence after diff-pager quit, inherited child stdout/stderr, and propagated status.

- [x] **Step 4: Run black-box and full regression suites**

Run: `cargo test --test cli_vcs && cargo test --test pty_pager -- --nocapture && cargo test --all-targets`

Expected: all tests pass with no timeout and no orphaned pager process.

- [x] **Step 5: Commit black-box coverage**

```bash
git add Cargo.toml Cargo.lock src tests/cli_vcs.rs tests/pty_pager.rs tests/cli_contract.rs
git commit -m "test: verify vcs and pager process contracts"
```

### Task 9: Close slice-2 documentation and verification evidence

**Files:**
- Modify: `README.md`
- Modify: `docs/parity/hunk.md`
- Modify: `docs/superpowers/plans/2026-07-20-vcs-pager-implementation-plan.md`

**Interfaces:**
- Consumes: all slice-2 tests, release artifact, Hunk reference behavior, and parity ledger status definitions.
- Produces: truthful user documentation and one evidence-backed checkpoint for the next delivery slice.

- [x] **Step 1: Audit every slice-2 ledger row before changing status**

Run:

```bash
rg -n "Git working|Git staged|Git untracked|Git show|Git moved|Jujutsu|Sapling|source fetch|Large-file|pager|difftool" docs/parity/hunk.md
```

Expected: the slice-2 rows are still `missing` or `implemented`; record each exact test that will justify `verified`.

- [x] **Step 2: Update documentation with only verified commands**

Document these command examples after their tests pass:

```bash
ramo diff
ramo diff --staged
ramo diff main...HEAD -- src
ramo show HEAD~1
ramo stash show stash@{0}
git diff --no-color | ramo pager
RAMO_TEXT_PAGER="less -R" command-producing-text | ramo pager
```

Explain automatic nearest-repository detection, `vcs = "git"|"jj"|"sl"` override, JJ/SL staged rejection, untracked exclusion, 1 MB/20,000-line placeholders, and safe plain-text pager argv parsing. Keep watch, UI replacement, notes, STML, sessions, and release-parity claims explicitly staged.

In `docs/parity/hunk.md`, mark a row `verified` only when the evidence column names a passing test. Split combined rows when one sub-behavior lacks evidence. Keep watcher execution `missing`; mark only `ReloadPlan::Vcs` as implemented because observation is slice 4.

- [x] **Step 3: Run the complete quality and release gate**

Run: `cargo fmt --all -- --check`

Expected: exit 0 with no diff.

Run: `cargo clippy --all-targets --all-features -- -D warnings`

Expected: exit 0 with no warning.

Run: `cargo test --all-targets`

Expected: every library, integration, CLI, Git, JJ, SL, pager, and PTY test passes with zero ignored slice-2 tests.

Run: `cargo build --release`

Expected: exit 0 and `target/release/ramo` exists.

Run on Linux: `file target/release/ramo && ldd target/release/ramo`

Run on macOS instead: `file target/release/ramo && otool -L target/release/ramo`

Expected: one native executable; no Node, Bun, JavaScript engine, or Hunk runtime dependency is listed.

- [x] **Step 4: Perform real-tool smoke checks**

Run `target/release/ramo diff` in this worktree inside a PTY, wait for the status line, send `q`, and assert exit 0. Run `target/release/ramo show HEAD`, `target/release/ramo diff --staged`, and pipe `printf 'plain text\n'` through `RAMO_TEXT_PAGER='less -FRX' target/release/ramo pager`. If `jj` or `sl` is installed, smoke its matching fixture; their deterministic fake-executable integration tests remain the portability authority when absent.

Expected: each available native path exits normally, owns/restores the terminal once, and shows no TypeScript/runtime process.

Verification record (2026-07-20): the complete format, strict Clippy, all-target test, and release build gate passed. The 4,346,576-byte Linux artifact was identified as one x86-64 ELF and `ldd` listed only `libgcc_s`, `libm`, `libc`, and the platform loader. Release-binary PTY smoke checks for `diff` and `show HEAD`, the staged-empty path, and `RAMO_TEXT_PAGER='less -FRX'` plain-text paging all exited 0. Neither `jj` nor `sl` was installed; their passing scripted-runner integration suites remain the portable native-command evidence.

- [x] **Step 5: Commit the slice-2 evidence**

```bash
git add README.md docs/parity/hunk.md docs/superpowers/plans/2026-07-20-vcs-pager-implementation-plan.md
git commit -m "docs: verify native vcs and pager parity"
```

## Self-review record

- **Spec coverage:** Tasks 1–6 cover adapter contracts, nearest detection/config override, Git working tree/staged/ranges/show/stash, JJ and Sapling operations, untracked/binary/large files, exact Git source sides, moved-line classification, and operation-specific errors. Tasks 7–8 cover diff-aware pager detection, sanitized non-diff fallback, shell-free argv parsing, recursion prevention, exit/signal propagation, terminal ownership, and PTY evidence. Task 9 closes documentation and the parity ledger. Difftool execution remains covered by the foundation file-pair loader and is re-run in the full regression gate.
- **Explicitly out of scope:** Source endpoint consumption for context expansion and new geometry belongs to slice 3. Filesystem observation, debounce, polling loops, serialized reload, and state preservation are slice 4; this plan supplies and tests the exact source data/read API and reload plan.
- **Dependency audit:** `shell-words` is a small Rust argv parser used without a shell. `portable-pty` is dev-only. Neither adds a runtime interpreter or second shipped executable.
- **Type consistency:** `VcsId` lives in `core::input`; config, detection, adapters, and errors use that one enum. All loaders receive `LoadContext`; all adapter results are `VcsPatch`; all user-visible load paths return `LoadOutcome`; watcher work consumes `ReloadPlan::Vcs` in slice 4.
- **Temporary behavior removed:** The foundation `UnsupportedInput` branches for Git/JJ/SL/pager are gone. Only adapter-operation mismatches such as JJ/SL stash and staged requests remain explicit product errors.
