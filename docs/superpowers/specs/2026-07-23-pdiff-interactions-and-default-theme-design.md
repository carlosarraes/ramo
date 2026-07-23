# Pdiff Interactions and Default Theme Design

## Context

Ramo's Hunk-parity rewrite retained the tmux transport and review actions, but
several old pdiff interactions no longer work through the current native review
UI:

- `Ctrl-D` and `Ctrl-U` are explicitly discarded even though old pdiff used
  them for half-page scrolling.
- `Ctrl-T` still reaches the tmux transport from normal review mode, but its
  first-use pane picker is not rendered.
- `Ctrl-T` is discarded by the current note editor, so a drafted comment cannot
  be sent to Claude or another tmux target.
- `Enter` inserts a newline in the note editor, while old pdiff used Enter to
  submit and Shift-Enter to insert a newline.
- Starting a note from a visual selection collapses its target to the cursor's
  final line, and saving the note leaves the visual-selection anchor active.

The automatic dark theme also still resolves to GitHub Dark. The existing
`tokyo-night` theme already uses Recife's `#1a1b26` background and `#a9b1d6`
foreground, matching the palette published by
[`tmux-recife`](https://github.com/carlosarraes/tmux-recife).

## Goals

- Restore old pdiff control-key ergonomics without removing the newer plain-key
  aliases.
- Make tmux target selection visible and usable in the current native renderer.
- Restore send-and-save from the current native note editor.
- Restore old pdiff note completion while retaining `Ctrl-S` as an additional
  save binding.
- Preserve visual ranges as note targets and return to normal navigation after
  the note is saved or cancelled.
- Make Tokyo Night the automatic dark default without changing explicit theme
  selections or the automatic light default.

## Non-goals

- No Claude-specific process integration, SDK, or network API.
- No TypeScript, Node.js, browser, fuzzy-finder, or new runtime dependency.
- No tmux dependency for ordinary review, copy, notes, or export.
- No new Recife theme. The existing Tokyo Night theme is the default dark
  palette.
- No changes to pager-mode restrictions beyond the restored scrolling aliases.

## Interaction Design

### Navigation

Both old and new half-page bindings remain available:

- `d` and `Ctrl-D`: half-page down.
- `u` and `Ctrl-U`: half-page up.

The aliases resolve to the same `ReviewAction::Scroll` values, so cursor,
viewport, and boundary behavior remain identical.

### Tmux target picker

`Ctrl-T` sends the current line or visual selection. When no remembered target
exists, Ramo opens a centered native overlay listing all other tmux panes using
their existing labels. The overlay supports:

- `j`/Down and `k`/Up to move.
- `g` and `G` to jump to the first and last target.
- Enter to send.
- Escape to cancel without losing the selection or note draft.

After a successful send, Ramo remembers the pane for the remainder of the
process. `Ctrl-Shift-T` clears the remembered target and always opens the
picker. A stale target also falls back to the visible picker.

The title and help text stay generic ("Send to tmux") because the transport
supports Claude, Pi, shells, and other tmux panes. Existing paste-mode detection
continues to use bracketed paste for Claude and other compatible targets and
plain paste for Pi.

If Ramo is not running inside tmux, it shows the existing `not in tmux` message.
List, load-buffer, and paste-buffer failures remain visible and do not mutate
the review or note.

### Notes

The current native note editor uses these controls:

- Enter: save the note and return to review.
- Shift-Enter: insert a newline.
- `Ctrl-S`: save alias.
- `Ctrl-T`: send the note with its bounded file/line/code context.
- Escape: cancel the draft.

When `c` starts a note from a visual selection, the draft target contains the
full selected old/new line range rather than only the final cursor row. The
range is preserved independently of the painted selection. Saving or cancelling
the draft clears the visual anchor and painted range, returning the review to
normal navigation.

`Ctrl-T` follows the same remembered-target and picker behavior as selection
sending. Ramo commits the human note only after tmux delivery succeeds. A
cancelled picker or failed delivery leaves the draft open and unchanged so the
user can retry or save locally.

### Default theme

`theme = "auto"` resolves dark terminals and unknown terminal appearance to
the bundled `tokyo-night` theme. Light terminals continue to resolve to
`github-light-default`.

Explicit values such as `theme = "github-dark-default"`, repository overrides,
CLI overrides, and custom themes are unchanged. Transparent-background
behavior remains unchanged.

## Architecture

- `ui::input` owns modifier-aware mappings for navigation, note completion, and
  note sending.
- `ReviewController` converts an optional visual `SelectionPoint` pair into a
  stable `NoteTarget`, restricted to the selected file and available old/new
  line numbers.
- `App` keeps the existing tmux target state and pending payload, but represents
  whether a successful send must commit a native note draft explicitly. It
  clears visual-selection state when note editing finishes.
- `ui::dialogs` owns a centered tmux-picker overlay rendered by the current
  review UI. The legacy side-by-side renderer is not reused.
- `ui::themes` changes only the automatic dark default constant; Tokyo Night's
  bundled palette remains unchanged.

The tmux command boundary remains `TmuxClient`; the UI never evaluates pane
labels, selected text, or note text as shell input.

## Testing Seams

Tests exercise behavior through five existing public seams:

1. `map_key_event` verifies modifier-aware navigation and note controls.
2. `ReviewController` verifies that forward and reverse visual selections
   produce complete old/new note ranges and that a single-row note is unchanged.
3. The Ratatui test backend verifies that the current renderer displays the
   pane picker, selected target, and controls at normal and small terminal
   sizes.
4. PTY/tmux integration verifies that `Ctrl-T` visibly opens the picker, that
   selection and note payloads reach a real isolated tmux pane, that the draft
   displays the complete visual range, and that saving leaves no active visual
   selection.
5. `ThemeRegistry::resolve` verifies automatic dark/light selection and
   explicit-theme stability.

Each regression test must fail against `v0.0.9` behavior before production code
changes. Existing tmux command-safety, review-navigation, note, theme,
cross-platform, and strict-clippy gates remain required.

## Documentation

The README and native help overlay will list both half-page key forms, the
visible tmux picker controls, note completion/send controls, and Tokyo Night as
the automatic dark default.
