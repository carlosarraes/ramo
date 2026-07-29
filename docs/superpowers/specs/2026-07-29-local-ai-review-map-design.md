# Local AI Review Map Design

**Date:** 2026-07-29

**Status:** Approved in product discussion; awaiting document review

**Scope:** Ramo terminal, Android application, shared Rust core, and a new optional local server

## Summary

Ramo will add a first-class Review Map that explains a pull request before the reviewer enters individual files. It combines an immediate deterministic diff tree with locally generated summaries, logical groups, risk cues, and a recommended review order.

Private source code must not be sent to an external AI provider. A new optional `ramo-server` Rust binary will run on the user's laptop, fetch GitHub pull-request data through the existing `gh` authentication, and call Ollama on localhost. Android reaches that service only through the user's Tailscale network. The terminal and Android clients share the same versioned Review Map contract from `ramo-core`.

The initial model will not be selected by reputation. Ramo will benchmark three local candidates on representative real pull requests and select the best useful model for the available hardware.

## Problem

Large pull requests currently begin as a flat sequence of changed files. Exact line totals and file navigation are available, but the reviewer must reconstruct the shape of the change manually:

- Which files form the core implementation?
- Which changes are tests, generated output, migrations, or documentation?
- Which module should be read first?
- Where are the risky boundaries?
- How much reviewer attention will the pull request require?

GitHub's general-purpose mobile interface is also too dense for the desired quick-review workflow. Ramo's Android application should present the smallest useful mental model of the pull request, then preserve the existing one-file-at-a-time review and comment experience.

## Goals

- Make the Review Map the default entry point for GitHub PR review in terminal and Android.
- Render an exact deterministic map immediately without waiting for AI.
- Enrich the map progressively with useful local-model interpretation.
- Keep source code out of external AI services and out of the Review Map API response.
- Share domain types and deterministic behavior between terminal, Android, and server.
- Continue working when the laptop, Tailscale, Ollama, or the selected model is unavailable.
- Select the default model through a repeatable, private benchmark.
- Preserve all existing diff navigation, commenting, and GitHub publication behavior.

## Non-goals for v1

- Running a model directly on Android.
- Cloud inference, Cloudflare-hosted inference, or OpenCode Go/Zen integration.
- Multi-user or team-hosted Ramo servers.
- AI-generated review comments or automatic verdicts.
- Publishing comments through `ramo-server`; existing clients retain that responsibility.
- AI enrichment for anonymous local branch/worktree diffs. They receive a deterministic map only.
- GitLab or Bitbucket Review Map fetching. The initial server integration is GitHub-only.
- Replacing exact classifications or diff facts with model output.

## Product Decisions

1. The Android PR entry flow is **Review Map first**.
2. `ramo pr <number>` is also Review Map first.
3. Plain local diff commands retain direct-to-code startup; `M` opens their deterministic map.
4. Review remains unified and one file at a time after leaving the map.
5. AI analysis starts automatically when a PR is opened.
6. Deterministic content renders immediately; enrichment updates in place.
7. Private PR content is processed only on the laptop through local Ollama.
8. Android connects to the laptop through Tailscale.
9. `ramo-server` is a separate Rust binary in the existing Ramo workspace.
10. The installer may install the optional server companion and configure a user-level background service.
11. The server owns GitHub fetching and uses the laptop's existing `gh` authentication.
12. Recent Mondrio PRs may be used as local-only benchmark fixtures.

## High-level Architecture

```text
Android app                                  Ramo terminal
    |                                             |
    | PR identity + expected head SHA             | PR identity + expected head SHA
    +------------------+--------------------------+
                       |
                       v
             Tailscale-only Review Map API
                       |
                       v
                 ramo-server (Rust)
                 |       |       |
                 |       |       +-- bounded structured-result cache
                 |       +---------- Ollama on localhost
                 +------------------ GitHub through gh authentication
                       |
                       v
              versioned ReviewMap response

Both clients also compile ramo-core's deterministic planner and can construct the
exact fallback map from an already-fetched PR snapshot without the server.
```

### Workspace boundaries

#### `ramo-core`

Owns the provider-neutral and UI-neutral domain:

- Versioned Review Map types
- Exact totals and tree construction
- File-kind classification
- CODEOWNERS-derived ownership
- Stable group and file identifiers
- AI enrichment request/response contracts
- Validation and deterministic merge of enrichment into the exact map
- Cache identity derivation
- UniFFI-safe projections used by Android

`ramo-core` must not know about HTTP, `gh`, Ollama, Ratatui, Compose, Tailscale, or disk layout.

#### `ramo-github`

Continues to own GitHub-specific data fetching and translation. It provides the server with PR metadata, file inventories, patches, head SHA, base SHA, repository identity, and CODEOWNERS content where available. Authentication remains delegated to `gh`.

#### `ramo-server`

A new workspace binary that owns:

- The private HTTP API
- Tailscale-facing setup and pairing
- GitHub fetch orchestration
- Background analysis jobs
- Ollama interaction
- Input budgeting and hierarchical analysis
- Schema validation and retries
- The bounded structured-result cache
- Health and actionable status reporting
- User-service installation and lifecycle commands
- The local model benchmark harness

#### Terminal `ramo`

Owns the Ratatui Review Map mode, its keyboard behavior, progressive status updates, and transition into the existing diff review.

#### `ramo-mobile` and Android

`ramo-mobile` exposes the shared deterministic contract through UniFFI. Android owns pairing storage, Review Map presentation, polling, dismissible status UI, navigation, and preservation of local review progress.

## Review Map Domain Model

The exact Rust spelling may change during implementation, but the contract must retain these conceptual fields:

```text
ReviewMap
  schema_version
  identity: repository, PR number, base SHA, head SHA
  source: deterministic | enriched
  status: ready | analyzing | enriched | stale | unavailable | failed
  totals: files, additions, deletions, authored, tests, generated, migrations
  groups: ReviewGroup[]
  files: ReviewFile[]
  coverage: included paths, truncated paths, omitted patch paths
  analysis: optional model/prompt/schema versions and completion timestamp

ReviewGroup
  stable_id
  label
  exact classification
  file_ids
  additions/deletions
  collapsed_by_default
  optional summary, risk cue, and review priority

ReviewFile
  stable_id and exact path
  additions/deletions and status
  deterministic kind and optional owner
  optional AI summary, risk cue, and recommended order
```

Review progress is client presentation state, not server truth. Each client stores reviewed-file state keyed by repository, PR, and head SHA. A new head SHA invalidates that progress according to existing review rules and always invalidates AI enrichment.

### Stable identifiers

File identifiers derive from normalized repository-relative paths and the head SHA. Deterministic group identifiers derive from their classification and normalized path prefix. AI may propose labels and memberships, but `ramo-core` assigns final identifiers after validation.

## Deterministic Planner

The deterministic planner guarantees a useful map without AI. It will:

1. Validate and normalize the file inventory.
2. Calculate exact totals.
3. Reuse Ramo's existing test-file classification behavior.
4. Identify generated files through established path/name patterns and diff metadata.
5. Identify migrations and documentation through configurable deterministic patterns.
6. Resolve ownership from CODEOWNERS when available.
7. Group remaining authored files by meaningful directory prefix without creating single-child noise.
8. Mark tests and generated groups collapsed by default.
9. Preserve every changed file exactly once.

The following invariants are mandatory:

- Every changed file appears in exactly one deterministic group.
- All totals equal the sum of the underlying exact file statistics.
- No generated or AI field can change exact paths, statuses, or counts.
- A failed classifier falls back to an authored/other group rather than dropping a file.
- Group construction is deterministic for a given normalized input.

User-configurable test/generated patterns are a later extension. V1 should structure the rule system so configuration can be added without changing the Review Map schema.

## AI Enrichment Contract

The model may interpret the diff but must never become the source of exact repository facts.

### Allowed AI output

- Human-readable logical group labels
- Brief group and file summaries
- Suggested review ordering
- Evidence-based risk cues
- Relationships between existing files

### Forbidden AI authority

- Inventing, renaming, or removing files
- Modifying additions/deletions or file status
- Assigning a file more than once
- Claiming patch coverage that was not supplied
- Replacing deterministic test/generated/migration classification
- Producing comments or a review verdict

### Validation

Ollama receives a strict JSON schema. `ramo-core` additionally validates semantic invariants:

- Every referenced path exists in the supplied inventory.
- All group and relationship references resolve.
- Membership is unique and complete after deterministic fallback.
- Review order contains valid unique identifiers.
- Text fields remain within configured limits.
- The response declares which supplied inputs it used.

Malformed JSON, unknown paths, invalid references, duplicate assignments, or oversized text reject the entire enrichment. The server retries once with a repair instruction. A second failure produces a typed failure state and leaves the deterministic map intact; partially trusted structure is never displayed.

## Context Budgeting and Large PRs

Ramo must not depend on feeding an entire large PR into one prompt.

1. Build the exact map and deterministic groups first.
2. Prioritize authored implementation and migration patches.
3. Omit generated patch bodies by default while retaining their exact metadata.
4. Budget tests after the implementation they cover.
5. Split oversized groups into bounded batches.
6. Produce validated per-batch summaries and file insights.
7. Run a final synthesis over structured batch results rather than raw patches.

The Review Map exposes coverage so the UI can distinguish a summary based on complete patches from one based on truncated or metadata-only files. No UI copy may imply complete analysis when inputs were omitted.

Only one GPU-heavy model job runs at a time on the initial target laptop. Requests for the same cache key coalesce into one job. A newer head SHA supersedes queued work for the old SHA.

## Model Benchmark

### Candidates

- `qwen3:8b`: speed/default candidate
- `qwen3-coder:30b`: quality candidate
- `qwen2.5-coder:7b`: smaller baseline

The benchmark command is `ramo server benchmark` (implemented by the server binary or a thin forwarding subcommand). It uses the same prompt, JSON schema, context budgets, generation parameters, and hardware for every candidate.

### Corpus

Use 6–10 representative PRs, including recent PRs from:

```text
/home/carraes/mondrio/mondrio-platform
```

The corpus should cover:

- A small focused change
- A multi-module feature
- A test-heavy PR
- A migration or schema change
- A generated-code-heavy PR
- A large PR requiring hierarchical analysis
- A refactor with moved or renamed files

The corpus manifest, patches, model outputs, and scoring artifacts remain in a permission-restricted ignored directory. No private fixture or generated summary is committed.

### Hard metrics

- Wall-clock latency and time to first valid result
- Peak RAM and GPU allocation where measurable
- JSON-schema success rate
- Semantic validation failure rate
- Invented-reference rate
- Retry rate
- Successful completion rate for large PRs

### Usefulness evaluation

Generate anonymous side-by-side outputs so the evaluator cannot see model names. Score:

- Logical grouping
- Summary accuracy and specificity
- Recommended review order
- Useful risk cues
- Noise and repetition
- Whether the result materially improves review orientation

The selected default is the highest-quality candidate that remains practical on the target laptop. The benchmark report records the decision and measured trade-offs, but not private outputs. Users retain a model override, and the benchmark can be rerun after hardware or model changes.

No model is selected in this design document; selection is an explicit implementation milestone backed by benchmark results.

## Server API

The API is versioned under `/v1` and uses JSON request/response envelopes. The exact resource layout may be refined without changing these semantics:

- `GET /v1/health`: service, GitHub, Ollama, model, cache, and version status
- `POST /v1/pair/exchange`: exchange a short-lived QR pairing code for a client token
- `POST /v1/review-maps`: idempotently resolve or start a map for repository + PR + expected head SHA
- `GET /v1/review-maps/{job_id}`: retrieve deterministic/enriched status and result
- `POST /v1/review-maps/{job_id}/retry`: retry an unavailable or failed enrichment
- `DELETE /v1/clients/{client_id}`: revoke a paired client

Creating a Review Map returns promptly with a job identifier and any cached valid result. Clients poll only while the Review Map is visible, using bounded backoff. V1 does not require WebSockets or push delivery.

Responses contain structured Review Map fields and operational status only. They never return raw model prompts or raw patch bodies.

## Pairing and Network Security

`ramo server setup` performs a guided dependency check and configuration:

1. Resolve and verify `gh`, Tailscale, Ollama, and the user service manager.
2. Install and start a user-level `ramo-server` service.
3. Keep the server bound to loopback.
4. Configure Tailscale Serve to expose the API only inside the user's tailnet through its stable MagicDNS endpoint.
5. Generate a short-lived, single-use pairing code and render a QR deep link containing the endpoint and code.
6. Exchange the code for a random long-lived client token.
7. Store the client token in Android encrypted storage.

The pairing endpoint is available only through the tailnet and only while a short-lived code exists. Client tokens are individually revocable and rotatable. Server-side token material and configuration use user-only filesystem permissions. Authentication comparisons must be timing-safe.

The server reuses `gh` authentication rather than copying a GitHub token into Ramo configuration. Setup records or validates the resolved executable environment needed by the user service.

Ollama remains bound to localhost and is never exposed directly to Tailscale or Android.

## Cache and Local Privacy

The cache stores only validated structured Review Map results and operational metadata. It does not persist raw patches or prompts.

Cache identity includes:

- Repository identity
- PR number
- Head SHA
- Model identifier/digest
- Generation parameters relevant to output
- Prompt version
- Review Map schema version
- Deterministic-classifier version

V1 uses atomic, permission-restricted cache files rather than introducing a database. Entries are bounded by total size and age and can be listed or cleared through server commands. Repository names do not need to appear in filenames; hashed cache keys avoid incidental disclosure through directory listings.

Logs must redact authorization headers, pairing codes, tokens, prompts, patches, and model responses. Operational logs may contain job identifiers, durations, byte/token counts, model name, cache outcome, and typed error category.

## Android Experience

Opening a PR lands on the dedicated Review Map.

### Immediate state

The Android client uses `ramo-core` through UniFFI to build an exact local map from its existing PR snapshot. It shows:

- PR title and repository
- File, addition, and deletion totals
- Deterministic folders and classifications
- Authored files expanded
- Tests and generated files collapsed
- Migration and ownership markers
- A compact “building local AI summary” state

Review can start immediately.

### Enriched state

When the server returns a valid enrichment, the stable layout updates in place with summaries, logical group labels, risk cues, and recommended order. AI text is subtly labeled and never replaces exact statistics.

The primary action opens the first recommended file. Tapping any file begins there. Returning from code preserves map scroll position, expansion state, reviewed-file progress, and the current valid enrichment.

### Offline or failed state

The deterministic map remains fully usable. A compact message distinguishes laptop unreachable, pairing failure, GitHub auth failure, Ollama unavailable, missing model, validation failure, or stale result. Retry and Dismiss are always available where meaningful. Dismissing the message persists for the current PR/head SHA but does not disable future analysis globally.

### Changed PR

If the head SHA changes, the old enrichment is visibly stale and is never merged into the new exact map. The deterministic map refreshes immediately and a new enrichment starts automatically.

## Terminal Experience

`ramo pr <number>` opens map mode first. A cached enrichment matching the head SHA appears immediately; otherwise deterministic structure renders while analysis runs.

Map-mode bindings:

| Key | Action |
|---|---|
| `j` / `k` | Move between groups and files |
| `h` / `l` | Collapse or expand the selected group |
| `Enter` | Review the selected file or the group's first recommended file |
| `/` | Filter groups and files |
| `r` | Retry or rerun analysis in map mode |
| `M` | Toggle between Review Map and code |
| `Esc` | Return from code to the map when the PR began map-first |
| `?` | Show map-specific help |

`M` is intentionally uppercase because lowercase `m` already toggles hunk headers. Once a file opens, all existing review bindings retain their current meaning.

Plain local branch/worktree diffs keep direct-to-code startup. `M` opens their deterministic map, but no AI enrichment runs in v1 because there is no canonical GitHub PR identity/cache key.

## Error Model

Errors crossing a process or UniFFI boundary use stable categories plus human-readable context:

- `server_unreachable`
- `pairing_rejected`
- `client_unauthorized`
- `github_auth_unavailable`
- `github_request_failed`
- `pr_not_found_or_forbidden`
- `ollama_unavailable`
- `model_missing`
- `analysis_timed_out`
- `analysis_invalid`
- `analysis_failed`
- `result_stale`
- `cache_unavailable`
- `server_incompatible`

No recoverable operational failure may panic the server, terminate Android, close the terminal UI, discard comments, or remove the deterministic map. Errors are dismissible and retryable where appropriate.

Protocol compatibility is explicit. A client that encounters a newer unsupported schema shows `server_incompatible` and falls back to deterministic behavior rather than attempting a partial decode.

## Installation and Lifecycle

The normal terminal binary remains independently installable and usable. `ramo-server` is an optional companion artifact for local AI and mobile enrichment.

On Linux, setup installs a `systemd --user` service with restart-on-failure and conservative resource limits. The service starts automatically for the logged-in user. Shutdown must allow the active cache write to finish atomically; model work may be cancelled and retried.

The installer reports the installed Ramo/server version and whether server setup remains pending. Large model downloads always show model names and expected sizes and require explicit confirmation.

## Testing Strategy

### `ramo-core`

- Fixture tests for exact grouping and totals
- Property tests proving every file appears exactly once
- Classification precedence tests
- CODEOWNERS ownership tests
- Stable identifier and cache-key tests
- AI semantic validation tests
- Schema compatibility tests
- UniFFI round-trip tests

### `ramo-server`

- Fake `gh` integration for PR fetch success, pagination, forbidden/not-found, and auth expiry
- Fake Ollama integration for valid output, malformed JSON, invented paths, duplicates, timeout, missing model, and retry
- Coalescing and stale-head cancellation tests
- Atomic cache write, eviction, corruption, and permission tests
- Pair-code expiry, single-use exchange, token revocation, and authorization tests
- Log-redaction tests
- Service setup tests isolated from the real user service

### Terminal

- Rendering snapshots for deterministic, analyzing, enriched, stale, offline, and failed maps
- Key-mapping tests for map mode
- PTY tests for map-to-file-to-map state preservation
- Regression coverage for navigation, filtering, context expansion, comments, publication, and exit

### Android

- Compose tests for all Review Map states
- Navigation and process-recreation state tests
- Pairing and encrypted-storage tests
- Polling cancellation/backoff tests
- Stale SHA and dismissed-message tests
- Existing review/comment publication regressions

### End to end

- Run the benchmark against private local fixtures without committing artifacts
- Open and review a real PR from terminal and Android
- Verify laptop sleep/wake and user-service restart
- Disconnect and reconnect Tailscale
- Stop and restart Ollama
- Expire GitHub auth
- Push a new PR commit during analysis
- Publish an existing inline comment after entering through the Review Map
- Install and validate the APK on the physical Samsung device

## Delivery Slices

1. **Core contract:** exact map, classifiers, ownership, validation, cache identity, UniFFI projection.
2. **Server:** API, GitHub fetch, local jobs, Ollama adapter, cache, pairing, setup, and service lifecycle.
3. **Benchmark:** local corpus harness, measurements, blind comparison, and documented model selection.
4. **Terminal:** map-first PR flow, map mode, progressive updates, and regressions.
5. **Android:** Review Map screen, pairing, progressive state, errors, and physical-device acceptance.

Each slice must pass its own tests and leave existing review paths usable before the next slice begins.

## Acceptance Criteria

- A PR shows an exact Review Map without waiting for AI.
- A valid cached enrichment appears only when repository, PR, head SHA, model, prompt, schema, and classifier versions match.
- No external AI endpoint receives source or patch content.
- Android can obtain enrichment through the private tailnet without direct Ollama access.
- Tests and generated files begin collapsed but remain expandable.
- Recommended order never references an unknown file.
- Any server/model/network failure preserves deterministic review and comments.
- Terminal and Android agree on exact totals, file membership, and classifications.
- Model selection is backed by a repeatable benchmark report.
- The feature works on the physical phone across laptop/Tailscale lifecycle changes.

## References

- [Ollama structured outputs](https://docs.ollama.com/capabilities/structured-outputs)
- [Ollama quickstart and supported desktop platforms](https://docs.ollama.com/quickstart)
- [Qwen3 models in Ollama](https://ollama.com/library/qwen3)
- [Qwen3 Coder models in Ollama](https://ollama.com/library/qwen3-coder)
- [Qwen2.5 Coder models in Ollama](https://ollama.com/library/qwen2.5-coder/tags)
- [Tailscale Services](https://tailscale.com/docs/features/tailscale-services)
- [Tailscale Android installation](https://tailscale.com/docs/install/android)
- [OpenCode Go documentation](https://opencode.ai/docs/go/) — evaluated and rejected for this private-code path
