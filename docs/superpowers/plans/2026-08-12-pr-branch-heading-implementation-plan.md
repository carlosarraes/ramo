# Pull Request Branch Heading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show `target ← source` branch context in the top bar for `ramo pr <NUMBER>` while preserving the beginning of long source branch names.

**Architecture:** Carry the existing `PullRequestReviewContext.base_ref` and `head_ref` values into the shared `ReviewHeading::PullRequest` variant. Format the relationship once in `ReviewHeading::label`, placing it before the title so the existing cell-aware truncation used by both review screens preserves the source branch prefix.

**Tech Stack:** Rust, Ratatui, Cargo integration tests

## Global Constraints

- Render pull request headings as `GitHub PR #123 · develop ← feat/mon-xxx · Improve review flow`.
- Put the target branch on the left and the source branch on the right.
- Preserve the beginning of a long source branch when the top bar truncates.
- Keep the existing single-row top bar and its file/addition/deletion totals.
- Apply the same heading to the Review Map and code review screens.
- Do not change local review headings, GitHub requests, or persisted data formats.

---

## File Structure

- `src/ui/review.rs`: Owns the shared pull request heading data and label formatting.
- `src/runtime.rs`: Builds the initial heading from loaded pull request metadata.
- `src/app.rs`: Rebuilds the heading when a pull request service is attached.
- `tests/ui_render.rs`: Verifies normal and constrained code-review top bars.
- `tests/ui_review_map.rs`: Verifies that the Review Map uses the same branch relationship.
- `tests/remote_review_flow.rs`: Verifies that attaching a pull request carries both refs into the heading.

### Task 1: Carry and Render Pull Request Branch Context

**Files:**
- Modify: `src/ui/review.rs:18-32`
- Modify: `src/runtime.rs:245-252`
- Modify: `src/app.rs:476-484`
- Test: `tests/ui_render.rs:646-705`
- Test: `tests/ui_review_map.rs:8-26,78-83`
- Test: `tests/remote_review_flow.rs:203-214`

**Interfaces:**
- Consumes: `PullRequestReviewContext::{number, title, base_ref, head_ref}`.
- Produces: `ReviewHeading::PullRequest { number: u64, title: String, base_ref: String, head_ref: String }` and the shared label `GitHub PR #{number} · {base_ref} ← {head_ref} · {title}`.

- [ ] **Step 1: Write the failing rendering and propagation tests**

In `tests/ui_render.rs`, enrich the existing pull request heading fixture and assert that the branch relationship is rendered before the title:

```rust
&ReviewHeading::PullRequest {
    number: 123,
    title: "Improve review flow".into(),
    base_ref: "develop".into(),
    head_ref: "feat/mon-xxx".into(),
},
```

Add this assertion to `review_chrome_keeps_colored_totals_and_progress_visible`:

```rust
assert!(header.contains("develop ← feat/mon-xxx"));
```

Add a constrained-width regression beside `narrow_review_chrome_keeps_totals_and_progress`:

```rust
#[test]
fn constrained_pr_heading_preserves_the_source_branch_prefix() {
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let mut snapshot = controller
        .snapshot(Viewport {
            width: 60,
            height: 8,
        })
        .clone();
    snapshot.total_additions = 200;
    snapshot.total_deletions = 50;
    let theme = ThemeRegistry::default().resolve("tokyo-night", None, false);
    let buffer = render_chrome(
        60,
        4,
        &ReviewHeading::PullRequest {
            number: 123,
            title: "Improve review flow".into(),
            base_ref: "develop".into(),
            head_ref: "feat/mon-very-long-description".into(),
        },
        &snapshot,
        None,
        &theme,
    );
    let frame = text(&buffer);
    let header = frame.lines().next().unwrap();

    assert!(header.contains("develop ← feat/"));
    assert!(!header.contains("description"));
    assert!(header.contains("+200 -50"));
}
```

In `tests/ui_review_map.rs`, add the relationship to the shared heading fixture:

```rust
let heading = ReviewHeading::PullRequest {
    number: 1914,
    title: "Billing proration".into(),
    base_ref: "develop".into(),
    head_ref: "feat/mon-xxx".into(),
};
```

Add `"develop ← feat/mon-xxx"` to the `expected` array in `enriched_map_shows_totals_groups_order_and_progress`.

In `tests/remote_review_flow.rs`, change the expected heading in `attaching_pull_request_sets_the_review_heading` to:

```rust
&ReviewHeading::PullRequest {
    number: 123,
    title: "Improve review flow".into(),
    base_ref: "main".into(),
    head_ref: "feature".into(),
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test --test ui_render --test ui_review_map --test remote_review_flow
```

Expected: compilation fails because `ReviewHeading::PullRequest` does not yet have `base_ref` and `head_ref` fields. This confirms the tests require the new data flow.

- [ ] **Step 3: Implement the shared heading and both construction paths**

In `src/ui/review.rs`, replace the pull request variant and its label arm with:

```rust
PullRequest {
    number: u64,
    title: String,
    base_ref: String,
    head_ref: String,
},
```

```rust
Self::PullRequest {
    number,
    title,
    base_ref,
    head_ref,
} => {
    format!("GitHub PR #{number} · {base_ref} ← {head_ref} · {title}")
}
```

In both `src/runtime.rs` and `src/app.rs`, construct the enriched variant with:

```rust
ReviewHeading::PullRequest {
    number: context.number,
    title: context.title.clone(),
    base_ref: context.base_ref.clone(),
    head_ref: context.head_ref.clone(),
}
```

The three test fixtures changed in Step 1 are the only test-only constructors.
Do not add fallback values or a second formatter.

- [ ] **Step 4: Run the focused tests and verify GREEN**

Run:

```bash
cargo test --test ui_render --test ui_review_map --test remote_review_flow
```

Expected: all tests pass, including normal-width rendering, constrained-width prefix preservation, Review Map rendering, and pull request attachment.

- [ ] **Step 5: Format and run repository-wide verification**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked
git diff --check
```

Expected: every command exits successfully with no warnings or whitespace errors.

- [ ] **Step 6: Commit the implementation**

```bash
git add src/ui/review.rs src/runtime.rs src/app.rs tests/ui_render.rs tests/ui_review_map.rs tests/remote_review_flow.rs
git commit -m "feat: show pull request branches in heading"
```
