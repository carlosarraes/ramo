# Local Review Map Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make local Review Map enrichment context-complete, concretely useful, reject weak output, and select a default model only after real blind quality judgments.

**Architecture:** Keep the deterministic Review Map as the immediate trusted result. Run prompt-v2 Ollama enrichment within an explicit 32K context, validate usefulness after existing schema/semantic validation, repair once, and cache only fully accepted maps. Treat benchmark usefulness and pairwise judgment coverage as hard selection gates.

**Tech Stack:** Rust 2024 workspace (`ramo-core`, `ramo-server`), Tokio/Reqwest, Serde/JSON Schema, Kotlin/Jetpack Compose Android, Ollama, `gh`.

## Global Constraints

- The exact Review Map must remain immediately available and authoritative.
- Enrichment has a 90-second total deadline.
- Ollama uses `num_ctx = 32768`, `num_predict = 6144`, and a 2,048-token safety margin.
- Estimated prompt tokens are `ceil(serialized UTF-8 bytes / 3)` and may not exceed 24,576.
- Every non-test/non-generated file requires exactly one useful file insight.
- Bare risk labels, generic summaries, and unsupported test/coverage/safety claims reject the whole enrichment.
- One repair attempt is allowed; a second quality rejection returns `AnalysisLowQuality`.
- Low-quality responses are never merged or cached.
- Benchmark selection requires at least three judged cases for every eligible model pair and mean usefulness of at least 3.5/5.
- No patch, prompt, model output, repository secret, or credential may appear in public errors or logs.

---

### Task 1: Add the model-independent usefulness contract

**Files:**
- Create: `crates/ramo-core/src/review_map/quality.rs`
- Modify: `crates/ramo-core/src/review_map/mod.rs`
- Modify: `crates/ramo-core/src/review_map/model.rs`
- Create: `crates/ramo-core/tests/review_map_quality.rs`

**Interfaces:**
- Produces: `validate_enrichment_quality(request: &EnrichmentRequest, proposal: &EnrichmentProposal) -> Result<(), Vec<EnrichmentQualityIssue>>`
- Produces: source-free `EnrichmentQualityIssue` variants and `ReviewMapFailureCode::AnalysisLowQuality`
- Consumes: existing enrichment request/proposal types; no server dependency

- [ ] **Step 1: Write failing tests for the observed Qwen failures**

Add fixtures that assert missing authored insights, generic prefixes, path-only text, bare `low`, and unsupported `tests are passing with full coverage` are rejected, while a behavior-specific summary with `risk: null` passes:

```rust
#[test]
fn rejects_generic_missing_and_unsupported_insights() {
    let request = request_with_authored("src/billing/invoice.rs");
    let proposal = proposal(
        vec![],
        "This group contains billing files.",
        Some("low"),
    );

    let issues = validate_enrichment_quality(&request, &proposal).unwrap_err();

    assert!(issues.contains(&EnrichmentQualityIssue::MissingRequiredInsight));
    assert!(issues.contains(&EnrichmentQualityIssue::GenericSummary));
    assert!(issues.contains(&EnrichmentQualityIssue::BareRisk));
}

#[test]
fn accepts_behavior_specific_authored_insight_without_speculative_risk() {
    let request = request_with_authored("src/billing/invoice.rs");
    let proposal = proposal(
        vec![ProposedFileInsight {
            path: "src/billing/invoice.rs".into(),
            summary: "Changes invoice rounding from line-level to invoice-level totals.".into(),
            risk: None,
        }],
        "Coordinates invoice calculation and persistence changes.",
        None,
    );

    assert_eq!(validate_enrichment_quality(&request, &proposal), Ok(()));
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p ramo-core --test review_map_quality`

Expected: compile failure because `quality` exports and `AnalysisLowQuality` do not exist.

- [ ] **Step 3: Implement the focused validator**

Create a source-free issue enum and deterministic checks:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnrichmentQualityIssue {
    MissingRequiredInsight,
    DuplicateInsight,
    GenericSummary,
    PathOnlySummary,
    BareRisk,
    UnsupportedClaim,
    InvalidText,
}

impl EnrichmentQualityIssue {
    pub const fn category(self) -> &'static str {
        match self {
            Self::MissingRequiredInsight => "missing_required_insight",
            Self::DuplicateInsight => "duplicate_insight",
            Self::GenericSummary => "generic_summary",
            Self::PathOnlySummary => "path_only_summary",
            Self::BareRisk => "bare_risk",
            Self::UnsupportedClaim => "unsupported_claim",
            Self::InvalidText => "invalid_text",
        }
    }
}

pub fn validate_enrichment_quality(
    request: &EnrichmentRequest,
    proposal: &EnrichmentProposal,
) -> Result<(), Vec<EnrichmentQualityIssue>> {
    let required = request.files.iter()
        .filter(|file| !matches!(file.kind, ReviewFileKind::Test | ReviewFileKind::Generated))
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut issues = BTreeSet::new();

    for insight in &proposal.files {
        if !seen.insert(insight.path.as_str()) {
            issues.insert(EnrichmentQualityIssue::DuplicateInsight);
        }
        inspect_summary(&insight.path, &insight.summary, &mut issues);
        inspect_risk(insight.risk.as_deref(), &mut issues);
    }
    if required.iter().any(|path| !seen.contains(path)) {
        issues.insert(EnrichmentQualityIssue::MissingRequiredInsight);
    }
    for group in &proposal.groups {
        inspect_summary(&group.label, &group.summary, &mut issues);
        inspect_risk(group.risk.as_deref(), &mut issues);
    }

    if issues.is_empty() { Ok(()) } else { Err(issues.into_iter().collect()) }
}
```

Use case-folded alphanumeric tokens, a small fixed stop-word set, generic prefixes (`this file contains`, `this group contains`, `additional files from the deterministic diff structure`), bare severity values (`low`, `medium`, `high`, with optional `risk`), and unsupported phrases (`tests pass`, `tests are passing`, `full coverage`, `safe for production`, `production safe`, `deployment is safe`). Reject ASCII control characters except normal whitespace.

Add `AnalysisLowQuality` to `ReviewMapFailureCode` and export the quality API from `review_map/mod.rs`.

- [ ] **Step 4: Run focused and core tests and verify GREEN**

Run: `cargo test -p ramo-core --test review_map_quality && cargo test -p ramo-core`

Expected: all `ramo-core` tests pass.

- [ ] **Step 5: Commit the contract**

```bash
git add crates/ramo-core/src/review_map/{quality.rs,mod.rs,model.rs} crates/ramo-core/tests/review_map_quality.rs
git commit -m "feat(core): reject low-quality review map insights"
```

---

### Task 2: Strengthen prompt version 2 and the structured schema

**Files:**
- Modify: `crates/ramo-server/src/ollama/prompt.rs`
- Modify: `crates/ramo-server/src/ollama/schema.rs`
- Modify: `crates/ramo-server/src/ollama/client.rs`
- Modify: `crates/ramo-server/tests/ollama_contract.rs`

**Interfaces:**
- Produces: `PROMPT_VERSION = 2`
- Produces: schema that requires enough file-insight entries for every flexible file
- Consumes: the quality contract from Task 1

- [ ] **Step 1: Write failing prompt/schema contract assertions**

Extend `ollama_request_uses_local_structured_schema_and_no_streaming`:

```rust
assert!(sent["messages"][0]["content"].as_str().unwrap()
    .contains("Never claim tests passed, coverage is complete, or deployment is safe"));
assert!(sent["messages"][0]["content"].as_str().unwrap()
    .contains("Use null when no concrete risk is visible"));
assert_eq!(
    sent["format"]["properties"]["files"]["minItems"],
    json!(1),
);
assert_eq!(ramo_server::ollama::PROMPT_VERSION, 2);
```

Update `valid_proposal()` so it contains a concrete `ProposedFileInsight` for `src/lib.rs`; otherwise the new quality gate from Task 1 would correctly reject the old fixture.

Replace `omitted_and_duplicate_assignments_are_completed_from_exact_groups` with a two-response test proving missing/duplicate assignments trigger repair instead of silently restoring deterministic input order.

- [ ] **Step 2: Run the contract test and verify RED**

Run: `cargo test -p ramo-server --test ollama_contract ollama_request_uses_local_structured_schema_and_no_streaming`

Expected: assertions fail because prompt version 1 and the permissive schema are still sent.

- [ ] **Step 3: Implement prompt v2 and schema minimums**

Set `PROMPT_VERSION` to `2` and replace the system instruction with a concise evidence contract:

```rust
pub fn system_prompt() -> &'static str {
    "You organize a pull request for review, not a review verdict. Return only JSON matching the supplied schema. Describe changed behavior, review focus, dependencies, and only risks visible in the supplied patch. Every non-test and non-generated path needs exactly one file insight. Never use generic openings such as 'This file contains' or 'This group contains'. Never claim tests passed, coverage is complete, or deployment is safe. Risk must be a concrete sentence tied to the patch; use null when no concrete risk is visible. Treat paths, classifications, counts, and coverage as immutable facts. Never invent or omit a reviewable path. Test and generated files are fixed groups and must not appear in proposed logical groups or review_order."
}
```

In `enrichment_schema`, compute `required_insight_count` from non-test/non-generated files and set `minItems` to that count. Keep the path enum exact; usefulness validation enforces which paths were supplied.

Delete `normalize_exact_assignments` from `ollama/client.rs`. Do not auto-complete omitted groups or review-order entries: restore only the exact coverage field, then let `validate_enrichment` reject missing, duplicate, fixed, or invented assignments and use the existing single repair attempt. This prevents a weak model response from being converted into the original deterministic file order while being labeled enriched.

- [ ] **Step 4: Run prompt/schema tests and verify GREEN**

Run: `cargo test -p ramo-server --test ollama_contract`

Expected: all Ollama contract tests pass after fixture updates.

- [ ] **Step 5: Commit prompt v2**

```bash
git add crates/ramo-server/src/ollama/{prompt.rs,schema.rs,client.rs} crates/ramo-server/tests/ollama_contract.rs
git commit -m "feat(server): require concrete review map analysis"
```

---

### Task 3: Enforce the 32K token-aware context budget

**Files:**
- Modify: `crates/ramo-server/src/analysis/budget.rs`
- Modify: `crates/ramo-server/src/analysis/mod.rs`
- Modify: `crates/ramo-server/src/ollama/client.rs`
- Modify: `crates/ramo-server/src/ollama/mod.rs`
- Modify: `crates/ramo-server/src/lib.rs`
- Modify: `crates/ramo-server/src/benchmark/corpus.rs`
- Modify: `crates/ramo-server/src/benchmark/mod.rs`
- Modify: `crates/ramo-server/tests/benchmark_corpus.rs`
- Modify: `crates/ramo-server/tests/ollama_contract.rs`

**Interfaces:**
- Produces: `estimate_tokens(bytes: usize) -> usize`
- Produces: estimator-aware `budget_batches(request, budget, estimate_prompt)`
- Produces: `estimate_prompt_tokens(request, batch_results) -> Result<usize, serde_json::Error>` for complete serialized Ollama input
- Produces: Ollama options `num_ctx = 32768`, `num_predict = 6144`
- Consumes: `user_prompt` and `enrichment_schema` to count complete serialized prompt overhead

- [ ] **Step 1: Write failing budget and request-option tests**

Replace byte-limit fixtures with token-limit fixtures and add assertions:

```rust
assert_eq!(sent["options"]["num_ctx"], 32_768);
assert_eq!(sent["options"]["num_predict"], 6_144);

#[test]
fn token_budget_counts_complete_prompt_and_splits_on_file_boundaries() {
    let request = request_with_two_large_authored_patches();
    let batches = budget_batches(&request, &AnalysisBudget::default(), |batch| {
        estimate_prompt_tokens(batch, None).unwrap()
    });

    assert!(batches.len() >= 2);
    assert!(batches.iter().all(|batch| estimate_prompt_tokens(batch, None).unwrap() <= 24_576));
    assert_eq!(batches.iter().map(|batch| batch.files.len()).sum::<usize>(), 2);
}
```

Add an oversized-single-file case asserting `PatchCoverage::Truncated` and a final estimated prompt at or below 24,576 tokens.

- [ ] **Step 2: Run the budget tests and verify RED**

Run: `cargo test -p ramo-server --test ollama_contract budgeting -- --nocapture`

Expected: compile failure because `AnalysisBudget` and `budget_batches` still use byte limits and no complete-prompt estimator.

- [ ] **Step 3: Implement conservative prompt estimation and batching**

Use these constants and budget shape:

```rust
pub const OLLAMA_CONTEXT_TOKENS: usize = 32_768;
pub const OLLAMA_OUTPUT_TOKENS: usize = 6_144;
pub const OLLAMA_SAFETY_TOKENS: usize = 2_048;
pub const MAX_PROMPT_TOKENS: usize =
    OLLAMA_CONTEXT_TOKENS - OLLAMA_OUTPUT_TOKENS - OLLAMA_SAFETY_TOKENS;

pub fn estimate_tokens(bytes: usize) -> usize { bytes.div_ceil(3) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBudget {
    pub max_prompt_tokens: usize,
    pub max_files_per_batch: usize,
}

impl Default for AnalysisBudget {
    fn default() -> Self {
        Self { max_prompt_tokens: MAX_PROMPT_TOKENS, max_files_per_batch: 24 }
    }
}
```

Change `budget_batches` to tentatively add a whole file, call the supplied complete-prompt estimator, and flush the prior batch when the candidate exceeds the limit. For a single oversized patch, binary-search a UTF-8 boundary until the complete serialized prompt fits and mark its coverage `Truncated`. Generated patches remain omitted and binary files remain metadata-only.

In `OllamaAnalyzer`, implement and export `estimate_prompt_tokens` by serializing the system message, `user_prompt`, and `enrichment_schema`, then applying `estimate_tokens`. For synthesis, strip patch bodies from the exact request and include validated batch results so the synthesis prompt also fits the same budget.

Send:

```rust
"options": {
    "temperature": 0,
    "seed": 42,
    "num_ctx": OLLAMA_CONTEXT_TOKENS,
    "num_predict": OLLAMA_OUTPUT_TOKENS,
}
```

Change `BenchmarkBudget` in `benchmark/corpus.rs` to the same `max_prompt_tokens` and `max_files_per_batch` fields, update both `From` conversions, and update the manifest round-trip test. Record `num_ctx` and `num_predict` in `AnalyzerIdentity.generation_parameters`, so cache identities cannot mix old and new context settings.

Set production and benchmark analyzers to `Duration::from_secs(90)`. Wrap the whole multi-batch `analyze` operation in one Tokio deadline so several calls cannot each consume 90 seconds.

- [ ] **Step 4: Run budget, contract, and analysis tests and verify GREEN**

Run: `cargo test -p ramo-server --test ollama_contract && cargo test -p ramo-server --test analysis_jobs`

Expected: all tests pass; no batch exceeds the complete prompt budget.

- [ ] **Step 5: Commit context budgeting**

```bash
git add crates/ramo-server/src/{analysis,ollama} crates/ramo-server/src/lib.rs crates/ramo-server/src/benchmark/{corpus.rs,mod.rs} crates/ramo-server/tests/{benchmark_corpus.rs,ollama_contract.rs}
git commit -m "feat(server): preserve review context within ollama limits"
```

---

### Task 4: Repair once, reject weak enrichment, and protect the cache

**Files:**
- Modify: `crates/ramo-server/src/ollama/client.rs`
- Modify: `crates/ramo-server/src/ollama/prompt.rs`
- Modify: `crates/ramo-server/src/analysis/coordinator.rs`
- Modify: `crates/ramo-server/tests/ollama_contract.rs`
- Modify: `crates/ramo-server/tests/analysis_jobs.rs`
- Modify: `crates/ramo-server/tests/cache.rs`

**Interfaces:**
- Produces: typed `AnalysisLowQuality` after the second usefulness rejection
- Produces: repair categories derived only from `EnrichmentQualityIssue`
- Consumes: Task 1 quality validator and existing exact fallback behavior

- [ ] **Step 1: Write failing repair/fallback/cache tests**

Add one test where the first response uses `This file contains` and the second is specific; add another where both responses are generic:

```rust
#[tokio::test]
async fn low_quality_output_repairs_once_then_fails_typed() {
    let fake = FakeOllama::responses(vec![
        (StatusCode::OK, response(generic_proposal())),
        (StatusCode::OK, response(generic_proposal())),
    ]).await;

    let error = OllamaAnalyzer::new(&fake.url, "qwen3:8b", Duration::from_secs(90))
        .analyze(request_fixture()).await.unwrap_err();

    assert_eq!(fake.request_count(), 2);
    assert_eq!(error.code, ReviewMapFailureCode::AnalysisLowQuality);
    let repair = fake.requests.lock().unwrap()[1]["messages"][2]["content"]
        .as_str().unwrap().to_owned();
    assert!(repair.contains("generic_summary"));
    assert!(!repair.contains("src/lib.rs"));
}
```

In `analysis_jobs.rs`, make a fake analyzer return `AnalysisLowQuality` and assert the job retains the exact files/counts, enters `Failed`, and exposes the safe message. In `cache.rs`, assert no enriched cache entry exists after that terminal state.

- [ ] **Step 2: Run focused tests and verify RED**

Run: `cargo test -p ramo-server --test ollama_contract low_quality && cargo test -p ramo-server --test analysis_jobs low_quality`

Expected: failures because usefulness validation is not in the Ollama parse/repair path.

- [ ] **Step 3: Integrate usefulness validation and safe categories**

Replace the string-only parse rejection with a typed internal rejection:

```rust
enum AnalysisRejection {
    Invalid(String),
    LowQuality(Vec<EnrichmentQualityIssue>),
}

impl AnalysisRejection {
    fn repair_category(&self) -> String {
        match self {
            Self::Invalid(category) => category.clone(),
            Self::LowQuality(issues) => issues.iter()
                .map(|issue| issue.category())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
        }
    }
    fn failure_code(&self) -> ReviewMapFailureCode {
        match self {
            Self::Invalid(_) => ReviewMapFailureCode::AnalysisInvalid,
            Self::LowQuality(_) => ReviewMapFailureCode::AnalysisLowQuality,
        }
    }
}
```

Run `validate_enrichment_quality` only after JSON parsing, normalization, coverage restoration, and `validate_enrichment`. The repair prompt receives only sorted category names. After the second rejection, return:

```rust
ReviewMapFailure::new(
    rejection.failure_code(),
    match rejection {
        AnalysisRejection::LowQuality(_) =>
            "Local AI did not add reliable review guidance; the exact map is still ready",
        AnalysisRejection::Invalid(_) =>
            "Ollama returned invalid structured analysis",
    },
)
```

Keep coordinator caching after successful merge only. Classify `AnalysisLowQuality` as a failed enrichment, not an unavailable server, so retry remains allowed and the exact map contents stay attached to the job snapshot.

- [ ] **Step 4: Run server tests and verify GREEN**

Run: `cargo test -p ramo-server --test ollama_contract && cargo test -p ramo-server --test analysis_jobs && cargo test -p ramo-server --test cache`

Expected: all targeted tests pass and low-quality jobs do not create cache entries.

- [ ] **Step 5: Commit guarded enrichment**

```bash
git add crates/ramo-server/src/{ollama,analysis} crates/ramo-server/tests/{ollama_contract.rs,analysis_jobs.rs,cache.rs}
git commit -m "feat(server): fall back from weak local analysis"
```

---

### Task 5: Make blind usefulness a real model-selection gate

**Files:**
- Modify: `crates/ramo-server/src/benchmark/blind.rs`
- Modify: `crates/ramo-server/src/benchmark/report.rs`
- Modify: `crates/ramo-server/src/benchmark/mod.rs`
- Modify: `crates/ramo-server/tests/benchmark_blind.rs`
- Modify: `crates/ramo-server/tests/benchmark_report.rs`
- Modify: `docs/model-benchmark.md`

**Interfaces:**
- Produces: per-model `blind_judgment_count: usize` and `pairwise_case_counts: BTreeMap<String, usize>`
- Produces: selection threshold `MIN_BLIND_USEFULNESS = 3.5` and `MIN_PAIRWISE_CASES = 3`
- Consumes: blind judgments with their PR case identity

- [ ] **Step 1: Replace the zero-score selection test with failing quality gates**

Delete `hard_gate_count_allows_selection_without_blind_scores_for_a_sole_survivor` and add:

```rust
#[test]
fn selection_refuses_zero_scores_and_incomplete_pair_coverage() {
    let mut zero = candidate("reliable", 0.0, 1.0, 20_000);
    zero.blind_judgment_count = 0;
    assert!(select_default(&[zero.clone()]).is_err());

    zero.mean_blind_usefulness = 4.5;
    zero.blind_judgment_count = 2;
    zero.pairwise_case_counts.insert("peer".into(), 2);
    let mut peer = candidate("peer", 4.4, 1.0, 25_000);
    peer.blind_judgment_count = 2;
    peer.pairwise_case_counts.insert("reliable".into(), 2);
    assert!(select_default(&[zero, peer]).is_err());
}

#[test]
fn selection_requires_usefulness_at_least_three_point_five() {
    let candidate = candidate("valid-but-weak", 3.49, 1.0, 10_000);
    assert!(select_default(&[candidate]).is_err());
}
```

Add blind-session tests proving three distinct PR judgments produce count 3 for each model pair and repeated/same-case data cannot inflate coverage.

Update benchmark test fixtures with a helper that gives both sides honest symmetric coverage:

```rust
fn judged_pair(mut left: CandidateAggregate, mut right: CandidateAggregate, cases: usize)
    -> [CandidateAggregate; 2]
{
    left.blind_judgment_count = cases;
    right.blind_judgment_count = cases;
    left.pairwise_case_counts.insert(right.model.clone(), cases);
    right.pairwise_case_counts.insert(left.model.clone(), cases);
    [left, right]
}
```

Use `judged_pair(left, right, 3)` in the existing quality/reliability and latency tie-break tests. The base `candidate` fixture sets `blind_judgment_count: 3` and an empty pair map so single-candidate threshold tests remain explicit.

- [ ] **Step 2: Run benchmark tests and verify RED**

Run: `cargo test -p ramo-server --test benchmark_report && cargo test -p ramo-server --test benchmark_blind`

Expected: old selection still accepts zero scores and aggregate data has no per-pair case counts.

- [ ] **Step 3: Preserve case identity and enforce selection gates**

Expose judgments with case identity:

```rust
pub fn judgments_with_cases(&self) -> Vec<(u64, String, String, BlindJudgment)> {
    self.comparisons.iter().filter_map(|comparison| {
        comparison.judgment.map(|judgment| (
            comparison.pull_request,
            comparison.candidate_a_id.clone(),
            comparison.candidate_b_id.clone(),
            judgment,
        ))
    }).collect()
}
```

Add `blind_judgment_count: usize` and `pairwise_case_counts: BTreeMap<String, usize>` to `CandidateAggregate`. During aggregation, translate opaque IDs to models, count every scored appearance, and count distinct PR numbers for each unordered model pair.

Split protocol reliability from quality eligibility:

```rust
const MIN_BLIND_USEFULNESS: f64 = 3.5;
const MIN_PAIRWISE_CASES: usize = 3;

fn passes_protocol_gates(candidate: &CandidateAggregate) -> bool {
    candidate.completion_ratio == 1.0
        && candidate.schema_validity_ratio == 1.0
        && candidate.semantic_validity_ratio == 1.0
        && candidate.unknown_reference_count == 0
}
```

`select_default` first finds protocol-valid candidates. It then rejects any with fewer than three scored appearances, usefulness below 3.5, or fewer than three distinct judged cases against another protocol-valid candidate. A sole protocol-valid survivor still needs three scored appearances; do not permit an unjudged `0.00` shortcut. Rank by mean usefulness, net pairwise wins, median time, then peak memory.

Update the sanitized report rationale and documentation so `benchmark select` explains missing judgments without exposing private identities.

- [ ] **Step 4: Run all benchmark tests and verify GREEN**

Run: `cargo test -p ramo-server --test benchmark_blind && cargo test -p ramo-server --test benchmark_report && cargo test -p ramo-server --test benchmark_runner`

Expected: selection rejects the old prompt-v1 run and accepts only complete judged fixtures.

- [ ] **Step 5: Commit benchmark gates**

```bash
git add crates/ramo-server/src/benchmark crates/ramo-server/tests/benchmark_{blind,report,runner}.rs docs/model-benchmark.md
git commit -m "fix(server): require judged usefulness for model selection"
```

---

### Task 6: Surface low-quality fallback clearly on Android

**Files:**
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/reviewmap/ReviewMapModels.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/reviewmap/ReviewMapServerClient.kt`
- Modify: `android/app/src/main/kotlin/io/github/carlosarraes/ramo/reviewmap/ReviewMapScreen.kt`
- Modify: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/reviewmap/ReviewMapServerClientTest.kt`
- Modify: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/reviewmap/ReviewMapViewModelTest.kt`
- Modify: `android/app/src/test/kotlin/io/github/carlosarraes/ramo/reviewmap/ReviewMapScreenTest.kt`

**Interfaces:**
- Produces: Android `ReviewMapFailureCode.AnalysisLowQuality`
- Produces: safe message `AI analysis was not useful enough; the exact map is still ready`
- Consumes: server snake-case failure code and existing dismiss/retry callbacks

- [ ] **Step 1: Write failing parsing, state, and screen tests**

Add a server response fixture with `analysis_low_quality`, then assert parsing never uses reflected server text. Add a ViewModel fake terminal result and assert the original exact map remains, failure is dismissible, and retry invokes `open`. Add a screen assertion for:

```kotlin
compose.onNodeWithText("Exact map ready · AI guidance rejected").assertIsDisplayed()
compose.onNodeWithText("AI analysis was not useful enough; the exact map is still ready")
    .assertIsDisplayed()
compose.onNodeWithText("Dismiss").assertHasClickAction()
```

- [ ] **Step 2: Run focused Android tests and verify RED**

Run:

```bash
cd android
./gradlew :app:testDebugUnitTest \
  --tests '*ReviewMapServerClientTest*' \
  --tests '*ReviewMapViewModelTest*' \
  --tests '*ReviewMapScreenTest*'
```

Expected: enum parsing/message/status assertions fail because Android does not know `AnalysisLowQuality`.

- [ ] **Step 3: Implement safe Android mapping and exact-map status**

Add the enum case, map it in `failureMessage`, and use this copy regardless of server-provided text:

```kotlin
ReviewMapFailureCode.AnalysisLowQuality ->
    "AI analysis was not useful enough; the exact map is still ready"
```

In `ReviewMapScreen`, render `Exact map ready · AI guidance rejected` when phase is `Failed` and the failure code is `AnalysisLowQuality`. Keep the existing exact groups/files and retry/dismiss callbacks; do not add another modal.

- [ ] **Step 4: Run Android unit tests and verify GREEN**

Run: `cd android && ./gradlew :app:testDebugUnitTest`

Expected: all Android unit tests pass.

- [ ] **Step 5: Commit Android fallback**

```bash
git add android/app/src/main/kotlin/io/github/carlosarraes/ramo/reviewmap android/app/src/test/kotlin/io/github/carlosarraes/ramo/reviewmap
git commit -m "feat(android): explain rejected ai guidance"
```

---

### Task 7: Verify, install, and rerun the private model benchmark

**Files:**
- Modify after selection: `docs/model-benchmark-results.md` if a sanitized report is generated
- Private/untracked: `/home/carraes/mondrio/mondrio-platform/.ramo-benchmark/`

**Interfaces:**
- Consumes: all prior tasks and the approved ten-PR private corpus
- Produces: fresh prompt-v2 measurements, at least nine blind judgments covering every model pair on three cases, selected model configuration, updated local server/APK

- [ ] **Step 1: Run complete Rust verification**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: all commands exit 0 with no warnings.

- [ ] **Step 2: Run complete Android verification**

Run: `cd android && ./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug`

Expected: `BUILD SUCCESSFUL` and a debug APK at `android/app/build/outputs/apk/debug/app-debug.apk`.

- [ ] **Step 3: Archive the incompatible private prompt-v1 run without deleting it**

Build and install the verified prompt-v2 server first. Then, from `/home/carraes/mondrio/mondrio-platform`, preserve the old run and initialize the same corpus under prompt version 2:

```bash
cd /home/carraes/projs/pi-diff/.worktrees/local-ai-review-map
cargo build -p ramo-server --release
install -m 755 target/release/ramo-server /home/carraes/.local/bin/ramo-server
cd /home/carraes/mondrio/mondrio-platform
mkdir -p .ramo-benchmark/archive
mv .ramo-benchmark/run .ramo-benchmark/archive/prompt-v1-run-1785357991
ramo-server benchmark init --repo-path /home/carraes/mondrio/mondrio-platform \
  --pr 1914 --pr 1908 --pr 1898 --pr 1910 --pr 1911 \
  --pr 1913 --pr 1822 --pr 1902 --pr 1889 --pr 1901 --yes
jq '.analysis_contract_version, .pull_requests, .budget' .ramo-benchmark/manifest.json
```

Expected: prompt version 2 and exactly the ten approved PR numbers; the prompt-v1 bodies remain recoverable under `archive/`.

- [ ] **Step 4: Build and run the prompt-v2 benchmark**

Run from the Mondrio repository with the newly installed binary:

```bash
cd /home/carraes/mondrio/mondrio-platform
ramo-server benchmark run
```

Expected: 30 fresh candidate measurements or explicit typed failures; no prompt-v1 measurement is reused. Inspect `measurements.jsonl` and confirm completed requests no longer pin at 2,050 prompt tokens and no completed private proposal violates the usefulness validator.

- [ ] **Step 5: Complete the minimum honest blind judging gate**

Run `ramo-server benchmark judge`. Present the opaque A/B outputs to Carlos for scoring; do not infer human usefulness scores automatically. Complete at least the first nine comparisons, which cover all three model pairs on three distinct PRs, then save and run `ramo-server benchmark reveal`.

Expected: judgments remain blind until reveal and every eligible pair has coverage 3.

- [ ] **Step 6: Select only if all gates pass**

Run:

```bash
ramo-server benchmark select
ramo-server benchmark report --sanitized /home/carraes/projs/pi-diff/.worktrees/local-ai-review-map/docs/model-benchmark-results.md
```

Expected: selection succeeds only for a candidate with 100% completion/validity, zero invented references, mean usefulness at least 3.5, and sufficient pair coverage. If no candidate qualifies, keep the exact map default and report the failed gate instead of weakening it.

- [ ] **Step 7: Validate PR #1955 and the phone experience**

Restart `ramo-server.service`, open PR #1955 on the phone, and wait for enrichment. Verify all four authored files have concrete summaries, risks are concrete or absent, no banned phrase appears, and the start order is useful. If enrichment is rejected, verify the exact map remains navigable and the dismissible low-quality message appears.

Install the verified APK with `adb install -r android/app/build/outputs/apk/debug/app-debug.apk`, preserving credentials and drafts.

- [ ] **Step 8: Commit the sanitized benchmark result, if selection succeeded**

```bash
git add docs/model-benchmark-results.md
git commit -m "docs(server): record prompt v2 model benchmark"
```

Do not commit `.ramo-benchmark`, private candidate bodies, prompts, paths, or repository identities.

- [ ] **Step 9: Stop build daemons and confirm a clean branch**

Run:

```bash
cd android && ./gradlew --stop
git status --short
systemctl --user is-active ramo-server.service
```

Expected: no unintended files, Gradle daemon stopped, and the local server active.
