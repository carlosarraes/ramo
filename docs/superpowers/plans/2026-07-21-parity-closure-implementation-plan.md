# Rust parity closure implementation plan

> Status: executable plan for delivery slice 7. The prior six slices are complete on `feat/hunk-parity`.

**Goal:** Close every remaining in-scope Hunk parity row with native Rust behavior and named evidence, leaving only the approved top-menu/dropdown and JavaScript embedding exclusions.

**Constraints:** One `pdiff` executable; no JS/TS source or runtime; no shell evaluation of user/protocol data; platform-specific terminal code stays behind common Rust interfaces; no invented timing threshold; performance scenarios compare shapes and bounded ownership.

## Task 1: Close small evidence gaps

- [x] Add an explicit multi-file patch-chunk boundary regression.
- [x] Tie contextual bottom-status behavior to a render/PTY assertion.
- [x] Add an isolated real-tmux smoke test when tmux is available, retaining deterministic injected tests everywhere.
- [x] Mark these rows verified only after the named tests pass.
- [x] Commit `test: close native integration evidence gaps`.

## Task 2: Add terminal background auto-detection

- [x] Write parser/classification/timeout tests for OSC 11 `rgb:` and `#rrggbb` responses.
- [x] Implement a bounded 150 ms native terminal probe that reads the controlling console/TTY rather than piped patch stdin, restores input mode on every path, and falls back predictably.
- [x] Make the built-in theme default `auto` and pass the detected light/dark appearance into initial theme resolution without affecting explicit/custom themes.
- [x] Add PTY evidence that a synthetic OSC 11 response selects the light/dark default and a timeout still starts.
- [x] Commit `feat: detect terminal background natively`.

## Task 3: Add the direct agent-skill dialog

- [x] Add an `A` direct binding, centered dialog, Escape behavior, and `y`/Enter OSC 52 copy action for the native `pdiff skill path` prompt.
- [x] Update help and README controls without adding a menu or dropdown.
- [x] Add input, render, and PTY clipboard evidence.
- [x] Commit `feat: add native agent skill dialog`.

## Task 4: Add performance and memory-shape evidence

- [x] Add a dependency-free `cargo bench` harness for large patches, many files, non-ASCII input, navigation/resizes, and watch reloads.
- [x] Add deterministic tests proving repeated navigation/resize/watch cycles retain bounded controller, geometry, highlight, context, and watcher state.
- [x] Run the benchmark once in release mode and record scenario output without declaring an arbitrary pass/fail duration.
- [x] Commit `perf: add parity stress benchmarks`.

## Task 5: Complete cross-platform and install contracts

- [x] Replace the Windows piped-review placeholder with native `CONIN$` standard-input restoration and keep unsupported job control as an explicit non-fatal action.
- [x] Add a CI workflow that checks/tests Linux, macOS, and Windows, with Unix PTY jobs clearly separated; make release builds use `--locked`.
- [x] Add a native PowerShell installer for Windows release zip artifacts and smoke-test archive/install selection without network mutation.
- [x] Cross-check the available macOS/Windows Rust targets locally and use CI results as the final runtime authority.
- [x] Commit `build: verify cross-platform native releases`.

## Task 6: Exhaustive parity and release closure

- [ ] Re-audit Hunk `53fcb2c`, the approved design, every CLI help surface, controls, config keys, integrations, and the full ledger; split or add rows when needed.
- [ ] Update README installation/controls/platform/performance documentation and mark only named-test/CI-backed rows verified.
- [ ] Run fail-fast: no JS/TS source, no JS runtime invocation, format, strict Clippy, every target/feature test, benchmarks, locked release build, diff check, executable type/size/linkage, install smoke, and clean worktree.
- [ ] Confirm `docs/parity/hunk.md` has no in-scope `missing` or `implemented` rows and lists only the two approved exclusions.
- [ ] Commit `docs: close rust hunk parity`.
