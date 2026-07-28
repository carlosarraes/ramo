# Direct GitHub API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a synchronous, Android-compatible Rust GitHub client that discovers review work, loads frozen PR snapshots and conversations, syncs viewed files, checks notifications, and publishes reviews without `gh`.

**Architecture:** Create `ramo-github` as a transport crate depending on `ramo-core`. Use a blocking `reqwest` client with Rustls because Kotlin will call it on `Dispatchers.IO`; keep base URLs injectable for fixture tests. Use GraphQL for rich inbox rows, review threads, and viewed mutations, and versioned REST for raw diffs, source context, notifications, current-head checks, and atomic review submission.

**Tech Stack:** Rust 2024, reqwest 0.13.4 (`blocking`, `json`, `rustls`), serde 1, serde_json 1, thiserror 2.0.19, zeroize 1.9.0, httpmock 0.8.3.

## Global Constraints

- The client must not invoke `gh`, Git, a browser, or a shell.
- The client is synchronous; callers must keep it off UI/main threads.
- Tokens are accepted at runtime, held in `Zeroizing<String>`, never logged, serialized, or embedded.
- Every review is tied to a captured head SHA and rechecked before submission.
- Fine-grained token access is limited to repositories selected by the user.
- Existing terminal `GithubCli` behavior remains available and unchanged.
- GitHub API errors, authentication failures, rate limits, and stale revisions are typed outcomes.

---

### Task 1: Define transport-independent mobile GitHub models

**Files:**
- Modify: `crates/ramo-core/src/lib.rs`
- Create: `crates/ramo-core/src/github.rs`
- Test: `crates/ramo-core/tests/github_models.rs`

**Interfaces:**
- Consumes: `PullRequestReviewContext`, `DiffFile`, `GithubReviewThread`, and `RemoteReviewRequest` from `ramo-core`.
- Produces: `PullRequestKey`, `InboxKind`, `InboxPage`, `PullRequestSummary`, `PullRequestSnapshot`, `ChangedFile`, `ConditionalCursor`, and `ReviewNotificationPage`.

- [ ] **Step 1: Write the failing model round-trip test**

```rust
use ramo_core::github::{InboxKind, PullRequestKey};

#[test]
fn github_keys_and_filters_have_stable_json() {
    let key = PullRequestKey { repository: "owner/repo".into(), number: 42 };
    assert_eq!(serde_json::to_string(&key).unwrap(), r#"{"repository":"owner/repo","number":42}"#);
    assert_eq!(serde_json::to_string(&InboxKind::ReviewRequests).unwrap(), r#""review_requests""#);
}
```

- [ ] **Step 2: Run it to verify the module is missing**

Run: `cargo test -p ramo-core --test github_models`

Expected: FAIL because `ramo_core::github` does not exist.

- [ ] **Step 3: Add exact value types**

Define serde-enabled records with these signatures:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PullRequestKey {
    pub repository: String,
    pub number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxKind { ReviewRequests, Authored }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PullRequestSummary {
    pub node_id: String,
    pub key: PullRequestKey,
    pub title: String,
    pub url: String,
    pub author_login: String,
    pub updated_at: String,
    pub is_draft: bool,
    pub additions: usize,
    pub deletions: usize,
    pub changed_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InboxPage {
    pub items: Vec<PullRequestSummary>,
    pub end_cursor: Option<String>,
    pub has_next_page: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
    pub patch: Option<String>,
    pub viewed: bool,
    pub binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PullRequestSnapshot {
    pub node_id: String,
    pub context: crate::remote_review::PullRequestReviewContext,
    pub files: Vec<ChangedFile>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConditionalCursor {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewNotification {
    pub id: String,
    pub key: PullRequestKey,
    pub title: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewNotificationPage {
    pub notifications: Vec<ReviewNotification>,
    pub cursor: ConditionalCursor,
    pub not_modified: bool,
}
```

Add `pub mod github;` to `ramo-core/src/lib.rs` and `serde_json = "1"` as a core dev dependency.

- [ ] **Step 4: Run the core tests**

Run: `cargo test -p ramo-core --test github_models`

Expected: PASS.

- [ ] **Step 5: Commit the domain addition**

```bash
git add crates/ramo-core
git commit -m "feat: model mobile github reviews"
```

### Task 2: Build the authenticated HTTP foundation and typed errors

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/ramo-github/Cargo.toml`
- Create: `crates/ramo-github/src/lib.rs`
- Create: `crates/ramo-github/src/client.rs`
- Create: `crates/ramo-github/src/error.rs`
- Create: `crates/ramo-github/tests/http_contract.rs`

**Interfaces:**
- Consumes: runtime token string and injectable REST/GraphQL base URLs.
- Produces: `GithubClient::new(token)`, test-only `GithubClient::with_endpoints`, `GithubError`, and `GithubErrorKind`.

- [ ] **Step 1: Write failing authentication and redaction tests**

```rust
#[test]
fn viewer_request_uses_bearer_auth_and_never_formats_the_token() {
    let server = httpmock::MockServer::start();
    let expected = server.mock(|when, then| {
        when.method("GET").path("/user").header("authorization", "Bearer secret-token");
        then.status(200).json_body_obj(&serde_json::json!({"login":"carraes","id":7}));
    });
    let client = test_client(&server, "secret-token");
    assert_eq!(client.viewer().unwrap().login, "carraes");
    assert!(!format!("{client:?}").contains("secret-token"));
    expected.assert();
}
```

- [ ] **Step 2: Run it to verify the crate is missing**

Run: `cargo test -p ramo-github --test http_contract`

Expected: FAIL because package `ramo-github` does not exist.

- [ ] **Step 3: Create the crate and client**

Add `crates/ramo-github` to workspace members. Use this manifest dependency shape:

```toml
[dependencies]
ramo-core = { path = "../ramo-core" }
reqwest = { version = "0.13.4", default-features = false, features = ["blocking", "json", "rustls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2.0.19"
zeroize = "1.9.0"

[dev-dependencies]
httpmock = "0.8.3"
```

Implement the public constructor and test seam:

```rust
pub struct GithubClient {
    http: reqwest::blocking::Client,
    rest_base: String,
    graphql_url: String,
    token: zeroize::Zeroizing<String>,
}

impl std::fmt::Debug for GithubClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GithubClient")
            .field("rest_base", &self.rest_base)
            .field("graphql_url", &self.graphql_url)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl GithubClient {
    pub fn new(token: String) -> Result<Self, GithubError> {
        Self::with_endpoints(token, "https://api.github.com", "https://api.github.com/graphql")
    }

    pub fn with_endpoints(
        token: String,
        rest_base: impl Into<String>,
        graphql_url: impl Into<String>,
    ) -> Result<Self, GithubError> {
        if token.trim().is_empty() {
            return Err(GithubError::invalid_credentials("GitHub token is empty"));
        }
        Ok(Self {
            http: reqwest::blocking::Client::builder()
                .user_agent(concat!("ramo/", env!("CARGO_PKG_VERSION")))
                .build()?,
            rest_base: rest_base.into(),
            graphql_url: graphql_url.into(),
            token: zeroize::Zeroizing::new(token),
        })
    }
}
```

All REST requests add `Accept: application/vnd.github+json`, `X-GitHub-Api-Version: 2026-03-10`, and bearer authorization. Map 401/403/rate-limit headers, 404, 422, transport, JSON, and GraphQL `errors` into stable `GithubErrorKind` variants.

- [ ] **Step 4: Run HTTP foundation tests**

Run: `cargo test -p ramo-github --test http_contract`

Expected: PASS for auth, redaction, status mapping, and rate-limit reset extraction.

- [ ] **Step 5: Commit the HTTP foundation**

```bash
git add Cargo.toml Cargo.lock crates/ramo-github
git commit -m "feat: add direct github transport"
```

### Task 3: Discover authored and requested-review PRs

**Files:**
- Create: `crates/ramo-github/src/inbox.rs`
- Create: `crates/ramo-github/tests/inbox.rs`
- Create: `crates/ramo-github/tests/fixtures/inbox.json`
- Create: `crates/ramo-github/tests/fixtures/teams.json`
- Modify: `crates/ramo-github/src/lib.rs`

**Interfaces:**
- Consumes: `InboxKind`, optional GraphQL cursor, authenticated viewer, and accessible team slugs.
- Produces: `GithubClient::list_inbox(kind, after) -> Result<InboxPage, GithubError>`.

- [ ] **Step 1: Write failing query and pagination tests**

Verify the GraphQL variables are exactly:

```json
{"query":"is:open is:pr user-review-requested:@me","first":20,"after":null}
```

for direct requests and:

```json
{"query":"is:open is:pr author:@me","first":20,"after":"cursor-1"}
```

for authored PRs. Add a fixture where a team request is returned by appending `team-review-requested:owner/backend` to a second query and ensure duplicate PR node IDs collapse to one item sorted by `updatedAt` descending.

- [ ] **Step 2: Run it to verify the method is missing**

Run: `cargo test -p ramo-github --test inbox`

Expected: FAIL because `list_inbox` does not exist.

- [ ] **Step 3: Implement GraphQL search mapping**

Use one reusable fragment containing `id`, `number`, `title`, `url`, `updatedAt`, `isDraft`, `additions`, `deletions`, `changedFiles`, `author.login`, and `repository.nameWithOwner`. Query direct review requests first. Discover the viewer's accessible team slugs through `/user/teams?per_page=100`; query each team qualifier and deduplicate by PR node ID.

If `/user/teams` returns 403 because the token's resource owner is not the organization (or the organization has not approved the token), return direct review results plus the exact warning `Team review requests need a token whose resource owner is that organization.` in `InboxPage::warnings` and cover it in fixture tests.

- [ ] **Step 4: Run inbox tests**

Run: `cargo test -p ramo-github --test inbox`

Expected: PASS for authored, direct, team, deduplication, sorting, pagination, and permission warning cases.

- [ ] **Step 5: Commit inbox discovery**

```bash
git add crates/ramo-core crates/ramo-github
git commit -m "feat: discover github review work"
```

### Task 4: Load frozen snapshots, file patches, source context, and threads

**Files:**
- Create: `crates/ramo-github/src/pull_request.rs`
- Create: `crates/ramo-github/src/graphql.rs`
- Create: `crates/ramo-github/tests/pull_request.rs`
- Create: `crates/ramo-github/tests/fixtures/pull.json`
- Create: `crates/ramo-github/tests/fixtures/files-page-1.json`
- Create: `crates/ramo-github/tests/fixtures/files-page-2.json`
- Create: `crates/ramo-github/tests/fixtures/threads.json`
- Modify: `crates/ramo-github/src/lib.rs`
- Modify: `src/github/threads.rs`
- Modify: `tests/github_cli.rs`
- Modify: `tests/remote_review_flow.rs`

**Interfaces:**
- Consumes: `PullRequestKey`.
- Produces: `load_snapshot`, `load_review_threads`, `load_source`, and `load_unified_diff` methods.

- [ ] **Step 1: Write failing snapshot fixture tests**

Assert that two REST file pages preserve API order, rename metadata, absent patches, and binary state; assert that the snapshot context records base SHA, head SHA, author, viewer, and PR node ID. Verify source paths containing spaces, `#`, and Unicode are percent-encoded per path segment and fetched with `ref=<sha>`.

- [ ] **Step 2: Run it to verify loaders are missing**

Run: `cargo test -p ramo-github --test pull_request`

Expected: FAIL with missing methods.

- [ ] **Step 3: Implement exact loaders**

Add methods with these signatures:

```rust
pub fn load_snapshot(&self, key: &PullRequestKey) -> Result<PullRequestSnapshot, GithubError>;
pub fn load_unified_diff(&self, key: &PullRequestKey) -> Result<String, GithubError>;
pub fn load_review_threads(&self, key: &PullRequestKey) -> Result<Vec<GithubReviewThread>, GithubError>;
pub fn load_source(&self, repository: &str, revision: &str, path: &str) -> Result<String, GithubError>;
```

`load_unified_diff` calls `GET /repos/{owner}/{repo}/pulls/{number}` with `Accept: application/vnd.github.diff`. `load_source` calls the contents endpoint with `Accept: application/vnd.github.raw+json`. Thread GraphQL maps file, line, range, outdated, and comment fields into the existing core thread model; add `is_resolved: bool` and `is_outdated: bool` fields with `#[serde(default)]`. Update the terminal GraphQL mapping and every existing struct literal to set these booleans explicitly, normally `false`.

- [ ] **Step 4: Run snapshot and existing CLI regression tests**

Run: `cargo test -p ramo-github --test pull_request && cargo test -p ramo --test github_cli --test github_context --test pull_request_loading`

Expected: all tests PASS.

- [ ] **Step 5: Commit snapshot loading**

```bash
git add crates/ramo-core crates/ramo-github
git commit -m "feat: load github review snapshots"
```

### Task 5: Sync viewed files and review-request notifications

**Files:**
- Create: `crates/ramo-github/src/viewed.rs`
- Create: `crates/ramo-github/src/notifications.rs`
- Create: `crates/ramo-github/tests/viewed.rs`
- Create: `crates/ramo-github/tests/notifications.rs`
- Create: `crates/ramo-github/tests/fixtures/notifications.json`
- Modify: `crates/ramo-github/src/lib.rs`

**Interfaces:**
- Consumes: PR node ID/path/viewed boolean and `ConditionalCursor`.
- Produces: `set_file_viewed` and `review_notifications`.

- [ ] **Step 1: Write failing mutation and conditional-request tests**

Assert that viewed `true` sends GraphQL mutation `markFileAsViewed`, viewed `false` sends `unmarkFileAsViewed`, and both include `pullRequestId` plus `path`. Assert notifications send `If-None-Match` and `If-Modified-Since`, accept 304, filter `reason == "review_requested"`, ignore non-PR subjects, and return new ETag/Last-Modified headers.

- [ ] **Step 2: Run them to verify methods are missing**

Run: `cargo test -p ramo-github --test viewed --test notifications`

Expected: FAIL with missing methods.

- [ ] **Step 3: Implement methods**

```rust
pub fn set_file_viewed(
    &self,
    pull_request_id: &str,
    path: &str,
    viewed: bool,
) -> Result<(), GithubError>;

pub fn review_notifications(
    &self,
    cursor: &ConditionalCursor,
) -> Result<ReviewNotificationPage, GithubError>;
```

For each review notification, follow its subject API URL once to resolve repository and PR number. Deduplicate by notification ID before returning. A 304 response returns the input cursor, an empty list, and `not_modified = true`.

- [ ] **Step 4: Run the focused tests**

Run: `cargo test -p ramo-github --test viewed --test notifications`

Expected: PASS.

- [ ] **Step 5: Commit sync operations**

```bash
git add crates/ramo-github
git commit -m "feat: sync github review progress"
```

### Task 6: Guard and publish atomic reviews

**Files:**
- Create: `crates/ramo-github/src/publish.rs`
- Create: `crates/ramo-github/tests/publish.rs`
- Modify: `crates/ramo-github/src/lib.rs`

**Interfaces:**
- Consumes: `PullRequestKey`, expected head SHA, and `RemoteReviewRequest`.
- Produces: `current_revision` and `submit_review`; stale heads return `GithubErrorKind::StaleRevision { expected, actual }` before POST.

- [ ] **Step 1: Write failing stale and exact-payload tests**

Test a changed head and assert no review POST occurs. Test an unchanged head and assert one body containing `commit_id`, `body`, `event`, and line/range comments using `line`, `side`, `start_line`, and `start_side` exactly like the existing terminal payload contract.

- [ ] **Step 2: Run it to verify publication is missing**

Run: `cargo test -p ramo-github --test publish`

Expected: FAIL with missing methods.

- [ ] **Step 3: Implement guarded submission**

```rust
pub fn current_revision(&self, key: &PullRequestKey) -> Result<String, GithubError>;

pub fn submit_review(
    &self,
    key: &PullRequestKey,
    expected_revision: &str,
    request: &RemoteReviewRequest,
) -> Result<(), GithubError>;
```

Fetch the current head first. Compare it to both `expected_revision` and `request.commit_id`. Only then POST to `/repos/{repository}/pulls/{number}/reviews`. Map 422 validation responses to a typed validation error preserving GitHub's sanitized message but never request headers.

- [ ] **Step 4: Run the complete transport gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy -p ramo-core -p ramo-github --all-targets -- -D warnings
cargo test -p ramo-core -p ramo-github
cargo test -p ramo --test github_cli --test github_context --test remote_review_flow
```

Expected: all commands exit 0.

- [ ] **Step 5: Commit publication**

```bash
git add crates/ramo-github
git commit -m "feat: publish guarded github reviews"
```
