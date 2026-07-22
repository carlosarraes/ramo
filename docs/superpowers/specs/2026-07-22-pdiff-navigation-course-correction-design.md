# Pdiff Navigation Course Correction Design

## Purpose

Ramo keeps its Rust-native loaders, watch mode, context expansion, notes, sessions, themes, and rendering geometry, but stops treating Hunk's viewport-scrolling interaction as the product baseline. The review experience returns to pdiff's explicit, visible, semantic cursor model.

The correction also removes the startup terminal probe that can place terminal response bytes on the same input stream as user keys and makes accidental filter entry recoverable with one Escape press.

## Goals

- Never send an active terminal query during normal review startup.
- Make one Escape press clear the filter and return to normal input mode.
- Give every review a visible semantic cursor anchored to a real diff row.
- Make `j`, `k`, Up, Down, `g`, `G`, `[`, and `]` move that cursor predictably.
- Keep horizontal scrolling, responsive split/stack rendering, sidebar navigation, watch reloads, notes, context expansion, selection, and session control.
- Remove the competing legacy navigation path from active input dispatch.
- Verify rendered movement in a real PTY, not only internal controller state.

## Non-goals

- Reverting Ramo to the pre-parity codebase or renderer.
- Removing Hunk-derived capabilities such as stack layout, context expansion, themes, notes, or sessions.
- Reproducing the old visual design pixel-for-pixel.
- Adding a configurable navigation-mode switch.
- Preserving every Hunk key when it conflicts with pdiff's navigation model.

## Considered Approaches

### 1. Add a semantic cursor to the current review controller (selected)

The current row plan and geometry remain authoritative. `ReviewController` owns a stable row key for the cursor, navigation moves between selectable rows, and rendering highlights that row. This retains the new backend and interaction features while restoring pdiff's strongest behavior.

### 2. Restore the legacy pdiff renderer and app state

This would recover the old feel quickly, but would fork geometry, selection, notes, context expansion, watch anchoring, and session projection into two implementations. It would preserve the architectural conflict that is already causing confusing behavior.

### 3. Fix only terminal input and filter cancellation

This would address the most obvious startup failure but leave invisible center-derived selection and detached viewport scrolling unchanged. It does not address the reported navigation regression.

## Terminal Startup

`theme = "auto"` must not send OSC 11 or any other active query. Auto resolution uses `COLORFGBG` when present and otherwise chooses `github-dark-default`. Explicit and custom themes continue to bypass auto resolution.

Removing the query is preferable to trying to drain late responses: a response can be fragmented, delayed by tmux, or delivered after terminal ownership changes. Ramo must not generate input that can later be interpreted as commands.

## Filter Behavior

`/` enters filter mode. Typing updates the file filter as today. One Escape press performs one atomic cancellation:

1. clear the filter buffer;
2. apply an empty filter to the review controller;
3. return to normal input mode.

Tab may still toggle between filter and normal mode without clearing the current filter. This preserves intentional focused filtering while making Escape an unconditional escape hatch.

## Semantic Cursor

`ReviewController` owns one optional cursor identified by `ReviewRowKey`. A selectable row is a rendered stack or split diff row with at least one side containing source content. Hunk headers, file separators, placeholders, collapsed-context controls, and note cards are not cursor stops.

On initial render, the cursor selects the first selectable row. If a review has no selectable rows, the cursor is absent and navigation is a no-op.

Navigation behaves as follows:

- `j`/Down moves to the next selectable row and clamps at the end.
- `k`/Up moves to the previous selectable row and clamps at the beginning.
- `g`/Home moves to the first selectable row.
- `G`/End moves to the last selectable row.
- `[` and `]` retain wrapping hunk navigation and place the cursor on the target hunk's first selectable row.
- `,` and `.` retain wrapping file navigation and place the cursor on the target file's first selectable row.
- page and half-page commands move the viewport by their current amount, then place the cursor on the selectable row nearest the viewport center.
- horizontal arrows continue to scroll long lines and do not change the semantic cursor.

Every cursor move calls a single `ensure_cursor_visible` operation. Ordinary row movement minimally scrolls only when the cursor leaves the viewport. Hunk/file jumps and page movement may center the cursor when space permits. `G` must render the last selectable row in the same event cycle.

The cursor key survives geometry rebuilds, resize, layout changes, context expansion, and watch reload when the same semantic row still exists. Fallback order is the nearest line in the same hunk, the first selectable row in the same file, then the first selectable row in the review.

## Side Focus and Key Map

Split layout has an explicit focused side. The cursor highlight is painted on the focused side for context rows and on the only populated side for additions or deletions. Moving between rows snaps focus when the current side is empty.

- `h` focuses the left side.
- `l` focuses the right side.
- `n` toggles line numbers, replacing Hunk's conflicting `l` binding.
- Left/Right arrows remain horizontal scrolling controls.

Stack layout has no meaningful side distinction; `h` and `l` leave the cursor on the same row and only update the side remembered for a later split layout.

Help text and README controls must document the corrected bindings.

## Rendering

The cursor row receives a distinct, theme-derived background across its active code cell, including gutter and content. It must remain distinguishable from addition/deletion backgrounds and from explicit text selection. Explicit mouse/keyboard selection takes visual precedence over the cursor highlight.

The right scrollbar remains a viewport indicator. It is not treated as the primary navigation feedback because integer thumb positions necessarily remain unchanged across some single-row movements in large diffs.

The first file header is rendered even when the sidebar is hidden so a tight or medium viewport never starts with anonymous code. Responsive `auto` behavior remains: tight terminals stack, wider terminals split, and the sidebar appears automatically only at the full-width threshold.

## State Ownership Cleanup

Review navigation is dispatched exclusively through `ReviewController`. The active input path must not fall back to legacy `App::cursor`, `scroll_offset`, `focus_side`, or `handle_nav_key` for review movement. Legacy state still required by annotation export or tmux integration may remain temporarily, but it must not receive navigation keys or influence the rendered review.

This establishes one authoritative chain:

`terminal event -> AppAction -> ReviewAction -> ReviewController -> ReviewSnapshot -> ReviewWidget`

## Error and Reload Behavior

Navigation on an empty review is a no-op. Watch reload failures retain both the last valid review and its cursor. Successful reloads restore the semantic cursor using the fallback rules above and keep it visible. No reload may re-enter filter mode or synthesize terminal input.

## Verification

Testing is test-first and covers behavior at four boundaries:

1. Unit input tests verify the corrected bindings and one-Escape filter cancellation.
2. Controller tests verify row-by-row movement, bounds, hunk/file jumps, page re-anchoring, side snapping, and cursor restoration across rebuild/reload.
3. Render tests verify cursor styling, selection precedence, first-file headers, and split-side focus.
4. PTY tests use uniquely identifiable first/middle/last lines and assert that `j`, `k`, `G`, `g`, `[`, and `]` change the rendered screen. Startup with `theme = "auto"` asserts that no OSC 11 query is emitted and that the review never enters filter mode without `/`.

The focused suites are followed by formatting, strict Clippy, all targets/features, and a release build.

## Success Criteria

- Starting Ramo without input never opens or populates the filter.
- One Escape always returns an accidental filter to a visible, unfiltered review.
- The current review row is always visually identifiable when selectable content exists.
- `G` visibly reaches the last selectable row; `g` visibly returns to the first.
- `j`/`k` move by semantic rows rather than moving an invisible center-derived selection.
- Hunk/file navigation, watch reload, context expansion, notes, selection, and session projection continue to work.
- There is one active navigation state machine, backed by PTY assertions of rendered behavior.
