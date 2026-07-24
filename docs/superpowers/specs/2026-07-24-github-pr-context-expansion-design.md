# GitHub Pull Request Context Expansion Design

## Goal

Allow `ramo pr <NUMBER>` reviewers to expand and collapse unchanged lines with
the existing `z` key and collapsed-row mouse action, without checking out the
pull request, fetching Git refs, or mutating the local repository.

## User Experience

Pull request reviews retain their frozen-snapshot behavior. Ramo initially
loads only pull request metadata and the unified diff. No full file contents
are downloaded during startup.

When the reviewer selects a collapsed gap and presses `z`, Ramo lazily requests
the corresponding complete file from GitHub at the immutable revision captured
when the review opened. The selected gap expands through the same rendering,
navigation, line-number, and collapse behavior used by local reviews. Clicking
a collapsed gap performs the same action.

The first expansion of a file may wait for GitHub. Ramo caches the result for
the remainder of the session, so later gaps in that file do not make another
request. Loading one file never downloads the other changed files.

If the source cannot be loaded, Ramo keeps the review and all draft notes
intact and shows a modal message that can be dismissed with Enter or Escape.
The message distinguishes missing content, oversized content, invalid UTF-8,
timeouts, bounded-output truncation, and GitHub CLI/API failures.

## Snapshot Semantics

Pull request metadata includes both `baseRefOid` and `headRefOid`. These object
IDs are captured with the title, author, and branch names during initial load.
They never change during the session.

Each parsed diff file receives immutable remote source specifications:

- The old source uses `baseRefOid` and the previous path. It is absent for an
  added file.
- The new source uses `headRefOid` and the current path. It is absent for a
  deleted file.
- Renames and copies use their previous path on the base revision and their
  current path on the head revision.

The existing context engine chooses the new source for modified, added,
renamed, and copied files and the old source for deleted files. This preserves
the line-number projection already used by local source expansion.

If the pull request changes after Ramo opens, expansions continue reading the
captured revisions and therefore remain consistent with the displayed diff.
Publishing retains the existing fresh-head comparison; a changed head prevents
submission and preserves all local review state.

## Architecture

### Provider-neutral source reference

Add a remote blob variant to `SourceSpec` containing an opaque repository
identifier, immutable revision, and repository-relative path. The diff and
review layers do not know which provider will load it.

The native local source loader treats remote blobs as unavailable. A dedicated
GitHub context loader handles only remote blobs and is injected into the app
for `ramo pr`. This keeps local reviews independent from `gh` and leaves room
for future provider-specific loaders.

### GitHub source boundary

Extend `GithubPullRequestSource` so initial metadata resolution returns the
base revision as part of `PullRequestReviewContext`. After parsing the unified
diff, the pull request loader attaches old and new remote source
specifications to every non-binary file.

`GithubContextSourceLoader` implements `ContextSourceLoader` over the existing
literal-argument `CommandExecutor`. It performs an authenticated raw contents
request equivalent to:

```text
gh api --method GET \
  repos/<owner>/<repo>/contents/<percent-encoded-path> \
  -f ref=<captured-oid> \
  -H Accept:application/vnd.github.raw+json
```

The endpoint path is percent-encoded by UTF-8 byte while preserving `/` as the
path separator. The revision is a separate literal argument. No value is
evaluated by a shell.

Each request has an 8 MiB stdout limit, an 8 KiB stderr limit, and a 15-second
timeout. A response that reaches the stdout limit is reported as oversized
rather than parsed as partial source. Source must be valid UTF-8. Terminal
control sanitization remains line-based in the existing expansion pipeline.

The loader caches `Result<Option<String>, SourceFailure>` by `SourceSpec`.
Successes and failures are both cached for the session to prevent repeated
network calls or repeated modal errors for the same source. `invalidate`
clears this cache, although pull request sessions remain non-reloadable.

The GitHub review publisher and context loader may own separate stateless
command executors. They share immutable context values, not mutable process
state or authentication tokens. Authentication remains delegated entirely to
the installed `gh` CLI.

### Application integration

Runtime construction selects the native context loader for local inputs and
the GitHub context loader for pull request inputs. Existing commands never
initialize or invoke GitHub source loading.

Remove the pull-request guard that currently rejects `ToggleContext`. Both
keyboard and mouse expansion flow through one app helper:

- On success, retain existing controller behavior.
- For local reviews, retain the existing short toast for source failures.
- For pull request reviews, translate the failure into a dismissible modal and
  return to the review afterward.

Watch, reload, and editor opening remain disabled in pull request mode.

## Error Handling

Initial metadata and diff failures remain command-line errors before terminal
entry. Lazy source failures occur inside the TUI and never close the review.

GitHub errors identify the operation as loading pull request context. Missing
files are reported as unavailable at the captured revision. Bounded stdout
maps to `TooLarge { limit: 8 MiB }`; timeout, invalid UTF-8, nonzero exit, and
I/O failures retain actionable diagnostics with terminal controls removed
from captured stderr.

No retry loop is added. The cached failure remains stable for the session; the
reviewer can dismiss it and continue reviewing or reopen `ramo pr <NUMBER>` to
start a fresh snapshot.

## Testing

All automated tests use scripted command executors or fake `gh` binaries and
must never contact GitHub.

Required evidence:

- Metadata adapter tests require and parse `baseRefOid`.
- Pull request loader tests verify old/new remote source mapping for modified,
  added, deleted, renamed, and copied files.
- GitHub context loader tests verify exact literal arguments, percent encoding,
  immutable revision selection, caching, timeout, truncation, nonzero exit,
  missing content, and invalid UTF-8.
- App-flow tests verify `z` and collapsed-row clicks expand pull request
  context, and failures open a dismissible modal without losing review state.
- A PTY test proves `ramo pr` makes no contents request at startup, makes one
  contents request on the first `z`, renders the expanded lines, and makes no
  second request when the same file is collapsed and expanded again.
- Existing local context, PR publication, CLI, and PTY suites remain green.

## Out of Scope

- Eagerly downloading all changed files.
- Fetching or creating Git refs, checking out branches, or changing worktrees.
- Reloading or watching a pull request snapshot.
- Expanding binary files or source files larger than 8 MiB.
- Importing GitHub review threads.
- GitLab or Bitbucket source loading.
- Adding an HTTP client, TLS stack, token storage, or non-Rust runtime.
