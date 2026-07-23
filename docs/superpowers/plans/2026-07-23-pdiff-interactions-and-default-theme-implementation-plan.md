# Pdiff Interactions and Default Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore old pdiff navigation, visual-note, comment-completion, and tmux-send behavior in Ramo's current native UI, and make Tokyo Night the automatic dark theme.

**Architecture:** Keep input decoding in `ui::input`, derive stable note ranges inside `ReviewController`, and let `App` coordinate native note and tmux lifecycles. Render the existing tmux pane model through a new current-UI `DialogOverlay`; retain `TmuxClient` as the only process boundary. Change only the automatic dark-theme constant so explicit theme choices remain stable.

**Tech Stack:** Rust 2024, Crossterm, Ratatui, portable-pty/vt100, tmux CLI integration, Cargo tests.

## Global Constraints

- Preserve the single native Rust binary; add no Node.js, TypeScript, browser, fuzzy-finder, SDK, or runtime dependency.
- Keep plain `d`/`u` while restoring `Ctrl-D`/`Ctrl-U`.
- Keep tmux sending generic for Claude, Pi, shells, and other panes.
- Save a native note after `Ctrl-T` only when tmux delivery succeeds.
- Preserve a visual selection as the complete note target, then clear visual state when note editing is saved or cancelled.
- Resolve automatic dark and unknown appearance to `tokyo-night`; keep automatic light on `github-light-default`.
- Do not change explicit, repository, CLI, custom, transparent-background, or pager-mode theme semantics.

## File Structure

- `src/ui/input.rs`: modifier-aware review and note key mappings.
- `src/review/state.rs`: convert visual `SelectionPoint` pairs into stable `NoteTarget` ranges and expose bounded draft context.
- `src/app.rs`: coordinate selection teardown, tmux picker state, delivery completion, and note saving.
- `src/ui/dialogs.rs`: render the current native tmux picker and updated control hints.
- `src/ui/themes.rs`, `src/ui/theme.rs`: select Tokyo Night as the automatic/legacy dark default without changing bundled palettes.
- `tests/ui_input.rs`: direct public keymap contract.
- `tests/notes_state.rs`: controller-level range and draft-context contract.
- `tests/ui_dialogs.rs`: current-renderer overlay contract.
- `tests/pty_notes.rs`: live visual-range and note-completion contract.
- `tests/pty_tmux.rs`: real isolated tmux picker and note-send contract.
- `tests/themes.rs`, `tests/config_resolution.rs`: automatic and explicit theme stability.
- `README.md`, `docs/parity/hunk.md`: user-visible controls and corrected parity evidence.

---

### Task 1: Restore navigation and note-editor key contracts

**Files:**
- Modify: `src/ui/input.rs`
- Modify: `src/ui/dialogs.rs`
- Test: `tests/ui_input.rs`
- Test: `tests/ui_dialogs.rs`

**Interfaces:**
- Consumes: `KeyEvent`, `InputMode`, `ReviewAction::Scroll`, and `ScrollUnit::HalfPage`.
- Produces: `AppAction::SendNote { reset_target: bool }` for Task 4; note Enter/Shift-Enter/Ctrl-S behavior for `App`.

- [ ] **Step 1: Write failing keymap tests**

Add a helper and assertions in `tests/ui_input.rs`:

```rust
fn controlled(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

assert_eq!(
    map_key_event(controlled(KeyCode::Char('d')), InputMode::Normal, false),
    Some(AppAction::Review(ReviewAction::Scroll {
        delta: 1,
        unit: ScrollUnit::HalfPage,
    }))
);
assert_eq!(
    map_key_event(controlled(KeyCode::Char('u')), InputMode::Normal, false),
    Some(AppAction::Review(ReviewAction::Scroll {
        delta: -1,
        unit: ScrollUnit::HalfPage,
    }))
);
assert_eq!(
    map_key_event(key(KeyCode::Enter), InputMode::Note, false),
    Some(AppAction::Confirm)
);
assert_eq!(
    map_key_event(shifted(KeyCode::Enter), InputMode::Note, false),
    Some(AppAction::Insert('\n'))
);
assert_eq!(
    map_key_event(controlled(KeyCode::Char('s')), InputMode::Note, false),
    Some(AppAction::Confirm)
);
assert_eq!(
    map_key_event(controlled(KeyCode::Char('t')), InputMode::Note, false),
    Some(AppAction::SendNote {
        reset_target: false,
    })
);
assert_eq!(
    map_key_event(
        KeyEvent::new(
            KeyCode::Char('t'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ),
        InputMode::Note,
        false,
    ),
    Some(AppAction::SendNote { reset_target: true })
);
```

Replace the two assertions that currently expect controlled `d` and `u` to
return `None`.

Update `tests/ui_dialogs.rs::help_lists_real_direct_bindings_and_contains_no_menu_instructions`
to require `"d / u / ^D / ^U"`, `"Enter save"`, `"Shift+Enter newline"`,
`"Ctrl-S save"`, and `"Ctrl-T send"`.

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test --test ui_input --test ui_dialogs
```

Expected: FAIL because controlled `d`/`u` return `None`, note Enter returns
`Insert('\n')`, note `Ctrl-T` returns `None`, and the help text lacks the new
bindings.

- [ ] **Step 3: Implement the minimal mappings**

Add the action in `src/ui/input.rs`:

```rust
pub enum AppAction {
    // existing variants
    SendSelection { reset_target: bool },
    SendNote { reset_target: bool },
    // existing variants
}
```

Handle controlled half-page keys before the general modifier rejection in
`map_normal`:

```rust
if event.modifiers.contains(KeyModifiers::CONTROL) {
    let action = match event.code {
        KeyCode::Char('d') => Some(ReviewAction::Scroll {
            delta: 1,
            unit: ScrollUnit::HalfPage,
        }),
        KeyCode::Char('u') => Some(ReviewAction::Scroll {
            delta: -1,
            unit: ScrollUnit::HalfPage,
        }),
        _ => None,
    };
    if let Some(action) = action {
        return Some(AppAction::Review(action));
    }
}
```

Make note-mode ownership explicit at the start of `map_text`:

```rust
if mode == InputMode::Note {
    if event.code == KeyCode::Char('t')
        && event.modifiers.contains(KeyModifiers::CONTROL)
    {
        return Some(AppAction::SendNote {
            reset_target: event.modifiers.contains(KeyModifiers::SHIFT),
        });
    }
    if event.code == KeyCode::Char('s')
        && event.modifiers.contains(KeyModifiers::CONTROL)
    {
        return Some(AppAction::Confirm);
    }
    if event.code == KeyCode::Enter {
        return Some(if event.modifiers.contains(KeyModifiers::SHIFT) {
            AppAction::Insert('\n')
        } else {
            AppAction::Confirm
        });
    }
}
```

Keep the existing literal-character guard so other controlled keys are not
inserted into notes.

Update `help_text` and the note overlay body in `src/ui/dialogs.rs`:

```rust
"d / u / ^D / ^U half page down / up"
```

```rust
format!(
    "{text}\n\nEnter save   Shift+Enter newline\nCtrl-S save   Ctrl-T send   Esc cancel"
)
```

- [ ] **Step 4: Run the focused tests and verify GREEN**

Run:

```bash
cargo test --test ui_input --test ui_dialogs
```

Expected: all keymap and dialog tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/input.rs src/ui/dialogs.rs tests/ui_input.rs tests/ui_dialogs.rs
git commit -m "fix: restore pdiff navigation and note keys"
```

---

### Task 2: Preserve visual ranges as native note targets

**Files:**
- Modify: `src/review/state.rs`
- Modify: `src/app.rs`
- Test: `tests/notes_state.rs`
- Test: `tests/pty_notes.rs`

**Interfaces:**
- Consumes: `Option<(SelectionPoint, SelectionPoint)>` owned by `App`.
- Produces: `ReviewController::begin_human_note(selection, viewport)` whose draft target contains complete old/new ranges from one file.

- [ ] **Step 1: Write a failing controller range test**

In `tests/notes_state.rs`, create a stack-layout file with five consecutive
addition rows, initialize geometry, capture the first selected line range, move
two rows, and begin a note:

```rust
#[test]
fn visual_selection_becomes_the_complete_note_target_in_both_directions() {
    let viewport = Viewport::new(100, 20);
    let mut controller = ReviewController::new(
        vec![added_file("src/range.rs", 5)],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    controller.snapshot(viewport);
    let (anchor, _) = controller.selected_line_range(viewport).unwrap();
    controller.apply(ReviewAction::MoveCursor(2), viewport);
    let (_, focus) = controller.selected_line_range(viewport).unwrap();

    controller
        .begin_human_note(Some((anchor, focus)), viewport)
        .unwrap();
    let target = &controller.human_note_draft().unwrap().target;
    assert_eq!(target.old_range, None);
    assert_eq!(target.new_range, Some(LineRange { start: 1, end: 3 }));

    controller.cancel_human_note_draft(viewport);
    controller
        .begin_human_note(Some((focus, anchor)), viewport)
        .unwrap();
    assert_eq!(
        controller.human_note_draft().unwrap().target.new_range,
        Some(LineRange { start: 1, end: 3 })
    );
}
```

Keep the existing single-row draft test by changing its call to
`begin_human_note(None, viewport)` and retaining its current assertions.

- [ ] **Step 2: Run the controller test and verify RED**

Run:

```bash
cargo test --test notes_state visual_selection_becomes_the_complete_note_target_in_both_directions -- --exact
```

Expected: FAIL to compile because `begin_human_note` does not accept a
selection, proving the missing range seam.

- [ ] **Step 3: Implement selection-to-target conversion**

Change the public method in `src/review/state.rs`:

```rust
pub fn begin_human_note(
    &mut self,
    selection: Option<(SelectionPoint, SelectionPoint)>,
    viewport: Viewport,
) -> Option<String> {
    self.ensure_geometry(viewport);
    let target = self.note_target_for_selection(selection)?;
    let id = format!("draft:{}", self.next_human_note_id);
    self.next_human_note_id = self.next_human_note_id.saturating_add(1);
    self.human_note_draft = Some(HumanNoteDraft {
        id: id.clone(),
        target,
        body: String::new(),
        editing: None,
    });
    self.dirty = true;
    self.rebuild(viewport, true);
    Some(id)
}
```

Add the private conversion beside it:

```rust
fn note_target_for_selection(
    &self,
    selection: Option<(SelectionPoint, SelectionPoint)>,
) -> Option<NoteTarget> {
    let geometry = self.geometry.as_ref()?;
    let (start, end) = selection.map_or_else(
        || {
            let index = geometry
                .rows
                .iter()
                .position(|row| Some(&row.key) == self.selected_row_key.as_ref())?;
            Some((index, index))
        },
        |(anchor, focus)| Some((
            anchor.row.min(focus.row),
            anchor.row.max(focus.row),
        )),
    )?;
    let anchor = geometry.rows.get(start)?;
    let file = self.files.iter().find(|file| file.id == anchor.key.file_id)?;
    let rows = geometry.rows.get(start..=end.min(geometry.rows.len() - 1))?;
    let matching = rows
        .iter()
        .filter(|row| row.key.file_id == anchor.key.file_id)
        .collect::<Vec<_>>();
    let old_range = line_range(matching.iter().filter_map(|row| row.key.old_line));
    let new_range = line_range(matching.iter().filter_map(|row| row.key.new_line));
    Some(resolve_ranges_target(file, old_range, new_range))
}
```

Use this helper:

```rust
fn line_range(lines: impl Iterator<Item = u32>) -> Option<LineRange> {
    let lines = lines.collect::<Vec<_>>();
    Some(LineRange {
        start: *lines.iter().min()?,
        end: *lines.iter().max()?,
    })
}
```

Call it for `old_range` and `new_range`. Do not include rows from another file.

In `App::apply_review_effect`, pass the visual selection:

```rust
ReviewEffect::StartNote => {
    if self
        .review_controller
        .begin_human_note(self.review_selection, viewport)
        .is_some()
    {
        self.comment_buf.clear();
        self.input_mode = InputMode::Note;
    }
}
```

Add one helper in `App`:

```rust
fn clear_review_selection(&mut self) {
    self.review_keyboard_anchor = None;
    self.review_selection = None;
}
```

Call it after native note save and native note cancel.

- [ ] **Step 4: Verify the controller behavior is GREEN**

Run:

```bash
cargo test --test notes_state
```

Expected: all note-state tests PASS, including forward, reverse, and single-row
targets.

- [ ] **Step 5: Write a failing PTY lifecycle test**

In `tests/pty_notes.rs`, add a helper that writes a five-line additions patch:

```rust
fn range_patch(path: &Path) {
    std::fs::write(
        path,
        concat!(
            "diff --git a/src/range.rs b/src/range.rs\n",
            "new file mode 100644\n",
            "--- /dev/null\n",
            "+++ b/src/range.rs\n",
            "@@ -0,0 +1,5 @@\n",
            "+one\n+two\n+three\n+four\n+five\n",
        ),
    )
    .unwrap();
}
```

Add:

```rust
#[test]
fn visual_note_keeps_the_range_then_returns_to_normal_navigation() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let patch = temp.path().join("range.patch");
    range_patch(&patch);
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", patch.to_str().unwrap(), "--mode", "stack"],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );

    process.read_until("one");
    process.send("Vjjc");
    process.read_until("R1–R3");
    process.send("range note\r");
    process.read_until("Your note");
    process.send("jc");
    let next = process.read_until("Draft note");
    assert!(next.contains("R4"), "{next}");
    assert!(!next.contains("R1–R4"), "{next}");
    process.send("\x1bq");
    assert_eq!(process.wait(), 0);
}
```

- [ ] **Step 6: Run the PTY test and verify RED, then GREEN**

Run before the App/controller fix:

```bash
cargo test --test pty_notes visual_note_keeps_the_range_then_returns_to_normal_navigation -- --exact
```

Expected RED: timeout waiting for `R1–R3` because the draft shows only `R3`.

Run after the fix:

```bash
cargo test --test pty_notes visual_note_keeps_the_range_then_returns_to_normal_navigation -- --exact
```

Expected GREEN: the first draft targets `R1–R3`, Enter saves it, and the next
draft targets only `R4`.

- [ ] **Step 7: Commit**

```bash
git add src/review/state.rs src/app.rs tests/notes_state.rs tests/pty_notes.rs
git commit -m "fix: preserve visual note ranges"
```

---

### Task 3: Render the tmux target picker in the current UI

**Files:**
- Modify: `src/ui/dialogs.rs`
- Modify: `src/app.rs`
- Test: `tests/ui_dialogs.rs`
- Create: `tests/pty_tmux.rs`

**Interfaces:**
- Consumes: `&[TmuxPane]` and the selected index already owned by `App`.
- Produces: `DialogOverlay::tmux(theme, panes, selected)` and visible picker text for Task 4's live send.

- [ ] **Step 1: Write a failing overlay test**

In `tests/ui_dialogs.rs`, import `ramo::tmux::TmuxPane` and render:

```rust
let panes = vec![
    TmuxPane {
        id: "%2".into(),
        label: "%2  work:1.2  agent  [claude]".into(),
        current_command: "claude".into(),
    },
    TmuxPane {
        id: "%3".into(),
        label: "%3  work:1.3  shell  [zsh]".into(),
        current_command: "zsh".into(),
    },
];
terminal
    .draw(|frame| {
        frame.render_widget(
            DialogOverlay::tmux(&theme, &panes, 1),
            frame.area(),
        );
    })
    .unwrap();
let frame = buffer_text(&terminal);
assert!(frame.contains("Send to tmux"), "{frame}");
assert!(frame.contains("[claude]"), "{frame}");
assert!(frame.contains("[zsh]"), "{frame}");
assert!(frame.contains("Enter send"), "{frame}");
```

- [ ] **Step 2: Run the overlay test and verify RED**

Run:

```bash
cargo test --test ui_dialogs overlays_render_centered_and_remain_usable_at_small_sizes -- --exact
```

Expected: FAIL to compile because `DialogOverlay::tmux` does not exist.

- [ ] **Step 3: Implement the native overlay**

Add a `Tmux` variant and constructor to `src/ui/dialogs.rs`:

```rust
Tmux {
    theme: &'a AppTheme,
    panes: &'a [crate::tmux::TmuxPane],
    selected: usize,
},
```

```rust
pub fn tmux(
    theme: &'a AppTheme,
    panes: &'a [crate::tmux::TmuxPane],
    selected: usize,
) -> Self {
    Self::Tmux {
        theme,
        panes,
        selected,
    }
}
```

Render it through `render_lines`:

```rust
Self::Tmux {
    theme,
    panes,
    selected,
} => {
    let height = (panes.len() as u16).saturating_add(5).min(22);
    let mut lines = panes
        .iter()
        .enumerate()
        .map(|(index, pane)| {
            let marker = if index == selected { "› " } else { "  " };
            Line::from(vec![
                Span::styled(marker, Style::default().fg(theme.accent)),
                Span::styled(pane.label.clone(), Style::default().fg(theme.text)),
            ])
        })
        .collect::<Vec<_>>();
    lines.push(Line::default());
    lines.push(Line::from(
        "j/k move   g/G first/last   Enter send   Esc cancel".to_owned(),
    ));
    render_lines(
        centered_rect(82, height, area),
        buffer,
        theme,
        "Send to tmux",
        lines,
    );
}
```

At the end of `App::draw`, after the ordinary input-mode overlay match, add:

```rust
if matches!(self.mode, Mode::TmuxPanePick) {
    frame.render_widget(
        DialogOverlay::tmux(
            &self.review_theme,
            &self.tmux_panes,
            self.tmux_cursor,
        ),
        area,
    );
}
```

- [ ] **Step 4: Run the dialog test and verify GREEN**

Run:

```bash
cargo test --test ui_dialogs
```

Expected: all dialog tests PASS, including the tmux overlay at a small terminal
size.

- [ ] **Step 5: Write a failing real-tmux visibility test**

Create `tests/pty_tmux.rs` with `#![cfg(unix)]`. Use a unique socket name based
on `std::process::id()`, start one detached `ramo patch` pane and one `cat` pane,
and always kill the isolated server in a guard's `Drop`.

The core assertion must be:

```rust
send_keys(&socket, &ramo_pane, &["C-t"]);
let screen = capture_until(&socket, &ramo_pane, "Send to tmux");
assert!(screen.contains("[cat]"), "{screen}");
assert!(screen.contains("Enter send"), "{screen}");
```

Use these exact tmux argv shapes:

```rust
["-L", socket, "new-session", "-d", "-s", session, "-x", "120", "-y", "30", command]
["-L", socket, "split-window", "-d", "-t", session, "cat"]
["-L", socket, "list-panes", "-t", session, "-F", "#{pane_id}\t#{pane_current_command}"]
["-L", socket, "send-keys", "-t", pane, "C-t"]
["-L", socket, "capture-pane", "-p", "-t", pane]
```

Poll capture output every 20 ms for at most five seconds; do not use a fixed
multi-second sleep. Skip only when `tmux -V` cannot execute.

- [ ] **Step 6: Run the real-tmux test RED, then GREEN**

Run before rendering the overlay:

```bash
cargo test --test pty_tmux tmux_picker_is_visible_in_the_current_review -- --exact
```

Expected RED: timeout waiting for `Send to tmux`.

Run after rendering:

```bash
cargo test --test pty_tmux tmux_picker_is_visible_in_the_current_review -- --exact
```

Expected GREEN: the captured current review contains the picker title, target,
and controls.

- [ ] **Step 7: Commit**

```bash
git add src/ui/dialogs.rs src/app.rs tests/ui_dialogs.rs tests/pty_tmux.rs
git commit -m "fix: render native tmux picker"
```

---

### Task 4: Restore note-to-tmux send-and-save

**Files:**
- Modify: `src/review/state.rs`
- Modify: `src/app.rs`
- Modify: `tests/notes_state.rs`
- Modify: `tests/pty_tmux.rs`

**Interfaces:**
- Consumes: `AppAction::SendNote`, `ReviewController::human_note_draft_annotation()`, and the visible picker from Task 3.
- Produces: `TmuxSendCompletion::{Review, SaveLegacyAnnotation, SaveHumanNote { viewport }}` so delivery success controls note persistence.

- [ ] **Step 1: Write a failing bounded draft-context test**

In `tests/notes_state.rs`, after creating and updating a range draft:

```rust
controller.update_human_note_draft("Explain this range", viewport);
let annotation = controller.human_note_draft_annotation().unwrap();
assert_eq!(annotation.file, "src/range.rs");
assert_eq!(annotation.display_range, "R1–R3");
assert_eq!(annotation.comment, "Explain this range");
assert!(annotation.diff_context.contains("+one"));
assert!(annotation.diff_context.contains("+three"));
```

- [ ] **Step 2: Run the context test and verify RED**

Run:

```bash
cargo test --test notes_state visual_selection_becomes_the_complete_note_target_in_both_directions -- --exact
```

Expected: FAIL to compile because `human_note_draft_annotation` does not exist.

- [ ] **Step 3: Implement bounded draft context**

Add beside `export_annotations` in `src/review/state.rs`:

```rust
pub fn human_note_draft_annotation(&self) -> Option<Annotation> {
    let draft = self.human_note_draft.as_ref()?;
    let file = self
        .files
        .iter()
        .find(|file| file.id == draft.target.file_id)?;
    Some(Annotation {
        file: file.path.clone(),
        flat_start: 0,
        flat_end: 0,
        display_range: target_display_range(&draft.target),
        diff_context: target_diff_context(file, &draft.target),
        comment: draft.body.clone(),
    })
}
```

This reuses the existing 40-line/16-KiB context limits.

- [ ] **Step 4: Run the context test and verify GREEN**

Run:

```bash
cargo test --test notes_state
```

Expected: all note-state tests PASS.

- [ ] **Step 5: Write a failing real-tmux note test**

Extend `tests/pty_tmux.rs`:

```rust
#[test]
fn ctrl_t_sends_native_note_context_and_saves_after_delivery() {
    // Start isolated ramo and cat panes with a five-line additions patch.
    send_literal(&socket, &ramo_pane, "VjjcExplain this range");
    send_keys(&socket, &ramo_pane, &["C-t"]);
    capture_until(&socket, &ramo_pane, "Send to tmux");
    send_keys(&socket, &ramo_pane, &["Enter"]);

    let target = capture_until(&socket, &cat_pane, "Explain this range");
    assert!(target.contains("src/range.rs"), "{target}");
    assert!(target.contains("one"), "{target}");
    assert!(target.contains("three"), "{target}");

    send_literal(&socket, &ramo_pane, "q");
    wait_for_exit(&socket, &ramo_pane);
    let markdown = std::fs::read_to_string(output).unwrap();
    assert!(markdown.contains("Explain this range"), "{markdown}");
    assert!(markdown.contains("R1–R3"), "{markdown}");
}
```

Also assert picker cancellation keeps the draft:

```rust
send_keys(&socket, &ramo_pane, &["C-t", "Escape"]);
let screen = capture_until(&socket, &ramo_pane, "Explain this range");
assert!(screen.contains("Draft note"), "{screen}");
```

- [ ] **Step 6: Run the live note test and verify RED**

Run:

```bash
cargo test --test pty_tmux ctrl_t_sends_native_note_context_and_saves_after_delivery -- --exact
```

Expected RED: timeout waiting for the picker because current note-mode
`Ctrl-T` is discarded.

- [ ] **Step 7: Implement explicit tmux completion state**

In `src/app.rs`, replace the boolean field with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TmuxSendCompletion {
    Review,
    SaveLegacyAnnotation,
    SaveHumanNote { viewport: Viewport },
}
```

Initialize:

```rust
tmux_completion: TmuxSendCompletion::Review,
```

Change `request_tmux_send` to accept a completion value:

```rust
fn request_tmux_send(&mut self, text: String, completion: TmuxSendCompletion) {
    if text.trim().is_empty() {
        self.toast = Some("nothing to send".to_owned());
        return;
    }
    if !crate::tmux::in_tmux() {
        self.toast = Some("not in tmux".to_owned());
        return;
    }
    self.tmux_pending_text = text;
    self.tmux_completion = completion;
    if let Some((target, mode)) = self.tmux_last_target.clone()
        && crate::tmux::pane_exists(&target)
    {
        self.dispatch_tmux_send(&target, mode);
        return;
    }
    self.tmux_last_target = None;
    self.open_tmux_picker();
}
```

When the completion is `SaveHumanNote`, `open_tmux_picker` sets
`self.input_mode = InputMode::Normal` so `Mode::TmuxPanePick` owns `j/k`,
Enter, and Escape.

Handle the new action in `apply_app_action`:

```rust
AppAction::SendNote { reset_target } => {
    if reset_target {
        self.tmux_last_target = None;
    }
    self.review_controller
        .update_human_note_draft(&self.comment_buf, viewport);
    if let Some(annotation) = self.review_controller.human_note_draft_annotation() {
        self.request_tmux_send(
            format_annotation_for_tmux(&annotation),
            TmuxSendCompletion::SaveHumanNote { viewport },
        );
    }
}
```

Add:

```rust
fn format_annotation_for_tmux(annotation: &Annotation) -> String {
    let language = std::path::Path::new(&annotation.file)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("diff");
    format!(
        "`{} {}`:\n\n```{}\n{}\n```\n\n{}",
        annotation.file,
        annotation.display_range,
        language,
        annotation.diff_context,
        annotation.comment,
    )
}
```

In `dispatch_tmux_send`, take the completion and branch only after successful
`send_to_pane`:

```rust
match completion {
    TmuxSendCompletion::Review => {}
    TmuxSendCompletion::SaveLegacyAnnotation => self.submit_comment(),
    TmuxSendCompletion::SaveHumanNote { viewport } => {
        self.review_controller.save_human_note_draft(viewport);
        self.comment_buf.clear();
        self.input_mode = InputMode::Normal;
        self.clear_review_selection();
    }
}
```

On failed delivery, restore `InputMode::Note` for `SaveHumanNote` and retain the
draft. On picker cancellation, do the same. Reset completion to `Review` after
success, failure, or cancellation. Convert existing legacy boolean call sites
to `Review` or `SaveLegacyAnnotation` without changing their behavior.

- [ ] **Step 8: Run the tmux and note suites and verify GREEN**

Run:

```bash
cargo test --test pty_tmux --test pty_notes --test integrations --test notes_state
```

Expected: picker, cancellation, exact payload, post-delivery save, range, and
tmux command-safety tests all PASS.

- [ ] **Step 9: Commit**

```bash
git add src/review/state.rs src/app.rs tests/notes_state.rs tests/pty_tmux.rs
git commit -m "fix: restore note sending to tmux"
```

---

### Task 5: Make Tokyo Night the automatic dark default and document behavior

**Files:**
- Modify: `src/ui/themes.rs`
- Modify: `src/ui/theme.rs`
- Modify: `tests/themes.rs`
- Modify: `tests/config_resolution.rs`
- Modify: `README.md`
- Modify: `docs/parity/hunk.md`

**Interfaces:**
- Consumes: `DEFAULT_DARK_THEME_ID` and `ThemeRegistry::resolve`.
- Produces: automatic dark/unknown appearance resolving to `tokyo-night`; explicit IDs remain unchanged.

- [ ] **Step 1: Write failing theme tests**

Update `tests/themes.rs::fallback_auto_and_transparent_surfaces_are_predictable`:

```rust
assert_eq!(registry.resolve("missing", None, false).id, "tokyo-night");
assert_eq!(
    registry
        .resolve("auto", Some(TerminalAppearance::Dark), false)
        .id,
    "tokyo-night"
);
assert_eq!(
    registry.resolve("github-dark-default", None, false).id,
    "github-dark-default"
);
let recife_like = registry.resolve("tokyo-night", None, false);
assert_eq!(recife_like.background, Color::Rgb(0x1a, 0x1b, 0x26));
assert_eq!(recife_like.text, Color::Rgb(0xa9, 0xb1, 0xd6));
```

Keep automatic light asserting `github-light-default`.

In `tests/config_resolution.rs::missing_files_are_ignored`, keep the resolved
configuration value `"auto"`; this proves only rendering resolution changes,
not config layering.

- [ ] **Step 2: Run theme tests and verify RED**

Run:

```bash
cargo test --test themes --test config_resolution
```

Expected: theme tests FAIL because automatic dark still resolves to
`github-dark-default`; configuration tests remain green.

- [ ] **Step 3: Change only the automatic dark constant**

In `src/ui/themes.rs`:

```rust
pub const DEFAULT_DARK_THEME_ID: &str = "tokyo-night";
```

Keep the legacy alias stable:

```rust
"graphite" => "github-dark-default",
```

In `src/ui/theme.rs`, remove the hard-coded dark ID:

```rust
let theme = ThemeRegistry::default().resolve(
    crate::ui::themes::DEFAULT_DARK_THEME_ID,
    None,
    false,
);
```

- [ ] **Step 4: Run theme tests and verify GREEN**

Run:

```bash
cargo test --test themes --test config_resolution
```

Expected: all tests PASS; automatic dark is Tokyo Night, automatic light and
explicit GitHub Dark are unchanged.

- [ ] **Step 5: Update user and parity documentation**

In `README.md`:

- List `d/u` and `Ctrl-D/U` together.
- State Enter saves a note, Shift-Enter inserts a newline, Ctrl-S also saves,
  and Ctrl-T sends to the visible tmux picker.
- Explain Ctrl-T remembers a target and Ctrl-Shift-T reselects.
- Replace the sentence saying auto falls back to GitHub Dark with Tokyo Night.

In `docs/parity/hunk.md`:

- Replace the row that says controlled `d/u` are intentionally absent.
- Add test evidence for visible tmux picker, native note send/save, visual note
  ranges, and selection teardown.
- Keep Hunk parity claims distinct from deliberate pdiff-compatible aliases.

- [ ] **Step 6: Run documentation and focused contract checks**

Run:

```bash
rg -n "Ctrl-D|Ctrl-U|Ctrl-T|Shift-Enter|Tokyo Night" README.md docs/parity/hunk.md
cargo test --test ui_dialogs --test themes --test pty_notes --test pty_tmux
```

Expected: documentation names every restored control and all focused suites
PASS.

- [ ] **Step 7: Commit**

```bash
git add src/ui/themes.rs src/ui/theme.rs tests/themes.rs tests/config_resolution.rs README.md docs/parity/hunk.md
git commit -m "feat: default to tokyo night theme"
```

---

### Task 6: Full regression and release-quality verification

**Files:**
- Verify only; modify a task-owned file only if a failing gate reveals a defect in this plan's scope.

**Interfaces:**
- Consumes: all prior task commits.
- Produces: a clean, warning-free branch with deterministic live evidence.

- [ ] **Step 1: Run formatting and strict lint gates**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: both exit successfully with no warnings.

- [ ] **Step 2: Run the complete test matrix**

Run:

```bash
cargo test --all-targets --all-features
```

Expected: all unit, integration, PTY, real-tmux, and benchmark targets PASS.

- [ ] **Step 3: Build and identify the release binary**

Run:

```bash
cargo build --release --locked
target/release/ramo --version
```

Expected: release build exits successfully and prints the current Ramo version.

- [ ] **Step 4: Replay the original user workflows**

Inside an isolated tmux server:

1. Open a five-line additions patch.
2. Verify `Ctrl-D/U` and plain `d/u` move the same half-page distance.
3. Press `Vjjc` and verify the draft header shows `R1–R3`.
4. Type a note, press `Ctrl-T`, visibly choose the Claude/cat pane, and verify
   the target receives file, range, code, and comment.
5. Verify the note is saved only after delivery.
6. Start a second note after moving once and verify it targets one line.
7. Verify Enter saves and Shift-Enter inserts a newline.

Expected: every workflow matches the approved spec and no invisible mode
remains.

- [ ] **Step 5: Verify repository hygiene**

Run:

```bash
git diff --check
git status --short --branch
git log --oneline -8
```

Expected: no unstaged or untracked files; task commits are focused and ordered.
