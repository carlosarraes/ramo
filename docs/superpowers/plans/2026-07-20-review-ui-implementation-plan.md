# Hunk-compatible Review UI Implementation Plan

> Execute this plan task by task with red-green-refactor checkpoints. The product remains one Rust binary; no JavaScript, TypeScript, Node, Bun, browser, or Hunk runtime code is introduced.

**Goal:** Replace the legacy coupled `App`/`side_by_side` UI with a reusable Rust review controller, shared row geometry, and Ratatui widgets that match Hunk's review-stream behavior while intentionally omitting the top menu bar and dropdown menus.

**Reference:** `/home/carraes/github/hunk` at commit `53fcb2c`, especially `src/ui/lib`, `src/ui/diff`, `src/ui/components/panes`, `src/ui/components/chrome`, `src/ui/themes.ts`, and the `AppHost.*.test.tsx` interaction suites.

**Architecture:** One deep `review` module owns visible-file derivation, stable row identities, split/stack row planning, geometry, navigation, filtering, wrapping, selection, collapsed-context state, and viewport anchoring. Its external interface is limited to `ReviewController`, `ReviewOptions`, `ReviewAction`, `ReviewEffect`, and immutable `ReviewSnapshot`; row planners and geometry are crate-private implementation. Focused modules under `src/ui/` turn snapshots into Ratatui widgets and map terminal events to typed review actions. `App` becomes a thin coordinator for the review module plus the retained comment/tmux/clipboard integrations. Rendering and interaction consume the same internal `ReviewGeometry`; no widget independently re-derives scroll heights or hit targets.

**Intentional exclusion:** Do not create a top menu bar, menu dropdown, F10 menu flow, menu visibility preference, or menu-only mouse target. Help, theme selection, save-preference confirmation, filtering, and every review action remain available through direct keys or centered dialogs.

**Runtime rule:** The binary remains a native Rust executable. Normal dependencies added by this slice must be Rust libraries linked into that executable. Test-only terminal dependencies stay under `dev-dependencies`.

---

## Target module map

```text
src/review/
  mod.rs              public reusable review surface
  row.rs              stable split/stack/collapsed/placeholder row plans
  emphasis.rs         character-level changed-content spans
  geometry.rs         row heights, file sections, hit targets, render windows
  state.rs            selected file/hunk/row, filter, layout and view options
  navigation.rs       step/page/hunk/file/annotation movement
  anchor.rs           stable viewport anchors across width/layout changes
  context.rs          collapsed-gap discovery and bounded source expansion
  selection.rs        terminal-cell-aware text selection/copy projection

src/ui/
  review.rs           top-level no-menu layout widget
  diff.rs             split/stack row widgets
  sidebar.rs          grouped/windowed file navigation
  dialogs.rs          help, theme, and save-preference overlays
  input.rs            key/mouse to typed action mapping
  themes.rs           embedded theme registry and semantic palettes
  highlight.rs        lazy bounded syntax span cache
```

The existing `src/ui/side_by_side.rs` remains only as a temporary consumer while Tasks 1–4 land. Task 5 removes it from runtime dispatch after equivalent render tests pass. The interface is the test surface: end-user behavior is tested through `ReviewController`/`ReviewSnapshot`; focused unit tests may exercise crate-private row/geometry implementation, but those types are not exported merely for tests.

---

### Task 1: Define stable review rows and split/stack planning

**Files:**
- Create: `src/review/mod.rs`
- Create: `src/review/row.rs`
- Create: `src/review/emphasis.rs`
- Modify: `src/lib.rs`
- Unit test in: `src/review/row.rs`
- Unit test in: `src/review/emphasis.rs`

**Implementation:**
- Consumes: `DiffFile`, `Hunk`, `DiffLine`, `LineType`, `MovedLineKind`, file metadata and stable file ids.
- Produces crate-private `ReviewRowKey`, `ReviewRow`, `ReviewCell`, `ChangedSpan`, `RowPlan`, and pure `build_row_plan` behind the later `ReviewSnapshot` interface.

- [x] **Step 1: Write failing row-plan tests**

Cover these contracts with crate-private unit tests:

1. Split mode pairs a deletion/addition block by ordinal position, emits explicit empty cells for an uneven block, and renders context on both sides.
2. Stack mode preserves deletion-before-addition order and carries both old/new numbers on context rows.
3. Every row key includes stable file id, hunk index, semantic row kind, and source-line identity; rebuilding cloned files returns identical keys.
4. File placeholders for binary, too-large, and no-hunk files each produce one typed placeholder row with the correct state label.
5. Moved-line classes survive row planning on their matching side.
6. Paired changed lines expose character-emphasis spans computed with `similar::TextDiff::from_chars`; common prefix/suffix stays neutral while changed content is emphasized.
7. Tabs expand to two spaces and C0/OSC/CSI controls never enter a row's visible text.

- [x] **Step 2: Run the row tests and verify red**

Run: `cargo test --lib review::`

Expected: compilation fails because the `review` module and row types do not exist.

- [x] **Step 3: Implement immutable row plans**

Use closed enums rather than renderer-specific callbacks:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReviewRowKey {
    pub file_id: String,
    pub hunk_index: Option<usize>,
    pub kind: ReviewRowKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewRow {
    HunkHeader { key: ReviewRowKey, text: String },
    Collapsed { key: ReviewRowKey, gap: GapKey, old: LineRange, new: LineRange, state: GapState },
    Split { key: ReviewRowKey, left: ReviewCell, right: ReviewCell },
    Stack { key: ReviewRowKey, cell: ReviewCell },
    Placeholder { key: ReviewRowKey, kind: PlaceholderKind, text: String },
}
```

`ReviewCell` records semantic kind, sign, optional old/new line numbers, moved class, and `Vec<ChangedSpan>`. Do not store Ratatui `Style` in review types. `build_row_plan(file, layout, show_hunk_headers)` returns rows plus hunk anchor keys and maximum line-number digits.

For split pairing, partition each hunk into context rows and adjacent changed blocks. Zip deletions with additions within each block, filling the shorter side with `CellKind::Empty`. Stack mode emits original diff order. Use shared terminal-text sanitization and a fixed two-column tab expansion before emphasis.

- [x] **Step 4: Run row, parser, and model regressions**

Run: `cargo test --lib review:: && cargo test --test input_loading && cargo test --lib`

Expected: all pass; normalized input types remain renderer-independent.

- [x] **Step 5: Commit row planning**

```bash
git add src/review src/lib.rs
git commit -m "refactor: add stable review row plans"
```

---

### Task 2: Build one shared geometry, windowing, and viewport-anchor model

**Files:**
- Create: `src/review/geometry.rs`
- Create: `src/review/anchor.rs`
- Modify: `src/review/mod.rs`
- Unit test in: `src/review/geometry.rs`
- Unit test in: `src/review/anchor.rs`

**Implementation:**
- Consumes: ordered visible files, their `RowPlan`s, layout, content width, line-number visibility, wrapping, sidebar width, and viewport.
- Produces crate-private `ReviewGeometry`, `FileSection`, `RowBounds`, `VisibleWindow`, `ViewportAnchor`, binary-search lookup and hit testing behind `ReviewController::snapshot`.

- [x] **Step 1: Write failing geometry tests from Hunk's invariants**

Add deterministic cases for:

1. The first file has no in-stream header; every later file adds one separator row and one header row without removing earlier files from the stream.
2. `auto` resolves to split at widths `>= 160`, stack below `160`; the sidebar is automatically visible only at widths `>= 220`. Explicit split/stack override only the diff layout, not the responsive sidebar rule.
3. Split widths reserve one active rail and one center divider. Stack gutters reserve old/new line columns. Width never underflows at tiny terminal sizes.
4. No-wrap rows have height one. Wrap rows use Unicode terminal-cell width, never byte or scalar count, and return at least one row for empty content.
5. Binary search maps stream offsets to file/hunk/row at the exact first/last boundary.
6. The visible window returns only intersecting rows plus bounded overscan and uses top/bottom spacer heights for a 100,000-row fixture.
7. Capturing a stable row anchor plus intra-row offset and restoring it after split/stack, wrap, line-number, sidebar, and width changes keeps the same semantic row at the same relative viewport location when it still exists.
8. Missing anchor keys fall back to the selected hunk, selected file, then clamped absolute offset in that order.

- [x] **Step 2: Run focused geometry tests and verify red**

Run: `cargo test --lib review::`

Expected: compilation fails because geometry and anchor APIs are absent.

- [x] **Step 3: Implement geometry as the single measurement authority**

`ReviewGeometry` owns ordered `FileSection`s and flattened `RowBounds`; each bound stores stable key, absolute top, measured height, file index, hunk index, and row index. Build lookup maps once per geometry. Use `unicode-width` for cell measurements and saturating arithmetic for terminal dimensions.

Cache at most the currently active and immediately previous geometry variant in the controller; do not accumulate every resize width. `VisibleWindow` uses binary search plus an adaptive but bounded overscan range. Rendering, keyboard reveal, sidebar selection, mouse hit testing, selection, scrollbar math, and context expansion all consume these same bounds.

- [x] **Step 4: Run geometry and strict lint gates**

Run: `cargo test --lib review:: && cargo clippy --all-targets --all-features -- -D warnings`

Expected: all pass without large retained caches or unchecked dimension casts.

- [x] **Step 5: Commit shared geometry**

```bash
git add Cargo.toml Cargo.lock src/review tests/review_geometry.rs
git commit -m "feat: add shared review geometry"
```

---

### Task 3: Introduce the pure review controller and Hunk navigation semantics

**Files:**
- Create: `src/review/state.rs`
- Create: `src/review/navigation.rs`
- Modify: `src/review/mod.rs`
- Test: `tests/review_state.rs`

**Interface:**
- Consumes: files, layout preference, view preferences, filter input, viewport size, geometry and stable human-note anchors.
- Produces: `ReviewController`, `ReviewAction`, visible files, grouped sidebar entries, selected row/file/hunk, scroll state, and transient status.

- [x] **Step 1: Write failing state and navigation tests**

Verify:

1. The default stream contains every file in order. Selecting a sidebar file changes selection and scrolls its section to the top; it does not filter other files.
2. File filter matching is case-insensitive across current path, previous path, and summary text. Filtering away the current file selects the first visible match. Clearing restores the full stream while retaining a still-valid file id.
3. `step`, `half page`, `full page`, top/bottom, previous/next hunk, and previous/next file clamp or wrap exactly as Hunk: hunk/file navigation wraps; scrolling clamps.
4. Annotated-hunk navigation uses stable file/hunk anchors and respects the active filter.
5. Layout `0/1/2`, sidebar, line-number, wrap, hunk-header, and horizontal-scroll actions rebuild geometry through an anchor transaction, preserving position.
6. Horizontal scroll clamps to the widest visible code line minus the active code viewport and resets to zero when wrapping is enabled.
7. Empty filters and no-match filters produce valid zero-row state without panics.
8. Pager chrome mode allows navigation/wrap/sidebar but suppresses filter, dialogs, notes, and preference persistence.

- [x] **Step 2: Run controller tests and verify red**

Run: `cargo test --test review_state`

Expected: compilation fails because `ReviewController` and typed actions are absent.

- [x] **Step 3: Implement action reduction and derived state**

Keep the external interface small:

```rust
pub fn apply(&mut self, action: ReviewAction, viewport: Viewport) -> ReviewEffect;
pub fn snapshot(&mut self, viewport: Viewport) -> &ReviewSnapshot;
```

`ReviewEffect` reports only external work: redraw, open help/theme/save dialog, load context, copy text, edit file, reload, or quit. Pure navigation never reaches the process layer. `ReviewSnapshot` contains already-planned visible rows, sidebar entries, status/dialog state, selection, scroll metrics and crate-owned hit metadata; callers do not assemble geometry. The controller stores stable ids/keys instead of raw vector positions across geometry rebuilds.

Group sidebar entries by POSIX-style directory while retaining file order. Show addition/deletion counts and `+` on truncated additions. Metadata labels distinguish added, deleted, renamed, copied, binary, large and untracked files.

- [x] **Step 4: Run state plus row/geometry regressions**

Run: `cargo test --test review_state && cargo test --lib review::`

Expected: all pass.

- [x] **Step 5: Commit review state**

```bash
git add src/review tests/review_state.rs
git commit -m "feat: add hunk-compatible review state"
```

---

### Task 4: Add semantic themes and lazy bounded highlighting

**Files:**
- Create: `src/ui/themes.rs`
- Replace: `src/ui/theme.rs`
- Modify: `src/ui/highlight.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/config/model.rs`
- Modify: `src/config/load.rs`
- Test: `tests/themes.rs`
- Test: `tests/highlighting.rs`

**Interfaces:**
- Consumes: Hunk theme identifiers/order, legacy aliases, custom TOML colors, transparent-background preference, file language/path and row spans.
- Produces: `ThemeRegistry`, semantic `AppTheme`, `resolve_theme`, theme selector items, validated custom palette, and an on-demand bounded syntax cache.

- [x] **Step 1: Write failing theme and cache tests**

Cover:

1. All Hunk bundled ids appear in the reference order, including `github-dark-default` and `github-light-default`.
2. Legacy ids map exactly: graphite, midnight, paper, ember, and zenburn.
3. Unknown ids fall back predictably; `auto` selects the supplied light/dark terminal appearance.
4. Transparent mode resets only neutral surfaces; semantic added/removed/moved rows remain painted and dialogs remain opaque.
5. Custom theme inherits a built-in base, validates every override as `#rrggbb`, preserves exact scope selectors, and rejects unknown fields with a path/key diagnostic.
6. Moved rows use moved palettes, line numbers use matching gutters, and changed spans use stronger content backgrounds.
7. Highlighting is created only for requested visible rows, cached by file id/content/theme, and bounded so cycling through many files/themes evicts old entries.

- [x] **Step 2: Run theme/highlight tests and verify red**

Run: `cargo test --test themes && cargo test --test highlighting`

Expected: compilation fails on the new registry and cache APIs.

- [x] **Step 3: Port semantic palettes as Rust data**

Embed theme ids and semantic base colors as Rust constants. Do not ship JSON or execute a theme converter at runtime. Syntect supplies syntax tokenization; custom exact TextMate scopes are translated to Syntect selectors where supported and preserved in config even when a selector has no current match.

Replace eager whole-review highlighting with `HighlightCache::spans(file, hunk, line, theme)`. Retain a fixed number of file/theme entries and replace stale content-sensitive keys. Geometry must never depend on the presence of highlight spans.

- [x] **Step 4: Run config, theme, and memory-shape regressions**

Run: `cargo test --test themes && cargo test --test highlighting && cargo test --test config_resolution && cargo clippy --all-targets --all-features -- -D warnings`

Expected: all pass; no dynamic runtime assets are loaded.

- [x] **Step 5: Commit themes and highlighting**

```bash
git add src/ui src/config tests/themes.rs tests/highlighting.rs Cargo.toml Cargo.lock
git commit -m "feat: add native themes and lazy highlighting"
```

---

### Task 5: Render the continuous review stream with no top chrome

**Files:**
- Create: `src/ui/review.rs`
- Create: `src/ui/diff.rs`
- Create: `src/ui/sidebar.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/app.rs`
- Remove runtime use of: `src/ui/side_by_side.rs`
- Test: `tests/ui_render.rs`

**Interfaces:**
- Consumes: controller snapshot, shared geometry visible window, theme, highlight cache and Ratatui buffer.
- Produces: reusable `ReviewWidget`, `DiffStreamWidget`, `SidebarWidget`, render metadata/hit regions, and snapshot-style buffer assertions.

- [x] **Step 1: Write failing render-model and buffer tests**

At 80, 160 and 220 columns assert:

1. Auto mode is stack/no-sidebar at 80 and 159, split/no-sidebar at 160, split/sidebar at 220.
2. Every visible file remains in the main stream; later files render an in-stream header and separator.
3. Split rows paint old/new rails, gutters, empty cells and divider; stack rows paint old/new number columns and semantic signs.
4. Added/deleted/renamed/copied/untracked/binary/large states appear in headers/sidebar/placeholders with correct stats.
5. Moved classes and character-emphasis spans use their semantic palette.
6. Hidden hunk headers occupy zero rows in both rendering and geometry.
7. Long no-wrap rows clip at horizontal offset; wrap rows occupy exactly the measured height.
8. No frame contains Hunk's top-menu labels, a top menu row, dropdown chrome, or `F10 menu`.
9. The bottom status row is absent when inactive and appears only for filter focus/value, dialog hints, warning, mode, or transient feedback. Pager mode omits normal app chrome.
10. Only rows in `VisibleWindow` are materialized into the buffer for a many-file fixture.

- [x] **Step 2: Run render tests and verify red**

Run: `cargo test --test ui_render`

Expected: compilation fails until the new widgets exist.

- [x] **Step 3: Implement widgets from geometry**

Widgets receive immutable snapshots and return hit regions; they do not mutate controller state. Fill each row to its assigned width so transparent and opaque backgrounds behave consistently. Apply syntax foregrounds after semantic row backgrounds, then selection and changed-span backgrounds in that order.

Keep legacy annotation popup/tmux picker rendering behind overlays temporarily, but main stream rows and sidebar must use the new widgets. Delete or stop exporting legacy `side_by_side::render` when no runtime caller remains.

- [x] **Step 4: Run render and existing PTY regressions**

Run: `cargo test --test ui_render && cargo test --test pty_pager -- --nocapture && cargo test --all-targets`

Expected: all pass; terminal entry/restoration remains exactly once.

- [x] **Step 5: Commit the native review renderer**

```bash
git add src/app.rs src/ui tests/ui_render.rs
git commit -m "feat: render continuous native review stream"
```

---

### Task 6: Replace key handling with the Hunk map and focused dialogs

**Files:**
- Create: `src/ui/input.rs`
- Create: `src/ui/dialogs.rs`
- Modify: `src/app.rs`
- Modify: `src/ui/review.rs`
- Test: `tests/ui_input.rs`
- Test: `tests/ui_dialogs.rs`

**Interfaces:**
- Consumes: Crossterm key events, current focus/dialog mode, controller actions and retained Vim/comment/tmux operations.
- Produces: precedence-safe `map_key_event`, help/theme/save dialogs and direct no-menu actions.

- [ ] **Step 1: Write failing key-policy tests**

Assert the direct map:

- arrows and `j/k`: step; left/right: horizontal columns; Shift-left/right: eight columns;
- Space/`f`, PageDown: page down; `b`, PageUp, Shift-Space: page up;
- `d/u`: half page without Ctrl; `g/G`, Home/End: bounds;
- `[/]`: hunks; `,/.`: files; `{/}`: annotated hunks;
- `1/2/0`: split/stack/auto; `s`: sidebar; `t`: theme; `a`: agent notes; `z`: context; `l`: line numbers; `w`: wrap; `m`: hunk headers; `e`: editor effect; `r`: reload effect; `/`: file filter; `c`: human note; Tab: file/filter focus; `?`: help; `q`: quit.

Retain Vim visual-line/yank, tmux send, and comment export on documented non-conflicting bindings. The chosen bindings must appear in help tests. Do not map F10 or `M` to a menu.

Mode precedence tests:

1. Save prompt owns all keys; Enter/`s` saves, repeated `q` discards, `n` disables future prompt, Escape cancels.
2. Theme dialog owns arrows/Tab/Enter/Escape and restores preview on cancel.
3. Help closes only on Escape/`?`/`q` and never passes the closing key through.
4. Filter focus accepts literal `q`, `t`, `s`, and other global keys; Escape clears a nonempty filter before leaving focus.
5. Note editing owns text; Escape cancels and Ctrl-S saves.
6. Pager mode accepts only navigation, wrap, sidebar and quit.

- [ ] **Step 2: Run input/dialog tests and verify red**

Run: `cargo test --test ui_input && cargo test --test ui_dialogs`

Expected: compilation fails on typed focus/dialog/input APIs.

- [ ] **Step 3: Implement action mapping and centered overlays**

`map_key_event` is pure and returns `Option<AppAction>`. `App` applies it after mode precedence. Help content lists only real pdiff bindings and explicitly contains no menu instructions. Theme selection previews without persistence until Enter. Dialog bounds use saturating centered rectangles and remain usable on small terminals.

- [ ] **Step 4: Run all controller/render/input tests**

Run: `cargo test --test ui_input && cargo test --test ui_dialogs && cargo test --test review_state && cargo test --test ui_render`

Expected: all pass.

- [ ] **Step 5: Commit interaction mapping**

```bash
git add src/app.rs src/ui tests/ui_input.rs tests/ui_dialogs.rs
git commit -m "feat: add hunk keyboard controls and dialogs"
```

---

### Task 7: Add collapsed-context expansion through bounded native sources

**Files:**
- Create: `src/review/context.rs`
- Modify: `src/review/row.rs`
- Modify: `src/review/state.rs`
- Modify: `src/vcs/source.rs`
- Modify: `src/app.rs`
- Modify: `src/runtime.rs`
- Test: `tests/context_expansion.rs`

**Interfaces:**
- Consumes: hunk ranges, `SourceSpec`, selected hunk, `SourceReader`, source-size bounds and layout.
- Produces: stable `GapKey`, collapsed rows, `ContextSourceLoader`, cached load/error state and expanded context rows.

- [ ] **Step 1: Write failing context tests**

Cover:

1. Gaps before/between hunks are derived from old/new starts/counts and receive stable keys. No negative/overlapping range is emitted.
2. `z` chooses the selected hunk's leading gap, then later leading gaps, then trailing gap when source length establishes one.
3. Expansion loads the new source except deleted files, which load old source.
4. Loaded CRLF source is normalized and sliced into split/stack context rows with exact old/new numbers.
5. Loading, missing, non-UTF-8, too-large and command failures render distinct one-row states and preserve the last valid geometry.
6. Repeated toggles and layout changes reuse the cached source; fake runner invocation count stays one per `SourceSpec`.
7. Files with `SourceSpec::None` report unavailable context without spawning.

- [ ] **Step 2: Run context tests and verify red**

Run: `cargo test --test context_expansion`

Expected: compilation fails because gap/source controller APIs do not exist.

- [ ] **Step 3: Implement an owned context loader boundary**

Keep the existing borrowed `SourceReader` public contract. Add a UI-facing `ContextSourceLoader` trait and native owned implementation that owns `SystemCommandRunner`, executable name, size limit and a `HashMap<SourceSpec, Result<Option<String>, SourceFailure>>`. Runtime passes it to `App`; unit tests pass a fake loader. Do not store closures or runtime objects in `DiffFile`.

Context loading may be synchronous in this slice because bounded preflight prevents unbounded reads; return terminal ownership/process async improvements to slice 4. Wrap every geometry change in viewport-anchor capture/restore.

- [ ] **Step 4: Run source/context/full regressions**

Run: `cargo test --test context_expansion && cargo test --test git_loading source_ && cargo test --all-targets`

Expected: all pass.

- [ ] **Step 5: Commit context expansion**

```bash
git add src/review src/vcs/source.rs src/app.rs src/runtime.rs tests/context_expansion.rs
git commit -m "feat: expand collapsed diff context natively"
```

---

### Task 8: Add terminal-cell selection, copy, mouse, and scrollbar interaction

**Files:**
- Create: `src/review/selection.rs`
- Modify: `src/review/geometry.rs`
- Modify: `src/review/state.rs`
- Modify: `src/ui/input.rs`
- Modify: `src/ui/review.rs`
- Modify: `src/app.rs`
- Modify: `src/runtime.rs`
- Test: `tests/review_selection.rs`
- Test: `tests/ui_mouse.rs`

**Interfaces:**
- Consumes: shared row bounds/cell spans, terminal cell coordinates, Crossterm mouse events and clipboard integration.
- Produces: text selection projections, hit-tested mouse actions, draggable sidebar divider/scrollbar and OSC 52 copy effects.

- [ ] **Step 1: Write failing selection and mouse tests**

Verify:

1. Dragging across ASCII and CJK full-width text copies exactly the selected terminal cells without splitting a wide glyph or over-including following ASCII.
2. Split selection resolves old/new side by x coordinate and copies only that column; stack selection preserves displayed semantic row order.
3. Word and line expansion helpers use row geometry and sanitized text.
4. Ordinary wheel scroll changes vertical position; Shift-wheel and native horizontal wheel change only horizontal code offset.
5. Clicking a sidebar file selects it and places its section at review top.
6. Clicking a collapsed row toggles that gap. Clicking/dragging the vertical scrollbar maps through total geometry.
7. Dragging the sidebar divider clamps to a minimum and a terminal-relative maximum; non-left buttons and release-without-drag do nothing.
8. Mouse coordinates outside current hit regions never mutate state.

- [ ] **Step 2: Run mouse/selection tests and verify red**

Run: `cargo test --test review_selection && cargo test --test ui_mouse`

Expected: compilation fails on selection and hit-testing APIs.

- [ ] **Step 3: Implement shared-coordinate interaction**

Enable mouse capture only while the review UI owns the terminal and disable it on every normal return. All hit regions come from the same rendered geometry snapshot. Use `unicode-width` to map cells to grapheme-safe boundaries; add `unicode-segmentation` only if tests demonstrate scalar iteration is insufficient.

Retain keyboard `V`/`y` selection and existing clipboard/tmux flows through the new stable row selection projection. Mouse copy uses the same projection and clipboard function.

- [ ] **Step 4: Run input/render/mouse regressions**

Run: `cargo test --test review_selection && cargo test --test ui_mouse && cargo test --test ui_input && cargo test --test ui_render && cargo clippy --all-targets --all-features -- -D warnings`

Expected: all pass.

- [ ] **Step 5: Commit mouse and selection behavior**

```bash
git add Cargo.toml Cargo.lock src/review src/ui src/app.rs src/runtime.rs tests/review_selection.rs tests/ui_mouse.rs
git commit -m "feat: add cell-correct review interaction"
```

---

### Task 9: Persist changed view preferences without destroying TOML

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/config/model.rs`
- Create: `src/config/save.rs`
- Modify: `src/config/mod.rs`
- Modify: `src/app.rs`
- Modify: `src/runtime.rs`
- Test: `tests/config_persistence.rs`
- Test: `tests/pty_ui.rs`

**Interfaces:**
- Consumes: initial/resolved preferences, runtime changes, user config path and save-dialog choice.
- Produces: dirty preference diff, comment-preserving TOML update and quit confirmation behavior.

- [ ] **Step 1: Write failing persistence tests**

Cover:

1. Only changed view keys are written: mode, theme, sidebar preference where supported, line numbers, wrapping, hunk headers, agent notes and transparent background.
2. Existing comments, unrelated keys/sections, ordering and custom theme tables remain byte-stable outside changed values.
3. Save creates the platform parent directory when absent; I/O errors name the path and keep the app open with feedback.
4. `prompt_save_view_preferences = false` exits without a dialog and without saving.
5. Save, discard, never-ask and cancel choices have distinct effects; repeated `q` while prompted discards.
6. Pager mode never persists view changes.

- [ ] **Step 2: Run persistence tests and verify red**

Run: `cargo test --test config_persistence`

Expected: compilation fails because the save layer does not exist.

- [ ] **Step 3: Implement targeted TOML editing**

Add `toml_edit` and update only keys owned by the user-global view layer. Never rewrite repository or command-specific configuration during interactive preference saving. Preserve unrelated formatting/comments. Surface a typed `ConfigSaveError` through the app without leaving raw mode.

- [ ] **Step 4: Verify dialog behavior under PTY**

Add PTY cases that change layout/theme, press `q`, observe the centered save prompt, then exercise save/discard/cancel. Assert no top menu text and exactly one alternate-screen restore on actual exit.

Run: `cargo test --test config_persistence && cargo test --test pty_ui -- --nocapture`

Expected: all pass.

- [ ] **Step 5: Commit preference persistence**

```bash
git add Cargo.toml Cargo.lock src/config src/app.rs src/runtime.rs tests/config_persistence.rs tests/pty_ui.rs
git commit -m "feat: persist native review preferences"
```

---

### Task 10: Prove the full UI contract in PTYs and close slice-3 evidence

**Files:**
- Modify: `tests/pty_ui.rs`
- Modify: `tests/pty_pager.rs`
- Modify: `README.md`
- Modify: `docs/parity/hunk.md`
- Modify: `docs/superpowers/plans/2026-07-20-review-ui-implementation-plan.md`

**Interfaces:**
- Consumes: release binary, multi-file fixtures, every UI action, resize/mouse sequences, docs ledger.
- Produces: black-box proof, truthful documentation, and the checkpoint for watch/process work.

- [ ] **Step 1: Add bounded PTY scenarios**

Reuse the five-second reader-thread harness and add cases for:

1. 220→160→159 resize changes sidebar/layout and retains the selected file/hunk anchor.
2. `1/2/0`, `s`, `l`, `w`, `m`, `[/]`, `,/.`, `g/G`, page and half-page keys update visible state exactly once.
3. Filter focus accepts literal global-key characters, narrows files, and Tab/Escape restore review focus correctly.
4. Help and theme dialogs open/close with no menu/dropdown text.
5. Binary/large/untracked/rename/moved/word-emphasis rows render semantic labels/colors.
6. Context expansion shows source lines and preserves position across layout/wrap changes.
7. Mouse wheel/sidebar/context/scrollbar and CJK selection produce expected state/copy output.
8. Pager diff mode uses minimal chrome but retains pager navigation/wrap/sidebar behavior.
9. Every normal quit restores the alternate screen exactly once and no test leaves a child alive.

- [ ] **Step 2: Run the complete UI and regression gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --test pty_ui -- --nocapture
cargo test --test pty_pager -- --nocapture
cargo build --release
```

Expected: all pass with no ignored slice-3 tests.

- [ ] **Step 3: Audit the parity ledger conservatively**

Mark only test-backed UI rows `verified`. Keep these staged for later slices even if a placeholder seam exists: watcher execution, editor/job control, normalized external agent notes, STML, session broker/CLI, panic restoration and final cross-platform/release closure. The top menu/dropdown exclusion remains verified by both source inspection and PTY absence checks.

- [ ] **Step 4: Update user documentation**

Document responsive thresholds, direct controls, filtering, collapsed context, theme selection/custom themes, mouse behavior and preference saving. State explicitly that there is no top menu bar or dropdown UI and no TypeScript runtime.

- [ ] **Step 5: Commit slice-3 evidence**

```bash
git add README.md docs/parity/hunk.md docs/superpowers/plans/2026-07-20-review-ui-implementation-plan.md tests/pty_ui.rs tests/pty_pager.rs
git commit -m "docs: verify native review ui parity"
```

---

## Self-review record

- **Deep module and seam:** the `review` module's external interface is controller/options/action/effect/snapshot. Crate-private planners and `ReviewGeometry` provide locality without expanding what callers must learn; widgets only render snapshots and emit typed actions.
- **No-menu decision:** no menu modules, F10 path, top row, dropdown widgets, or menu persistence key are created. Dialogs and direct shortcuts retain the useful actions.
- **Runtime footprint:** `unicode-width`, optional `unicode-segmentation`, and `toml_edit` are Rust libraries linked into the same binary. There is no interpreter, asset server, second executable or Hunk process.
- **Source expansion:** `SourceSpec` remains data. A UI-owned loader owns the runner/cache and preserves the existing public `SourceReader` API.
- **Performance shape:** whole-review syntax highlighting is removed; geometry caches retain only active variants; render/sidebar windows are bounded; large source reads remain preflighted.
- **Slice boundary:** external agent-note normalization/rendering is completed in slice 5. This slice provides stable note anchors, human-note navigation and note-ready geometry without inventing the later agent-context schema.
- **Temporary code removal:** legacy `side_by_side` runtime rendering and positional navigation are removed after Task 5; compatibility comment/tmux/clipboard behaviors are reattached to stable review positions rather than discarded.
