# Native Live-Session Parity Implementation Plan

> Status: approved architecture translated into an executable Rust plan for delivery slice 6.

**Goal:** Port Hunk's loopback daemon, live-session registration, projections, complete `session` command surface, and `skill path` command into the same Rust `pdiff` executable, with the TUI remaining authoritative for review state.

**Reference:** `/home/carraes/github/hunk` at `53fcb2c`, especially `src/session/*`, `src/session-broker/*`, `src/hunk-session/*`, the session CLI tests, and the approved parity design.

**Architecture:** `session` owns serializable protocol DTOs, projections, a bounded loopback HTTP daemon, an internal persistent framed TCP transport for registered TUI sessions, a blocking CLI client, and the app bridge. The daemon stores only registration/snapshot/routing metadata. Every mutation is forwarded to the selected live TUI and applied through `ReviewController`, `WatchRuntime`, and existing loader boundaries. The broker uses `std::net` and threads; no async runtime, browser server, JavaScript, shell command, or second executable is introduced.

**Transport:** CLI clients use bounded HTTP/1.1 JSON at `/session-api`; capabilities are exposed at `/session-api/capabilities`, health at `/health`, and legacy `/mcp` returns the Hunk-compatible tombstone. TUI sessions connect to the same loopback port with a private `PDIFF-SESSION/1` preface followed by length-prefixed JSON frames. Request ids bind command results to exactly one selected session. This internal transport need not preserve Hunk's JavaScript WebSocket implementation detail; it preserves the command, projection, lifecycle, and security contract.

**Defaults and bounds:** loopback `127.0.0.1:47657`; 256 KiB HTTP bodies; 1 MiB internal frames; 100 comments per batch; 64 KiB summary/rationale/markup fields; 5-second ordinary command timeout; 30-second reload/batch timeout; 45-second stale session TTL; 60-second daemon idle shutdown. `PDIFF_SESSION_HOST`/`PDIFF_SESSION_PORT` are accepted, with `HUNK_MCP_HOST`/`HUNK_MCP_PORT` compatibility aliases. Non-loopback hosts are always rejected.

---

## Task 1: Define bounded protocol models and controller projections

**Files:**

- Create: `src/session/mod.rs`
- Create: `src/session/model.rs`
- Create: `src/session/protocol.rs`
- Create: `src/session/projection.rs`
- Modify: `src/lib.rs`
- Modify: `src/review/state.rs`
- Modify: `src/review/row.rs`
- Test: `tests/session_projection.rs`

- [x] Write failing serialization, selector, projection, and live-note mutation tests.
- [x] Define explicit API/daemon/registration versions, capability actions, selectors, registration, snapshot, file/hunk projections, command inputs/results, and response envelopes with serde.
- [x] Add controller-owned live agent notes, distinct from human and file-sidecar notes, and route them through the canonical note rows/geometry.
- [x] Project selected file/hunk, note markup width, file patches, all note sources, and stable live comment ids without duplicating review state.
- [x] Validate target file/hunk/side/line ownership before mutations; add/list/remove/clear operations must not cross session or file boundaries.
- [x] Run `cargo test --test session_projection --test notes_state --test review_state --test ui_render` and commit `feat: add native session projections`.

## Task 2: Add the complete session, daemon, MCP-alias, and skill CLI model

**Files:**

- Modify: `src/cli/args.rs`
- Modify: `src/cli/normalize.rs`
- Modify: `src/cli/mod.rs`
- Create: `src/session/cli.rs`
- Create: `src/session/skill.rs`
- Create: `src/session/pdiff-review-SKILL.md`
- Modify: `src/runtime.rs`
- Test: `tests/session_cli.rs`
- Test: `tests/skill_path.rs`

- [x] Write failing parser tests for every command and conflict: selectors, navigation modes, reload `--` input, comment target exclusivity, bounded batch stdin, note type, JSON/text, and mandatory `--yes` clearing.
- [x] Add `session list|get|context|review|navigate|reload`, `session comment add|apply|list|rm|clear`, `daemon serve`, `mcp serve`, and `skill path` actions.
- [x] Canonicalize `--repo` selectors through native VCS detection; enforce exactly one id/repo/session-path selector as appropriate.
- [x] Parse reload's nested review command through the existing normalized CLI layer while rejecting stdin-backed/nested session commands.
- [x] Embed the pdiff review skill and materialize it atomically under the platform data directory so `skill path` preserves the one-installed-binary requirement.
- [x] Run `cargo test --test session_cli --test skill_path --test cli_parse --test cli_contract` and commit `feat: add native session commands`.

## Task 3: Implement the loopback daemon and bounded HTTP client

**Files:**

- Create: `src/session/config.rs`
- Create: `src/session/http.rs`
- Create: `src/session/daemon.rs`
- Create: `src/session/client.rs`
- Test: `tests/session_daemon.rs`
- Test: `tests/session_security.rs`

- [x] Write failing tests for loopback recognition, host/origin checks, JSON content type, methods, body/frame limits, capabilities, health, `/mcp` tombstone, address conflicts, idle shutdown, and structured errors.
- [x] Resolve only loopback IPv4/IPv6/localhost configurations and reject wildcard, DNS, mapped non-loopback, zero, and malformed ports before bind/connect.
- [x] Implement a threaded `TcpListener` with bounded header/body parsing, response size limits, read/write deadlines, and no shell or path execution from payloads.
- [x] Store sessions in an `Arc<Mutex<...>>` registry with connection generations, last-seen timestamps, pending request ids, stale pruning, and selector ambiguity/not-found diagnostics.
- [x] Implement HTTP actions and blocking CLI client calls with explicit 5/30-second timeouts and stable text/JSON formatting.
- [x] Add compatible-daemon discovery, same-binary launch from `current_exe`, readiness wait, stale-version shutdown/replacement, and clear foreign-port diagnostics.
- [x] Run `cargo test --test session_daemon --test session_security` and commit `feat: serve native loopback sessions`.

## Task 4: Register and reconnect normal TUI sessions

**Files:**

- Create: `src/session/wire.rs`
- Create: `src/session/registration.rs`
- Modify: `src/runtime.rs`
- Modify: `src/app.rs`
- Test: `tests/session_registration.rs`

- [x] Write failing frame, registration, disconnect, reconnect, snapshot update, and daemon auto-launch tests.
- [x] Implement the private preface plus big-endian length-prefixed JSON frames with strict 1 MiB limits and version checks.
- [x] Build registration from `LoadedReview`, canonical cwd/repo root, process metadata, stable file/hunk DTOs, and an initial controller snapshot.
- [x] Start/ensure the daemon before alternate-screen entry, connect on a background thread, and reconnect with bounded backoff when a compatible daemon restarts.
- [x] Publish content-sensitive snapshots from the app loop and update registration after accepted reloads; disconnect cleanly on TUI exit.
- [x] Run `cargo test --test session_registration --test pty_ui --test pty_watch` and commit `feat: register live native reviews`.

## Task 5: Dispatch navigation and all live comment operations through the TUI

**Files:**

- Create: `src/session/bridge.rs`
- Modify: `src/app.rs`
- Modify: `src/review/state.rs`
- Modify: `src/watch/runtime.rs`
- Test: `tests/session_bridge.rs`
- Test: `tests/pty_session.rs`

- [x] Write failing controller/PTY tests for hunk, line, next/previous annotation navigation; comment add/batch/list; focus modes; markup feedback; remove; live-only clear; include-user clear; and failed target isolation.
- [x] Poll broker requests inside the existing 50 ms app loop and apply them only through controller/watch methods on the UI thread.
- [x] Generate `mcp:<request-id>` ids, validate markup at the live note content width, return bounded layout notes, and reveal/focus only when requested.
- [x] Keep live comments, sidecar notes, and human notes source-distinct; destructive clear requires CLI `--yes`, and user notes are included only with `--include-user`/`--all`.
- [x] Return one result and updated snapshot per request; timeouts and disconnected responders leave TUI state unchanged where the operation was not applied.
- [x] Run `cargo test --test session_bridge --test pty_session --test notes_state --test pty_notes` and commit `feat: control live reviews natively`.

## Task 6: Support session review export and arbitrary reload safely

**Files:**

- Modify: `src/session/projection.rs`
- Modify: `src/session/bridge.rs`
- Modify: `src/watch/runtime.rs`
- Modify: `src/runtime.rs`
- Test: `tests/session_reload.rs`

- [ ] Write failing tests for patch/note inclusion flags, source-path resolution, Git/JJ/SL reloads, direct-file reloads, invalid nested/stdin inputs, preserved session identity, and selected-target fallback.
- [ ] Export bounded review models directly from current controller files; omit raw patches and notes unless explicitly requested.
- [ ] Add a synchronous native reload transaction that resolves the supplied `ReviewInput` in the requested source directory, replaces watch/reload plans only on success, and preserves human/live notes by stable target.
- [ ] Refresh registration metadata/snapshot after reload while preserving the session id and daemon route.
- [ ] Run `cargo test --test session_reload --test reload --test git_loading --test jj_loading --test sl_loading` and commit `feat: reload live native sessions`.

## Task 7: Prove multi-session routing, document, and close the slice

**Files:**

- Modify: `README.md`
- Modify: `docs/parity/hunk.md`
- Modify: this plan
- Test: `tests/session_e2e.rs`

- [ ] Add black-box tests with two live PTYs proving list/id/repo/path selection, ambiguity errors, per-session navigation/comment isolation, daemon reconnect, stale replacement, and clean daemon idle exit.
- [ ] Document every command, selectors, JSON modes, environment configuration, security bounds, skill path, and lifecycle behavior.
- [ ] Mark ledger rows verified only with named daemon/PTY evidence.
- [ ] Run the fail-fast slice gate: no JS/TS source, no JS runtime invocation, format, strict clippy, all targets, release build, diff check, ELF/file inspection, and linkage inspection.
- [ ] Commit `docs: verify native live-session parity`.

---

## Slice completion gate

- One installed Rust `pdiff` binary serves, registers, controls, and inspects sessions.
- The daemon cannot bind or accept routed browser origins outside loopback policy.
- Payloads, frames, comment batches, fields, response sizes, waits, stale entries, and reconnect work are bounded.
- The daemon never owns mutable review truth; every navigation, comment mutation, and reload is acknowledged by the selected TUI.
- Session id, canonical repository root, and session path selectors route deterministically and reject ambiguity.
- Every session command supports stable JSON and useful text output without entering the alternate screen.
- Clearing is explicitly confirmed and cannot clear human notes unless requested.
- Session reload uses the existing typed loader/config/VCS boundaries and never evaluates shell text.
- Normal TUI shutdown unregisters cleanly; daemon restart reconnects live sessions; idle daemon exits.
- The release remains a small native executable with no JavaScript/TypeScript runtime footprint.
