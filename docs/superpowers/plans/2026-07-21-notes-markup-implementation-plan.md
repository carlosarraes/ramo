# Native Notes and STML Parity Implementation Plan

> Status: approved architecture; execution plan for delivery slice 5.

**Goal:** Port Hunk's agent-context, inline human/AI/agent notes, deterministic STML, and markup commands into the single Rust `pdiff` executable without introducing a JavaScript runtime or a second review-state model.

**Reference:** `/home/carraes/github/hunk` at `53fcb2c`, especially `src/core/agent.ts`, `src/ui/lib/agentAnnotations.ts`, `src/ui/lib/agentNoteGeometry.ts`, `src/ui/lib/stml/*`, and `test/pty/notes.test.ts`.

**Architecture:** `notes` owns normalized external and human note data plus stable line targeting. `ReviewController` owns the authoritative live note set and inserts measured note rows into the same geometry used by rendering, navigation, hit testing, and session projections. `markup` is a pure bounded parser/layout engine whose symbolic styles are resolved by the Ratatui renderer. Loaders attach and reorder agent context before the controller is created; reloads re-read file-backed context and preserve human notes by stable target. The CLI calls the same markup library as inline notes.

**Constraints:** Rust only; no top menu/dropdown UI; no shell execution; malformed STML degrades with bounded diagnostics; terminal controls are sanitized; all ranges are positive, ordered, inclusive, and 1-based; external note payloads and markup are bounded.

---

## Task 1: Normalize and load agent context

**Files:**

- Create: `src/notes/mod.rs`
- Create: `src/notes/model.rs`
- Create: `src/notes/context.rs`
- Modify: `Cargo.toml`, `Cargo.lock`
- Modify: `src/lib.rs`
- Modify: `src/core/changeset.rs`
- Modify: `src/diff/model.rs`
- Modify: `src/input/mod.rs`
- Modify: `src/input/file_pair.rs`
- Modify: `src/input/patch.rs`
- Test: `tests/agent_context.rs`
- Test: existing loader suites

- [x] **Step 1: Write failing schema, ordering, matching, and reload tests**

Cover version/default summary, current/previous-path matching, context file order with unmatched files stable, optional metadata, string-only tags, invalid file objects, missing summaries, invalid ranges, malformed JSON, non-UTF-8 files, file-size and annotation-count limits, terminal-control sanitization, `--agent-context -` conflicts with patch stdin, and file-backed context reload.

- [x] **Step 2: Confirm the normalized contract is absent**

Run: `cargo test --test agent_context -- --nocapture`

Expected: compilation fails because `pdiff::notes` and attached file notes do not exist.

- [x] **Step 3: Add bounded serde models and contextual errors**

Add `serde_json`. Define `AgentContext`, `AgentFileContext`, `ReviewNote`, `NoteSource`, `NoteConfidence`, and `LineRange`. Deserialize through private raw structs, then validate and sanitize into the public model. Preserve `id`, `oldRange`, `newRange`, `summary`, `rationale`, `markup`, `tags`, `confidence`, `source`, `title`, `author`, `createdAt`, `updatedAt`, and `editable`. Reject empty paths/summaries and invalid ranges with the source path in the error.

Use explicit limits: 1 MiB sidecar, 2,000 files, 10,000 annotations total, 64 KiB markup per note, and 64 KiB combined summary/rationale text per note.

- [x] **Step 4: Attach context once at the normalized loader boundary**

Add `agent: Option<AgentFileContext>` to `DiffFile`; set it to `None` in every native loader/test constructor. Add `Changeset::apply_agent_context` to set `agent_summary`, attach by current path then previous path, and stable-sort matching files in sidecar order while retaining unmatched relative order.

Extend reload plans with a resolved `AgentContextSource` so file-backed sidecars are re-read on manual/watch reload. `-` is allowed only when the review itself does not consume stdin and produces a non-reloadable context snapshot. Emit a distinct ambiguity error for patch stdin plus agent-context stdin.

- [x] **Step 5: Run loader and reload regression suites**

Run: `cargo test --test agent_context --test input_loading --test reload --test git_loading --test jj_loading --test sl_loading`

Expected: all pass; existing inputs without context remain byte-for-byte equivalent at the normalized model boundary.

- [x] **Step 6: Commit agent-context normalization**

```bash
git add Cargo.toml Cargo.lock src/notes src/lib.rs src/core/changeset.rs src/diff/model.rs src/input tests/agent_context.rs tests/input_loading.rs tests/reload.rs tests/git_loading.rs tests/jj_loading.rs tests/sl_loading.rs
git commit -m "feat: load normalized agent context"
```

---

## Task 2: Make inline notes part of shared review geometry

**Files:**

- Create: `src/notes/target.rs`
- Modify: `src/review/row.rs`
- Modify: `src/review/geometry.rs`
- Modify: `src/review/state.rs`
- Modify: `src/review/mod.rs`
- Modify: `src/ui/review.rs`
- Modify: `src/ui/themes.rs`
- Test: `tests/notes_state.rs`
- Test: `tests/ui_render.rs`
- Test: `tests/ui_mouse.rs`

- [x] **Step 1: Write failing targeting and geometry tests**

Cover old/new overlap, new-side anchor precedence, file/hunk fallback, GitHub-style labels, stable synthesized ids, source classification (`ai`, `agent`, `user`), annotated-hunk indices, toggle visibility where user notes remain visible, split left/right docking, stack layout, narrow fallback, deterministic note height, scrollbar totals, and hit testing across inserted note rows.

- [x] **Step 2: Confirm note rows are not represented**

Run: `cargo test --test notes_state --test ui_render -- --nocapture`

Expected: compilation/assertion failure because row plans contain only diff/context rows.

- [x] **Step 3: Add stable note targets and controller-owned human notes**

Define `NoteTarget { file_id, old_range, new_range, hunk_index }` and `HumanNote { id, target, body, created_at, updated_at }`. Resolve external annotations against hunk ranges and exact line rows. Synthesize missing external ids from source, file id, ranges, and content. Preserve human notes through reload when the target file id remains; clamp missing lines to hunk/file fallback without crossing files.

Expose controller operations to begin, update, save, cancel, edit, remove, list, clear, and focus notes. These methods are also the future session mutation boundary.

- [x] **Step 4: Insert measured note rows into the canonical plan**

Add note-card rows immediately after their anchor diff row (or hunk/file fallback), using one placement function for layout width, left offset, content width, and height. Every note row has a stable key and occupies real geometry rows. User/draft notes are always visible; AI/agent notes obey `agent_notes`. The existing `{`/`}` navigation uses annotated hunk indices derived from the same targets.

- [x] **Step 5: Render plain note cards and preserve interaction geometry**

Render source/title/location, wrapped summary/rationale, tags/confidence/author, draft controls, and human note bodies with semantic theme colors. No top bar/menu is added. Mouse selection, scrollbar, context gaps, and sidebar file selection continue to use shared geometry.

- [x] **Step 6: Run state/render/mouse regressions and commit**

Run: `cargo test --test notes_state --test review_state --test ui_render --test ui_mouse --test review_selection`

```bash
git add src/notes/target.rs src/review src/ui/review.rs src/ui/themes.rs tests/notes_state.rs tests/ui_render.rs tests/ui_mouse.rs
git commit -m "feat: add inline note geometry"
```

---

## Task 3: Complete keyboard-first human note editing and export

**Files:**

- Modify: `src/app.rs`
- Modify: `src/ui/input.rs`
- Modify: `src/ui/dialogs.rs`
- Modify: `src/annotations/model.rs`
- Modify: `src/annotations/output.rs`
- Modify: `src/runtime.rs`
- Test: `tests/pty_notes.rs`
- Test: `tests/annotations.rs`
- Test: `tests/pty_ui.rs`

- [x] **Step 1: Write failing PTY and export tests**

Cover `a` reveal/hide, `c` opening an empty draft without leaking the key, typed text owning all shortcuts, newline insertion, Ctrl-S save, Escape cancel, editing an existing human note, deleting an emptied edit, selected old/new ranges, agent notes excluded from human Markdown export, explicit `--output`, and `--stdout` after TUI exit.

- [x] **Step 2: Confirm the current modal-only annotation path fails**

Run: `cargo test --test pty_notes --test annotations -- --nocapture`

Expected: inline draft/note assertions fail and new controller operations are not wired.

- [x] **Step 3: Route the composer through `ReviewController`**

Keep note editing keyboard-first and centered only when the terminal is too narrow for an inline composer. `c` targets the current semantic row/range. While composing, literal keys never reach navigation. Enter inserts a newline; Ctrl-S saves; Escape cancels. Saved notes become controller-owned inline user notes; editing reuses the stable note id.

- [x] **Step 4: Export normalized human notes**

Build Markdown annotations from human note targets at exit, including file, old/new side ranges, and bounded diff context. Remove the duplicate legacy flat-index annotation authority. Preserve the existing output heading and compatibility options.

- [x] **Step 5: Run PTY/output regressions and commit**

Run: `cargo test --test pty_notes --test annotations --test pty_ui --test runtime_resolution`

```bash
git add src/app.rs src/ui/input.rs src/ui/dialogs.rs src/annotations src/runtime.rs tests/pty_notes.rs tests/annotations.rs tests/pty_ui.rs
git commit -m "feat: edit and export inline review notes"
```

---

## Task 4: Port the bounded tolerant STML parser

**Files:**

- Create: `src/markup/mod.rs`
- Create: `src/markup/parse.rs`
- Modify: `src/lib.rs`
- Test: `tests/stml_parse.rs`

- [x] **Step 1: Port parser contract tests first**

Cover nested tags/attributes, bare angle brackets, comments, void tags, raw `code`/`pre`, mismatched/stray/unclosed tags, named and numeric entities, UTF-8 truncation, node/depth/error limits, case-insensitive raw closing tags, quoted/unquoted attributes, terminal-control stripping from text and attributes, and deterministic diagnostics.

- [x] **Step 2: Confirm parser API is absent**

Run: `cargo test --test stml_parse -- --nocapture`

- [x] **Step 3: Implement pure iterative parsing with fixed limits**

Define owned `StmlNode`, `StmlElement`, `StmlParseResult`, and `StmlParseLimits`. Default limits match Hunk: 64 KiB input, 2,000 nodes, depth 32, 20 diagnostics. Parsing never panics or rejects the whole note; malformed input returns the best-effort tree plus notes.

- [x] **Step 4: Run sanitizer/parser suites and commit**

Run: `cargo test --test stml_parse --test pager`

```bash
git add src/markup src/lib.rs tests/stml_parse.rs
git commit -m "feat: parse bounded terminal markup"
```

---

## Task 5: Add deterministic STML layout, rendering, guide, and CLI

**Files:**

- Create: `src/markup/layout.rs`
- Create: `src/markup/render.rs`
- Create: `src/markup/guide.md`
- Create: `src/markup/command.rs`
- Modify: `src/markup/mod.rs`
- Modify: `src/cli/args.rs`
- Modify: `src/cli/normalize.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/runtime.rs`
- Modify: `src/ui/review.rs`
- Test: `tests/stml_layout.rs`
- Test: `tests/markup_cli.rs`
- Test: `tests/ui_render.rs`

- [x] **Step 1: Write layout and black-box CLI tests**

Cover cell-aware wrapping, style preservation, explicit breaks, headings, badge/kbd padding, lists with hanging indents, spacers/rules, code clipping, bordered boxes/cards, padding/background fill, fixed/percentage widths, rows/gaps, narrow-row stacking diagnostics, unknown-tag fallback, exact-width Unicode output, deterministic repetition, JSON output, color modes, stdin/file input, invalid widths/themes/files, and guide snippets that all validate at reference width 56.

- [x] **Step 2: Confirm markup commands are missing**

Run: `cargo test --test stml_layout --test markup_cli -- --nocapture`

- [x] **Step 3: Port the symbolic terminal-cell layout engine**

Define `StmlStyle`, `StmlSpan`, `StmlLine`, and `StmlLayoutResult`. Support Hunk's block tags (`box`, `card`, `section`, `col`, `row`, `text`, `p`, headings, lists/items, rules, spacers, code/pre) and inline tags (`b`, `i`, `u`, `s`, `dim`, `c`/`color`/`span`, `kbd`, `badge`, `a`/`link`, `br`). Resolve widths with `unicode-width`; clip without splitting wide cells; keep colors symbolic until rendering.

Use a bounded content-sensitive LRU for repeated inline layouts; do not retain unbounded note markup.

- [x] **Step 4: Render STML inside note cards**

Resolve semantic tokens (`accent`, `success`, `warning`, `danger`, `info`, `muted`, `subtle`, `heading`, `badge-text`), ANSI names, and validated hex colors through `AppTheme`. Use the exact content width from note geometry; parse/layout notes appear as non-fatal note metadata and never shift geometry after render.

- [x] **Step 5: Add native markup commands and embedded guide**

Add:

```text
pdiff markup render <FILE|-> [--width 56] [--theme ID] [--color auto|always|never] [--json]
pdiff markup guide
```

`render` writes plain/ANSI lines, emits layout notes on stderr, and never enters the alternate screen. JSON is stable `{ "width", "lines", "notes" }`. The embedded guide names `pdiff`, not Hunk, and every fenced STML example is tested.

- [x] **Step 6: Run layout, CLI, render, and full review regressions**

Run: `cargo test --test stml_layout --test markup_cli --test ui_render --test pty_notes --test cli_contract`

- [x] **Step 7: Commit STML and commands**

```bash
git add src/markup src/cli src/runtime.rs src/ui/review.rs tests/stml_layout.rs tests/markup_cli.rs tests/ui_render.rs
git commit -m "feat: render deterministic terminal markup"
```

---

## Task 6: Document and verify notes/markup parity

**Files:**

- Modify: `README.md`
- Modify: `docs/parity/hunk.md`
- Modify: this plan

- [x] **Step 1: Update user documentation and the parity ledger**

Document agent-context schema/example, note keys, human-vs-external note behavior, export semantics, markup commands, limits, and tolerant degradation. Mark rows `verified` only with named automated evidence. Also reconcile any stale Pi/watch command-surface rows found during the audit.

- [x] **Step 2: Run the complete slice gate fail-fast**

```bash
set -e
if rg --files src -g '*.ts' -g '*.tsx' -g '*.js' -g '*.jsx' | rg .; then exit 1; fi
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
git diff --check
file target/release/pdiff
ldd target/release/pdiff
```

- [x] **Step 3: Commit the slice evidence**

```bash
git add README.md docs/parity/hunk.md docs/superpowers/plans/2026-07-21-notes-markup-implementation-plan.md
git commit -m "docs: verify native notes and markup parity"
```

---

## Slice completion gate

- Agent context is bounded, validated, sanitized, reloadable from files, matched across renames, and controls intentional file order.
- AI/agent/user notes share one normalized target model and one geometry/render path.
- Human drafts own input, save/cancel/edit inline, survive layout changes/reloads by stable target, and alone feed Markdown export.
- Note rows participate in measurement, scrollbars, hit testing, selection, and annotated-hunk navigation.
- STML parsing never crashes, is resource bounded, and sanitizes terminal controls.
- STML layout is deterministic and terminal-cell correct for all documented tags.
- `pdiff markup render` and `pdiff markup guide` are black-box verified and never initialize the TUI.
- The release remains one native Rust executable with no JavaScript/TypeScript source or runtime dependency.
