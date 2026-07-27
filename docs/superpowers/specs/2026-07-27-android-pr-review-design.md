# Ramo Android PR Review Design

**Date:** 2026-07-27
**Status:** Approved for implementation planning

## Summary

Ramo will gain a standalone Android application for focused GitHub pull-request
review. The app will show only open pull requests authored by the signed-in user
or awaiting that user's review. It will support reading unified diffs, viewing
existing review conversations, drafting inline comments, synchronizing viewed
files, and publishing a comment, approval, or request for changes.

The Android UI will use Kotlin and Jetpack Compose so the app feels native and
polished. Diff parsing, syntax highlighting, GitHub communication, review state,
validation, and publishing will remain in Rust and will be shared with the
terminal application where practical. No desktop relay or hosted Ramo backend is
required.

This first version is personal and Android-only. Authentication uses a
fine-grained personal access token stored with Android Keystore protection.

## Goals

- Make reviewing a pull request from a phone substantially lighter and faster
  than GitHub's general-purpose mobile interface.
- Work anywhere without requiring the user's computer to be powered on.
- Present a clean Tokyo Night interface with code as the dominant content.
- List only current review requests and pull requests authored by the user.
- Preserve Ramo's draft-first review workflow and publish a review deliberately.
- Reuse Rust domain logic between the terminal and Android applications.
- Keep private repository data and credentials on the device.
- Notify the user of new review requests without adding a hosted backend.
- Change terminal Ramo's default presentation to unified diff while retaining
  split diff as an option.

## Non-goals for v1

- iOS, tablet-specific, or desktop graphical clients
- GitLab, Bitbucket, or GitHub Enterprise Server
- Public distribution, Play Store packaging, or public OAuth login
- Replies to existing review threads or resolving/unresolving threads
- Editing reviews after they have been published
- Merge, rebase, queue, or branch-management operations
- Full offline storage of private source diffs
- Instant push notifications
- A hosted Ramo service or a connection to the terminal application

## Product Structure

### Inbox

The app opens on two tabs:

1. **Review requests** is the default and contains open pull requests where the
   user or one of their teams is currently requested for review.
2. **Your PRs** contains open draft and ready pull requests authored by the user.

Each compact row contains the repository and pull-request number, title, author,
updated time, changed-file count, additions, deletions, and a small state badge
when useful. Lists sort by latest activity, load additional pages while scrolling,
and support pull-to-refresh.

Opening the app or refreshing reconciles the inbox against GitHub rather than
treating notification read state as the source of truth.

### Pull-request summary

Opening a pull request shows:

- repository, number, title, author, base branch, and head branch;
- total additions and deletions;
- changed-file count;
- viewed-file progress; and
- a file drawer ordered consistently with the GitHub diff.

The pull request is loaded as a frozen review snapshot identified by its head
commit SHA.

### Diff reader

The mobile reader shows one file at a time using a full-width unified diff.
Side-by-side rendering is not available on mobile. Terminal Ramo also changes to
unified diff by default, but retains an explicit split-diff option.

The file header remains visible and contains the path, file-list control, and
synced Viewed toggle. Code is syntax highlighted with the Tokyo Night palette.
Added and removed backgrounds remain restrained enough that syntax colors and
text stay legible.

Code does not wrap by default. Horizontal movement scrolls code only. Files are
changed with explicit previous/next actions or the file drawer, avoiding a
conflict between horizontal code scrolling and swipe navigation.

Collapsed context rows can be tapped repeatedly to reveal additional unchanged
lines. Source context is fetched lazily from the base and head versions rather
than persisting full source files on disk.

Binary files and files GitHub cannot render display a clear explanation rather
than a blank screen. Large files are loaded and rendered incrementally so opening
one file does not require constructing the entire pull request in Compose.

### Existing conversations

Existing review threads appear next to their anchored line or range. They are
read-only in v1. Outdated or unmappable threads are shown in a separate compact
section for that file with their original path and context when available.

### Draft comments

Tapping a changed line's gutter begins a single-line comment. Long-pressing a
line enters range selection; draggable handles adjust the range and the UI shows
the exact selected lines before opening the editor.

The editor is a Markdown text field. Enter always inserts a newline. A distinct
**Save draft** action completes editing, removing the terminal ambiguity between
adding a newline and finishing a comment.

Drafts appear inline and can be edited or deleted. They auto-save locally and
survive process death or device restart. A draft anchor records the repository,
pull-request number, frozen head SHA, path, side, start/end lines, and nearby
source text needed for stale-revision recovery.

### Viewed files

Viewed state is synchronized with GitHub. Reaching the end of a file marks it
viewed automatically, and the visible toggle allows the user to undo that action.
The reader and file drawer both show overall progress.

### Finish review

**Finish review** opens a bottom sheet containing:

- all draft comments grouped by file and range;
- an optional overall Markdown comment;
- Comment, Approve, and Request changes verdicts; and
- an explicit final confirmation that includes the PR number and draft count.

The app submits the overall body, inline drafts, head commit, and verdict as one
GitHub pull-request review request. Approve and Request changes are not offered
on the user's own pull requests.

## Visual Direction

The approved interface is a dense but touch-friendly native Android design.
It avoids large cards, excessive metadata, gradients, glass effects, and
decorative navigation. Code receives most of the screen.

The default palette follows Tokyo Night:

- app background: `#1a1b26`
- elevated surface: `#24283b`
- primary text: `#c0caf5`
- muted text: `#565f89`
- blue: `#7aa2f7`
- cyan: `#7dcfff`
- green: `#9ece6a`
- red: `#f7768e`
- amber: `#e0af68`
- purple: `#bb9af7`

The app respects Android font scaling and screen readers. Code has an independent
configurable monospace size and uses horizontal scrolling rather than forced
wrapping.

## Architecture

### Repository layout

The repository becomes a Rust workspace with an Android application alongside
it. The exact migration may be staged, but responsibilities must settle into
these boundaries:

- `ramo-core`: repository-independent diff parsing, line models, syntax spans,
  review drafts, anchors, validation, and navigation state.
- `ramo-github`: direct GitHub REST/GraphQL transport and GitHub-specific mapping.
- terminal Ramo: the existing Ratatui application consuming shared core types.
- `ramo-mobile`: an Android `cdylib` exposing coarse mobile operations through
  UniFFI-generated Kotlin bindings.
- `android/`: Compose screens, ViewModels, Keystore integration, WorkManager,
  notifications, local encrypted persistence, and APK packaging.

The terminal's existing `gh`-based path may remain during the first extraction.
The direct API transport is mandatory for mobile and is hidden behind the same
remote-review abstraction so the domain does not depend on a subprocess or an
Android type.

### Rust/Kotlin boundary

Kotlin owns platform behavior and presentation:

- Compose rendering and navigation;
- lifecycle-aware ViewModels and UI state;
- Android Keystore operations;
- notification permissions and notification display;
- WorkManager scheduling; and
- encrypted local storage orchestration.

Rust owns product behavior:

- GitHub queries and mutations;
- diff parsing and context expansion;
- syntax tokenization;
- PR/file/thread domain mapping;
- draft anchoring and re-anchoring;
- review validation; and
- review publication.

The UniFFI boundary is intentionally coarse. Calls return complete screen/domain
models or accept user intents; scrolling, text-field keystrokes, and other
high-frequency UI events never cross FFI. Kotlin executes blocking bridge calls
off the main thread, and exported Rust objects have explicit lifetimes.

### GitHub API mapping

The Rust transport uses the versioned GitHub API directly:

- Search queries discover open authored and review-requested pull requests.
- Pull-request and file endpoints provide metadata, file order, patch anchors,
  additions, deletions, and head/base revisions.
- Base/head file content is fetched only when context expansion requires it.
- GraphQL loads review threads and performs mark/unmark-file-viewed mutations.
- The notifications endpoint supplies `review_requested` background events.
- The create-review REST endpoint publishes the body, inline comments, commit ID,
  and `COMMENT`, `APPROVE`, or `REQUEST_CHANGES` event in one request.

Requests use conditional headers where GitHub supports them. Pagination, rate
limits, and API version headers are handled inside `ramo-github`, not in UI code.

## Authentication and Device Data

The personal v1 asks the user to create a fine-grained personal access token with
access only to the repositories they want Ramo to review and Pull requests write
permission. The token is never compiled into the APK or committed to Git.

The Android layer generates a non-exportable Keystore key and uses it to encrypt
the token at rest. The decrypted token is passed to Rust only for an in-memory
session and is cleared when the session is dropped as far as the platform permits.

Stored device data is deliberately limited:

- encrypted token;
- encrypted review drafts and their minimal anchors;
- PR summaries, viewed progress, and notification cursors; and
- user preferences such as code font size.

Full diff/source bodies are not persisted in v1. Signing out removes the token,
drafts, and cached metadata. A revoked or expired token asks for replacement
without deleting drafts until the user explicitly signs out.

## Notifications

WorkManager performs a network-constrained periodic check with Android's minimum
15-minute interval. Timing is approximate and may be deferred by Doze or battery
optimization; the UI must not promise real-time delivery.

The worker reads GitHub notifications, filters for new `review_requested` pull
requests, and deduplicates by notification and PR identity. Tapping a notification
opens the corresponding PR. Notification permission is requested in context and
can be disabled without affecting manual refresh.

Failures use bounded backoff. Authentication failures surface in the app, and
rate-limit responses suspend checks until the reported reset time.

## Consistency and Recovery

Immediately before publication, the engine fetches the current PR head SHA.

- If it matches the snapshot, publication proceeds.
- If it differs, publication stops and the updated diff is loaded.
- Drafts are re-anchored using path, side, old/new lines, and nearby source text.
- Any ambiguous or missing anchor becomes a visible **Needs attention** draft.
- Ramo never silently deletes, moves, or publishes an uncertain comment.

Network and API errors leave the complete draft review intact. A failed submission
can be retried after the cause is shown. The app does not optimistically claim a
review was submitted until GitHub returns success.

Without connectivity, cached inbox rows and drafts remain available. Uncached
diffs and publishing clearly wait for connectivity.

## Testing Strategy

### Rust

- Unit tests for parsing, unified line construction, range anchors, syntax spans,
  context expansion, and stale-revision re-anchoring.
- Fixture-driven tests for GitHub pagination, missing patches, binary files,
  threads, rate limits, and API errors.
- Payload tests for viewed mutations and atomic review submission.
- Regression tests proving unified is the terminal default and split remains
  selectable.

### Android

- ViewModel tests for inbox, reader, draft editor, finish sheet, and error states.
- Compose UI tests for the two tabs, file drawer, range selection, explicit draft
  save, viewed toggle, and verdict confirmation.
- Persistence tests proving encrypted drafts survive restart and are removed on
  sign-out.
- WorkManager tests for filtering, deduplication, backoff, and notification taps.
- Accessibility checks for labels, focus order, contrast, and font scaling.

### End to end

A disposable GitHub pull request exercises list discovery, diff loading, context
expansion, existing threads, mark/unmark viewed, a multiline draft, approval or
request changes, and stale-head protection. The APK is then installed on the
connected phone through ADB for a physical-device smoke test.

## Delivery Sequence

Implementation planning should keep vertical slices demonstrable:

1. Extract the shared Rust core and make terminal unified diff the default.
2. Add the direct GitHub API client with fixture tests.
3. Establish the Android/UniFFI build and secure token entry.
4. Deliver the live inbox and notification worker.
5. Deliver the one-file diff reader, highlighting, context, and viewed sync.
6. Deliver existing conversations, range drafts, persistence, and stale recovery.
7. Deliver the finish-review sheet and publication flow.
8. Run automated verification, install via ADB, and complete the real-PR smoke
   test before producing a signed personal release APK.

Signing credentials remain outside the repository. No release is considered
complete until the exact APK artifact has passed the physical-device smoke test.
