# Margem Shared Review TUI Design

**Status:** Approved design
**Date:** 2026-07-28

## Context

Ramo has a mature terminal review experience: responsive diff rendering, syntax
highlighting, semantic navigation, visual selection, inline comments, draft
editing, themes, context expansion, and provider publication. Most of that
experience currently lives in Ramo's top-level crate and is coupled to diff
models. `ramo-core` contains reusable syntax and review primitives, but it is
not the interactive renderer and is also consumed by the Android application.

Snapdoc already renders Markdown and supports text-anchored comments in its web
review interface. Its CLI is currently written in Go and can list or reply to
comments, but it has no interactive terminal document reviewer. The Snapdoc CLI
will be migrated to Rust in a separate behavior-preserving project handled by
another agent.

The shared opportunity is not a shallow renderer that returns terminal lines.
It is a deep, provider-neutral module that owns the complete interactive review
session. Ramo will adapt diffs into it; Snapdoc will adapt published documents
and comments into it.

## Decision

Create an independent public Rust project named **Margem** (`margin` in
Portuguese), where review comments naturally live.

- Local repository: `/home/carraes/projs/margem`
- GitHub repository: `carlosarraes/margem`
- Initial package: `margem`
- Initial product shape: library only; no standalone binary
- Runtime: native Rust with Ratatui and Crossterm

Margem owns the terminal session, rendering, interaction state, anchoring, and
draft workflow. It contains no Git, GitHub, Snapdoc, HTTP, authentication, or
provider publication logic.

Ramo's existing `ramo-core` remains in Ramo for diff/domain/mobile concerns.
The new shared terminal module must not add terminal dependencies to
`ramo-core` or the Android build.

## Goals

- Reuse one terminal review experience across Ramo, Snapdoc, and future tools.
- Preserve current Ramo behavior as a hard acceptance requirement.
- Render Markdown semantically and fenced code with syntax highlighting.
- Make browser and terminal Snapdoc comments fully interchangeable.
- Keep all provider loading and publication in the consuming applications.
- Keep Margem's external interface small while hiding terminal complexity.
- Preserve the small, single-binary deployment model of consuming Rust CLIs.

## Non-goals

- Reimplement Snapdoc's Cloudflare worker, browser review rail, or Android app.
- Mix the Snapdoc Go-to-Rust migration with the new TUI integration.
- Publish comments directly from Margem.
- Render Mermaid diagrams or terminal images in the first version.
- Reproduce arbitrary browser HTML or schema-reference popovers in the first
  version.
- Redesign Ramo's navigation, key bindings, layouts, or visual language during
  extraction.
- Add a standalone Margem executable in the first version.

## Public Interface

Margem exposes one complete session operation:

```rust
pub fn run_review(
    session: ReviewSession,
    host: &mut dyn ReviewHost,
) -> Result<ReviewOutcome, ReviewError>;
```

The primary values are:

```rust
pub struct ReviewSession {
    pub document: ReviewDocument,
    pub existing_comments: Vec<ReviewComment>,
    pub options: SessionOptions,
}

pub struct ReviewOutcome {
    pub decision: ExitDecision,
    pub new_comments: Vec<DraftComment>,
    pub recoverable_draft: Option<DraftState>,
}
```

`ReviewSession` is fully loaded before terminal entry. `ReviewOutcome` contains
everything the host needs to publish or recover a failed submission. Provider
requests are not part of Margem's interface.

## Review Document

`ReviewDocument` is a semantic document independent of terminal dimensions. It
contains stable block and row identities so layout changes do not invalidate
the cursor, viewport, comments, or reviewed progress.

Supported block families are:

- headings and prose;
- ordered, unordered, and task lists;
- tables;
- blockquotes and thematic rules;
- code blocks with language and source metadata;
- diff files, hunks, rows, and expandable gaps;
- notices and safe unsupported-content placeholders.

Ramo converts `ramo-core` change sets into Margem diff blocks. Snapdoc supplies
Markdown plus its canonical browser text projection. Future consumers adapt
their domain data into the same semantic model rather than implementing a new
event loop or renderer.

## Comment Anchors

Margem supports two typed, provider-neutral anchors:

```rust
pub enum DocumentAnchor {
    TextQuote(TextQuoteAnchor),
    SourceRange(SourceRangeAnchor),
}
```

`TextQuoteAnchor` contains Snapdoc-compatible `exact`, `prefix`, `suffix`,
`start`, and `end` fields. `SourceRangeAnchor` contains a path, old/new side,
and inclusive line range suitable for GitHub and other code-review providers.

Margem owns the mapping from terminal cells and wrapped rows to document
anchors. Consumers must not recalculate selections.

Snapdoc offsets use UTF-16 code units because browser DOM ranges and JavaScript
string offsets use UTF-16. Margem therefore maintains explicit mappings among
UTF-8 bytes, Unicode scalar values, grapheme/terminal cells, and canonical
UTF-16 offsets. Emoji, combining characters, CJK text, and repeated quotes are
contract cases, not best-effort behavior.

Source-range selections cannot cross files, diff sides, or invalid provider
ranges. Text selections may cross wrapped rows and Markdown blocks. An invalid
selection remains visible and produces a dismissible explanation.

## Host Seam

Margem owns terminal initialization, the event loop, rendering, dialogs,
interaction state, and terminal restoration. One narrow host interface supports
operations requiring application knowledge:

```rust
pub trait ReviewHost {
    fn handle(&mut self, request: HostRequest) -> HostResponse;
    fn poll(&mut self) -> Vec<HostEvent>;
}
```

Requests cover generic capabilities:

- reload the current document;
- expand a collapsed context block;
- open a source location in an editor;
- invoke an application-configured external action.

Events deliver replacement documents, expanded context, and dismissible
notices or failures. Ramo maps these operations to its source loader, watch
coordinator, editor, tmux, and Codex integrations. Snapdoc initially registers
no custom actions.

Margem never learns provider or tool names. External actions use stable IDs,
labels, and configured key bindings supplied by the host.

## Submission and Drafts

New comments remain local for the duration of the review. Existing remote
comments are read-only. Saving a note creates a draft; users may edit or remove
new drafts before submission.

The host configures submission choices and labels. Margem renders the
confirmation interface and returns the selected opaque choice:

- Ramo: Comment, Approve, Request changes, or Cancel.
- Snapdoc: Publish comments or Cancel.

Margem restores the terminal before the application performs network
publication. A failed publication retains a serializable `DraftState` so the
application can offer retry or recovery without reconstructing anchors.

Snapdoc v1 supports displaying existing threads and creating new root comments.
Replies, resolving/reopening, and deletion of remote comments remain in
Snapdoc's existing browser/CLI workflow.

## Internal Rendering Model

Margem separates semantic content, derived terminal geometry, and interaction
state:

```text
ReviewDocument -> LayoutPlan -> ReviewState
semantic data     terminal rows  cursor/viewport/drafts
```

`LayoutPlan` owns wrapping, tables, diff geometry, syntax spans, comment-card
placement, and terminal-cell-to-anchor mappings. It is rebuilt from stable
semantic identities when width, theme, content, or expansion state changes.

Syntax highlighting is lazy and content-sensitive. Layout and syntax caches are
bounded by item count and retained bytes. Repeated navigation, resize, reload,
and theme cycles must not accumulate stale geometry or syntax state.

Existing comments render inline as read-only cards. New local drafts are
visually distinct and remain editable. Unplaced external comments appear in a
labeled trailer rather than disappearing.

## Interaction Contract

Ramo uses a compatibility keymap as a hard contract. The extraction preserves:

- `j`/`k`, arrows, `Ctrl-U`/`Ctrl-D`, and `g`/`G` navigation;
- `[`/`]` semantic navigation;
- `/` filtering;
- `V` visual selection;
- `C` comment creation;
- `Ctrl-S` comment saving;
- `Esc` cancellation and mode exit;
- current theme, layout, compaction, editor, context, and external-action
  bindings.

Snapdoc receives the same navigation and comment vocabulary. For Markdown,
`[`/`]` visits headings, code blocks, tables, and existing comment threads.

## Markdown Scope

The first Markdown renderer provides semantic terminal parity, not browser
pixel parity:

- headings, paragraphs, emphasis, links, and inline code;
- blockquotes, lists, task lists, tables, and thematic rules;
- fenced code with language-aware syntax highlighting;
- Mermaid as a labeled source block;
- images as `Image: alt text - URL`;
- raw HTML as safe text or a compact unsupported block.

Terminal Mermaid diagrams, terminal image protocols, schema-reference popovers,
and arbitrary HTML layout are deferred.

## Snapdoc Anchor Compatibility

Snapdoc's browser-generated flat text remains authoritative. Its future Rust
adapter supplies Markdown, the canonical flattened browser text, and existing
comments. Margem parses the Markdown and maps every displayed text span into
that projection.

The projection must be verified before new comments are enabled. If projections
disagree:

- the document remains readable;
- existing comments are placed by quote/context where possible;
- unplaced comments appear in the trailer;
- creating and publishing new comments is disabled;
- a dismissible message explains the incompatibility.

This prevents the terminal from publishing anchors that appear valid locally
but cannot resolve in the browser.

## Error Handling and Safety

Recoverable failures preserve the document, viewport, selection, and drafts.
They render as placeholders or dismissible overlays. These include malformed
Markdown, syntax detection failure, invalid external anchors, context loading,
reload failure, and external-action failure.

Only terminal initialization failure, an unusable document, or a corrupted
session invariant terminates the session. A terminal guard restores terminal
state after normal exit, returned errors, and panic unwinding.

Input limits bound document bytes, block count, nesting, table dimensions,
individual line length, comments, and cached layout data. Terminal control
characters are sanitized before layout. Margem never executes document content.

## Ramo Extraction

Reusable implementations move incrementally from Ramo into Margem:

- review geometry, row planning, anchors, navigation, progress, and selection;
- review rendering, split layout, highlighting, dialogs, input, and themes;
- draft state, targets, placement, and comment cards;
- safe rich comment-card layout;
- terminal ownership and restoration.

Ramo retains:

- Git, Jujutsu, and Sapling loading;
- diff parsing and `ramo-core` domain models;
- GitHub transport and publication;
- source/watch coordination;
- configuration persistence;
- editor, tmux, Codex, and other external integrations;
- CLI and session-daemon behavior.

The extraction proceeds in vertical slices. Ramo remains runnable after every
slice, and duplicate implementations are removed only after the Margem-backed
path passes the parity gate.

During development, Ramo consumes pinned Margem Git revisions. A stable Margem
release is published only after Ramo parity is demonstrated.

## Snapdoc Migration Contract

The Snapdoc CLI Go-to-Rust migration is a separate behavior-preserving project.
Its agent receives this approved interface as a target but does not implement
the TUI during migration. The worker, browser review interface, dashboard, and
Android application remain unchanged by that migration.

After both projects are stable, Snapdoc adds an HTTP/content adapter that loads
a published artifact, canonical text, and existing comments; runs Margem; then
publishes the returned new root-comment batch after confirmation.

The canonical text is not currently exposed by Snapdoc's owner content API.
The later Margem integration therefore adds an authenticated `anchor_text`
field or equivalent endpoint whose value is produced by the same flattening
implementation used by the browser. This API change belongs to the integration
project, not to the behavior-preserving Go-to-Rust CLI migration.

## Verification

Margem is tested through its public interface with Ratatui's test backend and an
in-memory host. Tests cover document construction, responsive layout,
navigation, selection, anchors, drafts, dialogs, host requests/events, failure
recovery, and terminal restoration.

Additional gates are:

- all existing Ramo unit, integration, PTY, rendering, navigation, and
  performance tests continue passing;
- representative pre-extraction and Margem-driven Ramo screen snapshots match
  at wide, narrow, and tiny terminal sizes;
- Snapdoc contract fixtures cover headings, lists, tables, code blocks,
  entities, repeated text, emoji, combining characters, and mixed scripts;
- fuzz/property tests exercise UTF-8/UTF-16/cell mappings and anchor resolution;
- benchmarks guard scrolling, resize, syntax highlighting, and large-document
  memory behavior;
- panic, error, and host-failure tests prove terminal restoration;
- sanitization and resource-limit tests reject hostile or unbounded input.

## Acceptance Criteria

- Ramo uses Margem for its interactive review UI with no intentional behavior
  or binding regressions.
- Ramo remains a native single binary and its Android dependency graph does not
  acquire terminal libraries.
- Margem renders the agreed Markdown subset and syntax-highlights fenced code.
- Margem produces browser-compatible Snapdoc text anchors, including complex
  Unicode cases, whenever the canonical projection verifies.
- Existing comments display inline; only new local root comments are returned
  for batch publication.
- Provider publication and authentication remain outside Margem.
- The public Margem interface remains provider-neutral and testable with an
  in-memory host.
