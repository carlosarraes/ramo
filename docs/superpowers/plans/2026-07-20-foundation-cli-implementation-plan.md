# Foundation and CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the Rust-only library, domain model, Hunk-shaped review CLI, layered configuration, patch/file loaders, and legacy `ramo` compatibility that every remaining parity slice builds on.

**Architecture:** Keep the existing Ratatui reviewer operational while moving process concerns out of `main.rs`. Parse CLI/config into a normalized `ReviewInput`, load every implemented source into one `Changeset`, and pass only normalized files plus output policy into the existing app. Define the VCS loader seam now; the next plan fills it with Git, jj, and Sapling adapters.

**Tech Stack:** Rust 2024, Clap 4 derive API, Serde, `toml`, `similar`, Ratatui 0.29, Crossterm 0.28, Syntect 5, standard-library filesystem/process APIs.

## Global Constraints

- The implementation is 100% Rust.
- Installation produces one `ramo` executable. It must not require Node.js, Bun, TypeScript, a browser runtime, or a separately installed helper service.
- `ramo daemon serve` and all session-broker behavior execute from that same binary.
- Git, Jujutsu, and Sapling executables are optional external tools. A VCS executable is required only when a user invokes a workflow backed by that VCS.
- The executable remains named `ramo`; Hunk command shapes are exposed beneath that name.
- Linux, macOS, and Windows are supported. PTY-only features may use platform-specific implementations behind common Rust interfaces.
- Existing `ramo` features remain available unless they conflict with Hunk parity. On a conflict, the Hunk-compatible command or key wins and the existing action receives a new non-conflicting binding.
- Existing `git diff | ramo`, `--input`, `--output`, and `--stdout` usage remains compatible.
- Hunk's top menu bar, dropdown menus, and menu-specific shortcuts are not ported. Their underlying actions remain accessible through direct shortcuts, dialogs, or command-line options.

## Program roadmap

This is plan 1 of 7. Each plan consumes stable interfaces from the preceding plans and ends in a usable executable.

| Plan | Deliverable | Depends on |
|---|---|---|
| 1. Foundation and CLI | Library surface, normalized inputs/models, config, patch and file loading | Current repository |
| 2. VCS and pager | Git/jj/sl adapters, show/stash/difftool, untracked/binary/large files, pager fallback | `ReviewInput`, `Changeset`, `ReviewLoader` |
| 3. Review UI | Continuous stream, sidebar/filter, auto/split/stack, full controls, mouse, themes | `Changeset`, stable file/hunk ids, source fetchers |
| 4. Watch and process integration | Reload plans, watcher/polling, editor/job control, Pi/tmux/clipboard lifecycle | normalized input and review controller |
| 5. Notes and markup | Agent context, inline notes, Markdown export, deterministic STML | stable row/hunk targets and review state |
| 6. Sessions | Same-binary daemon and complete session CLI | live note/review projections and reload API |
| 7. Parity closure | Performance, cross-platform hardening, docs, install, exhaustive audit | all preceding plans |

## File structure for this plan

- `src/lib.rs`: reusable crate exports; no process startup.
- `src/main.rs`: parse arguments and delegate to `runtime::run`.
- `src/error.rs`: typed application errors and process exit classification.
- `src/core/changeset.rs`: normalized changeset/file metadata.
- `src/core/input.rs`: normalized review input and common view/output options.
- `src/cli/args.rs`: Clap-only argument structs.
- `src/cli/normalize.rs`: convert Clap structs into normalized input without I/O beyond operand classification.
- `src/config/model.rs`: deserializable partial config and concrete resolved preferences.
- `src/config/load.rs`: layered config discovery, parsing, and merging.
- `src/input/mod.rs`: `ReviewLoader` boundary and dispatch.
- `src/input/patch.rs`: stdin/file patch loading and normalization.
- `src/input/file_pair.rs`: direct text-file comparison and binary placeholders.
- `src/runtime.rs`: non-UI startup actions, TTY handoff, app launch, and annotation output.
- `tests/cli_contract.rs`: black-box CLI help/version/error contract.
- `tests/fixtures/`: committed, deterministic patch and file-pair inputs.
- `docs/parity/hunk.md`: authoritative feature-status matrix.

---

### Task 1: Split the reusable library from process startup

**Files:**
- Create: `src/lib.rs`
- Create: `tests/library_surface.rs`
- Modify: `src/main.rs:1`
- Modify: `Cargo.toml:1`

**Interfaces:**
- Consumes: existing modules declared privately in `src/main.rs`.
- Produces: public crate modules addressable as `ramo::diff`, `ramo::annotations`, `ramo::app`, `ramo::ui`, and integrations used by later tasks.

- [ ] **Step 1: Write the failing public-library smoke test**

```rust
// tests/library_surface.rs
use ramo::diff::parser::parse_unified_diff;

#[test]
fn parser_is_available_from_the_library_crate() {
    let files = parse_unified_diff(
        "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
    );
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "a.txt");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test library_surface`

Expected: compilation fails because no library target named `ramo` exists.

- [ ] **Step 3: Add the library entry point and switch the binary to imports**

```rust
// src/lib.rs
pub mod annotations;
pub mod app;
pub mod clipboard;
pub mod diff;
pub mod pi_extension;
pub mod tmux;
pub mod ui;
pub mod vim;
```

Delete the eight `mod ...;` declarations from `src/main.rs` and replace internal imports with:

```rust
use ramo::annotations::output;
use ramo::app::App;
use ramo::diff::parser::parse_unified_diff;
use ramo::pi_extension;
```

Add an explicit library target so package intent is visible:

```toml
[lib]
name = "ramo"
path = "src/lib.rs"
```

- [ ] **Step 4: Verify the split without changing behavior**

Run: `cargo test --all-targets`

Expected: 17 tests pass, including `parser_is_available_from_the_library_crate`.

- [ ] **Step 5: Commit the library boundary**

```bash
git add Cargo.toml src/lib.rs src/main.rs tests/library_surface.rs
git commit -m "refactor: expose ramo as a reusable rust library"
```

---

### Task 2: Introduce the normalized changeset model

**Files:**
- Create: `src/core/mod.rs`
- Create: `src/core/changeset.rs`
- Create: `src/core/input.rs`
- Modify: `src/lib.rs:1`
- Modify: `src/diff/model.rs:1`
- Modify: `src/diff/parser.rs:21`
- Modify: `src/ui/side_by_side.rs:158`
- Test: `src/core/changeset.rs`
- Test: `src/diff/parser.rs`

**Interfaces:**
- Consumes: parsed `DiffFile`, `Hunk`, and `DiffLine` values.
- Produces: `Changeset`, `FileChangeKind`, `FileStats`, `ReviewInput`, `InputKind`, `CommonOptions`, `LayoutMode`, and `ReviewOutput`.

- [ ] **Step 1: Add failing model tests**

```rust
// Append inside src/core/changeset.rs after defining the test imports but before implementation.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::model::{DiffFile, FileChangeKind};

    #[test]
    fn changeset_totals_additions_and_deletions() {
        let file = DiffFile::for_test("src/lib.rs", FileChangeKind::Modified, 3, 2);
        let changeset = Changeset::new("working-tree", "Working tree", vec![file]);
        assert_eq!(changeset.stats(), FileStats { additions: 3, deletions: 2 });
    }

    #[test]
    fn file_ids_are_stable_across_reloads() {
        assert_eq!(stable_file_id("src/lib.rs", None), stable_file_id("src/lib.rs", None));
        assert_ne!(
            stable_file_id("src/lib.rs", None),
            stable_file_id("src/lib.rs", Some("src/old.rs")),
        );
    }
}
```

- [ ] **Step 2: Run the focused tests to verify they fail**

Run: `cargo test core::changeset`

Expected: compilation fails because the `core` module and normalized types do not exist.

- [ ] **Step 3: Define the normalized model**

Create these exact public types:

```rust
// src/core/changeset.rs
use crate::diff::model::{DiffFile, LineType};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileStats {
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone)]
pub struct Changeset {
    pub id: String,
    pub source_label: String,
    pub title: String,
    pub summary: Option<String>,
    pub agent_summary: Option<String>,
    pub files: Vec<DiffFile>,
}

impl Changeset {
    pub fn new(source_label: impl Into<String>, title: impl Into<String>, files: Vec<DiffFile>) -> Self {
        let source_label = source_label.into();
        let title = title.into();
        Self {
            id: format!("{source_label}:{title}"),
            source_label,
            title,
            summary: None,
            agent_summary: None,
            files,
        }
    }

    pub fn stats(&self) -> FileStats {
        self.files.iter().flat_map(|file| &file.hunks).flat_map(|hunk| &hunk.lines).fold(
            FileStats::default(),
            |mut stats, line| {
                match line.kind {
                    LineType::Addition => stats.additions += 1,
                    LineType::Deletion => stats.deletions += 1,
                    LineType::Context => {}
                }
                stats
            },
        )
    }
}

pub fn stable_file_id(path: &str, previous_path: Option<&str>) -> String {
    match previous_path {
        Some(previous) => format!("file:{previous}->{path}"),
        None => format!("file:{path}"),
    }
}
```

```rust
// src/core/input.rs
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutMode { #[default] Auto, Split, Stack }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind { Diff, Show, StashShow, Patch, Pager, Difftool }

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommonOptions {
    pub mode: Option<LayoutMode>,
    pub theme: Option<String>,
    pub agent_context: Option<PathBuf>,
    pub pager: Option<bool>,
    pub watch: Option<bool>,
    pub exclude_untracked: Option<bool>,
    pub line_numbers: Option<bool>,
    pub wrap_lines: Option<bool>,
    pub hunk_headers: Option<bool>,
    pub agent_notes: Option<bool>,
    pub transparent_background: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewOutput {
    pub markdown_path: Option<PathBuf>,
    pub stdout: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource { Stdin, File(PathBuf) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewInput {
    VcsDiff { range: Option<String>, staged: bool, pathspecs: Vec<String>, options: CommonOptions },
    Show { reference: Option<String>, pathspecs: Vec<String>, options: CommonOptions },
    StashShow { reference: Option<String>, options: CommonOptions },
    FilePair { left: PathBuf, right: PathBuf, display_path: Option<PathBuf>, options: CommonOptions },
    Patch { source: PatchSource, options: CommonOptions },
    Pager { options: CommonOptions },
}

impl ReviewInput {
    pub fn kind(&self) -> InputKind {
        match self {
            Self::VcsDiff { .. } => InputKind::Diff,
            Self::Show { .. } => InputKind::Show,
            Self::StashShow { .. } => InputKind::StashShow,
            Self::FilePair { display_path: Some(_), .. } => InputKind::Difftool,
            Self::FilePair { .. } => InputKind::Diff,
            Self::Patch { .. } => InputKind::Patch,
            Self::Pager { .. } => InputKind::Pager,
        }
    }
}
```

Export the modules from `src/core/mod.rs` and `src/lib.rs`.

- [ ] **Step 4: Replace boolean change state with one enum**

Change `DiffFile` to the following shape and update parser constructors and renderer field names in the same edit:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind { Modified, Added, Deleted, Renamed, Copied }

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub id: String,
    pub path: String,
    pub previous_path: Option<String>,
    pub patch: String,
    pub hunks: Vec<Hunk>,
    pub change_kind: FileChangeKind,
    pub is_binary: bool,
    pub is_untracked: bool,
    pub is_too_large: bool,
    pub stats_truncated: bool,
    pub language: Option<String>,
}
```

In `parse_file`, derive `change_kind` from new/deleted/rename/copy headers, set `id` with `stable_file_id`, retain the consumed file chunk in `patch`, and replace renderer uses of `old_path` with `previous_path`. Add parser assertions for added, deleted, renamed, copied, and binary files.

- [ ] **Step 5: Add a test-only constructor without leaking fixtures into production behavior**

```rust
#[cfg(test)]
impl DiffFile {
    pub fn for_test(path: &str, change_kind: FileChangeKind, additions: usize, deletions: usize) -> Self {
        use super::{DiffLine, Hunk, LineType};
        let mut lines = Vec::new();
        lines.extend((0..additions).map(|index| DiffLine {
            kind: LineType::Addition,
            content: format!("added {index}"),
            old_lineno: None,
            new_lineno: Some(index as u32 + 1),
        }));
        lines.extend((0..deletions).map(|index| DiffLine {
            kind: LineType::Deletion,
            content: format!("deleted {index}"),
            old_lineno: Some(index as u32 + 1),
            new_lineno: None,
        }));
        Self {
            id: crate::core::changeset::stable_file_id(path, None),
            path: path.into(), previous_path: None, patch: String::new(),
            hunks: vec![Hunk { old_start: 1, new_start: 1, header: "@@ -1 +1 @@".into(), lines }],
            change_kind, is_binary: false, is_untracked: false, is_too_large: false,
            stats_truncated: false, language: None,
        }
    }
}
```

- [ ] **Step 6: Verify all model and parser behavior**

Run: `cargo test core::changeset && cargo test diff::parser`

Expected: all model tests and the existing parser suite pass with the normalized fields.

- [ ] **Step 7: Commit the normalized domain model**

```bash
git add src/lib.rs src/core src/diff/model.rs src/diff/parser.rs src/ui/side_by_side.rs
git commit -m "refactor: normalize changesets and review inputs"
```

---

### Task 3: Parse the Hunk-shaped review command surface

**Files:**
- Create: `src/cli/mod.rs`
- Create: `src/cli/args.rs`
- Create: `src/cli/normalize.rs`
- Create: `tests/cli_parse.rs`
- Modify: `src/lib.rs:1`
- Modify: `Cargo.toml:9`

**Interfaces:**
- Consumes: `ReviewInput`, `CommonOptions`, `LayoutMode`, `PatchSource`, and `ReviewOutput` from Task 2.
- Produces: `cli::parse_from<I, T>(args, stdin_is_terminal) -> Result<Invocation, CliError>` where `Invocation` contains `Action` and `ReviewOutput`.

- [ ] **Step 1: Write failing parser-contract tests**

```rust
// tests/cli_parse.rs
use ramo::cli::{parse_from, Action};
use ramo::core::input::{LayoutMode, PatchSource, ReviewInput};

#[test]
fn bare_pipe_is_patch_stdin() {
    let invocation = parse_from(["ramo"], false).unwrap();
    assert!(matches!(invocation.action, Action::Review(ReviewInput::Patch { source: PatchSource::Stdin, .. })));
}

#[test]
fn diff_supports_range_flags_and_pathspecs() {
    let invocation = parse_from(
        ["ramo", "diff", "main...HEAD", "--mode", "split", "--watch", "--", "src", "tests"],
        true,
    ).unwrap();
    let Action::Review(ReviewInput::VcsDiff { range, staged, pathspecs, options }) = invocation.action else { panic!("expected vcs diff") };
    assert_eq!(range.as_deref(), Some("main...HEAD"));
    assert!(!staged);
    assert_eq!(pathspecs, ["src", "tests"]);
    assert_eq!(options.mode, Some(LayoutMode::Split));
    assert_eq!(options.watch, Some(true));
}

#[test]
fn existing_two_file_operands_become_a_file_pair() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("before.rs");
    let right = temp.path().join("after.rs");
    std::fs::write(&left, "old\n").unwrap();
    std::fs::write(&right, "new\n").unwrap();
    let invocation = parse_from(["ramo".into(), "diff".into(), left.into(), right.clone().into()], true).unwrap();
    assert!(matches!(invocation.action, Action::Review(ReviewInput::FilePair { right: value, .. }) if value == right));
}

#[test]
fn legacy_input_and_output_flags_remain_accepted() {
    let invocation = parse_from(["ramo", "--input", "review.patch", "--output", "review.md"], true).unwrap();
    assert!(matches!(invocation.action, Action::Review(ReviewInput::Patch { source: PatchSource::File(_), .. })));
    assert_eq!(invocation.output.markdown_path.unwrap(), std::path::PathBuf::from("review.md"));
}

#[test]
fn more_than_two_diff_targets_is_rejected() {
    let error = parse_from(["ramo", "diff", "one", "two", "three"], true).unwrap_err();
    assert!(error.to_string().contains("one revision or two existing files"));
}
```

- [ ] **Step 2: Add test-only CLI dependencies and verify red**

```toml
[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
```

Run: `cargo test --test cli_parse`

Expected: compilation fails because `ramo::cli` is absent.

- [ ] **Step 3: Define the Clap-only argument tree**

`src/cli/args.rs` defines the complete review argument tree below. Pathspec fields accept values only after `--`, paired positive/negative booleans override one another, and `disable_version_flag = true` keeps `-v` available for the explicit version action.

```rust
#[derive(Debug, Parser)]
#[command(name = "ramo", version, disable_version_flag = true)]
pub struct Cli {
    #[arg(short = 'v', long = "version", action = clap::ArgAction::Version)]
    pub version: Option<bool>,
    #[arg(short, long)] pub input: Option<PathBuf>,
    #[arg(short, long)] pub output: Option<PathBuf>,
    #[arg(long)] pub stdout: bool,
    #[command(subcommand)] pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Diff(DiffArgs), Show(ShowArgs), Patch(PatchArgs), Pager(PagerArgs),
    Difftool(DifftoolArgs),
    Stash { #[command(subcommand)] command: StashCommand },
    Install(IntegrationArgs), Uninstall(IntegrationArgs),
}

#[derive(Debug, Subcommand)]
pub enum StashCommand { Show(StashShowArgs) }
```

Every review argument struct flattens this exact common flag set; only reloadable command structs add `--watch`:

```rust
#[derive(Debug, Clone, Default, Args)]
pub struct ReviewFlags {
    #[arg(long, value_enum)] pub mode: Option<LayoutArg>,
    #[arg(long)] pub theme: Option<String>,
    #[arg(long)] pub agent_context: Option<PathBuf>,
    #[arg(long)] pub pager: bool,
    #[arg(long, overrides_with = "no_line_numbers")] pub line_numbers: bool,
    #[arg(long, overrides_with = "line_numbers")] pub no_line_numbers: bool,
    #[arg(long, overrides_with = "no_wrap")] pub wrap: bool,
    #[arg(long, overrides_with = "wrap")] pub no_wrap: bool,
    #[arg(long, overrides_with = "no_hunk_headers")] pub hunk_headers: bool,
    #[arg(long, overrides_with = "hunk_headers")] pub no_hunk_headers: bool,
    #[arg(long, overrides_with = "no_agent_notes")] pub agent_notes: bool,
    #[arg(long, overrides_with = "agent_notes")] pub no_agent_notes: bool,
    #[arg(long, overrides_with = "no_transparent_bg")] pub transparent_bg: bool,
    #[arg(long, overrides_with = "transparent_bg")] pub no_transparent_bg: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LayoutArg { Auto, Split, Stack }

#[derive(Debug, Args)]
pub struct DiffArgs {
    #[command(flatten)] pub review: ReviewFlags,
    #[arg(long)] pub watch: bool,
    #[arg(long)] pub staged: bool,
    #[arg(long)] pub cached: bool,
    #[arg(long, overrides_with = "no_exclude_untracked")] pub exclude_untracked: bool,
    #[arg(long, overrides_with = "exclude_untracked")] pub no_exclude_untracked: bool,
    #[arg(value_name = "TARGET", num_args = 0..=2)] pub targets: Vec<String>,
    #[arg(last = true, value_name = "PATHSPEC")] pub pathspecs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    #[command(flatten)] pub review: ReviewFlags,
    #[arg(long)] pub watch: bool,
    #[arg(value_name = "REF")] pub reference: Option<String>,
    #[arg(last = true, value_name = "PATHSPEC")] pub pathspecs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct StashShowArgs {
    #[command(flatten)] pub review: ReviewFlags,
    #[arg(value_name = "REF")] pub reference: Option<String>,
}

#[derive(Debug, Args)]
pub struct PatchArgs {
    #[command(flatten)] pub review: ReviewFlags,
    #[arg(value_name = "FILE")] pub file: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct PagerArgs { #[command(flatten)] pub review: ReviewFlags }

#[derive(Debug, Args)]
pub struct DifftoolArgs {
    #[command(flatten)] pub review: ReviewFlags,
    #[arg(long)] pub watch: bool,
    pub left: PathBuf,
    pub right: PathBuf,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct IntegrationArgs { pub target: String }
```

Using `StashShowArgs` keeps `stash show` from accidentally accepting watch or pathspec options.

- [ ] **Step 4: Normalize arguments without starting the TUI**

Define these exact public outputs in `src/cli/mod.rs`:

```rust
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Print(String),
    Review(ReviewInput),
    InstallPi,
    UninstallPi,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub action: Action,
    pub output: ReviewOutput,
}

pub fn parse_from<I, T>(args: I, stdin_is_terminal: bool) -> Result<Invocation, CliError>
where I: IntoIterator<Item = T>, T: Into<std::ffi::OsString> + Clone;
```

`normalize.rs` converts paired flags into `Option<bool>` by preserving absence, distinguishes one VCS target from two existing file operands, maps `--staged` and `--cached` to one boolean, and rejects `--input` combined with a subcommand. Define `CliError` with concrete variants `Parse(clap::Error)`, `ConflictingInput`, `InvalidDiffTargets(Vec<String>)`, and `UnsupportedIntegration(String)` and implement `Display` plus `Error`.

`parse_from` catches Clap `DisplayHelp` and `DisplayVersion` results and returns `Action::Print(rendered_text)` so they exit successfully. Bare `ramo` on a terminal also returns the rendered top-level help; bare `ramo` with non-terminal stdin returns patch-stdin review.

- [ ] **Step 5: Verify the complete parser contract**

Add cases for `show`, `stash show`, `patch -`, `pager`, `difftool`, `--cached`, all paired booleans, invalid layout values, `install pi`, `uninstall pi`, and rejected integration targets.

Run: `cargo test --test cli_parse`

Expected: all parser-contract tests pass without initializing a terminal.

- [ ] **Step 6: Commit the review CLI**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/cli tests/cli_parse.rs
git commit -m "feat: add hunk-compatible review commands"
```

---

### Task 4: Resolve layered TOML configuration

**Files:**
- Create: `src/config/mod.rs`
- Create: `src/config/model.rs`
- Create: `src/config/load.rs`
- Create: `tests/config_resolution.rs`
- Modify: `src/lib.rs:1`
- Modify: `Cargo.toml:9`

**Interfaces:**
- Consumes: `InputKind`, `CommonOptions`, and `LayoutMode`.
- Produces: `ConfigResolver::resolve(&ReviewInput) -> Result<ResolvedConfig, ConfigError>` and platform/repository config path discovery.

- [ ] **Step 1: Write the failing precedence test**

```rust
// tests/config_resolution.rs
use ramo::config::{ConfigPaths, ConfigResolver};
use ramo::core::input::{CommonOptions, LayoutMode, ReviewInput};

#[test]
fn builtin_user_repo_command_and_cli_layers_merge_in_order() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("user.toml");
    let repo = temp.path().join("repo/.ramo/config.toml");
    std::fs::create_dir_all(repo.parent().unwrap()).unwrap();
    std::fs::write(&user, "mode = \"stack\"\nline_numbers = false\n").unwrap();
    std::fs::write(&repo, "line_numbers = true\n[diff]\nwrap_lines = true\n").unwrap();
    let input = ReviewInput::VcsDiff {
        range: None, staged: false, pathspecs: vec![],
        options: CommonOptions { mode: Some(LayoutMode::Split), ..Default::default() },
    };
    let resolved = ConfigResolver::new(ConfigPaths { user: Some(user), repo: Some(repo) })
        .resolve(&input).unwrap();
    assert_eq!(resolved.mode, LayoutMode::Split);
    assert!(resolved.line_numbers);
    assert!(resolved.wrap_lines);
}
```

- [ ] **Step 2: Add Serde/TOML dependencies and verify red**

```toml
serde = { version = "1", features = ["derive"] }
toml = "1"
```

Run: `cargo test --test config_resolution`

Expected: compilation fails because the config API does not exist.

- [ ] **Step 3: Define partial and resolved config types**

```rust
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ConfigLayer {
    pub mode: Option<LayoutMode>, pub vcs: Option<String>, pub theme: Option<String>,
    pub watch: Option<bool>, pub exclude_untracked: Option<bool>,
    pub line_numbers: Option<bool>, pub wrap_lines: Option<bool>,
    pub hunk_headers: Option<bool>, pub agent_notes: Option<bool>,
    pub copy_decorations: Option<bool>, pub prompt_save_view_preferences: Option<bool>,
    pub transparent_background: Option<bool>, pub color_moved: Option<bool>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ConfigFile {
    #[serde(flatten)] pub global: ConfigLayer,
    #[serde(default)] pub diff: ConfigLayer,
    #[serde(default)] pub show: ConfigLayer,
    #[serde(default)] pub stash_show: ConfigLayer,
    #[serde(default)] pub patch: ConfigLayer,
    #[serde(default)] pub pager: ConfigLayer,
    #[serde(default)] pub difftool: ConfigLayer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub mode: LayoutMode, pub vcs: Option<String>, pub theme: String,
    pub watch: bool, pub exclude_untracked: bool, pub line_numbers: bool,
    pub wrap_lines: bool, pub hunk_headers: bool, pub agent_notes: bool,
    pub copy_decorations: bool, pub prompt_save_view_preferences: bool,
    pub transparent_background: bool, pub color_moved: bool,
}
```

Implement `Default` for `ResolvedConfig` with `Auto`, `github-dark-default`, `watch = false`, `exclude_untracked = false`, `line_numbers = true`, `wrap_lines = false`, `hunk_headers = true`, `agent_notes = false`, `copy_decorations = false`, `prompt_save_view_preferences = true`, `transparent_background = false`, and `color_moved = true`.

At this step, extend `LayoutMode` with `serde::Deserialize` and `#[serde(rename_all = "lowercase")]` so TOML accepts exactly `auto`, `split`, and `stack`.

- [ ] **Step 4: Implement deterministic layer merging and discovery**

`ConfigResolver::resolve` applies built-ins, user global, repository global, user command, repository command, pager sections for pager-mode invocations, and CLI overrides in that order. `ConfigPaths::discover(cwd)` uses `dirs::config_dir()/ramo/config.toml` and the nearest ancestor `.ramo/config.toml`. Parse errors include the path and TOML diagnostic. Before deserialization, validate each TOML table against the explicit preference keys and command-section names above so unknown keys fail rather than disappearing silently.

- [ ] **Step 5: Cover config failure and boolean precedence cases**

Add tests proving repository command values override user command values, `--no-*` CLI values override `true` config values, malformed TOML names its file, unknown fields fail, absent files are ignored, and discovery chooses the nearest repository config.

Run: `cargo test --test config_resolution`

Expected: every precedence and error case passes.

- [ ] **Step 6: Commit layered configuration**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/config tests/config_resolution.rs
git commit -m "feat: resolve layered ramo configuration"
```

---

### Task 5: Load patch and direct-file reviews into one changeset

**Files:**
- Create: `src/input/mod.rs`
- Create: `src/input/patch.rs`
- Create: `src/input/file_pair.rs`
- Create: `tests/input_loading.rs`
- Create: `tests/fixtures/simple.patch`
- Modify: `src/lib.rs:1`
- Modify: `Cargo.toml:9`

**Interfaces:**
- Consumes: `ReviewInput`, `PatchSource`, `Changeset`, `DiffFile`, and `parse_unified_diff`.
- Produces: `ReviewLoader::load(input, stdin) -> Result<LoadedReview, LoadError>`, where `LoadedReview` holds `Changeset` and a future-compatible `ReloadPlan`.

- [ ] **Step 1: Write failing loader tests**

```rust
// tests/input_loading.rs
use std::io::Cursor;
use ramo::core::input::{CommonOptions, PatchSource, ReviewInput};
use ramo::input::ReviewLoader;

#[test]
fn patch_stdin_loads_a_changeset() {
    let patch = include_str!("fixtures/simple.patch");
    let loaded = ReviewLoader::default().load(
        &ReviewInput::Patch { source: PatchSource::Stdin, options: CommonOptions::default() },
        &mut Cursor::new(patch),
    ).unwrap();
    assert_eq!(loaded.changeset.files[0].path, "src/main.rs");
    assert_eq!(loaded.changeset.source_label, "patch stdin");
}

#[test]
fn direct_files_are_diffed_without_an_external_program() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("before.txt");
    let right = temp.path().join("after.txt");
    std::fs::write(&left, "before\n").unwrap();
    std::fs::write(&right, "after\n").unwrap();
    let loaded = ReviewLoader::default().load(
        &ReviewInput::FilePair { left, right, display_path: None, options: CommonOptions::default() },
        &mut Cursor::new([]),
    ).unwrap();
    assert_eq!(loaded.changeset.files.len(), 1);
    assert_eq!(loaded.changeset.stats().additions, 1);
    assert_eq!(loaded.changeset.stats().deletions, 1);
}
```

- [ ] **Step 2: Add the pure-Rust diff dependency and verify red**

```toml
similar = "3"
```

Run: `cargo test --test input_loading`

Expected: compilation fails because `ReviewLoader` is absent.

- [ ] **Step 3: Implement patch normalization and loading**

Define `normalize_patch_text` to normalize CRLF to LF and remove CSI/OSC terminal control sequences while retaining plain text. `patch::load` reads stdin or a named file, rejects empty input with `LoadError::EmptyInput`, parses the patch, and returns a `Changeset::new(source_label, title, files)`. Invalid non-empty text returns `LoadError::InvalidPatch { source_label }`; an empty valid diff is represented by an empty file list only when the source command can legitimately produce no changes.

- [ ] **Step 4: Implement direct text comparison in Rust**

Use `similar::TextDiff::from_lines` and its unified diff formatter with three context lines. Prefix the result with a valid `diff --git` header so the shared parser handles it. Detect a binary file by a NUL byte in the first 8 KiB; binary comparisons return one `DiffFile` with `is_binary = true`, no hunks, and a descriptive patch placeholder. Use `/dev/null` semantics for an explicitly missing side only when introduced by difftool/VCS loaders, not for misspelled direct-file paths.

- [ ] **Step 5: Define the loader seam for subsequent inputs**

```rust
#[derive(Debug, Clone)]
pub enum ReloadPlan { None, Files { left: PathBuf, right: PathBuf } }

#[derive(Debug, Clone)]
pub struct LoadedReview { pub changeset: Changeset, pub reload_plan: ReloadPlan }

#[derive(Debug, Default)]
pub struct ReviewLoader;

impl ReviewLoader {
    pub fn load(&self, input: &ReviewInput, stdin: &mut dyn Read) -> Result<LoadedReview, LoadError>;
}
```

`Patch` and `FilePair` are implemented here. `VcsDiff`, `Show`, `StashShow`, and `Pager` return typed `LoadError::UnsupportedInput(InputKind)` until plan 2 replaces those branches; the error text names the exact unsupported input and never enters the alternate screen.

- [ ] **Step 6: Verify text, binary, empty, invalid, and CRLF inputs**

Add tests for patch files, stdin, ANSI-colored patches, CRLF patches, empty stdin, malformed non-empty input, missing direct files, identical direct files, binary pairs, and display-path preservation.

Run: `cargo test --test input_loading && cargo test diff::parser`

Expected: all loader and parser cases pass.

- [ ] **Step 7: Commit normalized loaders**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/input tests/input_loading.rs tests/fixtures/simple.patch
git commit -m "feat: normalize patch and file review inputs"
```

---

### Task 6: Move startup and legacy output into the runtime boundary

**Files:**
- Create: `src/error.rs`
- Create: `src/runtime.rs`
- Modify: `src/lib.rs:1`
- Modify: `src/main.rs:1`
- Modify: `src/app.rs:200`
- Create: `tests/runtime_resolution.rs`

**Interfaces:**
- Consumes: `Invocation`, `Action`, `ReviewLoader`, `LoadedReview`, existing `App`, annotations output, Pi integration, and TTY replacement.
- Produces: `runtime::run(invocation) -> Result<ExitCode, AppError>` and a minimal `main` that restores the terminal on every UI exit path.

- [ ] **Step 1: Write failing non-interactive action tests**

```rust
// tests/runtime_resolution.rs
use ramo::cli::Action;
use ramo::runtime::{resolve_action, StartupAction};

#[test]
fn integrations_do_not_initialize_the_review_ui() {
    assert_eq!(resolve_action(&Action::InstallPi), StartupAction::InstallPi);
    assert_eq!(resolve_action(&Action::UninstallPi), StartupAction::UninstallPi);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test runtime_resolution`

Expected: compilation fails because `runtime` and `StartupAction` do not exist.

- [ ] **Step 3: Define typed application errors and startup actions**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupAction { Print, Review, InstallPi, UninstallPi }

pub fn resolve_action(action: &Action) -> StartupAction {
    match action {
        Action::Print(_) => StartupAction::Print,
        Action::Review(_) => StartupAction::Review,
        Action::InstallPi => StartupAction::InstallPi,
        Action::UninstallPi => StartupAction::UninstallPi,
    }
}
```

`AppError` wraps `CliError`, `ConfigError`, `LoadError`, and `io::Error`, implements `Display`/`Error`, and returns exit code 2 for command/config errors and 1 for runtime failures. `Action::Print` writes its captured help/version text and exits 0 without loading configuration or initializing the terminal.

- [ ] **Step 4: Reduce `main.rs` to process orchestration**

```rust
fn main() -> std::process::ExitCode {
    let invocation = match ramo::cli::parse_from(std::env::args_os(), std::io::stdin().is_terminal()) {
        Ok(value) => value,
        Err(error) => { eprintln!("ramo: {error}"); return std::process::ExitCode::from(2); }
    };
    match ramo::runtime::run(invocation) {
        Ok(code) => code,
        Err(error) => { eprintln!("ramo: {error}"); std::process::ExitCode::from(error.exit_code()) }
    }
}
```

Import `std::io::IsTerminal`. `runtime::run` dispatches Pi install/uninstall without touching terminal state, resolves config before loading, loads the review, performs `/dev/tty` replacement only after piped input is consumed, runs `App::new(changeset.files)`, restores Ratatui before processing annotations, and preserves the existing `--stdout`, `--output`, prompt, fallback-file, and no-comments behavior.

- [ ] **Step 5: Make TTY replacement portable at the interface**

Keep the current Unix `dup2` implementation under `cfg(unix)`. Under `cfg(windows)`, leave stdin unchanged for file-backed inputs and return a clear `io::ErrorKind::Unsupported` for piped interactive review until the process-integration plan supplies the Windows console implementation. Unit-test the decision function separately from platform syscalls.

- [ ] **Step 6: Verify all existing behavior still passes**

Run: `cargo test --all-targets`

Expected: the original parser/annotation tests and all new library, CLI, config, loader, and runtime tests pass.

- [ ] **Step 7: Commit the runtime boundary**

```bash
git add src/lib.rs src/main.rs src/error.rs src/runtime.rs src/app.rs tests/runtime_resolution.rs
git commit -m "refactor: isolate ramo process and terminal startup"
```

---

### Task 7: Add black-box contracts, documentation, and the parity ledger

**Files:**
- Create: `tests/cli_contract.rs`
- Create: `docs/parity/hunk.md`
- Modify: `README.md:1`

**Interfaces:**
- Consumes: the completed foundation command surface and approved design spec.
- Produces: executable CLI evidence and the authoritative parity ledger consumed by all six subsequent plans.

- [ ] **Step 1: Add failing black-box CLI tests**

```rust
// tests/cli_contract.rs
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_every_foundation_review_command() {
    Command::cargo_bin("ramo").unwrap().arg("--help").assert().success().stdout(
        predicate::str::contains("diff").and(predicate::str::contains("show"))
            .and(predicate::str::contains("stash")).and(predicate::str::contains("patch"))
            .and(predicate::str::contains("pager")).and(predicate::str::contains("difftool")),
    );
}

#[test]
fn version_is_plain_and_successful() {
    Command::cargo_bin("ramo").unwrap().arg("--version").assert().success()
        .stdout(predicate::str::starts_with("ramo "));
}

#[test]
fn invalid_layout_fails_before_terminal_startup() {
    Command::cargo_bin("ramo").unwrap().args(["diff", "--mode", "columns"]).assert()
        .code(2).stderr(predicate::str::contains("invalid value 'columns'"));
}
```

- [ ] **Step 2: Run black-box tests and fix only contract mismatches**

Run: `cargo test --test cli_contract`

Expected: the tests initially expose help/version/error wording mismatches; adjust Clap help metadata and top-level error rendering until all pass.

- [ ] **Step 3: Create the parity ledger with explicit status semantics**

`docs/parity/hunk.md` begins with reference commit `53fcb2c` and defines `missing`, `implemented`, and `verified`. It contains one row per command/option, loader capability, configuration key, keyboard action, mouse action, layout/render feature, watch behavior, note/STML behavior, session operation, process integration, and cross-platform contract from the approved spec. Mark only foundation behaviors backed by passing tests as `verified`; mark parsed-but-not-executable VCS/pager behaviors `implemented`; mark untouched UI/session behavior `missing`. Every non-missing row links to a Rust source symbol and test name.

- [ ] **Step 4: Update the README for the usable foundation surface**

Document the new command forms, direct-file usage, bare-pipe compatibility, config paths and precedence, deliberate lack of a top menu bar, and the Rust-only single-executable constraint. Do not advertise VCS, pager, session, watch, or UI behaviors until their ledger row is `verified`.

- [ ] **Step 5: Run the complete foundation gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Expected: every command exits 0, the release build produces `target/release/ramo`, and `ldd target/release/ramo` on Linux (or `otool -L` on macOS) shows no Node/Bun/JavaScript runtime dependency.

- [ ] **Step 6: Manually smoke-test retained workflows**

Run in a real terminal:

```bash
git diff --no-color | target/release/ramo
target/release/ramo diff README.md README.md
target/release/ramo --input tests/fixtures/simple.patch --stdout
```

Expected: piped and file-pair inputs enter/exit the reviewer cleanly; the explicit patch opens; annotation stdout remains Markdown; identical files report an empty review without corrupting terminal state.

- [ ] **Step 7: Commit the foundation contract**

```bash
git add README.md docs/parity/hunk.md tests/cli_contract.rs
git commit -m "docs: establish hunk parity verification ledger"
```

## Plan self-review

- **Spec coverage:** This plan covers approved delivery slice 1: library/binary split, normalized models, Hunk-shaped review parsing, layered config, patch/file inputs, legacy aliases, and the parity ledger. VCS execution, pager fallback, review UI replacement, watch/process integration, notes/STML, sessions, and parity closure are explicitly routed to plans 2–7 in dependency order.
- **Type consistency:** `ReviewInput`, `CommonOptions`, `InputKind`, `Changeset`, `LoadedReview`, `ReviewLoader`, `Invocation`, and `Action` have one definition each and are consumed under the same names in every later task.
- **Temporary behavior:** Typed unsupported-loader branches exist only at the VCS/pager dispatch seam and are replaced by plan 2; they fail before terminal initialization and are recorded as non-verified in the parity ledger.
