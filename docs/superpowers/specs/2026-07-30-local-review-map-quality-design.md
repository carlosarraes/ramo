# Local Review Map Quality Design

## Problem

The first live local-AI enrichment for Mondrio PR #1955 was structurally valid but not useful. Qwen 2.5 Coder 7B produced generic directory paraphrases, assigned bare `low` risks without evidence, omitted insights for every authored file, and preserved the existing file order instead of explaining a review strategy. The stored benchmark selected that model because it completed every case and was fast; the blind usefulness score was `0.00` because no judgments had been recorded.

The input pipeline also allowed silent context loss. Ollama ran the selected model with a 4,096-token context while Ramo accepted as much as 96 KiB of patch text. Qwen 2.5 and Qwen 3 Coder reported exactly 2,050 evaluated prompt tokens across differently sized benchmark cases, which is consistent with large inputs being truncated. Valid JSON therefore measured protocol compliance, not comprehension.

## Product Decision

The deterministic Review Map remains the trusted product and must appear immediately. Local AI is optional, asynchronous enrichment. Quality takes priority over latency: enrichment may take up to 90 seconds, but Ramo must reject output that does not add concrete review guidance.

Rejected enrichment is all-or-nothing. Ramo keeps the exact map rather than mixing trusted exact data with a partial set of weak AI claims.

## Pipeline Architecture

The exact planner and its invariants remain unchanged. It continues to own paths, classifications, counts, coverage, fixed test/generated groups, and the fallback file order.

The enrichment pipeline has five distinct stages:

1. Build the exact enrichment request from the fetched PR diff.
2. Partition patch input using a conservative token estimate and an explicit context budget.
3. Ask Ollama for structured enrichment using prompt version 2.
4. Validate schema, exact-map semantics, and usefulness in that order.
5. Merge and cache only a proposal that passes every gate.

Ollama requests set `num_ctx` to 32,768 and `num_predict` to 6,144. Ramo keeps a 2,048-token safety margin, leaving at most 24,576 estimated prompt tokens including the system prompt, user JSON, and structured-output schema. The conservative estimate is `ceil(serialized UTF-8 bytes / 3)`, which is intentionally pessimistic for source code. Batching preserves whole-file boundaries where possible and retains the existing per-file truncation marker when a single patch cannot fit.

If batching is necessary, batch proposals are validated independently and a final synthesis receives the validated proposals plus the exact request. The final synthesis must pass the same complete usefulness rules as a single-batch response.

## Prompt and Structured Output

Prompt version 2 tells the model that it is producing a review map, not a code-review verdict. It must describe changed behavior, review focus, dependencies, and concrete risk visible in the supplied patch. It must not claim that tests pass, coverage is complete, deployment is safe, or runtime behavior is proven.

Every reviewable file whose kind is not `test` or `generated` must appear exactly once in the file-insight output. Tests remain available as evidence and remain in their deterministic collapsed group; individual test insights are not required. Generated files remain metadata-only and cannot be regrouped or ordered by the model.

Each file insight contains the existing `summary` and optional `risk` fields. A summary must state what behavior or contract changed and give the reviewer a useful focus. A risk is either a concrete sentence tied to the patch or `null`; categorical values such as `low`, `medium`, and `high` are invalid.

Every flexible path still appears exactly once in logical groups and exactly once in `review_order`. Group summaries must explain the relationship among their files instead of restating a directory name. The prompt directs review priority and order to reflect dependency or risk; blind usefulness scoring determines whether the resulting order actually helps.

## Usefulness Validation

A new focused quality module runs after schema parsing and exact semantic validation. It returns typed, source-free failure categories suitable for repair prompts, logs, metrics, and the phone UI.

The proposal is low quality when any of these conditions holds:

- a required non-test/non-generated file insight is missing or duplicated;
- a summary begins with known generic filler such as `This file contains` or `This group contains`;
- after case-folding and removing punctuation, a summary contains no meaningful token beyond stop words and tokens already present in its path, basename, or deterministic group label;
- a risk is only a severity label;
- output asserts test success, full coverage, production safety, or another fact not established by the diff;
- a required summary is empty after normalization;
- group or file text violates the existing size and control-character limits.

The first rejected response receives one repair attempt containing only typed failure categories, never source text or the invalid model response. If the replacement fails, analysis ends with `AnalysisLowQuality`. The exact map remains available, and the failed response is never merged or cached as enriched data.

## User Experience and Errors

The phone shows the exact map while enrichment runs. Successful enrichment replaces only the organizational insights and records its model and prompt version as it does today.

`AnalysisLowQuality` produces a safe, dismissible message explaining that local AI did not add reliable guidance and that the exact map is still ready. Retry starts a fresh analysis job. No patch, prompt, model response, repository secret, or pairing credential appears in the public error, logs, or Android UI.

## Cache Behavior

Prompt version 2 is part of the existing cache identity and invalidates all prompt-version-1 results automatically. Only fully validated enriched maps are stored in the enrichment cache. Exact maps remain independently reproducible and available after enrichment failure.

## Benchmark and Model Selection

The frozen Mondrio corpus remains private and continues comparing:

- `qwen3:8b`;
- `qwen3-coder:30b`;
- `qwen2.5-coder:7b`.

All candidates use the same prompt version, explicit context, token budget, schema, seed, temperature, and 90-second per-case timeout. A candidate must complete every corpus case, pass schema and semantic validation, pass the new usefulness gate, and invent zero references.

Benchmark selection must refuse to choose a model when no blind judgments exist or when eligible candidates have insufficient pairwise coverage. Every eligible model pair must be judged on at least three distinct corpus cases. A selectable model must have mean blind usefulness of at least 3.5 out of 5. Among eligible candidates, mean blind usefulness ranks first; pairwise result, median wall time, and peak memory are successive tie-breakers.

The existing prompt-version-1 run cannot justify a default because it has no blind judgments. After implementation, Ramo reruns the full corpus, presents opaque candidates for human scoring, reveals them only after judging, and writes a selected-model configuration only after all selection gates pass. No model is favored in advance.

## Test Strategy

Unit tests exercise the exact bad patterns observed in Qwen output: generic `This file contains` and `This group contains` summaries, missing authored insights, bare `low` risks, and unsupported `tests are passing` or `full coverage` claims. Positive fixtures demonstrate concise behavior-specific summaries with concrete risk sentences or `null` risk.

Budget tests prove that serialized prompt overhead and output reserve count against the 32K context, large requests split on file boundaries, oversized single files retain truncated coverage, and every original path remains represented.

Ollama contract tests assert `num_ctx`, output reservation, prompt version 2 behavior, one quality repair attempt, and typed `AnalysisLowQuality` failure after a second rejection. Coordinator and cache tests prove that low-quality output falls back to the exact map and is never cached as enriched.

Benchmark tests prove that selection rejects zero judgments, incomplete pairwise coverage, usefulness below 3.5, timeout/validity failures, and invented references. A passing fixture demonstrates the complete selection path.

Android tests prove that an analyzing map remains readable, low-quality failure is dismissible, retry is available, and the exact map stays navigable.

The final acceptance test reruns the private ten-PR Mondrio benchmark and includes a live PR #1955 analysis. Selection occurs only after the required blind scoring is complete, and the chosen output for PR #1955 must contain a concrete insight for each of its four authored files without any banned generic or unsupported language.

## Non-Goals

- Turning Review Map enrichment into a full autonomous code reviewer.
- Publishing model output, patches, prompts, or private benchmark bodies.
- Requiring per-test-file summaries.
- Showing partially accepted model output.
- Choosing a model solely from speed, JSON validity, or model reputation.
