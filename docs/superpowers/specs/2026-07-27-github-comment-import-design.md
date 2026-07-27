# GitHub Review Comment Import Design

## Summary

Ramo will optionally import existing GitHub pull-request review threads when a
review opens:

```bash
ramo pr 123 --with-comments
```

The import is an explicit, one-time snapshot. Ramo will show unresolved,
non-outdated threads as read-only review context while preserving the current
publication contract: only comments newly authored inside the current Ramo
process are submitted to GitHub.

The flag is named `--with-comments` rather than `--sync` because this version
does not poll, refresh, reply, resolve, or synchronize in either direction.

## Goals

- Import every unresolved, non-outdated GitHub review thread, regardless of
  author.
- Preserve the root comment and replies as one visually grouped thread.
- Place line-level and file-level threads in the review stream when possible.
- Keep unmappable threads visible in a distinct trailer section.
- Make imported feedback navigable without making it editable.
- Guarantee that imported content cannot be exported or republished as new
  Ramo feedback.
- Preserve the existing `ramo pr NUMBER` behavior and network calls when the
  flag is absent.

## Non-goals

- Importing resolved or outdated threads.
- Replying to, resolving, unresolving, editing, or deleting GitHub threads.
- Polling GitHub or refreshing imported threads while the TUI is open.
- Importing issue-level PR conversation comments or review summary bodies.
- Supporting GitLab or Bitbucket comments.
- Exposing imported threads through Markdown output, agent-context files, or
  the live-session comment mutation API.

## CLI contract

`PrArgs` gains a PR-specific boolean flag:

```text
--with-comments  Include unresolved GitHub review threads as read-only notes
```

The corresponding `ReviewInput::PullRequest` value carries
`with_comments: bool`; the option does not become part of shared review flags
and is not accepted by `diff`, `show`, `patch`, or other commands.

Without the flag, Ramo performs no review-thread query and behaves exactly as
it does today. With the flag, a successful query returning no eligible threads
opens the review normally and queues a dismissible
`No unresolved GitHub threads` notice.

## Domain model and boundaries

Imported feedback uses a dedicated model rather than `HumanNote`, `LiveNote`,
or agent-context `ReviewNote`:

```text
GithubReviewThread
  id
  path
  subject: Line { side, start_line, end_line } | File
  comments: Vec<GithubThreadComment>
  url

GithubThreadComment
  id
  author
  body
  created_at
  url
```

The model contains sanitized display data and stable GitHub node IDs. It has no
`editable` state and no conversion into `Annotation` or
`RemoteReviewComment`.

The GitHub boundary gains a read-only operation that loads threads for an
already resolved `PullRequestReviewContext`. `GithubPullRequestSource` remains
responsible for remote loading; the input loader returns imported threads next
to the loaded PR review instead of embedding them in `DiffFile`.

At runtime, the application attaches the imported collection to the
`ReviewController` before the first session snapshot and render. The controller
owns placement, filtering, navigation, and visibility. Publication continues
to read exclusively from `human_notes()`.

## GitHub query and pagination

Ramo will call `gh api graphql` through the existing bounded command executor.
The query addresses the repository and PR number already captured in
`PullRequestReviewContext` and requests `reviewThreads`, including:

- Thread ID, `isResolved`, `isOutdated`, `subjectType`, path, diff side,
  start/end line fields, and URL-capable comment data.
- For every comment: node ID, plain-text body, author login, creation time, and
  URL.
- `pageInfo` for the outer thread connection and each comment connection.

GitHub exposes `isResolved`, `isOutdated`, `subjectType`, `path`, `diffSide`,
`startLine`, and `line` on `PullRequestReviewThread`; these file-line fields are
used instead of deprecated diff-relative positions. Reference:
<https://docs.github.com/en/enterprise-cloud@latest/graphql/reference/pulls>.

Ramo paginates the outer connection explicitly in pages of 100, stopping after
500 threads. Each thread requests up to 100 comments. A thread whose comment
connection has another page exceeds the v1 per-thread limit and fails with a
clear limit error rather than being displayed partially. Each command response
and the accumulated decoded model are bounded.

After decoding, Ramo retains only threads where `isResolved` and `isOutdated`
are both false. A missing author is displayed as `[deleted]`. GitHub timestamps
are retained in their source ISO-8601 form and rendered compactly; invalid
timestamps remain safe text rather than failing placement.

## Placement rules

Placement is deterministic and uses the parsed frozen PR diff:

1. Match the thread path to a file's current path, then its previous path for a
   rename.
2. A `FILE` subject attaches beneath that file's identity header.
3. A `LINE` subject with `RIGHT` maps to new-side line numbers; `LEFT` maps to
   old-side line numbers.
4. A multiline thread uses its start and end lines when both sides are
   compatible with Ramo's single-side range model.
5. The complete range must be representable by the current parsed diff. A
   missing file, absent line, unsupported side combination, or malformed anchor
   makes the thread unplaced.

Placement failure never discards a successfully decoded thread. Unplaced
threads retain their path, original anchor label, complete conversation, URL,
and a short placement reason.

## Review stream and interaction

Placed threads render as read-only cards directly beneath their target line or
file header. The card header identifies GitHub, the author, and timestamp. The
root body and replies remain within one card and replies are visibly indented.
Bodies render as sanitized plain text; URLs are displayed as text and are not
interpreted as STML.

Imported threads are visible immediately when requested. The `a` binding keeps
its existing meaning and controls only AI and agent notes.

`{` and `}` navigate through all visible annotated targets, including imported
threads, agent notes, live notes, and local human notes. Clicking an imported
card selects it but never opens the note editor. The `c` binding still creates
a separate local comment at the current code selection; it is not a reply.

Imported cards add geometry rows but do not count as changed lines and do not
alter reviewed-line progress. Layout, wrapping, and terminal-width changes
preserve their stable thread selection.

When test-file compaction hides a file with imported feedback, its compact row
shows the number of unresolved threads. Enter or click expands that file using
the existing one-file exception behavior.

## Unplaced comments section

Unplaced threads render after the normal file stream under an
`Unplaced GitHub comments` heading. This is a real selectable trailer in shared
review geometry, not a synthetic diff file, so it does not affect file counts,
sidebar entries, additions/deletions, or progress.

Each unplaced card displays the path, original side/range when available, the
placement reason, the full conversation, and GitHub URL. The cards participate
in `{` and `}` navigation and mouse selection. File filtering applies to their
stored path; an empty filter shows all unplaced threads.

## Filtering and reload behavior

Filtering a file out also hides its placed threads. Unplaced threads use the
same case-insensitive path matching even when their path does not occur in the
current diff.

PR snapshots remain non-reloadable. The existing `r` diagnostic continues to
instruct users to reopen `ramo pr NUMBER`; reopening with `--with-comments`
fetches a new snapshot.

## Failure handling and limits

If `--with-comments` is present, failure to authenticate, execute GraphQL,
decode a page, or complete pagination stops before terminal entry with an
operation-specific error. Ramo must not silently open a review missing feedback
the user explicitly requested.

Limits are:

- 500 imported threads.
- 100 comments per thread.
- 64 KiB of text per comment body after decoding.
- Bounded stdout/stderr per GraphQL command and a bounded accumulated import.

Exceeding any limit is an explicit error. Terminal control sequences are
removed from author names, bodies, paths, timestamps, and URLs before the data
reaches review rendering. A malformed or unmappable anchor is not a fetch
failure and is represented in the unplaced section.

## Publication isolation

The existing publication confirmation count and payload continue to derive
only from locally authored `HumanNote` values. Imported thread IDs live in a
separate namespace and cannot enter edit, delete, Markdown export, tmux-send,
session-comment mutation, or GitHub submission paths.

Immediately before publication, Ramo performs the existing fresh-head check.
A stale PR head preserves both local notes and imported display context while
showing the current dismissible failure dialog.

## Testing

### CLI and compatibility

- Parse `ramo pr 123 --with-comments` into the PR-specific input field.
- Reject the flag on non-PR commands.
- Prove plain `ramo pr 123` performs no thread query.

### GitHub boundary

- Assert the exact GraphQL command, variables, and query fields.
- Cover outer pagination, the 500-thread limit, the 100-comment limit, empty
  results, deleted authors, malformed JSON, truncation, timeouts, authentication
  failures, and terminal-control sanitization.
- Verify resolved and outdated threads are excluded while all authors and
  replies of eligible threads are retained.

### Placement and state

- Cover right/left lines, multiline ranges, file-level subjects, renames,
  deleted files, missing paths, missing lines, incompatible sides, and unplaced
  reasons.
- Verify read-only behavior, stable IDs, filter visibility, layout preservation,
  compacted-file counts, progress isolation, and annotated navigation.
- Verify imported threads never appear in human-note exports or publication
  requests.

### Rendering and PTY behavior

- Snapshot full thread grouping, reply indentation, metadata, plain-text
  sanitization, URLs, narrow terminals, and the unplaced trailer.
- Run a PTY review with `--with-comments`, navigate imported feedback, author a
  new local note, quit, and assert that GitHub receives only the new note.

## Documentation

The README PR section will document `--with-comments`, the one-time read-only
semantics, unresolved/non-outdated filter, unplaced section, and publication
isolation. The existing v1 limitation paragraph will be revised to retain only
the limitations that remain true.
