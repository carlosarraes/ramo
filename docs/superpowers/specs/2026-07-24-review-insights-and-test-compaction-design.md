# Review Insights and Test Compaction Design

## Goal

Make every Ramo review easier to orient and finish by adding:

- sticky whole-diff addition and deletion totals;
- monotonic reviewed-line progress;
- visible syntax highlighting without weakening diff emphasis; and
- reversible compaction of test files.

The behavior applies to local and pull-request reviews.

## Chosen Architecture

Review state owns progress and compaction. Geometry and snapshots derive from that
state, while the application frame owns the sticky header and footer. Syntax
highlighting remains a renderer concern.

This avoids renderer-only hiding, which would leave navigation, mouse hit targets,
comments, and progress out of sync with what is visible. It also avoids removing
test files from the underlying diff, so users can restore or inspect them without
reloading.

## Whole-Diff Summary

The sticky top row summarizes the original, unfiltered changeset:

- Pull request: `GitHub PR #123 · Improve review flow · 14 files · +200 -50`
- Local review: `Working tree · 14 files · +200 -50`

Addition totals use the configured addition color and deletion totals use the
configured deletion color. Filtering and test compaction do not change these
totals.

On narrow terminals, descriptive text truncates before the file count and colored
totals. The review viewport excludes the sticky header and footer so scrolling and
mouse geometry use only the content region.

## Reviewed Progress

The footer always shows a right-aligned label such as `Reviewed 50%`. Existing
filter input, notices, and transient messages continue to use the left side.

The denominator is the total number of additions plus deletions in the original
changeset. Unchanged context, file headers, notes, wrapping, and metadata do not
count.

Progress is monotonic:

- reaching changed lines in the viewport advances the furthest reviewed position;
- scrolling backward never lowers it;
- filtering never resets it;
- compacting a test file marks all of its changed lines reviewed; and
- restoring or individually expanding that file never lowers the result.

In split mode, a visual row can reveal both an addition and a deletion. Each
changed line contributes independently.

The controller tracks a reviewed prefix in stable diff order plus files explicitly
marked reviewed through compaction. The displayed numerator is the union of those
sets, preventing double counting while preserving the agreed furthest-point
behavior.

An empty diff displays `Reviewed 100%`.

## Syntax Highlighting

Ramo continues to use its existing Syntect-based, in-process highlighter. No
runtime dependency or external parser is added.

The renderer composes three layers:

1. diff row background and gutter colors;
2. syntax foreground colors for recognized file types; and
3. stronger character-level emphasis for changed spans.

Unknown file types remain plain text. Highlight failures fall back to the existing
diff styling instead of preventing the review from rendering.

## Test-File Detection

The built-in patterns recognize common test layouts:

- directories: `test/`, `tests/`, `__tests__/`, and `spec/`;
- filenames: `test_*`, `*_test.*`, `*.test.*`, and `*.spec.*`.

The configuration key `test_file_patterns` accepts additional glob patterns.
Configured patterns extend rather than replace the built-ins. Matching uses the
pure-Rust `globset` crate, compiled into the standalone Ramo binary.

Patterns are compiled once when configuration is loaded. Invalid patterns produce
a clear configuration error rather than being silently ignored.

## Compaction Interaction

Test files start expanded.

Pressing `T` toggles global test compaction:

- when enabled, every matching test file becomes a selectable one-line summary;
- the summary retains the file path, status, and addition/deletion totals;
- pressing Enter on a selected summary temporarily expands that file;
- clicking a summary performs the same temporary expansion;
- other matching files remain compacted; and
- pressing `T` again restores every file and clears temporary overrides.

Compacted summaries participate in normal keyboard navigation and mouse hit
testing. Comments, filtering, copy behavior, and session data continue to refer to
the unchanged underlying diff.

## State and Data Flow

The review controller owns:

- whether global test compaction is active;
- per-file temporary expansion overrides;
- the monotonic reviewed prefix;
- files marked reviewed through compaction; and
- compiled test-path matching supplied by configuration.

Review geometry emits either a normal file section or a compact selectable summary
for each visible file. The snapshot exposes whole-diff totals and current progress
to the application frame.

Reloads rebuild geometry and preserve progress where stable file and changed-line
identities still match. Removed lines disappear from both numerator and
denominator; new lines begin unreviewed.

## Key Binding and Documentation

Normal-mode `T` toggles test-file compaction. Existing lowercase `t` theme cycling
and Ctrl-T send behavior remain unchanged.

The built-in help and README document:

- the sticky totals and reviewed percentage;
- `T` compaction and Enter/click temporary expansion; and
- the `test_file_patterns` configuration option.

## Verification

Automated coverage includes:

- accurate whole-diff additions, deletions, and file counts;
- sticky header and footer rendering, including narrow terminals;
- monotonic progress while scrolling backward and across filters;
- correct changed-line accounting in unified and split layouts;
- compaction marking test-file lines reviewed without double counting;
- `T` restore and Enter/click temporary expansion;
- navigation and hit testing across compact summaries;
- built-in and configured test-pattern matching;
- invalid glob diagnostics;
- syntax foregrounds composed with diff backgrounds and character emphasis; and
- plain-text fallback for unknown file types or highlighting failures.

Manual verification covers local, branch, worktree, and GitHub PR review entry
points to confirm the shared behavior appears in every review.
