# Private local Review Maps

`ramo-server` is an optional Rust companion for Review Maps. It fetches a frozen GitHub pull-request snapshot through your existing `gh` authentication, builds exact structure with `ramo-core`, and asks Ollama on the same laptop for bounded enrichment. The terminal and Android clients receive only the validated structured map.

## Privacy boundary

- The HTTP server binds to `127.0.0.1:47831`, never `0.0.0.0`.
- Tailscale Serve is the only supported remote exposure path and remains private to your tailnet.
- The analyzer backend is selectable. The default from 0.1.0 is the `pi` CLI against a remote
  provider, which **sends changed-file patches off the machine** and is why `[map] enabled`
  defaults to false. Selecting the Ollama backend keeps enrichment local.
- Ollama, when selected, is accepted only on a loopback URL. Proxies and redirects are disabled
  for model requests.
- The terminal-to-server and phone-to-server hops remain loopback and tailnet respectively,
  whichever backend is in use.
- GitHub credentials come from `gh auth token`, remain in memory, and are never copied into Ramo configuration.
- Cache files contain validated maps and version identities only. Raw patches, prompts, model responses, GitHub tokens, pairing codes, and bearer tokens are not cached or logged.
- Paired-client files store SHA-256 token digests, labels, IDs, and creation times with user-only permissions. Pairing-code files store only digests and expiry times.

AI structure is untrusted until validated. The generated schema limits references to exact changed paths; Rust removes duplicate assignments and restores omitted paths from the deterministic diff groups before validation. Unknown paths, attempts to regroup fixed test/generated files, invalid coverage, and oversized text still reject the response. Ramo makes one repair attempt and otherwise keeps the deterministic map usable.

## Requirements

Automatic setup currently supports Linux and requires:

- `gh`, already authenticated for the repositories you review;
- Ollama running locally with the configured candidate model installed;
- Tailscale connected with MagicDNS and HTTPS enabled;
- a working `systemd --user` session;
- `ramo` and `ramo-server` installed beside each other.

Review the complete mutation-free plan first:

```bash
ramo server setup --dry-run
```

The output names every resolved dependency, the loopback bind, user-unit path, MagicDNS endpoint, and exact Tailscale action. Apply it explicitly:

```bash
ramo server setup
ramo server status
```

Setup creates a hardened `ramo-server.service` user unit with `UMask=0077`, `Restart=on-failure`, `NoNewPrivileges=true`, and a loopback backend. It then runs the equivalent of:

```bash
tailscale serve --bg --yes --https=443 http://127.0.0.1:47831
```

The setup path restores the prior unit and endpoint files if service installation or publication fails. It never exposes Ollama.

## Pair Android

With the service running:

```bash
ramo server pair
```

Ramo prints a QR deep link plus copyable endpoint and code. The code expires after five minutes and is single-use. A successful exchange returns one individually revocable long-lived client credential; only its digest persists on the laptop. Re-running `pair` creates a new code without invalidating existing clients.

## Pair the terminal

The terminal talks only to loopback and reads its bearer credential from an absolute file path. With the service running, call `ramo server pair`, then exchange the printed code against `http://127.0.0.1:47831/v1/pair/exchange` and save the returned JSON with user-only permissions. Configure:

```toml
review_map_server = "http://127.0.0.1:47831"
review_map_token_file = "/home/you/.config/ramo/review-map-client.json"
ai_summaries = true
```

The file may contain the full pairing JSON or only its `token` value. The client caps it at 16 KiB, redacts the token from diagnostics, accepts only `127.0.0.0/8` or `::1`, bounds HTTP headers and bodies, and rejects chunked or incompatible responses. Set `ai_summaries = false` to keep the exact map without starting a background request.

`ramo pr N` renders the exact tree before contacting the companion. Local diffs remain code-first and expose a deterministic map through `M`; pager mode never creates an analysis worker. Enrichment failures are dismissible and do not discard review comments.

## Cache and lifecycle

```bash
ramo server status
ramo server cache list
ramo server cache clear
```

Cache filenames are SHA-256 identities, so directory listings do not reveal repository names. Identity includes repository, PR, head SHA, model and installed digest, generation parameters, prompt version, schema version, and classifier version. Entries are atomically replaced, age/size bounded, and discarded if corrupt or incompatible. `cache clear` removes only Review Map `.json` entries from the dedicated cache directory.

The server performs one GPU-heavy analysis at a time. Identical requests share a job, a newer head SHA removes older queued work, and clients can review the exact deterministic map while enrichment runs.

The default local model can be selected through Ramo's private, resumable A/B benchmark. See [Local Review Map model benchmark](model-benchmark.md).

## API failures

Every API failure returns a stable code and a safe message:

- `server_unreachable`: the local companion cannot be reached or bound;
- `pairing_rejected`: code invalid, expired, or already used;
- `client_unauthorized`: bearer token missing, invalid, or revoked;
- `github_auth_unavailable`: `gh` missing, signed out, or expired;
- `github_request_failed`: GitHub transport, rate-limit, or response failure;
- `pull_request_unavailable`: PR missing or inaccessible;
- `ollama_unavailable`: local Ollama cannot be reached or is not loopback;
- `model_missing`: configured model is not installed;
- `analysis_timed_out`: bounded local generation timed out;
- `analysis_invalid`: structured output failed validation after repair;
- `analysis_failed`: another local analysis failure;
- `result_stale`: PR head or installed model digest changed;
- `cache_unavailable`: cache cannot be read or written safely;
- `server_incompatible`: setup platform or protocol schema is unsupported.

These are recoverable states: they do not terminate the terminal UI or Android app, discard comments, or replace the exact map with partial AI output.

## Manual development run

```bash
cargo run -p ramo-server -- serve
```

The process stays on loopback. Use `ramo server setup` for the normal user service and Tailscale configuration; the installer deliberately does not enable either automatically.
