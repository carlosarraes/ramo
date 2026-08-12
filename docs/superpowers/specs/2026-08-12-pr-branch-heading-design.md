# Pull Request Branch Heading Design

## Goal

Show the pull request's target and source branches in Ramo's top bar so a
reviewer can immediately see what is being merged where.

## Display

For `ramo pr <NUMBER>`, the shared review heading will use this order:

```text
GitHub PR #123 · develop ← feat/mon-xxx · Improve review flow
```

The target branch appears on the left of the arrow and the source branch on
the right, matching GitHub's base/head relationship. Branch context appears
before the pull request title so it remains useful in constrained terminals.
Local review headings do not change.

The existing cell-width-aware top-bar truncation remains responsible for
fitting the heading beside file and line totals. Because the source branch is
placed before the title, truncation preserves the beginning of a long source
branch, such as `feat/mon-…`, rather than its suffix. No extra header row is
added.

## Implementation

Extend the pull-request variant of `ReviewHeading` with the base and head ref
names already available in `PullRequestReviewContext`. Both construction paths
(`runtime` startup and `App::attach_pull_request`) will copy those values into
the heading. The shared heading label will format the relationship once, so
the Review Map and code review screens stay consistent.

No GitHub request or persisted data format changes are required.

## Testing

Update heading fixtures to include base and head refs. Add rendering assertions
that:

- a normal-width pull request top bar shows `develop ← feat/mon-xxx`;
- narrow rendering preserves totals and safely truncates the heading;
- attaching a pull request transfers both refs into `ReviewHeading`;
- local headings remain unchanged.

Run focused UI and remote-review tests first, followed by formatting, Clippy,
and the full Rust test suite.
