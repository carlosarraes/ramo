# Android Diff Reader and Review Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the Android PR reader with one-file unified diffs, Rust syntax highlighting, context expansion, conversations, persisted range drafts, viewed sync, stale-revision recovery, and atomic review publication.

**Architecture:** Extend the Rust core with UI-neutral syntax spans and draft/re-anchoring types, then expose coarse file-screen models through `ramo-mobile`. Compose renders a virtualized one-file reader and owns touch selection/editor state. Kotlin encrypts serialized draft blobs with the existing Keystore store; Rust validates anchors, checks revisions, re-anchors stale drafts, and publishes.

**Tech Stack:** Rust 2024, syntect 5, UniFFI 0.32.0, Kotlin/Compose, Android Keystore, existing `ramo-core` diff parser and `ramo-github` transport.

## Global Constraints

- Mobile diffs are unified and display one file at a time.
- Code does not wrap by default; horizontal gestures scroll code and never navigate files.
- Existing conversations are read-only in v1.
- Enter inserts a newline; only the visible Save draft action finishes comment editing.
- Viewed state is synchronized to GitHub and can be undone.
- Drafts survive process death but full private source diffs are not persisted.
- Ramo never silently moves, drops, or publishes a comment whose anchor is uncertain.
- Approve and Request changes are unavailable on self-authored PRs.

---

### Task 1: Add platform-neutral syntax spans and mobile file-screen models

**Files:**
- Modify: `crates/ramo-core/Cargo.toml`
- Modify: `crates/ramo-core/src/lib.rs`
- Create: `crates/ramo-core/src/syntax.rs`
- Create: `crates/ramo-core/tests/syntax.rs`
- Modify: `crates/ramo-mobile/src/lib.rs`
- Create: `crates/ramo-mobile/src/models.rs`
- Create: `crates/ramo-mobile/tests/file_screen.rs`

**Interfaces:**
- Consumes: `DiffFile`, path/language/content, Tokyo Night appearance.
- Produces: `SyntaxHighlighter::highlight_line`, `SyntaxSpan`, `MobilePullRequest`, `MobileFileSummary`, `MobileDiffRow`, and `MobileFileScreen`.

- [ ] **Step 1: Write failing deterministic syntax tests**

```rust
#[test]
fn rust_keywords_and_strings_return_distinct_rgb_spans() {
    let mut highlighter = SyntaxHighlighter::tokyo_night();
    let spans = highlighter.highlight_line("src/lib.rs", None, "let value = \"ramo\";");
    assert_eq!(spans.iter().map(|span| span.text.as_str()).collect::<String>(), "let value = \"ramo\";");
    assert!(spans.windows(2).any(|pair| pair[0].foreground != pair[1].foreground));
}
```

- [ ] **Step 2: Run it to verify the module is missing**

Run: `cargo test -p ramo-core --test syntax`

Expected: FAIL because `SyntaxHighlighter` does not exist.

- [ ] **Step 3: Implement UI-neutral syntax output**

Move the syntect setup logic out of `src/ui/highlight.rs` into core and return:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RgbColor { pub red: u8, pub green: u8, pub blue: u8 }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyntaxSpan {
    pub text: String,
    pub foreground: RgbColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

pub struct SyntaxHighlighter;

impl SyntaxHighlighter {
    pub fn tokyo_night() -> Self;
    pub fn highlight_line(
        &mut self,
        path: &str,
        language: Option<&str>,
        content: &str,
    ) -> Vec<SyntaxSpan>;
}
```

Use the approved Tokyo Night syntax colors and preserve text exactly. Plain-text/error fallback returns one span with foreground `#c0caf5`.

- [ ] **Step 4: Adapt the terminal cache**

Keep `src/ui/highlight.rs` responsible for LRU keys and conversion only. Convert each core RGB span to Ratatui `Span<'static>` and retain existing cache counters/capacities. Run `cargo test -p ramo --test highlighting --test themes` to prove no terminal regression.

- [ ] **Step 5: Define coarse UniFFI screen records**

Use records with these stable fields:

```rust
#[derive(uniffi::Record)]
pub struct MobilePullRequest {
    pub node_id: String,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub author_login: String,
    pub viewer_login: String,
    pub base_ref: String,
    pub head_ref: String,
    pub captured_revision: String,
    pub additions: u64,
    pub deletions: u64,
    pub files: Vec<MobileFileSummary>,
}

#[derive(uniffi::Record)]
pub struct MobileDiffRow {
    pub key: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub kind: MobileLineKind,
    pub spans: Vec<MobileSyntaxSpan>,
    pub commentable: bool,
}

#[derive(uniffi::Record)]
pub struct MobileFileScreen {
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub additions: u64,
    pub deletions: u64,
    pub file_index: u64,
    pub file_count: u64,
    pub viewed_count: u64,
    pub file: MobileFileSummary,
    pub rows: Vec<MobileDiffRow>,
    pub next_row: Option<u64>,
    pub threads: Vec<MobileReviewThread>,
}
```

`MobileSession::open_pull_request(repository, number)` returns PR/file summaries. `file_screen(repository, number, file_index, start_row, row_limit)` lazily loads that file and returns at most 400 rows plus `next_row`; reject limits above 500. This keeps large-file FFI allocations bounded.

- [ ] **Step 6: Run core/mobile tests and commit**

Run: `cargo test -p ramo-core -p ramo-mobile && cargo test -p ramo --test highlighting --test themes`

Expected: PASS.

```bash
git add crates/ramo-core crates/ramo-mobile src/ui/highlight.rs tests/highlighting.rs
git commit -m "feat: share syntax-highlighted diff models"
```

### Task 2: Render the one-file unified reader and synchronize progress

**Files:**
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewModels.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewRepository.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModel.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewScreen.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/DiffRow.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/FileDrawer.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewPreferencesStore.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModelTest.kt`
- Create: `android/app/src/androidTest/kotlin/io/github/carlosarraes/ramo/review/ReviewScreenTest.kt`
- Modify: `crates/ramo-mobile/src/lib.rs`

**Interfaces:**
- Consumes: `MobileFileScreen`, explicit previous/next/file selection, and `set_file_viewed`.
- Produces: sticky PR/file headers, virtualized unified rows, file drawer, exact progress, and reversible auto-viewed behavior.

- [ ] **Step 1: Write failing ViewModel tests**

Test initial file 0, explicit next/previous bounds, drawer selection, independent horizontal offsets per file, row-page append without duplicate keys, auto-mark only after `nextRow == null` and the final row becomes visible, manual unmark, optimistic rollback on API failure, and `viewedCount / fileCount` progress.

- [ ] **Step 2: Implement immutable review state**

```kotlin
data class ReviewUiState(
    val loading: Boolean = true,
    val pullRequest: PullRequestUi? = null,
    val selectedFile: Int = 0,
    val screen: FileScreenUi? = null,
    val drawerOpen: Boolean = false,
    val error: String? = null,
)
```

The repository maps UniFFI records to Kotlin UI models on `Dispatchers.IO`. The ViewModel exposes explicit `selectFile(index)`, `previousFile()`, `nextFile()`, `loadMoreRows()`, `setViewed(viewed)`, and `lastRowVisible()` intents. `loadMoreRows()` starts when the last 40 loaded rows approach the viewport and is a no-op while a page is in flight.

- [ ] **Step 3: Implement the approved reader hierarchy**

Use a sticky top summary with back, repository/number, title, colored total, and `N / M files · P%`. Below it, use a sticky file header with path, Viewed checkbox, and file-drawer button. Render rows in `LazyColumn`; each code row contains a fixed line-number gutter plus a horizontally scrollable code surface that shares one `ScrollState` within the current file.

`ReviewPreferencesStore` persists a monospace code size from 11sp through 20sp with default 13sp. Expose it in the settings screen and apply it without changing normal UI text scaling.

Do not attach file changes to swipe gestures. Bottom controls are Previous file, Next file, and Finish review.

Every interactive icon has a content description; focus order follows summary, file header, rows, conversations, and bottom actions. Add Compose tests at font scales 1.0 and 1.5 and a 10,000-row fake file test proving only requested pages cross the repository boundary.

- [ ] **Step 4: Export viewed mutation**

```rust
pub fn set_file_viewed(
    &self,
    pull_request_id: String,
    path: String,
    viewed: bool,
) -> Result<(), MobileError>;
```

- [ ] **Step 5: Run reader tests**

Run: `cd android && ./gradlew :app:testDebugUnitTest :app:connectedDebugAndroidTest`

Expected: PASS including semantics for progress, Viewed, Previous file, Next file, and the file path.

- [ ] **Step 6: Commit the reader**

```bash
git add crates/ramo-mobile android/app/src
git commit -m "feat: add one-file mobile diff reader"
```

### Task 3: Expand context and render existing conversations

**Files:**
- Create: `crates/ramo-core/src/context.rs`
- Modify: `crates/ramo-core/src/lib.rs`
- Modify: `src/review/context.rs`
- Modify: `crates/ramo-mobile/src/lib.rs`
- Create: `crates/ramo-mobile/tests/context_threads.rs`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ConversationCard.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModel.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewScreen.kt`
- Modify: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModelTest.kt`

**Interfaces:**
- Consumes: gap key, selected file, base/head `RemoteBlob`, and GitHub review threads.
- Produces: `expand_context(file_index, gap_key)` and inline/unplaced read-only conversation cards.

- [ ] **Step 1: Write failing Rust tests**

Cover a middle gap, trailing gap, deleted file using old source, added file using new source, source too large, and thread ranges placed on left/right rows. Assert an expanded-only line is `commentable = false` when it is outside GitHub's patch hunks.

- [ ] **Step 2: Move pure context helpers into core**

Move `GapKey`, `CollapsedGap`, `derive_collapsed_gaps`, `source_for_context`, and `expand_gap_lines` into `crates/ramo-core/src/context.rs` and export it with `pub mod context;`. Keep terminal-specific command readers in `src/review/context.rs` implementing a core `ContextSourceLoader` trait. Re-export the moved symbols from `src/review/context.rs` so existing context tests compile.

- [ ] **Step 3: Export lazy expansion**

```rust
pub fn expand_context(
    &self,
    repository: String,
    number: u64,
    file_index: u64,
    gap_key: String,
) -> Result<MobileFileScreen, MobileError>;
```

Fetch source once per in-memory session and cache by repository/revision/path. Rebuild the selected file screen with expanded rows; never persist source bodies.

- [ ] **Step 4: Place and render threads**

Map current threads immediately after their anchor row. Render author, relative time, Markdown body, resolved status, and an external-link action. Place outdated/unmappable threads in a compact `Previous conversations` section at the end of that file. Provide no reply or resolve controls.

- [ ] **Step 5: Run context/thread tests and commit**

Run: `cargo test -p ramo-core -p ramo-mobile && cargo test -p ramo --test context_expansion --test github_context --test remote_review_targets && cd android && ./gradlew :app:testDebugUnitTest`

Expected: PASS.

```bash
git add crates/ramo-core crates/ramo-mobile src/review/context.rs android/app/src
git commit -m "feat: expand mobile context and conversations"
```

### Task 4: Add range selection, explicit draft saving, and encrypted persistence

**Files:**
- Modify: `crates/ramo-core/src/lib.rs`
- Create: `crates/ramo-core/src/drafts.rs`
- Create: `crates/ramo-core/tests/drafts.rs`
- Modify: `crates/ramo-mobile/src/lib.rs`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/DraftEditor.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/DraftStore.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModel.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/review/DraftViewModelTest.kt`
- Create: `android/app/src/androidTest/kotlin/io/github/carlosarraes/ramo/review/DraftEditorTest.kt`

**Interfaces:**
- Consumes: commentable line/range, body, frozen head SHA, and nearby source text.
- Produces: `DraftAnchor`, `DraftComment`, `DraftReview`, JSON encode/decode methods, encrypted `review-{repo-hash}-{number}.bin` files.

- [ ] **Step 1: Write failing core draft validation tests**

Test single right-side line, left-side deletion range, reversed-range normalization, cross-side rejection, cross-hunk rejection, blank-body rejection, and stable serde round trip.

- [ ] **Step 2: Define exact draft types**

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DraftAnchor {
    pub repository: String,
    pub number: u64,
    pub captured_revision: String,
    pub path: String,
    pub side: crate::remote_review::RemoteLineSide,
    pub start_line: u32,
    pub end_line: u32,
    pub context_before: Vec<String>,
    pub selected_text: Vec<String>,
    pub context_after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DraftComment { pub id: String, pub anchor: DraftAnchor, pub body: String }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DraftReview {
    pub repository: String,
    pub number: u64,
    pub captured_revision: String,
    pub body: String,
    pub comments: Vec<DraftComment>,
}
```

- [ ] **Step 3: Export draft validation and serialization**

```rust
pub fn create_draft(&self, input: MobileDraftInput) -> Result<MobileDraftComment, MobileError>;
pub fn encode_draft_review(&self, review: MobileDraftReview) -> Result<Vec<u8>, MobileError>;
pub fn decode_draft_review(&self, bytes: Vec<u8>) -> Result<MobileDraftReview, MobileError>;
```

- [ ] **Step 4: Implement unambiguous touch/editor behavior**

Tap a commentable gutter for one line. Long-press enters selection; dragging start/end handles stays on one side and one hunk. Show the exact `L7–L9` or `R42` label before editing. The multiline text field's IME action is newline; only the visible Save draft button calls `saveDraft()`. Cancel preserves no new draft; editing an existing draft never deletes it until Save succeeds or Delete is confirmed.

- [ ] **Step 5: Persist encrypted draft bytes**

Reuse `EncryptedBlobStore` with separate key alias `ramo.mobile.drafts.v1`. Kotlin asks Rust to encode/decode, then encrypts/decrypts opaque bytes. Auto-save after every successful draft/body mutation. Sign out deletes all `review-*.bin` files; token replacement does not.

- [ ] **Step 6: Run draft tests and commit**

Run: `cargo test -p ramo-core --test drafts && cargo test -p ramo-mobile && cd android && ./gradlew :app:testDebugUnitTest :app:connectedDebugAndroidTest`

Expected: PASS, including a device test proving plaintext comment text is absent from stored bytes.

```bash
git add crates/ramo-core crates/ramo-mobile android/app/src
git commit -m "feat: persist mobile review drafts"
```

### Task 5: Re-anchor stale drafts without silent data loss

**Files:**
- Modify: `crates/ramo-core/src/drafts.rs`
- Create: `crates/ramo-core/tests/reanchor.rs`
- Modify: `crates/ramo-mobile/src/lib.rs`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModel.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/NeedsAttentionSheet.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/review/ReanchorViewModelTest.kt`

**Interfaces:**
- Consumes: old draft anchor and refreshed file source/diff.
- Produces: `ReanchorResult::{Exact, Moved, NeedsAttention}` and a user-visible repair flow.

- [ ] **Step 1: Write failing re-anchor tests**

Cover unchanged line, unique moved three-line context, ambiguous duplicate block, deleted selected text, renamed file with matching previous path, side change, and binary replacement. Exact/moved outcomes must preserve body and ID; ambiguous/missing outcomes must preserve the original anchor and body under `NeedsAttention`.

- [ ] **Step 2: Implement conservative matching**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReanchorResult {
    Exact(DraftComment),
    Moved { draft: DraftComment, old_line: u32, new_line: u32 },
    NeedsAttention { draft: DraftComment, reason: String },
}
```

Match exact path/side/line first. Otherwise require one unique occurrence of `context_before + selected_text + context_after` on the same side. Never choose among multiple matches and never fuzzy-match changed selected text.

- [ ] **Step 3: Export refresh/re-anchor orchestration**

`MobileSession::refresh_and_reanchor(review)` reloads the head/snapshot, runs every draft through the core matcher, and returns the refreshed PR plus outcomes. It does not submit.

- [ ] **Step 4: Render repair UI**

If any outcome needs attention, block Finish review and show each original file/range, body, reason, Copy, Edit anchor, and Delete actions. Moved comments receive a visible `Moved from R42 to R57` notice that the user can dismiss. Persist all outcomes before returning to the reader.

- [ ] **Step 5: Run stale-revision tests and commit**

Run: `cargo test -p ramo-core --test reanchor && cargo test -p ramo-mobile && cd android && ./gradlew :app:testDebugUnitTest`

Expected: PASS.

```bash
git add crates/ramo-core crates/ramo-mobile android/app/src
git commit -m "feat: recover stale mobile review drafts"
```

### Task 6: Finish, confirm, and publish the review

**Files:**
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/FinishReviewSheet.kt`
- Create: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/PublishConfirmation.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/review/ReviewViewModel.kt`
- Create: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/review/PublishViewModelTest.kt`
- Create: `android/app/src/androidTest/kotlin/io/github/carlosarraes/ramo/review/FinishReviewSheetTest.kt`
- Modify: `crates/ramo-mobile/src/lib.rs`

**Interfaces:**
- Consumes: valid draft review, optional overall body, `Comment`/`Approve`/`RequestChanges` verdict.
- Produces: guarded GitHub review submission, success state, intact retryable failure state.

- [ ] **Step 1: Write failing publish-state tests**

Test Comment, Approve, Request changes, self-authored verdict restrictions, zero-inline overall comment, request-changes body requirement, stale-head transition, 422 retention, offline retry, double-tap suppression, and success clearing only that PR's draft file.

- [ ] **Step 2: Export guarded publication**

```rust
pub fn publish_review(
    &self,
    review: MobileDraftReview,
    verdict: MobileReviewVerdict,
) -> Result<(), MobileError>;
```

Convert only validated drafts to `RemoteReviewRequest`. Call `ramo-github::submit_review` with the captured revision. Map stale revision to a distinct mobile error kind that triggers Task 5; never retry mutation requests automatically.

- [ ] **Step 3: Build the finish sheet**

Group draft summaries by file and range, allow tapping one to return to it, add an optional multiline overall Markdown field, and show three verdict choices. Hide Approve and Request changes when `author_login` equals `viewer_login` case-insensitively.

- [ ] **Step 4: Add exact confirmations and result handling**

Use confirmation copy `Approve PR #<number> with <count> inline comments?`, substituting `Comment on` or `Request changes on` for other verdicts. Disable submission while in flight. On success, remove the encrypted draft, refresh inbox state, and show a dismissible success message. On error, preserve the sheet and every draft.

- [ ] **Step 5: Run publication tests**

Run: `cargo test -p ramo-github --test publish && cargo test -p ramo-mobile && cd android && ./gradlew :app:testDebugUnitTest :app:connectedDebugAndroidTest`

Expected: PASS.

- [ ] **Step 6: Commit publication**

```bash
git add crates/ramo-mobile android/app/src
git commit -m "feat: publish reviews from android"
```

### Task 7: Verify, install, and produce the personal release APK

**Files:**
- Modify: `.github/workflows/ci.yml`
- Create: `docs/android.md`
- Modify: `README.md`
- Create outside Git: Android signing keystore and `android/keystore.properties`

**Interfaces:**
- Consumes: completed Rust workspace, Android project, connected SM-S928B, disposable GitHub PR.
- Produces: tested signed arm64 APK and documented personal installation/authentication flow.

- [ ] **Step 1: Add CI gates that do not require secrets or a device**

Add a Linux Android job installing JDK 17, Rust Android target `aarch64-linux-android`, cargo-ndk 4.1.2, SDK 36, build-tools 36.0.0, and NDK 28.2.13676358. Run:

```text
cargo test --locked --workspace
cd android && ./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug
```

Upload the unsigned debug APK only as a CI artifact; do not add it to desktop GitHub releases yet.

- [ ] **Step 2: Run the complete local gate**

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-targets --all-features
cd android
./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug
```

Expected: every command exits 0.

- [ ] **Step 3: Execute the real-PR checklist**

On a disposable PR visible to the fine-grained token, verify in order: inbox discovery, notification deep link, total additions/deletions, file order, syntax colors, horizontal scroll, context expansion, existing conversation, viewed/unviewed sync, single-line draft, multiline range draft with newline, process restart persistence, stale-head block, overall comment, and one approval or request-changes publication. Confirm the resulting GitHub review locations and verdict in the browser or `gh api` from the desktop.

- [ ] **Step 4: Install the exact release candidate and inspect crashes**

```bash
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb shell am force-stop io.github.carlosarraes.ramo
adb shell monkey -p io.github.carlosarraes.ramo 1
adb logcat -c
```

Exercise the checklist, then run `adb logcat -d -t 1000 | rg 'AndroidRuntime|FATAL EXCEPTION|ramo_mobile'`. Expected: no fatal exception.

- [ ] **Step 5: Create personal signing material outside Git**

Generate an upload key in `$XDG_DATA_HOME/ramo/android/ramo-upload.jks`, write its path/password aliases to ignored `android/keystore.properties`, configure the release signing block to load that file, and run `./gradlew :app:assembleRelease`. Verify with `apksigner verify --verbose android/app/build/outputs/apk/release/app-release.apk`.

- [ ] **Step 6: Install and re-smoke the exact signed artifact**

Run: `adb install -r android/app/build/outputs/apk/release/app-release.apk`

Expected: install succeeds over the same package, token remains readable after upgrade, inbox opens, one PR/file loads, and no fatal log entry appears.

- [ ] **Step 7: Document and commit delivery**

Document token permissions, team-request caveat, notification delay, local build, ADB install, sign-out deletion, and v1 limitations in `docs/android.md`; link it from README.

```bash
git add .github/workflows/ci.yml README.md docs/android.md android/app/build.gradle.kts
git commit -m "docs: ship personal android review app"
```
