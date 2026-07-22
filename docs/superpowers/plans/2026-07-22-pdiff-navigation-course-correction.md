# Pdiff Navigation Course Correction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore pdiff's explicit semantic cursor and predictable navigation while preventing terminal-generated input from activating Ramo's filter.

**Architecture:** Keep `ReviewController`, row plans, geometry, watch reloads, notes, and the current renderer as the single review model. Turn the existing `selected_row_key` into an explicit cursor, add side focus and cursor actions at the controller boundary, render that cursor visibly, and stop normal-mode input from falling through to the legacy app cursor. Remove active terminal probing so startup never writes a query whose response can become keyboard input.

**Tech Stack:** Rust 2024, Crossterm, Ratatui, portable-pty, vt100, Cargo integration tests.

## Global Constraints

- Ramo remains a single Rust binary with no JavaScript or TypeScript runtime.
- Preserve native Git/Jujutsu/Sapling loading, watch mode, notes, sessions, context expansion, selection, and responsive layouts.
- `theme = "auto"` sends no active terminal query; it uses `COLORFGBG` and otherwise selects `github-dark-default`.
- One Escape clears the file filter and returns to normal input mode.
- `h`/`l` focus split sides; `n` toggles line numbers; Left/Right retain horizontal scrolling.
- Normal review navigation has exactly one state owner: `ReviewController`.
- Production behavior changes must follow red-green-refactor and have PTY coverage where the user observes terminal behavior.

---

### Task 1: Make startup and filter cancellation input-safe

**Files:**
- Modify: `src/ui/appearance.rs`
- Modify: `src/app.rs`
- Modify: `tests/terminal_appearance.rs`
- Modify: `tests/ui_input.rs`

**Interfaces:**
- Consumes: `appearance_from_colorfgbg(&str) -> Option<TerminalAppearance>` and `App::handle_ui_key`.
- Produces: `detect_terminal_appearance()` as an environment-only lookup and atomic Escape cancellation for `InputMode::Filter`.

- [ ] **Step 1: Replace the PTY query expectation with a failing no-query contract**

Change the terminal appearance PTY helper to launch once without responding and assert the raw output does not contain OSC 11:

```rust
#[test]
#[cfg(unix)]
fn auto_theme_starts_without_querying_or_waiting_for_terminal_input() {
    let (raw, elapsed) = launch_and_capture();
    assert!(!raw.windows(b"\x1b]11;?\x1b\\".len())
        .any(|window| window == b"\x1b]11;?\x1b\\"));
    assert!(elapsed < Duration::from_secs(2));
}
```

- [ ] **Step 2: Add a failing one-Escape app test**

Update `app_keys_mutate_the_rendering_controller_and_dialog_modes_own_closing_keys` so one Escape after `/q` must satisfy both:

```rust
assert_eq!(app.review_controller.snapshot(view).filter, "");
assert_eq!(app.input_mode(), InputMode::Normal);
```

Delete the second Escape from the assertion path.

- [ ] **Step 3: Run the focused tests and verify both fail for the intended reasons**

Run:

```bash
cargo test --test terminal_appearance --test ui_input
```

Expected: the PTY test finds the OSC 11 query and the app test observes `InputMode::Filter` after the first Escape.

- [ ] **Step 4: Remove active probing and make filter cancellation atomic**

Implement environment-only detection:

```rust
pub fn detect_terminal_appearance() -> Option<TerminalAppearance> {
    std::env::var("COLORFGBG")
        .ok()
        .as_deref()
        .and_then(appearance_from_colorfgbg)
}
```

Keep pure OSC parsing functions for compatibility, but delete `probe_terminal_background`. In `App::cancel_input`, replace the non-empty filter branch with a single branch that clears, applies `SetFilter(String::new())`, and assigns `InputMode::Normal`.

- [ ] **Step 5: Re-run the focused tests and commit**

Run:

```bash
cargo test --test terminal_appearance --test ui_input
```

Expected: PASS.

Commit:

```bash
git add src/ui/appearance.rs src/app.rs tests/terminal_appearance.rs tests/ui_input.rs
git commit -m "fix: prevent terminal input from activating filters"
```

### Task 2: Add the semantic cursor and focused side to `ReviewController`

**Files:**
- Modify: `src/review/row.rs`
- Modify: `src/review/state.rs`
- Modify: `src/review/mod.rs`
- Modify: `tests/review_state.rs`
- Modify: `tests/reload.rs`

**Interfaces:**
- Consumes: `ReviewRow`, `ReviewRowKey`, `ReviewGeometry`, and the existing stable `selected_row_key`.
- Produces: public `ReviewSide`, `ReviewAction::MoveCursor(i32)`, `ReviewAction::FocusSide(ReviewSide)`, `ReviewSnapshot::focused_side`, and controller cursor invariants.

- [ ] **Step 1: Add failing controller tests for semantic movement and bounds**

Add a fixture with blank context plus additions/deletions and assert:

```rust
let first = controller.snapshot(view).selected_position.clone();
controller.apply(ReviewAction::MoveCursor(1), view);
let second = controller.snapshot(view).selected_position.clone();
assert_ne!(second, first);
controller.apply(ReviewAction::JumpBottom, view);
assert_eq!(controller.snapshot(view).selected_position.as_ref().and_then(|p| p.new_line), Some(LAST_LINE));
assert_eq!(controller.snapshot(view).scroll_top, controller.snapshot(view).max_scroll_top);
controller.apply(ReviewAction::MoveCursor(1), view);
assert_eq!(controller.snapshot(view).selected_position.as_ref().and_then(|p| p.new_line), Some(LAST_LINE));
```

Also assert `JumpTop`, `MoveHunk`, and `MoveFile` put the cursor on a diff row rather than a hunk header.

- [ ] **Step 2: Add failing side-focus and reload-preservation tests**

Exercise `FocusSide(Left/Right)` across context, addition, and deletion rows. Assert additions force right focus, deletions force left focus, and context rows honor explicit focus. Extend `replacing_files_preserves_selected_file_and_viewport_anchor` to assert the semantic selected line survives replacement.

- [ ] **Step 3: Run the state tests and verify compile/test failure**

Run:

```bash
cargo test --test review_state --test reload
```

Expected: FAIL because `ReviewSide`, `MoveCursor`, `FocusSide`, and `focused_side` do not exist and existing jumps may select header anchors.

- [ ] **Step 4: Add row selectability and side availability**

In `row.rs`, add focused helpers without exposing renderer internals:

```rust
impl ReviewRow {
    pub(super) fn is_selectable(&self) -> bool {
        matches!(self, Self::Split { .. } | Self::Stack { .. })
    }

    pub(super) fn available_sides(&self) -> (bool, bool) {
        match self {
            Self::Split { left, right, .. } =>
                (left.kind != CellKind::Empty, right.kind != CellKind::Empty),
            Self::Stack { .. } => (true, true),
            _ => (false, false),
        }
    }
}
```

- [ ] **Step 5: Implement cursor actions and one visibility function**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewSide { Left, Right }

ReviewAction::MoveCursor(i32)
ReviewAction::FocusSide(ReviewSide)
```

Use geometry row indexes to find selectable rows. `MoveCursor` clamps. `JumpTop`/`JumpBottom` choose first/last selectable rows. Hunk/file selection chooses the first selectable row matching its target. Centralize scrolling in `ensure_cursor_visible(viewport, center: bool)`. Keep `select_from_viewport` only for page/wheel scrolling, changing it to choose the selectable row nearest the viewport midpoint. Snap `focused_side` using `available_sides` after every cursor change.

- [ ] **Step 6: Preserve cursor identity across rebuild and reload**

During rebuild, retain the exact `ReviewRowKey` when present; otherwise choose the nearest selectable row in the same hunk, then file, then review. During `replace_files`, capture `selected_row_key` before clearing geometry and restore through the same fallback.

- [ ] **Step 7: Run state, reload, context, note, and session projection tests**

Run:

```bash
cargo test --test review_state --test reload --test context_expansion --test notes_state --test session_projection
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/review/row.rs src/review/state.rs src/review/mod.rs tests/review_state.rs tests/reload.rs
git commit -m "feat: restore semantic review cursor"
```

### Task 3: Render the cursor and the first file identity

**Files:**
- Modify: `src/review/state.rs`
- Modify: `src/review/geometry.rs`
- Modify: `src/ui/review.rs`
- Modify: `tests/ui_render.rs`
- Modify: `tests/ui_mouse.rs`

**Interfaces:**
- Consumes: controller `selected_row_key`, `focused_side`, row `available_sides`, and `AppTheme::selected_hunk`.
- Produces: `ReviewRenderView::{cursor_key, focused_side}` and visible cursor styling with selection precedence.

- [ ] **Step 1: Add failing render tests for cursor paint and selection precedence**

Render a split diff into `Buffer`, locate the cursor line, and assert the active side gutter/content cells use `theme.selected_hunk`. Focus the other side and assert paint moves. Apply an explicit selection and assert its selected cells use `theme.accent_muted`, overriding cursor paint.

- [ ] **Step 2: Add a failing first-file header test**

At width 80 with no sidebar, render one file and assert its path and status header appear before its first hunk row. Update geometry expectations so the first section has `header_height == 1` and `body_top == 1`.

- [ ] **Step 3: Run render, geometry, and mouse tests and verify failure**

Run:

```bash
cargo test --lib review::geometry --test ui_render --test ui_mouse
```

Expected: FAIL because the first header is zero-height and cursor style is not painted.

- [ ] **Step 4: Expose render-only cursor state and paint it**

Add `cursor_key: Option<&ReviewRowKey>` and `focused_side: ReviewSide` to `ReviewRenderView`. In `render_row`, compute which stack/split cell is active. In `render_cell`, after text is painted and before explicit selection, patch the active cell rectangle with:

```rust
Style::default().bg(theme.selected_hunk)
```

Then retain the current selection paint as the final style operation.

- [ ] **Step 5: Give every file a header row**

In `build_review_geometry`, keep separators conditional on `file_index > 0` but set `header_height = 1` for every file. Update hit regions, scrollbar expectations, and tests using the shared geometry.

- [ ] **Step 6: Run focused render suites and commit**

Run:

```bash
cargo test --lib review::geometry --test ui_render --test ui_mouse --test review_selection
```

Expected: PASS.

Commit:

```bash
git add src/review/state.rs src/review/geometry.rs src/ui/review.rs tests/ui_render.rs tests/ui_mouse.rs
git commit -m "feat: render explicit review cursor"
```

### Task 4: Route normal input exclusively through the corrected controller

**Files:**
- Modify: `src/ui/input.rs`
- Modify: `src/ui/dialogs.rs`
- Modify: `src/app.rs`
- Modify: `tests/ui_input.rs`
- Modify: `tests/ui_dialogs.rs`

**Interfaces:**
- Consumes: `ReviewAction::MoveCursor`, `ReviewAction::FocusSide`, and `ReviewSide`.
- Produces: the corrected public key map and no legacy normal-mode navigation fallback.

- [ ] **Step 1: Add failing exact key-map tests**

Assert:

```rust
map_key_event(key(KeyCode::Char('j')), Normal, false)
    == review(ReviewAction::MoveCursor(1));
map_key_event(key(KeyCode::Char('h')), Normal, false)
    == review(ReviewAction::FocusSide(ReviewSide::Left));
map_key_event(key(KeyCode::Char('l')), Normal, false)
    == review(ReviewAction::FocusSide(ReviewSide::Right));
map_key_event(key(KeyCode::Char('n')), Normal, false)
    == review(ReviewAction::ToggleLineNumbers);
```

Assert `j/k`, Up/Down, `g/G`, Home/End, hunk/file jumps, and side focus remain allowed in pager mode where applicable.

- [ ] **Step 2: Add a failing app test proving normal `h/l` change controller state**

Create a split review, send `h` and `l`, and assert `snapshot.focused_side`. Also assert `n` changes `line_numbers` while `l` does not.

- [ ] **Step 3: Run input/dialog tests and verify failure**

Run:

```bash
cargo test --test ui_input --test ui_dialogs
```

Expected: FAIL on old scrolling and `l` line-number mappings.

- [ ] **Step 4: Replace the key map and stop normal fallback**

Map Up/Down and `j/k` to `MoveCursor`, h/l to side focus, and n to line numbers. In `App::handle_key`, dispatch `Mode::Normal` exclusively through `handle_ui_key`; retain legacy handlers only for explicitly active legacy modes such as tmux pane picking. Do not call `handle_nav_key` for an unmapped normal key.

- [ ] **Step 5: Update help text and verify tests**

Document:

```text
j/k, ↑/↓    previous / next line
h / l       focus left / right
n / w / m   numbers / wrap / hunk headers
```

Run:

```bash
cargo test --test ui_input --test ui_dialogs --test integrations
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/ui/input.rs src/ui/dialogs.rs src/app.rs tests/ui_input.rs tests/ui_dialogs.rs
git commit -m "fix: unify review navigation input"
```

### Task 5: Prove rendered navigation in a PTY

**Files:**
- Modify: `tests/pty_ui.rs`
- Modify: `tests/pty_watch.rs`

**Interfaces:**
- Consumes: the installed test binary, existing `PtyProcess`, and unique fixture lines.
- Produces: black-box regression coverage for startup, row movement, bounds, jumps, and reload cursor preservation.

- [ ] **Step 1: Add a failing long-patch PTY test**

Generate a patch whose first, second, hunk-two, and final rows have unique text. Launch at a small height and use the PTY's post-mark output to assert this sequence:

```text
j  -> SECOND_CURSOR_LINE is redrawn
]  -> SECOND_HUNK_CURSOR_LINE is visible
G  -> FINAL_CURSOR_LINE is visible
g  -> FIRST_CURSOR_LINE is visible
```

Use vt100 screen parsing when cumulative terminal output would otherwise produce a false positive.

- [ ] **Step 2: Add a startup filter regression assertion**

Launch with `--theme auto`, wait for the first review screen without sending input, and assert the current vt100 screen contains neither `Filter:` nor an OSC 11 query in raw output.

- [ ] **Step 3: Add watch reload cursor preservation coverage**

Move the cursor to a uniquely named line, atomically replace the watched file while retaining that line, wait for reload, and assert the line remains cursor-highlighted/selected through a session snapshot or rendered redraw.

- [ ] **Step 4: Run PTY tests and verify the new tests fail before final integration if any seam remains**

Run:

```bash
cargo test --test pty_ui --test pty_watch
```

Expected before any missing integration is fixed: FAIL at the relevant rendered assertion. Apply only the minimal integration correction required, then rerun until PASS.

- [ ] **Step 5: Commit**

```bash
git add tests/pty_ui.rs tests/pty_watch.rs
git commit -m "test: prove cursor navigation in a real terminal"
```

### Task 6: Update product documentation and close verification

**Files:**
- Modify: `README.md`
- Modify: `docs/parity/hunk.md`

**Interfaces:**
- Consumes: final key map and terminal behavior.
- Produces: accurate public controls and evidence; no claim that Ramo intentionally matches Hunk's viewport interaction.

- [ ] **Step 1: Update README controls, themes, and navigation language**

Document semantic cursor navigation, `h/l`, `n`, one-Escape filtering, and environment-only auto-theme resolution. Remove the statement that auto theme sends OSC 11.

- [ ] **Step 2: Correct the parity ledger**

Replace Hunk-behavior claims for arrow scrolling and `l` with Ramo's intentional pdiff-derived cursor behavior, naming the new controller/render/PTY tests as evidence.

- [ ] **Step 3: Run formatting and strict focused verification**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Expected: all commands exit 0 with no warnings or failures.

- [ ] **Step 4: Verify the release artifact and source constraints**

Run:

```bash
cargo build --release --locked
file target/release/ramo
! rg -n --glob '*.ts' --glob '*.tsx' --glob '*.js' --glob '*.jsx' '.' src tests
git diff --check
```

Expected: one native executable, no JavaScript/TypeScript source matches, and no whitespace errors.

- [ ] **Step 5: Commit documentation and verification record**

```bash
git add README.md docs/parity/hunk.md
git commit -m "docs: restore pdiff navigation identity"
```

- [ ] **Step 6: Inspect final history and worktree**

Run:

```bash
git status --short --branch
git log --oneline --decorate -8
```

Expected: clean `fix/pdiff-navigation` worktree with focused conventional commits on top of the approved design commit.
