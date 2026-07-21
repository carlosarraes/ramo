# Native Watch and Process Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add native file/VCS reload, editor and terminal job control, and verified optional integrations without introducing a JavaScript runtime or a second executable.

**Architecture:** A backend-neutral `watch` module derives observation targets from each loader's `ReloadPlan`, coalesces event and polling hints, and executes at most one synchronous reload at a time from the TUI event loop. `ReviewController::replace_files` owns semantic selection and viewport restoration, while a `TerminalSession` owns enter/restore/suspend/resume behavior and an injected process boundary owns editor and tmux commands. Pi integration becomes a Markdown prompt template that instructs Pi to invoke the native `pdiff` CLI; the existing TypeScript extension is removed.

**Tech Stack:** Rust 2024, Ratatui, Crossterm, `notify` for linked native filesystem events, standard-library channels/processes/time, `shell-words`, `portable-pty`, and temporary native Git fixtures.

## Global Constraints

- The implementation is 100% Rust.
- Installation produces one `pdiff` executable. It must not require Node.js, Bun, TypeScript, a browser runtime, or a separately installed helper service.
- Git, Jujutsu, and Sapling executables remain optional and are invoked only for their matching workflows.
- Linux, macOS, and Windows remain supported through common Rust interfaces; platform-specific terminal behavior is isolated behind `cfg` modules.
- Existing piped patch, `--input`, `--output`, `--stdout`, Vim selection, Markdown comments, tmux send, and OSC 52 behavior remains compatible.
- Hunk's top menu bar, dropdown menus, and menu-specific shortcuts remain excluded.
- Watch and integration failures never discard the last valid changeset.

---

## File map

- `src/input/mod.rs`: public reload-plan variants and `ReviewLoader::reload` entry point.
- `src/input/patch.rs`: reload plan for patch files; stdin patches remain non-reloadable.
- `src/input/vcs.rs`: records the selected VCS in the reload plan.
- `src/review/state.rs`: atomically replaces files while preserving semantic review state.
- `src/watch/plan.rs`: deterministic observation targets derived from reload plans.
- `src/watch/coordinator.rs`: debounce, polling, serialization, generation, and duplicate-error state machine.
- `src/watch/observer.rs`: `notify` adapter that turns native events into hints and degrades cleanly.
- `src/watch/runtime.rs`: owns reload execution and content fingerprints for one review session.
- `src/process/editor.rs`: shell-free `$EDITOR` parsing, file/line arguments, and launch results.
- `src/process/command.rs`: injectable child-process boundary shared by editor and tmux tests.
- `src/terminal.rs`: alternate-screen, raw-mode, mouse, panic, suspend, and resume ownership.
- `src/app.rs`: timed event loop, reload/editor effects, replacement feedback, and suspend key.
- `src/runtime.rs`: constructs watch/process services and the terminal session.
- `src/pi_extension.rs`: installs/removes a Markdown Pi prompt template.
- `src/pi_prompt.md`: embedded Rust-only Pi workflow text.
- `src/pi_extension_src.ts`: removed.
- `src/tmux.rs`: uses the shared process boundary and preserves literal stdin transport.
- `tests/reload.rs`: loader reload and state-preservation integration coverage.
- `tests/watch.rs`: deterministic watch plans/coordinator and real filesystem events.
- `tests/pty_watch.rs`: manual and passive reload behavior in a real PTY.
- `tests/editor.rs`: editor argv, exit, and terminal-suspension contracts.
- `tests/terminal_lifecycle.rs`: PTY restoration on quit, error, panic, and suspend/resume.
- `tests/integrations.rs`: Pi filesystem install and fake-tmux process coverage.

---

### Task 1: Make every file-backed review reloadable without losing review state

**Files:**
- Modify: `src/input/mod.rs`
- Modify: `src/input/patch.rs`
- Modify: `src/input/vcs.rs`
- Modify: `src/review/state.rs`
- Test: `tests/reload.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `ReviewLoader::load_with_context`, `ReloadPlan`, stable `DiffFile::id`, and the existing viewport-anchor machinery.
- Produces: `ReloadPlan::Files { left, right, display_path }`, `ReloadPlan::PatchFile { path: PathBuf }`, `ReloadPlan::Vcs { input, repo_root, vcs }`, `ReviewLoader::reload(&ReloadPlan, &LoadContext<'_>) -> Result<LoadedReview, LoadError>`, and `ReviewController::replace_files(Vec<DiffFile>, Viewport)`.

- [ ] **Step 1: Write failing loader and state-preservation tests**

Create `tests/reload.rs` with native temporary files and a controller whose selected file survives reordered replacement:

```rust
use std::fs;

use pdiff::config::ResolvedConfig;
use pdiff::core::input::{CommonOptions, PatchSource, ReviewInput};
use pdiff::diff::model::{DiffFile, FileChangeKind};
use pdiff::input::{LoadContext, ReloadPlan, ReviewLoader};
use pdiff::review::{ReviewAction, ReviewController, ReviewOptions, Viewport};
use pdiff::vcs::SystemCommandRunner;

#[test]
fn patch_file_reload_reads_the_replacement_contents() {
    let temp = tempfile::tempdir().unwrap();
    let patch = temp.path().join("review.patch");
    fs::write(&patch, "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+first\n").unwrap();
    let input = ReviewInput::Patch {
        source: PatchSource::File(patch.clone()),
        options: CommonOptions::default(),
    };
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    let context = LoadContext { cwd: temp.path(), config: &config, runner: &runner };
    let loaded = ReviewLoader.load_with_context(&input, &mut std::io::empty(), &context).unwrap();
    assert_eq!(loaded.reload_plan, ReloadPlan::PatchFile { path: patch.clone() });

    fs::write(&patch, "--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+second\n").unwrap();
    let reloaded = ReviewLoader.reload(&loaded.reload_plan, &context).unwrap();
    assert!(reloaded.changeset.files[0].patch.contains("+second"));
}

#[test]
fn replacing_files_preserves_selected_file_and_viewport_anchor() {
    let viewport = Viewport { width: 120, height: 12 };
    let a = DiffFile::for_test("src/a.rs", FileChangeKind::Modified, 30, 3);
    let b = DiffFile::for_test("src/b.rs", FileChangeKind::Modified, 30, 3);
    let mut controller = ReviewController::new(vec![a.clone(), b.clone()], ReviewOptions::default());
    controller.apply(ReviewAction::SelectFile(b.id.clone()), viewport);
    controller.apply(ReviewAction::Scroll { delta: 1, unit: pdiff::review::ScrollUnit::HalfPage }, viewport);
    let before = controller.snapshot(viewport).clone();

    controller.replace_files(vec![b, a], viewport);
    let after = controller.snapshot(viewport);
    assert_eq!(after.selected_file_id, before.selected_file_id);
    assert!(after.scroll_top > 0);
}
```

- [ ] **Step 2: Run the focused tests and confirm the contract is absent**

Run: `cargo test --test reload -- --nocapture`

Expected: compilation fails because `PatchFile`, `ReviewLoader::reload`, and `ReviewController::replace_files` do not exist.

- [ ] **Step 3: Add typed reload plans and one loader entry point**

In `src/input/mod.rs`, add the file-backed patch variant, record the selected VCS, and implement reload without reading process stdin:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadPlan {
    None,
    Files { left: PathBuf, right: PathBuf, display_path: Option<PathBuf> },
    PatchFile { path: PathBuf },
    Vcs { input: ReviewInput, repo_root: PathBuf, vcs: crate::core::input::VcsId },
}

impl ReviewLoader {
    pub fn reload(
        &self,
        plan: &ReloadPlan,
        context: &LoadContext<'_>,
    ) -> Result<LoadedReview, LoadError> {
        match plan {
            ReloadPlan::None => Err(LoadError::NotReloadable),
            ReloadPlan::Files { left, right, display_path } => {
                file_pair::load(left, right, display_path.as_deref())
            }
            ReloadPlan::PatchFile { path } => patch::load(
                &crate::core::input::PatchSource::File(path.clone()),
                &mut std::io::empty(),
            ),
            ReloadPlan::Vcs { input, .. } => vcs::load(input, context),
        }
    }
}
```

Add `LoadError::NotReloadable` with display text `this review input cannot be reloaded because it came from stdin`. In `src/input/file_pair.rs`, retain `display_path.map(Path::to_path_buf)` in the reload plan so difftool identity survives refresh. In `src/input/patch.rs`, return `PatchFile` only for `PatchSource::File`; stdin remains `None`. In `src/input/vcs.rs`, set `vcs: selected.id` when constructing the plan. Update existing exact `ReloadPlan` assertions for the new fields and use `..` where the field values are not under test.

- [ ] **Step 4: Add atomic review-model replacement**

In `src/review/state.rs`, capture semantic selection and viewport position before replacing the files, drop cached context/source state, and rebuild through the existing anchor fallback chain:

```rust
pub fn replace_files(&mut self, files: Vec<DiffFile>, viewport: Viewport) {
    self.ensure_geometry(viewport);
    let anchor = self.geometry.as_ref().map(|geometry| {
        capture_viewport_anchor(
            geometry,
            self.scroll_top,
            self.selected_file_id.as_deref(),
            self.selected_hunk_index,
        )
    });
    let selected_file = self.selected_file_id.clone();
    let selected_hunk = self.selected_hunk_index;
    self.files = files;
    self.contexts.clear();
    self.geometry = None;
    self.planned_files.clear();
    self.selected_file_id = selected_file.filter(|id| self.files.iter().any(|file| file.id == *id));
    if self.selected_file_id.is_none() {
        self.selected_file_id = self.files.first().map(|file| file.id.clone());
    }
    self.selected_hunk_index = self.selected_file_id.as_ref().map(|id| {
        let hunk_count = self.files.iter().find(|file| file.id == *id).map_or(0, |file| file.hunks.len());
        selected_hunk.unwrap_or(0).min(hunk_count.saturating_sub(1))
    });
    self.selected_row_key = anchor.as_ref().and_then(|value| value.row_key.clone());
    self.dirty = true;
    self.rebuild(viewport, false);
    if let (Some(geometry), Some(anchor)) = (self.geometry.as_ref(), anchor.as_ref()) {
        self.scroll_top = restore_viewport_anchor(geometry, anchor);
        self.refresh_snapshot();
    }
}
```

Ensure an empty replacement remains a valid state with no selected file and a zero scroll offset.

- [ ] **Step 5: Run reload and existing review tests**

Run: `cargo test --test reload --test input_loading --test git_loading --test jj_loading --test sl_loading --test review_state`

Expected: all tests pass; stdin patch reload returns the distinct `NotReloadable` error.

- [ ] **Step 6: Commit the reload contract**

```bash
git add src/input src/review/state.rs src/lib.rs tests/reload.rs tests/input_loading.rs tests/git_loading.rs tests/jj_loading.rs tests/sl_loading.rs
git commit -m "feat: reload native review inputs"
```

---

### Task 2: Build native observation, debounce, polling fallback, and stale-result protection

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `src/watch/mod.rs`
- Create: `src/watch/plan.rs`
- Create: `src/watch/coordinator.rs`
- Create: `src/watch/observer.rs`
- Create: `src/watch/runtime.rs`
- Modify: `src/lib.rs`
- Test: `tests/watch.rs`

**Interfaces:**
- Consumes: `ReloadPlan`, `ReviewLoader::reload`, `LoadedReview`, and `ResolvedConfig`.
- Produces: `WatchPlan::from_reload_plan`, `WatchCoordinator::{event_hint, manual_hint, tick, finish}`, `NativeObserver`, and `WatchRuntime::{new, manual_reload, poll}` returning `WatchUpdate`.

- [ ] **Step 1: Write deterministic plan and coordinator tests**

Create `tests/watch.rs` with fake time so debounce behavior never sleeps:

```rust
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pdiff::core::input::VcsId;
use pdiff::input::ReloadPlan;
use pdiff::watch::{Coverage, WatchCoordinator, WatchPlan, WatchTarget};

#[test]
fn direct_files_watch_parent_entries_so_atomic_replacements_are_seen() {
    let plan = WatchPlan::from_reload_plan(&ReloadPlan::Files {
        left: PathBuf::from("/tmp/review/before.rs"),
        right: PathBuf::from("/tmp/review/after.rs"),
        display_path: None,
    }).unwrap();
    assert_eq!(plan.coverage, Coverage::Hybrid);
    assert_eq!(plan.targets, vec![WatchTarget::Entries {
        directory: PathBuf::from("/tmp/review"),
        entries: vec![PathBuf::from("/tmp/review/after.rs"), PathBuf::from("/tmp/review/before.rs")],
    }]);
}

#[test]
fn jj_and_sapling_use_poll_only_plans() {
    for vcs in [VcsId::Jj, VcsId::Sl] {
        let plan = WatchPlan::for_vcs(PathBuf::from("/repo"), vcs);
        assert_eq!(plan.coverage, Coverage::PollOnly);
        assert!(plan.targets.is_empty());
    }
}

#[test]
fn bursts_coalesce_and_an_inflight_hint_gets_one_trailing_generation() {
    let start = Instant::now();
    let mut coordinator = WatchCoordinator::new(start, Duration::from_millis(200), Duration::from_secs(1));
    coordinator.event_hint(start);
    coordinator.event_hint(start + Duration::from_millis(150));
    assert_eq!(coordinator.tick(start + Duration::from_millis(349)), None);
    let first = coordinator.tick(start + Duration::from_millis(350)).unwrap();
    coordinator.event_hint(start + Duration::from_millis(360));
    assert_eq!(coordinator.tick(start + Duration::from_secs(2)), None);
    coordinator.finish(first, start + Duration::from_secs(2));
    let second = coordinator.tick(start + Duration::from_secs(2)).unwrap();
    assert!(second > first);
    assert!(!coordinator.accept_result(first));
    assert!(coordinator.accept_result(second));
}
```

- [ ] **Step 2: Run the watch tests and verify failure**

Run: `cargo test --test watch -- --nocapture`

Expected: compilation fails because `pdiff::watch` does not exist.

- [ ] **Step 3: Add the linked native watcher dependency and deterministic plan types**

Add `notify = "8"` to `[dependencies]`. In `src/watch/plan.rs`, define:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage { Hybrid, PollOnly }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchTarget {
    Entries { directory: PathBuf, entries: Vec<PathBuf> },
    Tree { directory: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchPlan {
    pub coverage: Coverage,
    pub targets: Vec<WatchTarget>,
}
```

Resolve file and patch paths to absolute paths before grouping, sort and deduplicate entries, watch Git repository content recursively, and use `PollOnly` for Jujutsu and Sapling. `ReloadPlan::None` returns `None`.

- [ ] **Step 4: Implement the pure coordinator**

In `src/watch/coordinator.rs`, maintain one in-flight generation, one dirty bit, a quiet deadline, a maximum deadline, and a safety-poll deadline:

```rust
pub struct WatchCoordinator {
    next_generation: u64,
    latest_requested: u64,
    in_flight: Option<u64>,
    dirty: bool,
    quiet_delay: Duration,
    maximum_delay: Duration,
    quiet_deadline: Option<Instant>,
    maximum_deadline: Option<Instant>,
    safety_deadline: Instant,
    safety_interval: Duration,
}

pub fn tick(&mut self, now: Instant) -> Option<u64> {
    if self.in_flight.is_some() { return None; }
    let due = self.quiet_deadline.is_some_and(|deadline| now >= deadline)
        || self.maximum_deadline.is_some_and(|deadline| now >= deadline)
        || now >= self.safety_deadline;
    if !due { return None; }
    self.next_generation += 1;
    self.latest_requested = self.next_generation;
    self.in_flight = Some(self.next_generation);
    self.quiet_deadline = None;
    self.maximum_deadline = None;
    Some(self.next_generation)
}

pub fn accept_result(&self, generation: u64) -> bool {
    self.in_flight == Some(generation) && generation == self.latest_requested
}
```

`manual_hint` schedules an immediate generation. `finish` clears only the matching in-flight generation, schedules the next safety poll, and converts an in-flight dirty hint into one immediate trailing generation. Keep duplicate error suppression as `(message, Instant)` state with a ten-second interval.

- [ ] **Step 5: Adapt `notify` events into non-blocking hints**

In `src/watch/observer.rs`, construct `notify::RecommendedWatcher` handles, send callback results through `std::sync::mpsc`, and filter entry targets by exact absolute path. Watch entry parents with `RecursiveMode::NonRecursive`; watch Git content with `RecursiveMode::Recursive`. Construction or runtime errors set `degraded = true`, are returned once for UI feedback, and leave polling active. `Drop` un-watches all targets.

Expose only this interface:

```rust
pub struct ObserverPoll {
    pub changed: bool,
    pub error: Option<String>,
}

impl NativeObserver {
    pub fn start(plan: &WatchPlan) -> Result<Self, notify::Error>;
    pub fn poll(&mut self) -> ObserverPoll;
}
```

- [ ] **Step 6: Implement synchronous serialized reload runtime with fingerprints**

In `src/watch/runtime.rs`, define a deterministic FNV-1a fingerprint over changeset source label, title, file ids, paths, and patch text. `WatchRuntime::poll(now)` drains watcher hints, advances the coordinator, executes at most one `ReviewLoader::reload`, and returns:

```rust
pub enum WatchUpdate {
    Unchanged,
    Replaced { files: Vec<DiffFile>, generation: u64 },
    Empty { generation: u64 },
    Error { message: String },
}
```

Only update the applied fingerprint after a successful accepted generation. Reload errors return `Error` and preserve the old fingerprint, allowing the next hint/poll to retry. Because execution is synchronous in the single TUI loop, there can be no simultaneous loaders; the generation check still prevents a superseded result from being applied if this boundary later becomes asynchronous.

- [ ] **Step 7: Test real atomic replacement and fallback behavior**

Extend `tests/watch.rs` with a real `NativeObserver` test that watches a parent directory, writes a sibling temporary file, renames it over the watched path, and polls with a bounded two-second deadline. Add a construction-failure test using a missing parent and verify a `WatchRuntime` created without an observer still refreshes on its safety-poll deadline.

Run: `cargo test --test watch -- --nocapture`

Expected: all deterministic tests pass and the real watcher observes the atomic replacement without an unbounded wait.

- [ ] **Step 8: Commit the watch engine**

```bash
git add Cargo.toml Cargo.lock src/watch src/lib.rs tests/watch.rs
git commit -m "feat: add native watch coordination"
```

---

### Task 3: Connect passive and manual reload to the TUI

**Files:**
- Modify: `src/app.rs`
- Modify: `src/runtime.rs`
- Modify: `src/ui/dialogs.rs`
- Test: `tests/pty_watch.rs`
- Modify: `tests/ui_input.rs`

**Interfaces:**
- Consumes: `WatchRuntime`, `WatchUpdate`, `ReviewEffect::Reload`, and `ReviewController::replace_files`.
- Produces: `App::run_with_runtime(&mut TerminalSession, Option<&mut WatchRuntime>, &mut EditorLauncher)` and visible reload/error status that never replaces the last valid model on failure.

- [ ] **Step 1: Add failing PTY tests for manual, passive, and failed reloads**

Create `tests/pty_watch.rs` by reusing the bounded `portable_pty` harness pattern from `tests/pty_ui.rs`. Cover:

```rust
#[test]
fn manual_r_reloads_a_direct_file_without_watch_mode() {
    let fixture = WatchFixture::new();
    let mut session = PtySession::spawn(fixture.dir.path(), &["diff", fixture.before(), fixture.after(), "--mode", "stack"]);
    session.wait_for("initial change");
    fixture.replace_after("manual replacement");
    session.send("r");
    session.wait_for("manual replacement");
    session.send("q");
    assert_eq!(session.wait(), 0);
}

#[test]
fn watch_mode_refreshes_after_an_atomic_save() {
    let fixture = WatchFixture::new();
    let mut session = PtySession::spawn(fixture.dir.path(), &["diff", fixture.before(), fixture.after(), "--watch", "--mode", "stack"]);
    session.wait_for("initial change");
    fixture.replace_after("passive replacement");
    let screen = session.wait_for("passive replacement");
    assert!(!screen.contains("initial change"));
    session.send("q");
    assert_eq!(session.wait(), 0);
}

#[test]
fn watch_error_keeps_the_last_valid_review_visible() {
    let fixture = WatchFixture::new();
    let mut session = PtySession::spawn(fixture.dir.path(), &["diff", fixture.before(), fixture.after(), "--watch", "--mode", "stack"]);
    session.wait_for("initial change");
    std::fs::remove_file(fixture.after()).unwrap();
    let screen = session.wait_for("failed to read");
    assert!(screen.contains("initial change"));
    session.send("q");
    assert_eq!(session.wait(), 0);
}
```

- [ ] **Step 2: Run PTY tests and confirm `r` is only a toast**

Run: `cargo test --test pty_watch -- --nocapture`

Expected: manual and passive replacement tests time out at their bounded assertions because no reload executes.

- [ ] **Step 3: Poll terminal and watch sources from one event loop**

Change the blocking `event::read()` loop in `src/app.rs` to a bounded poll:

```rust
while !self.should_quit {
    terminal.draw(|frame| self.draw(frame))?;
    if event::poll(Duration::from_millis(50))? {
        match event::read()? {
            Event::Key(key) => self.handle_key(key, viewport),
            Event::Mouse(mouse) => self.handle_mouse(mouse, viewport),
            Event::Resize(_, _) => {}
            Event::FocusGained | Event::FocusLost | Event::Paste(_) => {}
        }
    }
    if let Some(runtime) = watch.as_deref_mut() {
        self.apply_watch_update(runtime.poll(Instant::now()), viewport);
    }
}
```

Do not redraw in a busy loop: track `needs_redraw`, set it for input, resize, and non-`Unchanged` watch updates, and draw only when true. The 50 ms event poll bounds watch latency without a background async runtime.

- [ ] **Step 4: Route reload effects and apply results atomically**

Make `ReviewEffect::Reload` call `watch.manual_reload(Instant::now())` even when `resolved_config.watch` is false; therefore construct a reload runtime whenever `ReloadPlan != None`, but start `NativeObserver` only when watch is enabled. Apply updates as follows:

```rust
fn apply_watch_update(&mut self, update: WatchUpdate, viewport: Viewport) {
    match update {
        WatchUpdate::Unchanged => {}
        WatchUpdate::Replaced { files, .. } => {
            self.files.clone_from(&files);
            self.flat_lines = build_flat_lines(&self.files);
            self.file_starts = build_file_starts(&self.flat_lines);
            self.line_counts = self.files.iter().map(DiffFile::line_counts).collect();
            self.review_highlights.clear();
            self.review_selection = None;
            self.review_keyboard_anchor = None;
            self.review_controller.replace_files(files, viewport);
            self.toast = Some("Reloaded".into());
        }
        WatchUpdate::Empty { .. } => self.toast = Some("No changes; press r to check again".into()),
        WatchUpdate::Error { message } => self.toast = Some(format!("Reload failed: {message}")),
    }
}
```

An empty watch result must not destroy the current review while the app is running; it presents the message and retries later. This matches the last-valid-review error rule and avoids an abrupt TUI exit during a transient clean state.

- [ ] **Step 5: Construct runtime services from the original stable load context**

In `src/runtime.rs`, retain the original `cwd`, resolved config, reload plan, and initial changeset fingerprint. Pass `config.watch` to `WatchRuntime::new`; reject `--watch` before terminal entry for `ReloadPlan::None` with `LoadError::NotReloadable`. Pager input remains unwatched.

- [ ] **Step 6: Run focused UI, PTY, and loader tests**

Run: `cargo test --test pty_watch --test pty_ui --test ui_input --test reload --test watch -- --nocapture`

Expected: all tests pass; manual reload works without `--watch`, passive reload observes atomic saves, and read errors retain old content.

- [ ] **Step 7: Commit TUI reload integration**

```bash
git add src/app.rs src/runtime.rs src/ui/dialogs.rs tests/pty_watch.rs tests/ui_input.rs
git commit -m "feat: reload live reviews natively"
```

---

### Task 4: Own terminal lifecycle and launch editors with correct job control

**Files:**
- Create: `src/terminal.rs`
- Create: `src/process/mod.rs`
- Create: `src/process/command.rs`
- Create: `src/process/editor.rs`
- Modify: `src/lib.rs`
- Modify: `src/app.rs`
- Modify: `src/runtime.rs`
- Modify: `src/main.rs`
- Test: `tests/editor.rs`
- Test: `tests/terminal_lifecycle.rs`

**Interfaces:**
- Consumes: `ReviewEffect::EditFile { path, line }`, `shell_words`, Crossterm terminal primitives, and Ratatui drawing.
- Produces: `EditorCommand`, `EditorLauncher::open`, `TerminalSession::{enter, terminal, suspend, resume, with_suspended}`, a panic restoration hook, and Unix Ctrl-Z suspend/resume.

- [ ] **Step 1: Write editor-command and fake-runner tests**

Create `tests/editor.rs`:

```rust
use std::path::Path;
use pdiff::process::editor::{build_editor_command, should_suspend_for_editor};

#[test]
fn editor_commands_are_argv_not_shell_text() {
    let path = Path::new("/tmp/a file.rs");
    assert_eq!(
        build_editor_command("nvim --clean", path, 17).unwrap().argv,
        ["nvim", "--clean", "+17", "/tmp/a file.rs"]
    );
    assert_eq!(
        build_editor_command("code --wait", path, 17).unwrap().argv,
        ["code", "--wait", "--goto", "/tmp/a file.rs:17"]
    );
    assert_eq!(
        build_editor_command("hx", path, 17).unwrap().argv,
        ["hx", "/tmp/a file.rs:17"]
    );
}

#[test]
fn gui_editors_do_not_take_terminal_ownership() {
    assert!(!should_suspend_for_editor("code --wait").unwrap());
    assert!(should_suspend_for_editor("vim").unwrap());
}
```

Add fake-runner cases for unset/empty `$EDITOR`, missing selected file, paths relative to repository root, process-spawn failure, and non-zero exit status.

- [ ] **Step 2: Run editor tests and verify the module is absent**

Run: `cargo test --test editor -- --nocapture`

Expected: compilation fails because `pdiff::process::editor` does not exist.

- [ ] **Step 3: Implement a shell-free process boundary and editor commands**

Define `CommandRequest { argv: Vec<OsString>, stdin: Option<Vec<u8>>, inherit_stdio: bool }`, `CommandResult { status: ExitStatus, stdout: Vec<u8>, stderr: Vec<u8> }`, and a `CommandExecutor` trait in `src/process/command.rs`. `SystemCommandExecutor` must invoke `Command::new(&argv[0]).args(&argv[1..])` directly and never use `sh -c`, `cmd /C`, or string interpolation.

In `src/process/editor.rs`, use `shell_words::split`, normalize the first executable basename across `/` and `\`, and implement Hunk-compatible arguments:

```rust
pub struct EditorCommand { pub argv: Vec<OsString>, pub suspend_terminal: bool }

pub fn build_editor_command(editor: &str, path: &Path, line: u32) -> Result<EditorCommand, EditorError> {
    let mut argv = shell_words::split(editor).map_err(EditorError::InvalidCommand)?
        .into_iter().map(OsString::from).collect::<Vec<_>>();
    let program = normalized_program(&argv[0]);
    match program.as_str() {
        "vi" | "vim" | "nvim" => argv.extend([OsString::from(format!("+{}", line.max(1))), path.into()]),
        "code" | "code-insiders" | "cursor" => argv.extend([OsString::from("--goto"), OsString::from(format!("{}:{}", path.display(), line.max(1)))]),
        "hx" => argv.push(OsString::from(format!("{}:{}", path.display(), line.max(1)))),
        _ => argv.push(path.into()),
    }
    let suspend_terminal = !matches!(program.as_str(), "code" | "code-insiders" | "cursor");
    Ok(EditorCommand { argv, suspend_terminal })
}
```

- [ ] **Step 4: Add RAII terminal ownership and panic restoration**

`TerminalSession::enter()` calls Ratatui/Crossterm initialization and marks itself active. `restore()` disables mouse capture, restores raw/alternate-screen state, and is idempotent. `Drop` calls `restore()`. `with_suspended` restores, executes a closure, and re-enters even when the closure returns an error. Install a panic hook before terminal entry that calls the low-level idempotent restoration function and then invokes the prior hook.

Use this public shape:

```rust
pub struct TerminalSession { terminal: DefaultTerminal, active: bool }

impl TerminalSession {
    pub fn enter() -> io::Result<Self>;
    pub fn terminal(&mut self) -> &mut DefaultTerminal;
    pub fn suspend(&mut self) -> io::Result<()>;
    pub fn resume(&mut self) -> io::Result<()>;
    pub fn with_suspended<T>(&mut self, operation: impl FnOnce() -> io::Result<T>) -> io::Result<T>;
}
```

- [ ] **Step 5: Connect `e` and Unix suspend/resume**

Resolve selected diff paths against the original VCS repository root or review cwd, require the file to exist, and use line 1 when no selected line is available. Terminal editors run inside `TerminalSession::with_suspended`; GUI editors run without suspending. Report distinct feedback for unset `$EDITOR`, invalid command text, missing file, spawn failure, and non-zero status.

Map Ctrl-Z to `AppAction::Suspend`. On Unix, restore the terminal, raise `SIGTSTP`, and re-enter/redraw after `SIGCONT`; on Windows, return a stable `suspend is not supported by this console` toast. Keep the platform code inside `src/terminal.rs`.

- [ ] **Step 6: Add bounded PTY restoration tests**

Create `tests/terminal_lifecycle.rs` with helper test modes exposed only under `PDIFF_TEST_PANIC=1` and `PDIFF_TEST_RUNTIME_ERROR=1`. For each child, assert it enters the alternate screen, exits or panics, and emits `\x1b[?1049l` plus cursor restoration before the diagnostic. Add a Unix-only test that sends Ctrl-Z, waits for stopped status, sends `SIGCONT`, verifies the review redraws, and then quits.

Run: `cargo test --test editor --test terminal_lifecycle --test pty_ui -- --nocapture`

Expected: all tests pass and every terminal-taking path restores ownership.

- [ ] **Step 7: Commit terminal and editor integration**

```bash
git add src/terminal.rs src/process src/lib.rs src/app.rs src/runtime.rs src/main.rs tests/editor.rs tests/terminal_lifecycle.rs
git commit -m "feat: add native editor and terminal job control"
```

---

### Task 5: Verify tmux and clipboard through injectable native boundaries

**Files:**
- Modify: `src/tmux.rs`
- Modify: `src/clipboard.rs`
- Modify: `src/app.rs`
- Test: `tests/integrations.rs`
- Modify: `tests/pty_ui.rs`

**Interfaces:**
- Consumes: `CommandExecutor`, stable review selection projection, and OSC 52 encoding.
- Produces: `TmuxClient<E: CommandExecutor>`, literal buffer transport, non-tmux feedback, and a `ClipboardWriter` boundary whose system implementation remains OSC 52.

- [ ] **Step 1: Write fake-tmux and clipboard tests**

Create `tests/integrations.rs` cases that assert exact argv and stdin:

```rust
#[test]
fn tmux_send_uses_literal_stdin_and_never_a_shell() {
    let executor = RecordingExecutor::successful();
    let mut tmux = TmuxClient::new(executor);
    tmux.send_to_pane("%7", "line one\n$(touch /tmp/nope)", PasteMode::Bracketed).unwrap();
    assert_eq!(tmux.executor().requests[0].argv, ["tmux", "load-buffer", "-b", "pdiff-send", "-"]);
    assert_eq!(tmux.executor().requests[0].stdin.as_deref(), Some(b"line one\n$(touch /tmp/nope)".as_slice()));
    assert_eq!(tmux.executor().requests[1].argv, ["tmux", "paste-buffer", "-p", "-r", "-b", "pdiff-send", "-t", "%7", "-d"]);
}

#[test]
fn osc52_encodes_wide_character_selection_exactly() {
    let mut output = Vec::new();
    pdiff::clipboard::write_osc52(&mut output, "界 old").unwrap();
    assert_eq!(output, b"\x1b]52;c;55WMIG9sZA==\x07");
}
```

Also cover failed `list-panes`, vanished cached pane, plain paste for Pi, filtering the current pane, and empty clipboard selection.

- [ ] **Step 2: Run integration tests before refactoring**

Run: `cargo test --test integrations --test pty_ui -- --nocapture`

Expected: compilation fails because `TmuxClient`, `RecordingExecutor`, and `write_osc52` are not exposed.

- [ ] **Step 3: Move tmux process calls behind `CommandExecutor`**

Keep existing free functions as thin `SystemCommandExecutor` compatibility wrappers, but implement behavior in `TmuxClient<E>`. Parse `list-panes` output exactly once, preserve pane ids as literal arguments, write send text only to child stdin, and include stderr in operation-specific errors. Rename the shared buffer to `pdiff-send`.

- [ ] **Step 4: Separate OSC 52 formatting from stdout ownership**

Expose `write_osc52(writer: &mut dyn Write, text: &str) -> io::Result<()>`; keep `copy_to_clipboard` as a stdout-lock wrapper. Do not introduce `xclip`, `pbcopy`, PowerShell, or another installed prerequisite in this slice.

- [ ] **Step 5: Run integration and PTY tests**

Run: `cargo test --test integrations --test pty_ui --test review_selection -- --nocapture`

Expected: all tests pass, including exact CJK OSC 52 bytes and shell-metacharacter tmux payloads.

- [ ] **Step 6: Commit native optional integrations**

```bash
git add src/tmux.rs src/clipboard.rs src/app.rs tests/integrations.rs tests/pty_ui.rs
git commit -m "refactor: verify native tmux and clipboard boundaries"
```

---

### Task 6: Replace the TypeScript Pi extension and close the slice ledger

**Files:**
- Delete: `src/pi_extension_src.ts`
- Create: `src/pi_prompt.md`
- Modify: `src/pi_extension.rs`
- Modify: `src/runtime.rs`
- Modify: `tests/integrations.rs`
- Modify: `README.md`
- Modify: `docs/parity/hunk.md`
- Modify: `docs/superpowers/plans/2026-07-21-watch-process-implementation-plan.md`

**Interfaces:**
- Consumes: Pi's global prompt-template convention `~/.pi/agent/prompts/*.md` and the normalized native `pdiff` CLI.
- Produces: `pi_extension::{install_at, uninstall_at}` for testable filesystem integration and `/pdiff` prompt text that directs Pi to invoke only the native executable.

- [ ] **Step 1: Add failing Pi filesystem tests**

Extend `tests/integrations.rs`:

```rust
#[test]
fn pi_install_writes_a_markdown_prompt_and_no_typescript() {
    let home = tempfile::tempdir().unwrap();
    let installed = pdiff::pi_extension::install_at(home.path()).unwrap();
    assert_eq!(installed, home.path().join(".pi/agent/prompts/pdiff.md"));
    let text = std::fs::read_to_string(&installed).unwrap();
    assert!(text.contains("pdiff diff --staged"));
    assert!(text.contains("pdiff --output"));
    assert!(!text.contains("registerCommand"));
    assert!(!home.path().join(".pi/agent/extensions/pdiff/index.ts").exists());
    pdiff::pi_extension::uninstall_at(home.path()).unwrap();
    assert!(!installed.exists());
}
```

- [ ] **Step 2: Run the focused test and confirm current TypeScript behavior fails it**

Run: `cargo test --test integrations pi_install -- --nocapture`

Expected: compilation fails because `install_at` and `uninstall_at` do not exist.

- [ ] **Step 3: Embed a prompt template and remove all shipped TypeScript**

Create `src/pi_prompt.md`:

```markdown
---
description: Review changes with the native pdiff executable
argument-hint: "[staged|branch <name>|commit <sha>]"
---
Use the native `pdiff` executable to review the requested target. Choose the command from `$ARGUMENTS`:

- no argument or `staged`: run `pdiff diff --staged --output pdiff-review.md`
- `branch <name>`: run `pdiff diff <name>...HEAD --output pdiff-review.md`
- `commit <sha>`: run `pdiff show <sha> --output pdiff-review.md`

After pdiff exits, read `pdiff-review.md` if it exists, return its review comments to this conversation, and remove only that generated file. If pdiff reports no changes or no comments, say so directly. Do not construct a JavaScript or TypeScript wrapper.
```

Use `include_str!("pi_prompt.md")`, create `~/.pi/agent/prompts`, and write `pdiff.md` atomically through a sibling temporary file plus rename. `uninstall_at` removes only `pdiff.md` and removes `prompts/` only if it is empty. Production `install`/`uninstall` resolve `dirs::home_dir()` and delegate. Delete `src/pi_extension_src.ts`.

- [ ] **Step 4: Verify there is no runtime or source-language contradiction**

Run:

```bash
rg -n "include_str!.*\.ts|index\.ts|registerCommand|node:|Bun" src Cargo.toml README.md docs/parity/hunk.md
```

Expected: no shipped TypeScript/JavaScript integration or runtime invocation remains. References that explicitly state those runtimes are absent are allowed.

- [ ] **Step 5: Update docs and parity evidence conservatively**

Document `--watch`, manual `r`, atomic-save observation, polling fallback, `$EDITOR` conventions, Ctrl-Z behavior, and the Markdown-based Pi command in `README.md`. In `docs/parity/hunk.md`, mark watch/reload/editor/terminal/Pi rows `verified` only when their named tests pass. Keep Windows `implemented` or `missing` unless an actual Windows CI run exists; do not claim cross-platform verification from Linux.

- [ ] **Step 6: Run the complete slice verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
git diff --check
file target/release/pdiff
ldd target/release/pdiff
```

Expected: formatting, lint, all unit/integration/PTY tests, and release build pass; the artifact is one native executable and its dynamic libraries contain no Node.js, Bun, browser, Hunk, or JavaScript engine.

- [ ] **Step 7: Commit the Rust-only integration closure**

```bash
git add src/pi_extension.rs src/pi_prompt.md tests/integrations.rs README.md docs/parity/hunk.md docs/superpowers/plans/2026-07-21-watch-process-implementation-plan.md
git rm src/pi_extension_src.ts
git commit -m "feat: complete native watch and process parity"
```

---

## Slice completion gate

Before starting notes/markup work, confirm all of the following from fresh command output:

- Direct files, patch files, Git, Jujutsu, and Sapling can be manually reloaded when their source is reloadable.
- `--watch` observes direct-file atomic replacements and Git working-tree changes; Jujutsu and Sapling refresh through bounded polling.
- Burst events coalesce, reload execution is serialized, superseded generations are rejected, and reload errors retain the last valid review.
- Stable file/hunk selection and viewport anchors survive replacement whenever their ids still exist.
- `$EDITOR` receives a literal absolute file path and correct line convention without a shell.
- Normal exit, app errors, panic, editor launch, and Unix suspend/resume restore terminal ownership.
- Tmux remains optional and literal; OSC 52 remains exact for non-ASCII selection.
- `pdiff install pi` writes Markdown only, and `rg --files src | rg '\.(ts|tsx|js|jsx)$'` returns no files.
- `target/release/pdiff` is one native Rust binary with no JavaScript runtime dependency.
