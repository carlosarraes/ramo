# GitHub Pull Request Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `ramo pr <NUMBER>` so an authenticated GitHub user can review a frozen pull-request diff in Ramo and publish newly-created inline comments, an editable overall body, and a Comment/Approve/Request-changes verdict as one GitHub review.

**Architecture:** Add a provider-neutral remote-review model and a focused `GithubCli` adapter over Ramo's literal-argv `CommandExecutor`. A dedicated pull-request loader converts GitHub metadata plus a color-free unified diff into the existing `Changeset` and an immutable remote-review context. `ReviewController` continues to own notes and geometry, but exposes a strict, provider-neutral one-side line projection for publishable notes. `App` owns the PR quit/publish state machine and a boxed remote-review service; the runtime only wires the concrete GitHub service into PR sessions and suppresses local Markdown handoff after publish/discard.

**Tech Stack:** Rust 2024, Clap, Serde/serde_json, Ratatui, Crossterm, the existing diff parser, the existing process executor, `gh`, Cargo integration tests, and Unix portable-PTY tests.

## Global Constraints

- Preserve the single native Rust binary. Do not add JavaScript, TypeScript, a browser, a GitHub SDK, an HTTP client, token storage, or a TLS dependency.
- `gh` is optional and is invoked only for `ramo pr`; every existing command must work when `gh` is absent.
- Never invoke a shell. Every helper call is a literal `CommandRequest`, and review JSON is supplied through stdin.
- Do not fetch, checkout, switch, or mutate branches, refs, the index, or the working tree.
- A PR review is a frozen snapshot. Disable watch, manual reload, local editor opening, and unchanged-context expansion in PR mode.
- Do not call `gh api` until the user has confirmed publication and selected a verdict.
- Before submission, compare a freshly fetched `headRefOid` with the captured revision. A mismatch submits nothing and preserves all local state.
- Submit exactly one GitHub review document containing the overall body, verdict event, and every new inline comment.
- Existing GitHub threads are out of scope. Only human notes created in the current Ramo session are submitted.
- Every PR note must resolve to one file, one side, and a nonempty contiguous inclusive line range. Reject ambiguous selections before opening the note editor.
- Initial load failures remain CLI errors. In-TUI validation, stale-head, and submission failures use a modal dismissed by Enter or Escape and return to the prior state.
- `n`/Escape at the publication prompt returns to the review with drafts intact. Only `d` discards and quits.
- A self-authored PR offers Comment only.
- Keep all helper diagnostics bounded and terminal-control sanitized.
- Use test doubles and fake `gh` executables. Automated tests must never publish to a real repository.
- Follow red-green-refactor for every task and commit after each independently passing slice.

---

### Task 1: Add the typed PR command and provider-neutral review model

**Files:**

- Create: `src/remote_review.rs`
- Modify: `src/lib.rs`
- Modify: `src/cli/args.rs`
- Modify: `src/cli/normalize.rs`
- Modify: `src/core/input.rs`
- Modify: `src/session/registration.rs`
- Modify: `tests/cli_parse.rs`
- Modify: `tests/runtime_resolution.rs`
- Test: `tests/remote_review_model.rs`

**Boundary contract:**

- Consumes: `ramo pr <positive integer>` plus ordinary `ReviewFlags`.
- Produces: `ReviewInput::PullRequest`, `InputKind::PullRequest`, provider-neutral context/target/verdict/request types, and session input kind `pr`.
- Does not execute `gh` or enter the terminal.

- [ ] **Step 1: Write the failing CLI and model tests**

Add these CLI cases to `tests/cli_parse.rs`:

```rust
#[test]
fn pr_accepts_a_positive_number_and_review_flags() {
    let invocation = parse_from(
        ["ramo", "pr", "123", "--mode", "split", "--theme", "tokyo-night"],
        true,
    )
    .unwrap();
    assert!(matches!(
        invocation.action,
        Action::Review(ReviewInput::PullRequest { number: 123, options })
            if options.mode == Some(LayoutMode::Split)
                && options.theme.as_deref() == Some("tokyo-night")
                && options.watch == Some(false)
    ));
}

#[test]
fn pr_rejects_zero_and_non_numeric_identifiers() {
    for value in ["0", "abc", "owner/repo#12", "https://github.com/o/r/pull/12"] {
        assert!(parse_from(["ramo", "pr", value], true).is_err(), "{value}");
    }
}
```

Create `tests/remote_review_model.rs` and assert:

```rust
use ramo::remote_review::{
    InlineCommentTarget, RemoteLineSide, RemoteReviewComment, RemoteReviewRequest,
    ReviewVerdict,
};

#[test]
fn verdicts_have_stable_provider_neutral_values() {
    assert_eq!(ReviewVerdict::Comment.event_name(), "COMMENT");
    assert_eq!(ReviewVerdict::Approve.event_name(), "APPROVE");
    assert_eq!(ReviewVerdict::RequestChanges.event_name(), "REQUEST_CHANGES");
}

#[test]
fn inline_targets_are_inclusive_and_one_sided() {
    let target = InlineCommentTarget {
        path: "src/lib.rs".into(),
        side: RemoteLineSide::Right,
        start_line: 42,
        end_line: 44,
    };
    assert_eq!(target.range(), 42..=44);
    let comment = RemoteReviewComment {
        target,
        body: "Please extract this branch.".into(),
    };
    let request = RemoteReviewRequest {
        commit_id: "abc123".into(),
        body: "Review submitted from Ramo with 1 inline comment.".into(),
        verdict: ReviewVerdict::Comment,
        comments: vec![comment],
    };
    assert_eq!(request.comments.len(), 1);
}
```

- [ ] **Step 2: Run the focused tests and verify red**

Run:

```bash
cargo test --test cli_parse --test remote_review_model
```

Expected: compilation fails because the PR input and remote-review types do not exist.

- [ ] **Step 3: Add the command and input variants**

Add this exact Clap argument shape in `src/cli/args.rs`:

```rust
#[derive(Debug, Args)]
pub struct PrArgs {
    #[command(flatten)]
    pub review: ReviewFlags,
    #[arg(value_name = "NUMBER", value_parser = clap::value_parser!(u64).range(1..))]
    pub number: u64,
}
```

Add `Command::Pr(PrArgs)` with help text “Review and publish feedback on a
GitHub pull request.” Normalize it through a dedicated helper so
repository/global `watch = true` cannot turn a frozen PR into a reloadable
review:

```rust
fn normalize_pr(args: PrArgs) -> ReviewInput {
    let mut options = common_options(args.review, false, None);
    options.watch = Some(false);
    ReviewInput::PullRequest {
        number: args.number,
        options,
    }
}
```

Add `InputKind::PullRequest` and:

```rust
ReviewInput::PullRequest {
    number: u64,
    options: CommonOptions,
}
```

Update every exhaustive `ReviewInput`/`InputKind` match, including
`ReviewInput::kind`, `ReviewInput::options`, `ReviewLoader` unsupported-input
handling, configuration resolution, session registration, and test fixtures.
Map the session label to `"pr"` and map the PR configuration layer to
`config.diff` so existing diff view preferences apply. Do not add `--watch` to
`PrArgs`; normalized PR options must carry `watch: Some(false)`.

- [ ] **Step 4: Add the provider-neutral model**

Create `src/remote_review.rs` with these public types:

```rust
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteLineSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineCommentTarget {
    pub path: String,
    pub side: RemoteLineSide,
    pub start_line: u32,
    pub end_line: u32,
}

impl InlineCommentTarget {
    pub fn range(&self) -> RangeInclusive<u32> {
        self.start_line..=self.end_line
    }

    pub fn display_label(&self) -> String {
        let side = match self.side {
            RemoteLineSide::Left => "LEFT",
            RemoteLineSide::Right => "RIGHT",
        };
        if self.start_line == self.end_line {
            format!("{} {side}:{}", self.path, self.end_line)
        } else {
            format!(
                "{} {side}:{}-{}",
                self.path, self.start_line, self.end_line
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}

impl ReviewVerdict {
    pub const fn event_name(self) -> &'static str {
        match self {
            Self::Comment => "COMMENT",
            Self::Approve => "APPROVE",
            Self::RequestChanges => "REQUEST_CHANGES",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReviewComment {
    pub target: InlineCommentTarget,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReviewRequest {
    pub commit_id: String,
    pub body: String,
    pub verdict: ReviewVerdict,
    pub comments: Vec<RemoteReviewComment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReviewError {
    pub message: String,
}

impl std::fmt::Display for RemoteReviewError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestReviewContext {
    pub repository: String,
    pub repository_url: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub captured_revision: String,
    pub author_login: String,
    pub viewer_login: String,
}

impl PullRequestReviewContext {
    pub fn is_self_authored(&self) -> bool {
        self.author_login.eq_ignore_ascii_case(&self.viewer_login)
    }

    pub fn status_label(&self) -> String {
        format!(
            "GitHub PR #{} · {} · {} ← {}",
            self.number, self.title, self.base_ref, self.head_ref
        )
    }
}

pub trait RemoteReviewPublisher {
    fn current_revision(
        &mut self,
        context: &PullRequestReviewContext,
    ) -> Result<String, RemoteReviewError>;

    fn submit_review(
        &mut self,
        context: &PullRequestReviewContext,
        request: &RemoteReviewRequest,
    ) -> Result<(), RemoteReviewError>;
}
```

Export the module from `src/lib.rs`.

- [ ] **Step 5: Run focused and regression tests**

Run:

```bash
cargo test --test cli_parse --test remote_review_model --test runtime_resolution
```

Expected: PASS, and no helper process has been introduced.

- [ ] **Step 6: Commit**

```bash
git add src/remote_review.rs src/lib.rs src/cli/args.rs src/cli/normalize.rs src/core/input.rs src/session/registration.rs tests/cli_parse.rs tests/runtime_resolution.rs tests/remote_review_model.rs
git commit -m "feat: add pull request review input"
```

---

### Task 2: Make the process boundary bounded, timed, and reusable by `GithubCli`

**Files:**

- Modify: `src/process/command.rs`
- Modify: `src/process/editor.rs`
- Modify: `src/tmux.rs`
- Test: `tests/process_command.rs`
- Modify: `tests/editor.rs`

**Boundary contract:**

- Consumes: literal argv, optional stdin, inherited/captured stdio, and explicit capture limits/timeout.
- Produces: a result that reports exit status, bounded stdout/stderr, truncation, and timeout without deadlocking on full pipes.
- Preserves all existing editor and tmux behavior.

- [ ] **Step 1: Write failing bounded-process tests**

Create `tests/process_command.rs` with Unix-gated tests that run the current test binary as a child fixture, or a literal `sh -c` only inside the test fixture, and prove:

```rust
#[test]
fn captured_streams_are_drained_but_retained_only_to_the_requested_limit() {
    // Child emits 128 KiB to both stdout and stderr.
    // Assert each returned Vec is exactly 1024 bytes and both truncation flags are true.
}

#[test]
fn a_timed_out_child_is_killed_and_reported() {
    // Use a 25 ms timeout around a child that remains alive for at least one second.
    // Assert timed_out is true and the test completes within one second.
}

#[test]
fn stdin_reaches_the_child_without_appearing_in_argv() {
    // Send a sentinel through stdin, echo it from the fixture, and assert argv omits it.
}
```

The fixture may use `std::env::current_exe()` plus an environment variable so production code is still tested without depending on shell parsing.

- [ ] **Step 2: Run the focused test and verify red**

Run:

```bash
cargo test --test process_command -- --nocapture
```

Expected: compilation fails because request limits and result status fields do not exist.

- [ ] **Step 3: Extend the command request/result**

Use this model in `src/process/command.rs`:

```rust
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureLimits {
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub timeout: Duration,
}

impl CaptureLimits {
    pub const fn new(
        stdout_bytes: usize,
        stderr_bytes: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            stdout_bytes,
            stderr_bytes,
            timeout,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    pub argv: Vec<OsString>,
    pub stdin: Option<Vec<u8>>,
    pub inherit_stdio: bool,
    pub limits: Option<CaptureLimits>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
}
```

For captured execution:

1. Spawn the child with piped stdout/stderr.
2. Start one reader thread per pipe. Each thread drains to EOF but retains only the configured number of bytes.
3. Write optional stdin and close the pipe.
4. Poll `try_wait` until completion or deadline using a maximum 10 ms interval.
5. On timeout, kill and wait for the child.
6. Join both readers and return their bounded buffers plus flags.

For `limits: None`, preserve current behavior and result semantics. For `inherit_stdio`, reject `limits: Some(_)` as invalid input because there are no captured streams to bound. Ensure a writer or reader failure kills/reaps the child before returning.

- [ ] **Step 4: Update existing literal request sites**

Set `limits: None` in `src/process/editor.rs` and every `src/tmux.rs` request. Update fake `CommandResult` values in tests with false truncation/timeout flags.

- [ ] **Step 5: Run focused regressions**

Run:

```bash
cargo test --test process_command --test editor --test pty_tmux
```

Expected: PASS, including high-volume dual-stream capture without a deadlock.

- [ ] **Step 6: Commit**

```bash
git add src/process/command.rs src/process/editor.rs src/tmux.rs tests/process_command.rs tests/editor.rs
git commit -m "refactor: bound helper process execution"
```

---

### Task 3: Implement the GitHub CLI adapter

**Files:**

- Create: `src/github/mod.rs`
- Create: `src/github/cli.rs`
- Create: `src/github/model.rs`
- Modify: `src/lib.rs`
- Test: `tests/github_cli.rs`

**Boundary contract:**

- Consumes: a PR number, a `CommandExecutor`, and a provider-neutral review request.
- Produces: typed repository/PR/viewer metadata, a color-free unified diff, a refreshed revision, or a submitted review.
- Owns every `gh` argv, JSON schema, helper timeout, output limit, error classification, and GitHub payload translation.

- [ ] **Step 1: Write a scripted fake executor and failing adapter tests**

In `tests/github_cli.rs`, implement a queue-backed `FakeExecutor` that records every `CommandRequest` and returns scripted `CommandResult`/`io::Error` values. Cover:

- missing executable (`io::ErrorKind::NotFound`);
- viewer/auth failure;
- repository resolution failure;
- inaccessible PR;
- malformed repo/PR JSON;
- a missing required JSON field;
- oversized/truncated metadata;
- exact diff argv;
- refreshed head argv;
- exact single-line and multiline review payloads;
- bounded, sanitized stderr;
- timeout classification.

The happy-path assertions must expect these literal calls in order:

```text
gh api user --jq .login
gh repo view --json nameWithOwner,url
gh pr view 123 --json number,title,url,author,baseRefName,headRefName,headRefOid
gh pr diff 123 --color=never
```

Refresh must be:

```text
gh pr view 123 --json headRefOid --jq .headRefOid
```

Submission must be:

```text
gh api --method POST repos/OWNER/REPO/pulls/123/reviews --input -
```

Assert that the JSON document exists only in `request.stdin` and never in `argv`.

- [ ] **Step 2: Run the focused test and verify red**

Run:

```bash
cargo test --test github_cli
```

Expected: compilation fails because the GitHub adapter does not exist.

- [ ] **Step 3: Define the loading service and typed errors**

Expose the GitHub-specific loading service from `src/github/mod.rs`:

```rust
pub trait GithubPullRequestSource {
    fn resolve_pr(
        &mut self,
        number: u64,
    ) -> Result<crate::remote_review::PullRequestReviewContext, GithubError>;

    fn load_diff(&mut self, number: u64) -> Result<String, GithubError>;
}
```

`GithubCli<E>` also implements
`crate::remote_review::RemoteReviewPublisher`. That implementation maps each
typed `GithubError` to a sanitized `RemoteReviewError` for the app, so the
application state machine has no GitHub-specific service or error type.

Define `GithubOperation::{Authenticate, ResolveRepository, ResolvePullRequest, LoadDiff, RefreshPullRequest, SubmitReview}` and `GithubError::{MissingCli, TimedOut { operation }, Truncated { operation }, InvalidUtf8 { operation }, InvalidJson { operation, detail }, Failed { operation, code, stderr }, Io { operation, source }}`. `Display` must name the operation and provide direct remediation:

- Missing: install GitHub CLI.
- Authentication: run `gh auth login`.
- Repository: run inside a GitHub repository or configure a GitHub remote.
- PR lookup: verify the number and access.
- Submission: preserve GitHub's bounded response.

Sanitize terminal control characters with `sanitize_terminal_text` before placing helper text in an error.

- [ ] **Step 4: Implement `GithubCli<E>`**

Use:

```rust
pub struct GithubCli<E> {
    executor: E,
}

impl<E> GithubCli<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }
}
```

Use 64 KiB stdout/8 KiB stderr and a 10-second timeout for metadata/API calls. Permit up to 32 MiB stdout/8 KiB stderr and a 30-second timeout for `gh pr diff`; return a load error if the diff is truncated. Require successful exit code zero, non-truncated required output, valid UTF-8, and nonempty required strings.

Deserialize private raw structs:

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRepository {
    name_with_owner: String,
    url: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPullRequest {
    number: u64,
    title: String,
    url: String,
    author: RawAuthor,
    base_ref_name: String,
    head_ref_name: String,
    head_ref_oid: String,
}

#[derive(serde::Deserialize)]
struct RawAuthor {
    login: String,
}
```

Serialize private payload structs with `skip_serializing_if = "Option::is_none"` so single-line comments omit `start_line`/`start_side`, while multiline comments contain both:

```rust
#[derive(serde::Serialize)]
struct GithubReviewPayload<'a> {
    commit_id: &'a str,
    body: &'a str,
    event: &'a str,
    comments: Vec<GithubReviewComment<'a>>,
}

#[derive(serde::Serialize)]
struct GithubReviewComment<'a> {
    path: &'a str,
    body: &'a str,
    line: u32,
    side: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_side: Option<&'static str>,
}
```

Translate `RemoteLineSide::{Left,Right}` to `LEFT`/`RIGHT`. Translate verdicts through `ReviewVerdict::event_name()`.

- [ ] **Step 5: Validate the installed helper syntax without contacting a repository**

Run:

```bash
gh help api
gh pr view --help
gh pr diff --help
gh repo view --help
```

Confirm that `--input -`, the JSON fields, `--jq`, and `--color=never` are accepted. If the installed help contradicts a flag, update both implementation and exact-argv tests while preserving the approved semantics.

- [ ] **Step 6: Run the adapter tests**

Run:

```bash
cargo test --test github_cli
```

Expected: PASS, with no real GitHub calls.

- [ ] **Step 7: Commit**

```bash
git add src/github src/lib.rs tests/github_cli.rs
git commit -m "feat: add github cli review adapter"
```

---

### Task 4: Load a frozen PR diff through the existing changeset pipeline

**Files:**

- Create: `src/input/pull_request.rs`
- Modify: `src/input/mod.rs`
- Modify: `src/error.rs`
- Modify: `src/runtime.rs`
- Test: `tests/pull_request_loading.rs`
- Modify: `tests/input_loading.rs`

**Boundary contract:**

- Consumes: `ReviewInput::PullRequest`, a `GithubPullRequestSource`, and the
  existing config/cwd/stdin context.
- Produces: the normal `LoadedReview` with `ReloadPlan::None` plus immutable `PullRequestReviewContext`.
- Rejects empty/unparseable PR diffs before terminal entry.

- [ ] **Step 1: Write failing loader tests**

Create `tests/pull_request_loading.rs` with a
`FakeGithubPullRequestSource`. Cover:

```rust
#[test]
fn valid_metadata_and_diff_become_a_frozen_review() {
    // Assert source_label == "GitHub PR #123".
    // Assert title is the PR title.
    // Assert files/line numbers come from parse_unified_diff.
    // Assert reload_plan == ReloadPlan::None.
    // Assert every context field is preserved.
}

#[test]
fn an_empty_or_unparseable_pr_diff_fails_before_terminal_entry() {
    // Empty text and ordinary prose must each return a distinct load error.
}

#[test]
fn agent_context_can_still_be_attached_to_a_pr_diff() {
    // Use a file-backed sidecar and assert the parsed PR files receive it.
}
```

Also prove `ReviewLoader::load_with_context` returns `UnsupportedInput(InputKind::PullRequest)` when called without a PR service, so ordinary loader APIs never instantiate `gh` implicitly.

- [ ] **Step 2: Run the focused test and verify red**

Run:

```bash
cargo test --test pull_request_loading --test input_loading
```

Expected: compilation fails because the dedicated loader result/method is absent.

- [ ] **Step 3: Add the dedicated loader result and method**

Add:

```rust
#[derive(Debug, Clone)]
pub struct LoadedPullRequest {
    pub review: LoadedReview,
    pub context: crate::remote_review::PullRequestReviewContext,
}
```

Add this method to `ReviewLoader`:

```rust
pub fn load_pull_request(
    &self,
    input: &ReviewInput,
    stdin: &mut dyn Read,
    context: &LoadContext<'_>,
    service: &mut dyn crate::github::GithubPullRequestSource,
) -> Result<LoadedPullRequest, LoadError>
```

The method must:

1. Match only `ReviewInput::PullRequest`.
2. Resolve optional agent context with `review_uses_stdin = false`.
3. Call `resolve_pr(number)`.
4. Call `load_diff(number)`.
5. Reject empty text as `LoadError::EmptyPullRequestDiff { number }`.
6. Parse with `parse_unified_diff`.
7. Reject no parsed files as `LoadError::InvalidPullRequestDiff { number }`.
8. Build `Changeset::new(format!("GitHub PR #{number}"), context.title.clone(), files)`.
9. Apply agent context.
10. Return `ReloadPlan::None`.

Add `LoadError::Github(GithubError)`, the two PR-diff variants, `Display`, `Error::source`, and `From<GithubError>`.

- [ ] **Step 4: Add a runtime loading seam**

Refactor `run_review` into:

```rust
fn run_review(input: ReviewInput, review_output: ReviewOutput) -> Result<ExitCode, AppError> {
    let mut github = crate::github::GithubCli::new(SystemCommandExecutor);
    run_review_with_github(input, review_output, &mut github)
}

fn run_review_with_github(
    input: ReviewInput,
    review_output: ReviewOutput,
    github: &mut dyn GithubPullRequestSource,
) -> Result<ExitCode, AppError>
```

At this task, branch only during loading:

```rust
let (loaded, pull_request_context) = if matches!(input, ReviewInput::PullRequest { .. }) {
    let loaded = ReviewLoader.load_pull_request(
        &input,
        &mut stdin_lock,
        &load_context,
        github,
    )?;
    (loaded.review, Some(loaded.context))
} else {
    let outcome = ReviewLoader.load_outcome_with_context(
        &input,
        &mut stdin_lock,
        &load_context,
    )?;
    // Preserve the existing pager/plain-text branch exactly.
    (loaded, None)
};
```

Do not attach the service to `App` yet. Keep the context available for Tasks 6–7. Do not construct `GithubCli` for non-PR inputs in the final refactor; this temporary concrete value is inert and performs no process call.

- [ ] **Step 5: Run focused regressions**

Run:

```bash
cargo test --test pull_request_loading --test input_loading --test agent_context --test pager
```

Expected: PASS. Fake service call order is resolve then diff.

- [ ] **Step 6: Commit**

```bash
git add src/input/pull_request.rs src/input/mod.rs src/error.rs src/runtime.rs tests/pull_request_loading.rs tests/input_loading.rs
git commit -m "feat: load frozen github pull requests"
```

---

### Task 5: Project selections into publishable one-side inline targets

**Files:**

- Modify: `src/notes/target.rs`
- Modify: `src/notes/mod.rs`
- Modify: `src/review/state.rs`
- Modify: `src/review/row.rs`
- Test: `tests/remote_review_targets.rs`
- Modify: `tests/notes_state.rs`
- Modify: `tests/ui_render.rs`

**Boundary contract:**

- Consumes: the selected semantic diff rows, effective split/stack layout, and focused side.
- Produces: one `InlineCommentTarget` retained on the human draft/note, or a precise validation error.
- Existing local notes continue to work with `remote_target: None`.

- [ ] **Step 1: Write the failing target matrix**

Create `tests/remote_review_targets.rs`. Build controllers from explicit patches rather than `DiffFile::for_test`, and cover all of:

- split-left context/deletion → `LEFT`;
- split-right context/addition → `RIGHT`;
- stack addition → `RIGHT`;
- stack deletion → `LEFT`;
- stack context → `RIGHT`;
- forward/reverse selections produce identical targets;
- a contiguous multiline selection produces the inclusive start/end;
- a split selection containing no line on the focused side fails;
- a stack selection mixing additions and deletions fails as cross-side;
- a cross-file selection fails rather than silently truncating;
- noncontiguous chosen-side line numbers fail;
- failure creates no draft;
- the draft and saved note retain the exact target;
- editing a note preserves the target.

Use assertions shaped like:

```rust
let target = controller
    .begin_remote_human_note(Some(selection), viewport)
    .unwrap()
    .expect("draft");
assert_eq!(
    target,
    InlineCommentTarget {
        path: "src/lib.rs".into(),
        side: RemoteLineSide::Right,
        start_line: 42,
        end_line: 44,
    }
);
assert_eq!(
    controller.human_note_draft().unwrap().remote_target,
    Some(target.clone())
);
```

- [ ] **Step 2: Run focused tests and verify red**

Run:

```bash
cargo test --test remote_review_targets --test notes_state
```

Expected: compilation fails because drafts/notes do not retain a remote target and strict projection does not exist.

- [ ] **Step 3: Retain optional targets on human notes**

Add:

```rust
pub struct HumanNote {
    pub id: String,
    pub target: NoteTarget,
    pub remote_target: Option<InlineCommentTarget>,
    pub body: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

pub struct HumanNoteDraft {
    pub id: String,
    pub target: NoteTarget,
    pub remote_target: Option<InlineCommentTarget>,
    pub body: String,
    pub editing: Option<String>,
}
```

All existing local-note constructors set `remote_target: None`. Saving and editing copy the field unchanged.

- [ ] **Step 4: Add strict semantic projection**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineTargetError {
    NoDiffLine,
    CrossFile,
    CrossSide,
    EmptySide(RemoteLineSide),
    Discontinuous(RemoteLineSide),
}
```

Implement:

```rust
pub fn begin_remote_human_note(
    &mut self,
    selection: Option<(SelectionPoint, SelectionPoint)>,
    viewport: Viewport,
) -> Result<Option<InlineCommentTarget>, InlineTargetError>
```

The implementation must inspect the concrete `ReviewRow`s in the selected geometry range:

- Require every selected diff row to have the same `file_id`.
- In effective split layout, choose `self.focused_side`; every row must expose a line on that side.
- In effective stack layout, map addition to right, deletion to left, and context to right; every row must map to the same side.
- Read old line numbers for left and new line numbers for right.
- Sort by visual order, remove only exact duplicate line numbers introduced by row pairing, reject an empty result, and require every adjacent pair to increase by exactly one.
- Resolve the current file path from the stable file ID.
- Create the ordinary `NoteTarget` from the projected old or new range.
- Store the same `InlineCommentTarget` on the draft.

Return user-facing `Display` text that asks the user to split cross-side or discontinuous selections.

- [ ] **Step 5: Make draft cards show the publish target**

In `src/review/row.rs`, use `remote_target.display_label()` as a human note/draft card's location when present. Continue using the current provider-neutral `NoteTarget` label for ordinary notes.

- [ ] **Step 6: Run target and rendering tests**

Run:

```bash
cargo test --test remote_review_targets --test notes_state --test ui_render --test annotations
```

Expected: PASS. Markdown export remains unchanged and does not expose GitHub-only JSON fields.

- [ ] **Step 7: Commit**

```bash
git add src/notes/target.rs src/notes/mod.rs src/review/state.rs src/review/row.rs tests/remote_review_targets.rs tests/notes_state.rs tests/ui_render.rs
git commit -m "feat: map review notes to remote diff lines"
```

---

### Task 6: Add the PR publication state machine and dismissible dialogs

**Files:**

- Modify: `src/app.rs`
- Modify: `src/ui/input.rs`
- Modify: `src/ui/dialogs.rs`
- Test: `tests/remote_review_flow.rs`
- Modify: `tests/ui_input.rs`
- Modify: `tests/ui_dialogs.rs`
- Modify: `tests/config_persistence.rs`

**Boundary contract:**

- Consumes: PR context, human notes with retained targets, key events, and a
  boxed provider-neutral `RemoteReviewPublisher`.
- Produces: confirmation → verdict/body → fresh-head check → one submission, or an explicit return/discard.
- Owns all in-TUI remote state and preserves it across dismissible failures.

- [ ] **Step 1: Write failing pure app-flow tests**

Create `tests/remote_review_flow.rs` with a fake service recording `current_revision` and `submit_review`. Use `App::new_with_services`, attach a remote review, call `handle_ui_key`, and assert:

1. `q` opens publication confirmation and performs zero service calls.
2. `n` and Escape return to normal review with notes intact.
3. `d` exits with `RemoteReviewOutcome::Discarded` and performs zero service calls.
4. `y` opens verdict selection with the generated singular/plural/zero body.
5. `o` opens the overall editor; Shift-Enter inserts a newline; save returns to verdict selection.
6. Canceling the editor restores the pre-edit body.
7. Comment/Approve/Request changes generate `COMMENT`/`APPROVE`/`REQUEST_CHANGES`.
8. Self-authored context accepts only Comment and renders no approve/request keys.
9. A fresh revision submits exactly once and then enters normal preference-save/quit handling.
10. A changed revision submits zero reviews, shows a message, and preserves notes/body.
11. Refresh/API errors show a message; Enter and Escape return to verdict selection with state intact.
12. A human note missing `remote_target` blocks submission rather than being silently dropped.
13. `r`, `e`, and `z` show dismissible PR-mode explanations and invoke no reload/editor/context source.
14. A rejected selection shows a dismissible message and opens no note editor.

- [ ] **Step 2: Add failing input/dialog tests**

Extend `tests/ui_input.rs` with exact mode ownership:

```rust
assert_eq!(
    map_key_event(key(KeyCode::Char('y')), InputMode::PublishPrompt, false),
    Some(AppAction::ConfirmPublish)
);
assert_eq!(
    map_key_event(key(KeyCode::Char('d')), InputMode::PublishPrompt, false),
    Some(AppAction::DiscardRemoteReview)
);
assert_eq!(
    map_key_event(key(KeyCode::Char('o')), InputMode::VerdictPrompt, false),
    Some(AppAction::EditOverallComment)
);
assert_eq!(
    map_key_event(key(KeyCode::Enter), InputMode::Message, false),
    Some(AppAction::DismissMessage)
);
```

Extend `tests/ui_dialogs.rs` to render publication, verdict, overall-editor, and message overlays on both normal and tiny terminals.

- [ ] **Step 3: Run focused tests and verify red**

Run:

```bash
cargo test --test remote_review_flow --test ui_input --test ui_dialogs
```

Expected: compilation fails because PR input modes and app state do not exist.

- [ ] **Step 4: Add state and service ownership**

Add these app-level types:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteReviewOutcome {
    Published,
    Discarded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteReturnState {
    Review,
    Verdict,
}

struct RemoteReviewSession {
    context: PullRequestReviewContext,
    service: Box<dyn RemoteReviewPublisher>,
    overall_body: String,
    overall_body_edited: bool,
    overall_edit_original: Option<String>,
    message_title: String,
    message_body: String,
    message_return: RemoteReturnState,
    outcome: Option<RemoteReviewOutcome>,
}

pub struct AppRunResult {
    pub annotations: Vec<Annotation>,
    pub remote_outcome: Option<RemoteReviewOutcome>,
}
```

Add:

```rust
pub fn attach_pull_request(
    &mut self,
    context: PullRequestReviewContext,
    service: Box<dyn RemoteReviewPublisher>,
)
```

The initial body is generated lazily from the current saved human-note count:

```rust
fn default_overall_body(count: usize) -> String {
    match count {
        0 => "Review submitted from Ramo.".into(),
        1 => "Review submitted from Ramo with 1 inline comment.".into(),
        count => format!("Review submitted from Ramo with {count} inline comments."),
    }
}
```

Change only `run_with_services`/the private loop to return `AppRunResult`. Keep `App::run` and `run_with_watch` source-compatible by returning `result.annotations`.

- [ ] **Step 5: Add dedicated input modes/actions**

Extend `InputMode` with:

```rust
PublishPrompt,
VerdictPrompt,
OverallComment,
Message,
```

Extend `AppAction` with:

```rust
ConfirmPublish,
KeepReviewing,
DiscardRemoteReview,
ChooseVerdict(ReviewVerdict),
EditOverallComment,
SaveOverallComment,
DismissMessage,
```

Mode key ownership:

- Publish: `y` confirms; `n`/Escape returns; `d` discards.
- Verdict: `c`, `a`, `r` choose verdict; `o` edits; Escape returns to review.
- Overall editor: ordinary characters insert, Backspace deletes, Shift-Enter inserts newline, Enter/Ctrl-S saves, Escape cancels and restores.
- Message: Enter/Escape dismiss.

Pager filtering must not apply to PR modes because `ramo pr` is never pager mode.

- [ ] **Step 6: Implement the quit and submission transitions**

Split current quit handling into:

```rust
fn request_quit(&mut self, viewport: Viewport) {
    if self.remote_review.is_some()
        && self.remote_review.as_ref().and_then(|session| session.outcome).is_none()
    {
        self.input_mode = InputMode::PublishPrompt;
        return;
    }
    self.request_local_quit(viewport);
}
```

On publish confirmation, regenerate the default body only if the user has not edited it. On verdict:

1. Reject Approve/Request changes for `is_self_authored`.
2. Convert every human note into `RemoteReviewComment`; fail if any target is absent.
3. Call `current_revision`.
4. Compare it to `captured_revision`.
5. On mismatch, open a message returning to Verdict.
6. Build `RemoteReviewRequest` with the captured revision.
7. Call `submit_review` once.
8. On failure, open a message returning to Verdict.
9. On success, set `Published` and call `request_local_quit` so existing preference persistence still runs.

On `d`, set `Discarded`, set normal input mode, and quit immediately. Do not export Markdown or ask to save view preferences on this explicit discard path.

- [ ] **Step 7: Integrate strict note creation and PR-disabled actions**

In `ReviewEffect::StartNote`, call `begin_remote_human_note` when a PR session is attached; otherwise preserve `begin_human_note`.

Intercept:

- `ReviewEffect::Reload`;
- `ReviewEffect::EditFile`;
- `AppAction::ToggleContext`.

In PR mode each opens `InputMode::Message` with:

- reload: “Pull request snapshots cannot reload. Reopen `ramo pr N`.”
- editor: “The local checkout may not match this pull request snapshot.”
- context: “Unchanged local source is unavailable for pull request snapshots.”

Use the same message state for `InlineTargetError`, stale revisions, and service failures.

- [ ] **Step 8: Render the PR status and dialogs**

When no filter, toast, or startup notice occupies the bottom status row, render `context.status_label()`.

Add dialog constructors and render branches:

```rust
DialogOverlay::publish(theme, number, count)
DialogOverlay::verdict(theme, self_authored, overall_body)
DialogOverlay::overall_comment(theme, edit_buffer)
DialogOverlay::message(theme, title, body)
```

The publication text and keys must exactly match the approved design. The verdict dialog must omit approve/request-changes text for self-authored PRs. The overall editor must show `Enter/Ctrl-S save`, `Shift+Enter newline`, and `Esc cancel`.

- [ ] **Step 9: Run state, input, dialog, and preference tests**

Run:

```bash
cargo test --test remote_review_flow --test ui_input --test ui_dialogs --test config_persistence --test notes_state
```

Expected: PASS, including state preservation after every error path.

- [ ] **Step 10: Commit**

```bash
git add src/app.rs src/ui/input.rs src/ui/dialogs.rs tests/remote_review_flow.rs tests/ui_input.rs tests/ui_dialogs.rs tests/config_persistence.rs
git commit -m "feat: add pull request publish flow"
```

---

### Task 7: Wire the GitHub service through runtime without affecting local reviews

**Files:**

- Modify: `src/runtime.rs`
- Modify: `src/app.rs`
- Modify: `src/session/registration.rs`
- Test: `tests/runtime_resolution.rs`
- Test: `tests/github_runtime.rs`
- Modify: `tests/workflow_contract.rs`

**Boundary contract:**

- Consumes: the concrete `GithubCli<SystemCommandExecutor>` only for PR input.
- Produces: an attached PR app session and disposition-aware shutdown.
- Leaves local Markdown export, watch runtime, editor base, pager, startup notices, and live session registration unchanged for non-PR reviews.

- [ ] **Step 1: Write failing runtime tests**

Expose a crate-visible/testable `run_review_with_services` seam whose GitHub argument is a boxed service factory or service instance. In `tests/github_runtime.rs`, prove:

- non-PR input never calls the GitHub fake;
- PR input loads through the fake and receives `ReloadPlan::None`;
- PR input rejects configured watch before terminal entry;
- published/discarded remote outcomes skip `finish_annotations`;
- local outcomes still use existing stdout/file/prompt behavior;
- editor/context/watch services are not constructed for PR mode;
- PR session descriptors use input kind `pr` and the PR title.

The runtime tests must not enter a real terminal. Extract the outcome-routing decision into:

```rust
pub fn should_finish_local_annotations(
    input: &ReviewInput,
    remote_outcome: Option<RemoteReviewOutcome>,
) -> bool {
    !matches!(input, ReviewInput::PullRequest { .. }) && remote_outcome.is_none()
}
```

Assert it is true only for ordinary local review completion.

- [ ] **Step 2: Run runtime tests and verify red**

Run:

```bash
cargo test --test github_runtime --test runtime_resolution --test workflow_contract
```

Expected: compilation fails because service ownership and disposition routing are incomplete.

- [ ] **Step 3: Move the concrete service exactly once**

Do not leave a borrowed service attached to `App`. Structure the PR branch so runtime owns a concrete value, uses it to load, then moves the same value into the app:

```rust
let mut github = GithubCli::new(SystemCommandExecutor);
let loaded_pr = ReviewLoader.load_pull_request(
    &input,
    &mut stdin_lock,
    &load_context,
    &mut github,
)?;
let loaded = loaded_pr.review;
let pull_request = Some((
    loaded_pr.context,
    Box::new(github) as Box<dyn RemoteReviewPublisher>,
));
```

For non-PR input, do not create a `GithubCli`. Use an enum or two small runtime branches instead of an `Option` whose generic types become difficult to infer.

After constructing `App`, call `attach_pull_request` when present. Do not create `WatchRuntime` for PR input. Continue registering the live session, but its reload request naturally returns the existing non-reloadable message.

- [ ] **Step 4: Route completion**

After terminal restoration:

```rust
let result = app_result?;
if should_finish_local_annotations(&input, result.remote_outcome) {
    finish_annotations(result.annotations, review_output)?;
}
```

A successfully published review and an explicit discard both exit successfully without the local `ramo-review.md` prompt. Initial load or terminal failures remain nonzero through `AppError`.

- [ ] **Step 5: Run runtime and local regression tests**

Run:

```bash
cargo test --test github_runtime --test runtime_resolution --test workflow_contract --test annotations --test pty_watch --test pty_pager
```

Expected: PASS. Existing local reviews continue to finish annotations exactly as before.

- [ ] **Step 6: Commit**

```bash
git add src/runtime.rs src/app.rs src/session/registration.rs tests/github_runtime.rs tests/runtime_resolution.rs tests/workflow_contract.rs
git commit -m "feat: wire github reviews into runtime"
```

---

### Task 8: Prove the end-to-end PR workflow in a PTY with a fake `gh`

**Files:**

- Create: `tests/pty_pr.rs`
- Modify: `tests/support/mod.rs`
- Create: `tests/fixtures/github-pr.patch`

**Boundary contract:**

- Consumes: the public `ramo pr 123` command, a fake executable named `gh` on `PATH`, and terminal keystrokes.
- Produces: observable PR identity/diff/note/prompt/verdict/error/exit behavior plus a captured review JSON document.

- [ ] **Step 1: Add a reusable fake-`gh` fixture**

Extend test support with a temp directory containing an executable `gh` script/program. It must:

- respond to `api user --jq .login`;
- respond to `repo view --json nameWithOwner,url`;
- respond to
  `pr view 123 --json number,title,url,author,baseRefName,headRefName,headRefOid`;
- respond to `pr diff 123 --color=never` with `tests/fixtures/github-pr.patch`;
- respond to the fresh-head `pr view` call;
- capture
  `api --method POST repos/OWNER/REPO/pulls/123/reviews --input -` stdin to a
  temp file;
- support environment-controlled auth failure, stale SHA, and submission failure;
- record argv lines separately from payload bytes.

Put this directory first on the child `PATH`. Never replace the parent process's real `gh`.

- [ ] **Step 2: Write the PTY scenarios**

Cover:

```rust
#[test]
fn pr_review_renders_identity_creates_a_targeted_note_and_publishes_once() {
    // Wait for "GitHub PR #123" and the fixture code.
    // Select a known RIGHT-side line, press c, type a note, press Enter.
    // Assert the draft/saved card shows "RIGHT:12".
    // Press q, assert publish prompt, press y, assert verdict/body prompt.
    // Press o, edit/save overall body, choose c.
    // Wait for clean exit and inspect the captured single JSON payload.
}

#[test]
fn publish_cancel_and_discard_do_not_call_github_api() {
    // q+n and q+Esc return to the review with the note visible.
    // q+d exits. Assert no review payload file was created.
}

#[test]
fn stale_and_api_errors_are_dismissible_without_losing_comments() {
    // Trigger each fake failure, assert the modal, dismiss with Enter/Escape,
    // and assert the saved note and verdict prompt are still present.
}

#[test]
fn self_authored_pr_offers_comment_only() {
    // Fake equal viewer/author; assert dialog omits Approve/Request changes.
}

#[test]
fn pr_only_actions_explain_the_frozen_snapshot() {
    // Press r, e, and z in turn and dismiss each message.
}
```

Also add initial-load command tests for missing fake `gh`, auth failure, missing PR, malformed metadata, and malformed diff. Assert nonzero exit, actionable stderr, and no alternate-screen escape before failure.

- [ ] **Step 3: Run PTY tests and verify red**

Run:

```bash
cargo test --test pty_pr -- --nocapture
```

Expected before completing the fixture/runtime details: at least one scenario fails with a missing dialog, wrong payload, or lifecycle mismatch.

- [ ] **Step 4: Make only integration corrections**

Correct terminal/key/render/runtime wiring revealed by PTY evidence. Do not weaken unit assertions or add sleeps; use the existing deadline/read-until helpers. Keep all child processes bounded and ensure the fake submission capture is flushed before Ramo exits.

- [ ] **Step 5: Run PTY and adjacent suites**

Run:

```bash
cargo test --test pty_pr --test pty_notes --test pty_ui --test pty_session -- --nocapture
```

Expected: PASS with no network access and exactly one captured submission in the happy path.

- [ ] **Step 6: Commit**

```bash
git add tests/pty_pr.rs tests/support/mod.rs tests/fixtures/github-pr.patch
git commit -m "test: cover github pr review workflow"
```

---

### Task 9: Document the public workflow and close regression evidence

**Files:**

- Modify: `README.md`
- Modify: `docs/parity/hunk.md`
- Modify: `src/cli/args.rs`
- Modify: `src/ui/dialogs.rs`
- Test: `tests/cli_contract.rs`
- Test: `tests/ui_dialogs.rs`

**Boundary contract:**

- Consumes: verified behavior from Tasks 1–8.
- Produces: user-facing installation/authentication, command, limitations, controls, and evidence.

- [ ] **Step 1: Add failing documentation contract assertions**

Extend `tests/cli_contract.rs` and `tests/ui_dialogs.rs` to require:

- top-level help lists `pr`;
- `ramo pr --help` shows `ramo pr <NUMBER>`;
- no PR help advertises `--watch`;
- controls explain the PR-specific quit flow;
- self-authored verdict help does not claim approve/request-changes availability.

- [ ] **Step 2: Run documentation contract tests and verify red**

Run:

```bash
cargo test --test cli_contract --test ui_dialogs
```

Expected: FAIL until help/dialog copy is updated.

- [ ] **Step 3: Update README**

Replace the primary piped GitHub example with:

```bash
# Review GitHub PR #123 and publish new comments as one GitHub review
ramo pr 123
```

Retain `gh pr diff 123 --color=never | ramo` under generic patch input as a
view-only alternative.

Add a “GitHub pull request reviews” section documenting:

- required installed/authenticated `gh` (`gh auth login`);
- run inside the target repository;
- frozen snapshot and pre-submit head check;
- only new Ramo notes are included;
- `q` → publish confirmation → verdict;
- `o` edits the overall body;
- `n`/Escape returns; `d` discards;
- Comment/Approve/Request changes and self-authored Comment-only behavior;
- no existing thread import, reply/resolution, watch, local editor, or unchanged-context expansion in v1;
- no checkout or working-tree mutation;
- GitLab/Bitbucket are not yet supported.

Update the controls table so `q` describes local quit for ordinary reviews and the publish flow for PR reviews.

- [ ] **Step 4: Update verified evidence**

Add a GitHub PR section to `docs/parity/hunk.md` with named implementation boundaries and test names from:

- `tests/github_cli.rs`;
- `tests/pull_request_loading.rs`;
- `tests/remote_review_targets.rs`;
- `tests/remote_review_flow.rs`;
- `tests/pty_pr.rs`.

Do not mark unimplemented GitHub thread synchronization, GitLab, or Bitbucket as parity.

- [ ] **Step 5: Run docs/help tests**

Run:

```bash
cargo test --test cli_contract --test ui_dialogs
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add README.md docs/parity/hunk.md src/cli/args.rs src/ui/dialogs.rs tests/cli_contract.rs tests/ui_dialogs.rs
git commit -m "docs: explain github pull request reviews"
```

---

### Task 10: Simplify, audit, and run release-grade verification

**Files:**

- Review: all files changed in Tasks 1–9
- Modify only where simplification or verification finds a concrete issue

**Boundary contract:**

- Consumes: the complete implementation.
- Produces: one coherent, formatted, warning-free, tested Rust feature ready for a separate release decision.

- [ ] **Step 1: Use the simplify skill**

Run the `simplify` skill over the changed Rust code. Focus on:

- duplicate PR-mode checks in `App`;
- duplicated `gh` execution/error decoding;
- state transitions that can be represented by one enum;
- borrow workarounds that obscure ownership;
- unnecessarily public GitHub raw JSON types;
- test helpers duplicated across GitHub unit/PTY suites.

Do not change behavior or collapse distinct error variants.

- [ ] **Step 2: Audit the approved design line by line**

Compare the implementation with:

```text
docs/superpowers/specs/2026-07-23-github-pr-review-design.md
```

Explicitly verify every goal, non-goal, command contract, side-mapping rule, prompt key, verdict, stale-head rule, self-review rule, error class, and acceptance criterion. Add a regression test for any uncovered approved behavior before changing code.

- [ ] **Step 3: Run formatting and focused lint**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: both exit zero with no warnings.

- [ ] **Step 4: Run the complete test suite**

Run:

```bash
cargo test --all-targets --all-features --locked
```

Expected: every unit, integration, PTY, and benchmark-compilation target passes.

- [ ] **Step 5: Build the release binary**

Run:

```bash
cargo build --release --locked
```

Expected: `target/release/ramo` is produced successfully and has no runtime dependency on `gh` until `ramo pr` is invoked.

- [ ] **Step 6: Inspect repository state and diff**

Run:

```bash
git status --short
git diff --check
git log --oneline --decorate -12
```

Expected: no whitespace errors, no accidental fixture payloads or credentials, and only intentional source/docs/test changes.

- [ ] **Step 7: Commit any final verified cleanup**

If Step 1–6 changed files, inspect `git status --short`, stage each cleanup
file by its exact listed path, and run:

```bash
git commit -m "refactor: simplify github review workflow"
```

If no files changed, do not create an empty commit.

- [ ] **Step 8: Stop before release**

Report the passing commands, commits, and any intentionally deferred non-goals. Do not bump the version, tag, push, or publish a release unless the user separately requests it.
