# GitHub Pull Request Review Design

## Context

Ramo can already review a GitHub pull request when the user manually pipes
`gh pr diff` into it, but that workflow loses pull-request identity. Ramo
cannot verify that the pull request stayed unchanged, map its local notes back
to GitHub review comments, or submit an approve/request-changes verdict.

The first provider-specific workflow will be:

```text
ramo pr 123
```

It runs inside a local Git repository, uses the authenticated GitHub CLI as an
optional helper, loads the pull request into Ramo's existing native review UI,
and publishes the user's new notes as one GitHub pull request review.

## Goals

- Load a GitHub pull request by number from the repository inferred by `gh`.
- Preserve the existing Rust review, navigation, selection, note, theme, and
  tmux behavior.
- Keep every new review note local until the user explicitly confirms
  publication and chooses a review verdict.
- Submit all inline comments, one overall body, and one verdict atomically as a
  single GitHub review.
- Support Comment only, Approve, and Request changes.
- Detect a changed pull-request head before submission and publish nothing
  against a stale snapshot.
- Keep `gh` optional: existing Ramo commands must continue to work without it.
- Keep process execution shell-free and credential handling outside Ramo.

## Non-goals

- GitLab or Bitbucket support in v1.
- A native HTTP client, GitHub SDK, token store, OAuth flow, or added TLS
  dependency.
- Importing, displaying, replying to, resolving, or synchronizing existing
  GitHub review threads.
- Creating GitHub draft reviews incrementally while Ramo is open.
- Checking out the pull request, changing branches, fetching refs, or mutating
  the working tree.
- Watching or automatically refreshing a pull request while it is being
  reviewed.
- Publishing one API request per note.
- Supporting selections that GitHub cannot represent as one contiguous
  comment on one diff side.

## Command Contract

The new review input is:

```text
ramo pr <NUMBER>
```

`NUMBER` is a positive GitHub pull request number. The command must be invoked
inside a repository that `gh` can resolve. URLs, branch names, GitLab merge
request identifiers, and Bitbucket pull request identifiers are excluded from
v1.

Before entering the TUI, Ramo verifies:

1. `gh` can be executed.
2. `gh` has an authenticated account for the current repository.
3. The repository can be identified.
4. The pull request exists and is readable.
5. The pull request diff parses as a Ramo changeset.

Failures name the failed operation and include the bounded stderr returned by
`gh`. Missing and unauthenticated helpers provide direct remediation such as
installing `gh` or running `gh auth login`.

## Architecture

### GitHub CLI boundary

A focused `GithubCli` module owns all `gh` interaction. It depends on the
existing literal-argument `CommandExecutor`, accepts structured requests, and
returns typed values. It never builds a shell command.

The module exposes three conceptual operations:

- `resolve_pr(number)` returns repository identity, PR metadata, current viewer
  identity, and head commit SHA.
- `load_diff(number)` returns a color-free unified diff.
- `submit_review(request)` sends one JSON review document through `gh api`
  using stdin.

The expected helper operations are equivalent to:

```text
gh repo view --json nameWithOwner,url
gh pr view 123 --json number,title,url,author,baseRefName,headRefName,headRefOid
gh pr diff 123 --color=never
gh api user --jq .login
gh api --method POST repos/OWNER/REPO/pulls/123/reviews --input -
```

Exact flags are validated against the installed `gh` behavior during
implementation. Argument and stdin construction remain covered by injected
executor tests.

### Pull-request review context

The input loader produces the normal `Changeset` plus an immutable
`PullRequestReviewContext` containing:

- repository owner/name;
- pull request number, title, and URL;
- base and head branch names;
- captured head commit SHA;
- pull request author login;
- current viewer login.

The review controller remains provider-neutral. The application holds the PR
context and uses it only for PR-specific chrome, note targeting constraints,
quit flow, and publication.

The TUI identifies the session as `GitHub PR #123`, shows the title, and shows
`base ← head`. The diff itself uses the existing parser and renderer.

### Static snapshot

The loaded pull request is a frozen snapshot. PR mode does not watch or mutate
the checkout. Manual reload is disabled with a dismissible explanation telling
the user to reopen the PR. This prevents local note targets from silently
moving underneath the captured head SHA.

Opening a file in the local editor and expanding unchanged source context are
also disabled in PR mode v1 with dismissible explanations. The local checkout
may not contain the captured PR revision, so using it as source would show or
edit the wrong content.

Immediately before submission, Ramo resolves the PR metadata again. If
`headRefOid` differs from the captured SHA, Ramo sends no review and displays a
dismissible stale-review error. The user must reopen `ramo pr 123`.

## GitHub Inline Comment Targets

GitHub review comments require one file path, one diff side, and one
contiguous line or range. Ramo derives that publish target when the note is
created:

- In split view, the focused review side determines `LEFT` or `RIGHT`.
- In stack view, an addition targets `RIGHT`, a deletion targets `LEFT`, and
  unchanged context targets `RIGHT`.
- A visual selection is projected onto the chosen side.
- Single-line comments use `line` and `side`.
- Multiline comments use `start_line`, `start_side`, `line`, and `side`.

The projected side must contain at least one line and its line numbers must be
contiguous. If a selection crosses sides or becomes discontinuous on the
chosen side, Ramo does not open an ambiguous draft. It displays a dismissible
message asking the user to split the selection.

The draft header shows the provider target, including side and range, before
the user writes the comment. This makes the eventual GitHub placement visible
during review.

Local note rendering and Markdown annotations remain provider-neutral.
PR-specific target data is retained alongside the local note so submission
does not need to reinterpret the final screen geometry.

## Submission Interaction

Pressing `q` in PR mode starts a local state machine. It does not invoke `gh`
until the final verdict is selected.

### Step 1: publication confirmation

With inline notes:

```text
Publish 4 comments to GitHub PR #123?
y publish   n/Esc keep reviewing   d discard and quit
```

With no inline notes:

```text
Submit a review to GitHub PR #123 with no inline comments?
y continue   n/Esc keep reviewing   d discard and quit
```

`n` and Escape dismiss the prompt and return to the review with all drafts
intact. `d` is the only path that abandons the unpublished review and exits.
Discard remains explicit so an accidental negative response cannot lose work.

### Step 2: verdict and overall body

After `y`, Ramo presents:

```text
c Comment only   a Approve   r Request changes
o Edit overall comment   Esc keep reviewing
```

The initial overall body is generated automatically:

```text
Review submitted from Ramo with 4 inline comments.
```

For zero comments it is:

```text
Review submitted from Ramo.
```

`o` opens a multiline overall-comment editor. Saving returns to the verdict
screen; canceling the editor preserves the prior body. The user may revisit the
editor before choosing a verdict.

The verdict maps to GitHub's review events:

- Comment only → `COMMENT`
- Approve → `APPROVE`
- Request changes → `REQUEST_CHANGES`

When the viewer is the pull request author, Ramo offers only Comment only
because GitHub rejects self-review verdicts.

Selecting a verdict triggers the fresh-head check and then one review
submission. The JSON document contains:

- captured `commit_id`;
- overall `body`;
- selected `event`;
- every inline `comments` entry.

Successful submission shows confirmation and continues through Ramo's normal
quit/preferences lifecycle. A failed head check or failed `gh api` call leaves
the review open with every inline note and the overall body unchanged.

## Dismissible Messages

Nonfatal validation and GitHub failures use a modal message owned by a
dedicated input state:

- Enter or Escape dismisses the message.
- Dismissal returns to the prior review or submission screen.
- Draft notes, the overall body, selection, and verdict availability remain
  unchanged.
- The message cannot trap navigation input or quit Ramo.

Short success and informational feedback may remain bounded status toasts.
Errors that prevent initial PR loading are ordinary CLI errors because the TUI
has not started.

## Error Handling

The following failures are distinct and actionable:

- `gh` executable missing;
- unauthenticated `gh`;
- current directory not resolvable to a GitHub repository;
- pull request absent or inaccessible;
- metadata JSON malformed or missing required fields;
- empty or malformed PR diff;
- invalid or unpublishable note range;
- current PR head differs from the captured head;
- reviewer lacks permission to submit the selected verdict;
- GitHub rejects the review payload;
- helper process spawn, timeout, nonzero exit, or malformed output.

Helper output is bounded before entering errors or UI messages. Secrets and
environment variables are never copied into diagnostics. Submission JSON is
passed through stdin, not command-line arguments.

## Provider Boundary

The v1 command is GitHub-specific, but PR review state and submission types use
provider-neutral concepts where they are already stable:

- remote review identity;
- captured revision;
- inline comment target;
- review verdict;
- overall review body;
- review submission result.

`GithubCli` performs the provider translation. GitLab and Bitbucket can later
implement separate adapters without changing the review controller or making
the GitHub CLI a dependency of unrelated commands. No generic provider plugin
system is introduced in v1.

## Testing

Tests do not publish to a real repository. They use the injected command
executor and PTY fixtures.

### Command and loader tests

- Missing `gh`, auth failure, repository failure, and inaccessible PR produce
  distinct bounded errors.
- Metadata and diff calls use exact literal argv and never a shell.
- Valid metadata plus unified diff becomes the expected changeset and immutable
  PR context.
- Malformed metadata, missing fields, empty output, and malformed patches fail
  before terminal entry.

### Target mapping tests

- Split left/right focus maps to GitHub `LEFT`/`RIGHT`.
- Stack additions, deletions, and context use the documented sides.
- Forward and reverse visual selections produce identical ranges.
- Contiguous multiline selections produce correct start/end fields.
- Cross-side, empty, cross-file, and discontinuous selections are rejected
  without creating a draft.
- The visible draft location matches the eventual submission target.

### Submission tests

- `n` and Escape return to review without invoking `gh`.
- `d` discards and quits without invoking `gh`.
- Comment, Approve, and Request changes produce exact event values.
- The generated overall body uses the correct comment count.
- Editing and canceling the overall body preserve the documented value.
- Self-authored PRs expose only Comment.
- Zero-inline-comment reviews are valid.
- Fresh head SHA submits exactly one JSON document through stdin.
- Changed head SHA submits nothing and preserves all drafts.
- API failures are dismissible and preserve inline and overall comments.

### PTY tests

- `ramo pr 123` renders PR identity and the parsed diff using a fake `gh`.
- Visual note creation shows its GitHub side/range.
- The two-step quit/verdict flow owns its documented keys.
- The overall editor returns to the verdict screen.
- Validation and API error dialogs dismiss with Enter and Escape.
- Successful submission exits only after the fake helper accepts the payload.

### Regression gates

- Existing review inputs work when `gh` is absent.
- Existing pager, watch, local-note, Markdown-output, tmux, and session tests
  remain unchanged.
- Strict clippy, formatting, all-target tests, and a locked release build pass.

## Acceptance Criteria

The feature is complete when:

1. An authenticated user inside a GitHub repository can run `ramo pr 123` and
   review the PR diff without changing the checkout.
2. New Ramo notes visibly map to valid GitHub inline targets.
3. Quit confirmation cannot accidentally publish or discard feedback.
4. The user can submit Comment, Approve, or Request changes with a generated or
   edited overall body.
5. GitHub receives exactly one review containing all comments.
6. A changed PR head or failed submission publishes nothing and loses no work.
7. Every nonfatal in-TUI error is dismissible with Enter or Escape.
8. Ramo remains a single Rust binary and requires `gh` only for `ramo pr`.
