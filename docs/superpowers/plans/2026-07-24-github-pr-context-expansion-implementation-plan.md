# GitHub Pull Request Context Expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `z` and collapsed-row clicks lazily expand unchanged source in `ramo pr <NUMBER>` while preserving the frozen snapshot and never mutating the local repository.

**Architecture:** Extend the diff source model with provider-neutral immutable remote blobs, attach base/head blob references while loading the PR patch, and inject a GitHub-specific bounded context loader into PR app sessions. The loader calls `gh api` only on the first expansion of a file, caches the result, and feeds the existing context geometry; app-level failures become dismissible PR modals.

**Tech Stack:** Rust 2024, the existing literal-argv `CommandExecutor`, `gh api`, Ratatui/Crossterm, Cargo integration tests, and Unix portable-PTY tests.

## Global Constraints

- Preserve the single native Rust binary; add no HTTP client, TLS stack, token storage, JavaScript, TypeScript, or runtime dependency.
- Do not fetch or create Git refs, check out branches, or mutate the index, worktree, or repository.
- Keep PR reviews frozen at the initially captured `baseRefOid` and `headRefOid`.
- Make no file-content API call during `ramo pr` startup.
- Load only the selected file on the first `z` or collapsed-row click and cache the result for the session.
- Invoke `gh` with literal argv only; never invoke a shell.
- Bound each source request to 8 MiB stdout, 8 KiB stderr, and 15 seconds.
- Preserve local context expansion behavior and keep all non-PR commands independent from `gh`.
- Keep watch, reload, and editor opening disabled in PR mode.
- Show lazy PR source failures in a modal dismissible with Enter or Escape without losing notes or review state.
- Automated tests must use scripted executors or fake `gh` binaries and must never contact GitHub.
- Use red-green-refactor for every behavior slice and commit each independently passing task.

---

### Task 1: Capture immutable base/head revisions and model remote blobs

**Files:**

- Modify: `src/diff/model.rs`
- Modify: `src/remote_review.rs`
- Modify: `src/github/mod.rs`
- Modify: `src/review/context.rs`
- Modify: `tests/github_cli.rs`
- Modify: `tests/remote_review_flow.rs`
- Modify: `tests/remote_review_model.rs`
- Modify: `tests/pull_request_loading.rs`
- Modify: `tests/context_expansion.rs`
- Modify: any remaining `PullRequestReviewContext` fixtures found by `rg`

**Interfaces:**

- Produces `SourceSpec::RemoteBlob { repository, revision, path }`.
- Produces `PullRequestReviewContext::base_revision`.
- Updates GitHub metadata resolution to require `baseRefOid`.
- Native source loading returns `SourceFailure::Unavailable` for remote blobs.

- [ ] **Step 1: Write failing model and metadata tests**

Add a model assertion to `tests/remote_review_model.rs`:

```rust
use ramo::diff::model::SourceSpec;

#[test]
fn remote_blob_sources_are_immutable_and_provider_neutral() {
    let source = SourceSpec::RemoteBlob {
        repository: "owner/repo".into(),
        revision: "base123".into(),
        path: "src/lib.rs".into(),
    };
    assert!(matches!(
        source,
        SourceSpec::RemoteBlob {
            repository,
            revision,
            path,
        } if repository == "owner/repo"
            && revision == "base123"
            && path == "src/lib.rs"
    ));
}
```

Update the expected context in `tests/github_cli.rs` to contain:

```rust
base_revision: "base123".into(),
captured_revision: "head123".into(),
```

Change the scripted PR JSON to:

```json
{
  "number": 123,
  "title": "Improve review flow",
  "url": "https://github.com/owner/repo/pull/123",
  "author": {"login": "author"},
  "baseRefName": "main",
  "baseRefOid": "base123",
  "headRefName": "feature",
  "headRefOid": "head123"
}
```

Assert the exact metadata argv includes:

```rust
[
    "gh",
    "pr",
    "view",
    "123",
    "--json",
    "number,title,url,author,baseRefName,baseRefOid,headRefName,headRefOid",
]
```

Add a malformed metadata case whose `baseRefOid` is empty and assert the error
contains `base revision`.

- [ ] **Step 2: Run the focused tests and verify red**

Run:

```bash
cargo test --test remote_review_model --test github_cli
```

Expected: compilation fails because `RemoteBlob` and `base_revision` do not
exist.

- [ ] **Step 3: Add the model fields**

Add this variant to `SourceSpec` in `src/diff/model.rs`:

```rust
RemoteBlob {
    repository: String,
    revision: String,
    path: String,
},
```

Add this field to `PullRequestReviewContext` in `src/remote_review.rs`:

```rust
pub base_revision: String,
```

Update every context fixture found by:

```bash
rg -n 'PullRequestReviewContext \\{' src tests
```

Use `base123` for the base revision and `head123` for the captured head
revision in every adapter/loader fixture.

- [ ] **Step 4: Parse and validate `baseRefOid`**

Add `base_ref_oid: String` to `RawPullRequest`. Include `baseRefOid` in
`GithubCli::pull_request`'s JSON field list, require it with:

```rust
require_field(
    GithubOperation::ResolvePullRequest,
    "base revision",
    &pull_request.base_ref_oid,
)?;
```

Populate:

```rust
base_revision: pull_request.base_ref_oid,
captured_revision: pull_request.head_ref_oid,
```

- [ ] **Step 5: Make the native loader reject remote blobs explicitly**

Extend `NativeContextSourceLoader::load` with this guard before its cache
lookup, so the VCS reader remains native-only:

```rust
if matches!(spec, SourceSpec::RemoteBlob { .. }) {
    return Err(SourceFailure::Unavailable);
}
```

Add to `tests/context_expansion.rs`:

```rust
#[test]
fn native_loader_never_attempts_remote_blobs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut loader = NativeContextSourceLoader::new(
        CountingRunner {
            calls: Arc::clone(&calls),
        },
        "git",
        1024,
    );
    let source = SourceSpec::RemoteBlob {
        repository: "owner/repo".into(),
        revision: "abc123".into(),
        path: "src/lib.rs".into(),
    };
    assert_eq!(loader.load(&source), Err(SourceFailure::Unavailable));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
```

- [ ] **Step 6: Run focused regressions**

Run:

```bash
cargo test --test remote_review_model --test github_cli --test context_expansion --test remote_review_flow --test pull_request_loading
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/diff/model.rs src/remote_review.rs src/github/mod.rs src/review/context.rs src/vcs/source.rs tests/github_cli.rs tests/remote_review_flow.rs tests/remote_review_model.rs tests/pull_request_loading.rs tests/context_expansion.rs
git commit -m "feat: model immutable pull request sources"
```

Only add `src/vcs/source.rs` if Step 5 changed it.

---

### Task 2: Attach remote old/new sources to every parsed PR file

**Files:**

- Modify: `src/input/pull_request.rs`
- Modify: `tests/pull_request_loading.rs`

**Interfaces:**

- Consumes `PullRequestReviewContext::{repository,base_revision,captured_revision}`.
- Produces old/new `SourceSpec::RemoteBlob` values on parsed non-binary files.
- Added files have no old source; deleted files have no new source.

- [ ] **Step 1: Write the failing source-mapping matrix**

Extend `tests/pull_request_loading.rs` with a single parseable diff containing:

```text
diff --git a/src/modified.rs b/src/modified.rs
--- a/src/modified.rs
+++ b/src/modified.rs
@@ -2 +2 @@
-old
+new
diff --git a/src/added.rs b/src/added.rs
new file mode 100644
--- /dev/null
+++ b/src/added.rs
@@ -0,0 +1 @@
+new
diff --git a/src/deleted.rs b/src/deleted.rs
deleted file mode 100644
--- a/src/deleted.rs
+++ /dev/null
@@ -1 +0,0 @@
-old
diff --git a/src/old.rs b/src/new.rs
similarity index 80%
rename from src/old.rs
rename to src/new.rs
--- a/src/old.rs
+++ b/src/new.rs
@@ -1 +1 @@
-old
+new
diff --git a/src/copied.rs b/src/copy.rs
similarity index 80%
copy from src/copied.rs
copy to src/copy.rs
--- a/src/copied.rs
+++ b/src/copy.rs
@@ -1 +1 @@
-old
+new
```

Use a helper:

```rust
fn remote(repository: &str, revision: &str, path: &str) -> SourceSpec {
    SourceSpec::RemoteBlob {
        repository: repository.into(),
        revision: revision.into(),
        path: path.into(),
    }
}
```

Assert:

```rust
assert_eq!(modified.old_source, remote("owner/repo", "base123", "src/modified.rs"));
assert_eq!(modified.new_source, remote("owner/repo", "head123", "src/modified.rs"));
assert_eq!(added.old_source, SourceSpec::None);
assert_eq!(added.new_source, remote("owner/repo", "head123", "src/added.rs"));
assert_eq!(deleted.old_source, remote("owner/repo", "base123", "src/deleted.rs"));
assert_eq!(deleted.new_source, SourceSpec::None);
assert_eq!(renamed.old_source, remote("owner/repo", "base123", "src/old.rs"));
assert_eq!(renamed.new_source, remote("owner/repo", "head123", "src/new.rs"));
assert_eq!(copied.old_source, remote("owner/repo", "base123", "src/copied.rs"));
assert_eq!(copied.new_source, remote("owner/repo", "head123", "src/copy.rs"));
```

Also add a binary diff and assert both sources remain `SourceSpec::None`.

- [ ] **Step 2: Run the focused test and verify red**

Run:

```bash
cargo test --test pull_request_loading valid_pr_files_receive_immutable_remote_sources
```

Expected: FAIL because parsed PR files still contain `SourceSpec::None`.

- [ ] **Step 3: Add one focused source-assignment helper**

In `src/input/pull_request.rs`, add:

```rust
fn attach_remote_sources(
    files: &mut [crate::diff::model::DiffFile],
    context: &crate::remote_review::PullRequestReviewContext,
) {
    use crate::diff::model::{FileChangeKind, SourceSpec};

    for file in files.iter_mut().filter(|file| !file.is_binary) {
        let old_path = file.previous_path.as_deref().unwrap_or(&file.path);
        file.old_source = if file.change_kind == FileChangeKind::Added {
            SourceSpec::None
        } else {
            SourceSpec::RemoteBlob {
                repository: context.repository.clone(),
                revision: context.base_revision.clone(),
                path: old_path.to_owned(),
            }
        };
        file.new_source = if file.change_kind == FileChangeKind::Deleted {
            SourceSpec::None
        } else {
            SourceSpec::RemoteBlob {
                repository: context.repository.clone(),
                revision: context.captured_revision.clone(),
                path: file.path.clone(),
            }
        };
    }
}
```

Make parsed files mutable, call the helper after validating the diff and before
constructing the `Changeset`.

- [ ] **Step 4: Run loader and parser regressions**

Run:

```bash
cargo test --test pull_request_loading --test input_loading --test remote_review_targets
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/input/pull_request.rs tests/pull_request_loading.rs
git commit -m "feat: attach remote sources to pull request files"
```

---

### Task 3: Implement the bounded lazy GitHub context loader

**Files:**

- Modify: `src/github/mod.rs`
- Create: `tests/github_context.rs`

**Interfaces:**

- Produces `GithubContextSourceLoader<E: CommandExecutor>`.
- Implements `ContextSourceLoader::load(&SourceSpec)`.
- Exposes `new(executor)` and `into_executor()` for runtime and deterministic tests.
- Executes one raw GitHub contents request per uncached remote source.

- [ ] **Step 1: Write the failing exact-argv and cache tests**

Create `tests/github_context.rs` with this scripted executor:

```rust
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use ramo::process::command::{
    CommandExecutor, CommandRequest, CommandResult,
};

#[derive(Default)]
struct FakeExecutor {
    requests: Vec<CommandRequest>,
    results: VecDeque<io::Result<CommandResult>>,
}

impl FakeExecutor {
    fn with_results(
        results: impl IntoIterator<Item = io::Result<CommandResult>>,
    ) -> Self {
        Self {
            results: results.into_iter().collect(),
            ..Self::default()
        }
    }
}

impl CommandExecutor for FakeExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult> {
        self.requests.push(request);
        self.results.pop_front().expect("scripted result")
    }
}

fn result(code: i32, stdout: &[u8], stderr: &[u8]) -> io::Result<CommandResult> {
    Ok(CommandResult {
        code: Some(code),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
    })
}

fn argv(request: &CommandRequest) -> Vec<String> {
    request
        .argv
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
}
```

Add:

```rust
#[test]
fn remote_source_is_percent_encoded_fetched_at_its_revision_and_cached() {
    let executor = FakeExecutor::with_results([
        result(0, b"one\ntwo\n", b""),
    ]);
    let mut loader = GithubContextSourceLoader::new(executor);
    let source = SourceSpec::RemoteBlob {
        repository: "owner/repo".into(),
        revision: "abc123".into(),
        path: "src/space # unicode-ç.rs".into(),
    };

    assert_eq!(loader.load(&source).unwrap().as_deref(), Some("one\ntwo\n"));
    assert_eq!(loader.load(&source).unwrap().as_deref(), Some("one\ntwo\n"));

    let executor = loader.into_executor();
    assert_eq!(executor.requests.len(), 1);
    assert_eq!(
        argv(&executor.requests[0]),
        [
            "gh",
            "api",
            "--method",
            "GET",
            "repos/owner/repo/contents/src/space%20%23%20unicode-%C3%A7.rs",
            "-f",
            "ref=abc123",
            "-H",
            "Accept:application/vnd.github.raw+json",
        ]
    );
    let limits = executor.requests[0].limits.as_ref().unwrap();
    assert_eq!(limits.stdout_bytes, 8 * 1024 * 1024);
    assert_eq!(limits.stderr_bytes, 8 * 1024);
    assert_eq!(limits.timeout, Duration::from_secs(15));
}
```

Add:

```rust
#[test]
fn unrelated_source_specs_are_unavailable_without_spawning() {
    let mut loader = GithubContextSourceLoader::new(FakeExecutor::default());
    assert_eq!(loader.load(&SourceSpec::None), Err(SourceFailure::Unavailable));
    assert!(loader.into_executor().requests.is_empty());
}
```

- [ ] **Step 2: Run the new test and verify red**

Run:

```bash
cargo test --test github_context
```

Expected: compilation fails because `GithubContextSourceLoader` does not exist.

- [ ] **Step 3: Add path encoding and the loader skeleton**

In `src/github/mod.rs`, add:

```rust
const SOURCE_STDOUT_LIMIT: usize = 8 * 1024 * 1024;
const SOURCE_TIMEOUT: Duration = Duration::from_secs(15);

pub struct GithubContextSourceLoader<E> {
    github: GithubCli<E>,
    cache: std::collections::HashMap<
        crate::diff::model::SourceSpec,
        Result<Option<String>, crate::review::SourceFailure>,
    >,
}
```

Provide:

```rust
impl<E> GithubContextSourceLoader<E> {
    pub fn new(executor: E) -> Self {
        Self {
            github: GithubCli::new(executor),
            cache: std::collections::HashMap::new(),
        }
    }

    pub fn into_executor(self) -> E { self.github.into_executor() }
}
```

Implement percent encoding without another dependency:

```rust
fn encode_repository_path(path: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/')
        {
            encoded.push(char::from(byte));
        } else {
            write!(&mut encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}
```

- [ ] **Step 4: Add a source-specific GitHub operation**

Add `GithubOperation::LoadContext` with description
`"load pull request context"`.

Add a private `GithubCli::load_context_text` that executes:

```rust
let endpoint = format!(
    "repos/{repository}/contents/{}",
    encode_repository_path(path)
);
let reference = format!("ref={revision}");
self.execute_text(
    GithubOperation::LoadContext,
    &[
        "api",
        "--method",
        "GET",
        &endpoint,
        "-f",
        &reference,
        "-H",
        "Accept:application/vnd.github.raw+json",
    ],
    CaptureLimits::new(SOURCE_STDOUT_LIMIT, STDERR_LIMIT, SOURCE_TIMEOUT),
    None,
)
```

- [ ] **Step 5: Implement `ContextSourceLoader` and error mapping**

Implement:

```rust
impl<E: CommandExecutor + Send> ContextSourceLoader for GithubContextSourceLoader<E> {
    fn load(&mut self, spec: &SourceSpec) -> Result<Option<String>, SourceFailure> {
        let SourceSpec::RemoteBlob {
            repository,
            revision,
            path,
        } = spec
        else {
            return Err(SourceFailure::Unavailable);
        };
        if let Some(cached) = self.cache.get(spec) {
            return cached.clone();
        }
        let result = self
            .github
            .load_context_text(repository, revision, path)
            .map(Some)
            .map_err(map_context_error);
        self.cache.insert(spec.clone(), result.clone());
        result
    }

    fn invalidate(&mut self) {
        self.cache.clear();
    }
}
```

Use this mapping:

- `GithubError::Truncated { operation: LoadContext }` becomes
  `SourceFailure::TooLarge { limit: SOURCE_STDOUT_LIMIT }`.
- `GithubError::InvalidUtf8 { operation: LoadContext }` becomes
  `SourceFailure::NonUtf8`.
- `GithubError::Failed { operation: LoadContext, stderr, .. }` whose sanitized
  stderr contains `HTTP 404` becomes `SourceFailure::Missing`.
- Every other error becomes `SourceFailure::Command(error.to_string())`.

- [ ] **Step 6: Add failure and invalidation tests**

Add independent cases to `tests/github_context.rs` proving:

```rust
stdout_truncated => SourceFailure::TooLarge { limit: 8 * 1024 * 1024 }
invalid UTF-8 => SourceFailure::NonUtf8
timed_out => SourceFailure::Command(message containing "timed out")
HTTP 404 stderr => SourceFailure::Missing
nonzero sanitized stderr => SourceFailure::Command(without ANSI controls)
```

Prove failures are cached by calling the same source twice with only one
scripted result. Prove `invalidate()` causes a second request and observes a
second scripted result.

- [ ] **Step 7: Run adapter and process regressions**

Run:

```bash
cargo test --test github_context --test github_cli --test process_command
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/github/mod.rs tests/github_context.rs
git commit -m "feat: lazily load github pull request context"
```

---

### Task 4: Enable PR expansion through one keyboard/mouse app path

**Files:**

- Modify: `src/app.rs`
- Modify: `src/runtime.rs`
- Modify: `tests/context_expansion.rs`
- Modify: `tests/remote_review_flow.rs`
- Modify: `tests/runtime_resolution.rs`

**Interfaces:**

- Runtime injects `GithubContextSourceLoader<SystemCommandExecutor>` only for PR inputs.
- Keyboard `z` and `ReviewHit::Collapsed` share one result handler.
- PR failures enter `InputMode::Message`; local failures retain toasts.

- [ ] **Step 1: Write failing PR app-flow tests**

In `tests/context_expansion.rs`, create a PR file with a
`SourceSpec::RemoteBlob`, a `SharedLoader` returning twelve source lines, and a
minimal `FakePublisher`. Attach a `PullRequestReviewContext`.

Add:

```rust
#[test]
fn z_expands_pull_request_context_through_the_injected_loader() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut app = pull_request_app(
        Box::new(SharedLoader {
            calls: Arc::clone(&calls),
            source: source_lines(12),
        }),
    );
    let before = app.review_controller.snapshot(VIEWPORT).total_height;

    app.handle_ui_key(
        KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        VIEWPORT,
    );

    assert!(app.review_controller.snapshot(VIEWPORT).total_height > before);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(app.input_mode(), InputMode::Normal);
}
```

Add a failure case:

```rust
#[test]
fn pull_request_context_failure_is_dismissible_and_preserves_state() {
    let mut app = pull_request_app(Box::new(FailingLoader(SourceFailure::Missing)));
    let before = app.review_controller.snapshot(VIEWPORT).total_height;

    app.handle_ui_key(key('z'), VIEWPORT);
    assert_eq!(app.input_mode(), InputMode::Message);
    assert_eq!(app.review_controller.snapshot(VIEWPORT).total_height, before);

    app.handle_ui_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), VIEWPORT);
    assert_eq!(app.input_mode(), InputMode::Normal);
    assert!(!app.should_quit);
}
```

Add a mouse case using the established first collapsed-row coordinate:

```rust
app.handle_mouse(
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 4,
        row: 1,
        modifiers: KeyModifiers::NONE,
    },
    VIEWPORT,
);
assert_eq!(calls.load(Ordering::SeqCst), 1);
assert!(app.review_controller.snapshot(VIEWPORT).total_height > before);
```

- [ ] **Step 2: Run the focused app tests and verify red**

Run:

```bash
cargo test --test context_expansion
```

Expected: the PR keyboard test remains blocked by the current
`Unavailable for pull request` guard.

- [ ] **Step 3: Route both interactions through one helper**

In `App`, add:

```rust
fn finish_context_toggle(&mut self, result: Result<bool, SourceFailure>) {
    match result {
        Ok(_) => self.toast = None,
        Err(failure) if self.remote_review.is_some() => self.show_remote_message(
            "Could not expand pull request context",
            &failure.to_string(),
            RemoteReturnState::Review,
        ),
        Err(failure) => self.toast = Some(failure.to_string()),
    }
}
```

For keyboard `AppAction::ToggleContext`, remove the PR rejection and use:

```rust
let result = self
    .review_controller
    .toggle_context(self.context_loader.as_mut(), viewport);
self.finish_context_toggle(result);
```

For `ReviewHit::Collapsed(gap)`, use:

```rust
let result = self.review_controller.toggle_context_gap(
    &gap,
    self.context_loader.as_mut(),
    viewport,
);
self.finish_context_toggle(result);
```

Keep all reload/editor PR guards unchanged.

- [ ] **Step 4: Inject the GitHub loader only for PR sessions**

In `run_review`, introduce:

```rust
let mut context_loader: Box<dyn ContextSourceLoader> =
    Box::new(NativeContextSourceLoader::default());
```

Inside the PR branch, after loading metadata and diff:

```rust
context_loader = Box::new(
    crate::github::GithubContextSourceLoader::new(SystemCommandExecutor),
);
```

Pass `context_loader` to `App::new_with_services`. Keep the existing loaded
`GithubCli` instance boxed as the publisher, so lazy source reads and
publication remain independent stateless helper invocations.

- [ ] **Step 5: Add runtime isolation assertions**

Extend `tests/runtime_resolution.rs` or `tests/github_context.rs` with a
constructor-level assertion that ordinary review inputs retain the native
loader path and do not require `gh`. Preserve the existing CLI test that local
commands work when `gh` is absent.

- [ ] **Step 6: Run focused regressions**

Run:

```bash
cargo test --test context_expansion --test remote_review_flow --test runtime_resolution --test ui_input --test ui_mouse
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs src/runtime.rs tests/context_expansion.rs tests/remote_review_flow.rs tests/runtime_resolution.rs
git commit -m "feat: expand context in pull request reviews"
```

Only add tests actually modified.

---

### Task 5: Prove lazy expansion end-to-end and document it

**Files:**

- Modify: `tests/pty_pr.rs`
- Modify: `tests/cli_contract.rs`
- Modify: `README.md`
- Modify: `docs/parity/hunk.md`

**Interfaces:**

- Public workflow remains `ramo pr <NUMBER>`.
- Startup performs metadata and diff calls only.
- First `z` performs one contents call; collapse/re-expand performs no second call.

- [ ] **Step 1: Extend the fake `gh` fixture with a source-call log**

Change `PtyProcess::spawn` and `fake_gh` to pass a `FAKE_GH_SOURCE_LOG` path.
Update the metadata JSON to include:

```json
"baseRefOid":"base123","headRefOid":"head123"
```

Change the fake diff so the first hunk starts after a collapsed gap:

```text
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -4 +4 @@
-OLD_PR_LINE
+NEW_PR_LINE
```

Add this fake command:

```sh
"api --method GET repos/owner/repo/contents/src/lib.rs -f ref=head123 -H Accept:application/vnd.github.raw+json")
  printf 'source\n' >> "$FAKE_GH_SOURCE_LOG"
  printf '%s\n' 'CONTEXT_ONE' 'CONTEXT_TWO' 'CONTEXT_THREE' 'NEW_PR_LINE'
  ;;
```

Update the publication fixture to expect `head123`.

- [ ] **Step 2: Write the failing PTY lazy-call scenario**

Add a test that:

1. Starts `ramo pr 123`.
2. Waits for `GitHub PR #123`.
3. Asserts the source log does not exist or is empty.
4. Sends `z`.
5. Waits for `CONTEXT_ONE`.
6. Asserts the source log contains exactly one line.
7. Sends `z` to collapse, then `z` to expand again.
8. Waits for `CONTEXT_ONE` again.
9. Asserts the source log still contains exactly one line.
10. Sends `q`, then `d`, and asserts exit code zero.

Name it:

```rust
public_pr_context_is_fetched_only_on_first_expansion
```

- [ ] **Step 3: Run the PTY integration evidence**

Run:

```bash
cargo test --test pty_pr public_pr_context_is_fetched_only_on_first_expansion -- --nocapture
```

Expected: PASS. The app-level test in Task 4 supplied the required red-green
cycle; this PTY test proves the already-green behavior through the public
binary and fake helper boundary.

- [ ] **Step 4: Add failing documentation contract assertions**

Add to `tests/cli_contract.rs`:

```rust
assert!(readme.contains("Press `z` on a collapsed gap"));
assert!(readme.contains("captured PR revision"));
assert!(!readme.contains("expand unchanged local source"));
```

Run:

```bash
cargo test --test cli_contract
```

Expected: FAIL because README still documents PR context expansion as
unsupported.

- [ ] **Step 5: Update public documentation**

Replace the README limitation saying PR v1 cannot expand unchanged source with:

```markdown
Press `z` on a collapsed gap to lazily load that file from GitHub at the
captured PR revision. Ramo caches the bounded source for the session and does
not check out or fetch the branch. The first expansion requires authenticated
GitHub access through `gh`; failures are dismissible and keep the review open.
```

Keep the limitations for existing threads, watch/reload, editor opening,
GitLab, and Bitbucket.

Add a parity entry in `docs/parity/hunk.md` identifying lazy immutable PR
context expansion and the PTY evidence.

- [ ] **Step 6: Run public-contract and PTY regressions**

Run:

```bash
cargo test --test pty_pr --test cli_contract --test github_context --test pull_request_loading
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add tests/pty_pr.rs tests/cli_contract.rs README.md docs/parity/hunk.md
git commit -m "docs: explain pull request context expansion"
```

---

### Task 6: Simplify, audit, and run release-grade verification

**Files:**

- Modify only files required by verified cleanup findings.

**Interfaces:**

- Produces a clean, warning-free branch ready for integration.
- Does not tag, push, merge, or release without a separate user request.

- [ ] **Step 1: Use the simplify skill**

Run the simplify review over the new remote-source model, GitHub loader, app
toggle helper, and PTY fixture. Prefer:

- one source-assignment helper,
- one percent-encoding helper,
- one GitHub-to-`SourceFailure` mapper,
- one app result handler,
- no provider checks in the review geometry.

Do not refactor unrelated PR publication or local VCS loading.

- [ ] **Step 2: Audit the approved spec line by line**

Check:

```bash
rg -n 'RemoteBlob|base_revision|LoadContext|GithubContextSourceLoader|finish_context_toggle' src tests
rg -n 'expand unchanged local source|Press `z` on a collapsed gap' README.md
git diff main...HEAD --stat
git diff --check
```

Confirm:

- no content API call during initial load,
- immutable base/head revisions,
- correct added/deleted/rename/copy mapping,
- exact bounded raw contents call,
- cached success and failure,
- keyboard and mouse paths,
- dismissible PR errors,
- unchanged local behavior,
- no repository mutation,
- no real-network tests.

- [ ] **Step 3: Run format and lint**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: both exit zero with no warnings.

- [ ] **Step 4: Run the complete locked test suite**

Run:

```bash
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --all-targets --all-features --locked
```

Expected: every unit, integration, PTY, and benchmark target passes.

- [ ] **Step 5: Build the release binary**

Run:

```bash
cargo build --release --locked
target/release/ramo --version
```

Expected: build exits zero and reports the current package version.

- [ ] **Step 6: Inspect final repository state**

Run:

```bash
git diff --check
git status --short
git log --oneline --decorate -12
```

Expected: no uncommitted changes after any final cleanup commit.

- [ ] **Step 7: Commit verified cleanup only if needed**

If the simplify or audit pass produced a real change, rerun its focused tests
and commit:

```bash
git add src/diff/model.rs src/remote_review.rs src/github/mod.rs src/review/context.rs src/vcs/source.rs src/input/pull_request.rs src/app.rs src/runtime.rs tests/github_cli.rs tests/github_context.rs tests/pull_request_loading.rs tests/context_expansion.rs tests/remote_review_flow.rs tests/remote_review_model.rs tests/runtime_resolution.rs tests/pty_pr.rs tests/cli_contract.rs README.md docs/parity/hunk.md
git commit -m "refactor: simplify pull request context loading"
```

If no cleanup change was required, do not create an empty commit.

- [ ] **Step 8: Stop before integration**

Report the verified branch name, worktree path, commits, and test/build
evidence. Do not merge, push, tag, or release unless the user explicitly asks.
