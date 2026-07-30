# Local Review Map model benchmark

Ramo does not treat a familiar model name as evidence that it is the best default. The private benchmark runs the same frozen PR inputs, prompt, schema, generation settings, classifier, and patch budget through every candidate, then separates hard reliability gates from blind human usefulness scoring.

The initial candidates are:

- `qwen3:8b` — approximately 5.2 GB;
- `qwen3-coder:30b` — approximately 18 GB;
- `qwen2.5-coder:7b` — approximately 4.7 GB.

Model sizes are estimates shown before download. Ramo asks before running `ollama pull` unless `--yes` is supplied.

## Private corpus

From a repository with authenticated `gh` access:

```bash
ramo server benchmark init --repo-path /path/to/repository --recent 10
```

Ramo prints the selected non-draft PR numbers and asks before writing `.ramo-benchmark/manifest.json`. You can instead pass six to ten explicit `--pr N` values. The manifest identifies cases but contains no patch, prompt, or model-output body. The entire `.ramo-benchmark/` tree is ignored by Git and protected with user-only permissions on Unix.

## Run and resume

```bash
cd /path/to/repository
ramo server benchmark run
```

Each PR is fetched once per invocation. All candidates receive the same canonical request digest and execute sequentially. Public `measurements.jsonl` records timing, token counts, validity, repairs, unknown references, completion, and optional resource data; private bodies remain under `.ramo-benchmark/run/private/`.

The command appends each completed measurement immediately. Running it again skips an exact completed `(PR, model, installed digest, prompt version)` tuple and retries incomplete or failed work.

## Blind evaluation

```bash
ramo server benchmark judge
```

Every valid candidate pair is shown as Candidate A/B in deterministic balanced order. For each side, enter five scores from 1–5 in this order:

1. grouping usefulness;
2. factual accuracy;
3. recommended review order;
4. risk usefulness;
5. lack of noise.

Then enter the overall choice: `A`, `B`, or `tie`. Enter `q` to save and resume later. The judging file stores opaque candidate IDs, never model names. Reveal identities only when ready:

```bash
ramo server benchmark reveal
```

## Selection and sanitized report

```bash
ramo server benchmark select
ramo server benchmark report --sanitized docs/model-benchmark-results.md
```

A candidate is eligible only with successful completion on every corpus case, 100% final schema and semantic validity after the single allowed repair, zero invented references, at least three blind scored appearances, and mean usefulness of at least 3.5. When multiple candidates pass the protocol gates, each also needs judgments from at least three distinct PRs against another passing candidate. Repeated judgments from one PR cannot inflate this coverage. Eligible candidates rank by mean blind usefulness, net pairwise wins, median wall time, then peak memory when both measurements exist.

Selection asks before atomically writing the model, installed digest, prompt version, and benchmark run ID to the server configuration. The sanitized report contains only model identities, aggregate metrics, category labels, hardware summary, and rationale. It omits repository names, PR numbers, paths, summaries, risks, prompts, patches, and model response bodies.

Before committing a generated report, check it:

```bash
rg -n 'github.com/.+/pull|src/|backend/|frontend/' docs/model-benchmark-results.md
```

Keep the full ignored run directory locally for reproducibility.
