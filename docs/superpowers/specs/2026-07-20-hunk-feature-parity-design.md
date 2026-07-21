# Hunk Feature Parity Design

## Goal

Evolve `ramo` into a small, self-contained Rust diff-review executable that matches Hunk's user-visible capabilities and command shapes while retaining `ramo`'s existing review workflow. The only intentional Hunk UI omission is its top menu bar and dropdown-menu chrome.

## Product constraints

- The implementation is 100% Rust.
- Installation produces one `ramo` executable. It must not require Node.js, Bun, TypeScript, a browser runtime, or a separately installed helper service.
- `ramo daemon serve` and all session-broker behavior execute from that same binary.
- Git, Jujutsu, and Sapling executables are optional external tools. A VCS executable is required only when a user invokes a workflow backed by that VCS.
- The executable remains named `ramo`; Hunk command shapes are exposed beneath that name.
- Linux, macOS, and Windows are supported. PTY-only features may use platform-specific implementations behind common Rust interfaces.
- Existing `ramo` features remain available unless they conflict with Hunk parity. On a conflict, the Hunk-compatible command or key wins and the existing action receives a new non-conflicting binding.
- Existing `git diff | ramo`, `--input`, `--output`, and `--stdout` usage remains compatible.
- Hunk's top menu bar, dropdown menus, and menu-specific shortcuts are not ported. Their underlying actions remain accessible through direct shortcuts, dialogs, or command-line options.

## Scope and source of truth

The behavioral reference is the Hunk checkout at `/home/carraes/github/hunk`, including its source, command parser, README, tests, and agent workflow documentation as of commit `53fcb2c`. A checked parity manifest in `docs/parity/hunk.md` will enumerate every command, option, keyboard action, config preference, loader, note operation, and testable UI capability. A feature is not considered ported until the manifest points to Rust implementation and verification evidence.

The port covers Hunk's product behavior, not its TypeScript APIs or its OpenTUI implementation details. Hunk's embeddable OpenTUI component is represented by reusable public Rust core and Ratatui widget modules so downstream Rust programs can embed the same model and renderer.

## Architecture

```text
CLI arguments / stdin / pager input
              |
              v
      command and config resolver
              |
              v
       normalized ReviewInput
              |
              v
 Git / jj / sl / file / patch / pager loaders
              |
              v
          Changeset model <---- agent context / live notes
              |
              v
        review-state controller <---- watcher / session bridge
              |
              v
 geometry + render plan ----> Ratatui widgets ----> terminal
```

The current `App` and `side_by_side` modules combine input handling, review state, geometry, and rendering. They will be split along the boundaries above. Existing parsing, syntax highlighting, annotations, clipboard, tmux, and Pi integration code will be retained behind the new interfaces and improved where parity requires it.

### Crate shape

The package will expose both a library and binary:

- `src/lib.rs`: reusable public entry point for normalized models, loaders, and widgets.
- `src/main.rs`: process startup, terminal lifecycle, and command dispatch only.
- `src/cli/`: Clap definitions and normalization into command inputs.
- `src/config/`: built-in defaults, user and repository TOML loading, validation, and persistence.
- `src/core/`: changeset, file, source-fetcher, annotation, and review-note models.
- `src/input/`: VCS adapters, file comparison, patch normalization, pager detection, and reload plans.
- `src/review/`: selection, navigation, filtering, context expansion, layout, and viewport state.
- `src/ui/`: focused Ratatui widgets, render planning, geometry, themes, dialogs, and input mapping.
- `src/watch/`: observation, polling fallback, debounce, and reload coordination.
- `src/session/`: loopback daemon, protocol, registration, projections, and CLI client.
- `src/markup/`: deterministic STML parsing, line layout, terminal rendering, and authoring guide.
- Existing `annotations`, `clipboard`, `tmux`, and `pi_extension` modules remain focused integrations.

Each loader produces the same `Changeset` model. Rendering, navigation, agent commands, watch reloads, and exported comments therefore operate on one source of truth.

## Command contract

### Review inputs

The following Hunk-compatible commands will be supported under `ramo`:

- `ramo diff [target] [-- <pathspec...>]`
- `ramo diff --staged [-- <pathspec...>]`
- `ramo diff --cached [-- <pathspec...>]`
- `ramo diff <left-file> <right-file>`
- `ramo show [target] [-- <pathspec...>]`
- `ramo stash show [ref]`
- `ramo patch [file|-]`
- `ramo pager`
- `ramo difftool <left> <right> [path]`

Common review flags match Hunk:

- `--mode auto|split|stack`
- `--watch` for reloadable inputs
- `--theme <theme>`
- `--agent-context <path>`
- `--pager`
- `--line-numbers` / `--no-line-numbers`
- `--wrap` / `--no-wrap`
- `--hunk-headers` / `--no-hunk-headers`
- `--agent-notes` / `--no-agent-notes`
- `--transparent-bg` / `--no-transparent-bg`
- `--exclude-untracked` / `--no-exclude-untracked` where applicable

Git diff flags include `--staged` and its `--cached` alias. Invalid layouts, partially numeric positive integers, conflicting selectors, missing operands, and unsupported VCS operations fail before terminal initialization with actionable errors.

### Session and agent commands

The same executable exposes:

- `ramo session list`
- `ramo session get`
- `ramo session context`
- `ramo session review`
- `ramo session navigate`
- `ramo session reload`
- `ramo session comment add`
- `ramo session comment apply`
- `ramo session comment list`
- `ramo session comment rm`
- `ramo session comment clear`
- `ramo daemon serve`
- `ramo mcp serve` as the compatibility alias accepted by Hunk
- `ramo markup render`
- `ramo markup guide`
- `ramo skill path`

Session selectors, navigation targets, note filters, destructive confirmations, batch stdin input, and `--json` output match the Hunk CLI contract. Text output remains human-readable; JSON output uses stable versioned structures.

### Existing compatibility commands

- Bare piped input is normalized to `ramo patch -`.
- `ramo --input <patch>` remains an alias for patch-file input.
- `ramo --output <path>` and `ramo --stdout` continue to export human review notes as Markdown after a TUI review.
- `ramo install pi` and `ramo uninstall pi` remain supported.
- `ramo --version`, `ramo -v`, and top-level help remain supported.

## Configuration

Configuration uses TOML and applies layers in this order:

1. built-in defaults;
2. user config at the platform config directory under `ramo/config.toml`;
3. repository `.ramo/config.toml`;
4. command-specific section;
5. pager-specific section when applicable;
6. CLI flags.

Supported preferences include VCS selection, theme, layout mode, watch, untracked-file policy, line numbers, wrapping, hunk headers, agent-note visibility, copied decorations, save-preference prompting, transparent background, and moved-line coloring. `menu_bar` is intentionally absent because `ramo` has no menu bar.

Runtime changes to persistent view preferences trigger a quit confirmation that can save, discard, permanently disable the prompt, or cancel. Updates preserve unrelated TOML sections and comments.

## Input loading and normalization

### VCS adapters

One adapter trait represents Git, Jujutsu, and Sapling. Detection walks to the containing checkout and may be overridden by config. Adapters support the operations their native tool exposes:

- working-tree diff;
- revision/range diff;
- last or selected commit/change display;
- pathspec filtering;
- staged diff where supported;
- Git stash display;
- untracked-file inclusion where supported;
- old/new source retrieval for context expansion and editor targeting.

The adapter owns native command construction and emits normalized patch text plus source metadata. Unsupported combinations fail explicitly rather than silently falling back to Git semantics.

### Files, patches, and pager input

Direct file comparison handles text, empty files, `/dev/null`, and binary files. Patch loading accepts files and stdin, strips terminal control sequences, normalizes line endings and Git patch variants, preserves rename/copy metadata, and creates placeholders for binary or intentionally skipped large content.

Pager mode detects patch-like stdin. Diff input opens a minimal-chrome diff viewer; non-diff text is delegated to a configurable text pager without shell evaluation. Recursive `ramo pager` resolution is prevented. Exit status, signals, and terminal ownership are propagated correctly.

### Changeset model

A changeset contains a stable identity, source label, title, summaries, and an ordered list of files. Each file records paths, change type, patch, language, additions/deletions, binary/large/untracked state, moved-line classifications, optional source fetchers, and associated agent context.

Agent-context file order is intentional and reorders matching files first while keeping unmatched files stable. Every row and hunk receives stable keys so selection, viewport anchoring, notes, watch reloads, and session commands survive layout changes and content refreshes where the underlying target still exists.

## Review UI

### Layout and review stream

The default is one continuous top-to-bottom stream of all visible file diffs. The sidebar navigates that stream; selecting a file jumps to it instead of hiding other files.

Layout modes are:

- `auto`: split on sufficiently wide terminals and stack on narrow terminals;
- `split`: old and new code columns;
- `stack`: deletions and additions rendered as full-width rows.

Explicit modes override responsive behavior. Resizing preserves a stable viewport anchor when the geometry changes. The sidebar hides responsively when space is insufficient but can be forced open. Large file lists and review streams are windowed so memory and render work scale with visible content plus bounded overscan.

### Display capabilities

The renderer supports:

- syntax highlighting by detected language;
- line numbers and change markers;
- character-level changed-content emphasis;
- moved-line coloring;
- file status, rename, binary, large, and untracked metadata;
- optional hunk headers;
- line wrapping or horizontal scrolling;
- collapsed unchanged context with per-hunk expansion;
- inline agent, AI, and human notes beside their target rows;
- note guide geometry shared by measurement, scrolling, and rendering;
- selection copy with correct terminal-cell handling for wide characters;
- theme-aware or transparent backgrounds that fill the host terminal width.

### Interaction

Hunk-compatible direct keys are the primary interface:

- arrows move or horizontally scroll;
- `Space`/`f`, `b`, `d`, `u`, and `Shift-Space` scroll by page or half-page;
- `g`/`G` and Home/End jump to bounds;
- `[`/`]` navigate hunks;
- `,`/`.` navigate files;
- `{`/`}` navigate annotated hunks;
- `1`/`2`/`0` select split, stack, or auto;
- `s`, `t`, `a`, `z`, `l`, `w`, `m`, `e`, `r`, `/`, `c`, Tab, `?`, and `q` retain Hunk meanings;
- Escape consistently closes the active dialog/input before affecting the review;
- note editing owns typed keys until saved or cancelled.

Existing Vim visual-line selection, yank, tmux send, and comment-export actions remain available on non-conflicting bindings documented in the help dialog. No action depends on a top menu or dropdown.

Mouse support covers wheel scrolling, shifted horizontal wheel scrolling where terminals report it, sidebar navigation, scrollbar interaction, context expansion, and text selection/copy. There are no mouse-oriented top-bar controls.

### Dialogs and status

Keyboard-driven help, theme selection, confirmation, and agent-skill dialogs are centered overlays with consistent Escape behavior. The bottom status area appears only when it has active filter, mode, warning, or transient feedback to show. Pager mode omits application chrome.

## Themes

Built-in themes are embedded in the executable and preserve Hunk's theme identifiers and cycle order. `auto` probes the terminal background, chooses a light or dark default, and falls back predictably when the terminal does not answer.

Custom themes inherit from a built-in theme and override semantic colors with validated `#rrggbb` values. Exact syntax scope overrides map onto Syntect scopes in declaration order. Hunk's legacy semantic syntax table is accepted with a deprecation warning and translated to approximate scopes. Transparent-background mode changes surface painting without losing readable diff and selection colors.

## Watch and reload

Reloadable file and VCS inputs expose a reload plan. File-system observation provides prompt updates for Git working trees and direct files, while debounced polling remains a fallback for unavailable or missed events. Jujutsu and Sapling initially use polling to match Hunk's current behavior.

Reloads are serialized, coalesce bursts, do not replace newer state with stale results, and preserve the selected file/hunk and viewport anchor when possible. `r` performs a manual reload. Watch errors are shown without destroying the last valid changeset.

## Notes, agent context, and STML

Agent-context JSON contains a changeset summary, ordered file entries, and hunk/line-targeted annotations. Notes can include summary, rationale, author, confidence, tags, source, timestamps, and optional STML markup.

Human notes created with `c` are editable inline and remain distinct from external live agent notes. The existing Markdown review export serializes human notes, including selected ranges and side information. Live notes are manipulated through session commands and can be listed separately or together with AI/agent/user notes.

STML is parsed and laid out by a deterministic terminal-cell engine. For the same markup and width it always returns the same symbolic-color lines, allowing note heights to be known before widgets mount. `ramo markup render` previews text or JSON output, and `ramo markup guide` prints the embedded authoring guide.

## Live-session broker

Normal review sessions register with one loopback-only daemon. If no compatible daemon exists, a session starts `ramo daemon serve` from the current executable and waits for readiness. The daemon brokers commands to multiple live TUI sessions; sessions do not expose separate public ports.

The protocol has explicit API and daemon compatibility versions, capability discovery, bounded payloads, timeouts, stale-daemon replacement, session reconnection, and clean shutdown. Session selectors support session id, session path, and canonical repository root. The daemon stores only live routing and projections; the TUI remains authoritative for review state.

Security properties are part of the contract:

- bind only to loopback;
- reject non-loopback configuration;
- validate and cap request bodies and note batches;
- execute no shell text from protocol payloads;
- require explicit confirmation for destructive comment clearing;
- prevent one session's identifiers from mutating another session.

## Existing `ramo` integrations

Pi installation continues to install a command that lets the user choose a staged, branch, commit, or other supported review target and returns exported comments to Pi. It will be updated to use the normalized command layer instead of constructing legacy-only input.

Tmux pane discovery and safe paste remain optional runtime integrations. They must degrade to clear feedback outside tmux and must not become prerequisites for ordinary use. Clipboard support prefers native terminal protocols such as OSC 52 and keeps platform fallbacks behind one interface.

## Error handling and terminal lifecycle

Command and config errors occur before alternate-screen entry. Runtime failures restore terminal modes before printing an error. Panics install a restoration hook. Suspend/resume and child-editor launches temporarily return terminal ownership and redraw after resumption.

Errors identify the failed operation, relevant path or VCS, and corrective action. Empty valid diffs, invalid patches, missing tools, binary inputs, unavailable TTYs, stale daemons, pager failures, watch failures, and editor failures have distinct behavior and tests.

Startup notices surface deprecated configuration and other actionable compatibility warnings without taking over the review. Update checks are bounded, non-blocking, failure-tolerant, and may be disabled; they never prevent startup or add a runtime dependency.

## Performance requirements

- Startup and first render must not require loading every file's full before/after contents.
- Syntax highlighting and full-source retrieval are lazy and cached with content-sensitive keys.
- Geometry is derived once per state/width combination and shared by rendering and interaction.
- Large review streams and sidebars use row/file windowing with adaptive overscan.
- Watch reloads do not leak tasks or grow retained geometry/highlight caches without bounds.
- Benchmarks cover large patches, many files, non-ASCII input, repeated navigation, resize, watch reloads, and long-running memory behavior.

No fixed numerical performance threshold is invented during the port. The Hunk benchmark fixtures provide comparative scenarios, and regressions are evaluated against the preceding Rust implementation on the same machine.

## Verification strategy

Testing is layered by contract:

- unit tests for parsers, normalization, VCS command construction, config precedence, geometry, navigation, STML, themes, and session projections;
- snapshot-style render-model tests that assert rows and styles without relying on a specific terminal backend;
- black-box CLI tests for every command, option, error, compatibility alias, and text/JSON output form;
- PTY tests for navigation, layout, filtering, dialogs, notes, mouse events, pager behavior, reload, resize anchoring, and terminal restoration;
- integration tests for Git/jj/sl fixtures, watcher behavior, editor launching, daemon registration, multi-session routing, and all session comment operations;
- cross-platform CI for unit and CLI contracts, with Unix-specific PTY suites clearly separated;
- benchmark and memory checks for the performance scenarios above.

The parity manifest records one of `missing`, `implemented`, or `verified` for each reference behavior. Only `verified` entries count toward completion. The full project is complete when every in-scope entry is verified and the only exclusions are the documented top menu/dropdown UI and JavaScript-specific embedding surface.

## Delivery decomposition

This product is too broad for one safe implementation patch. Work proceeds through independently testable vertical slices while retaining the single final parity target:

1. **Foundation and CLI:** library/binary split, normalized models, command surface, config loading, patch/file inputs, and compatibility aliases.
2. **VCS and pager:** Git/jj/sl adapters, untracked/binary/large-file handling, show/stash/difftool, source fetchers, and plain-text pager fallback.
3. **Review UI:** continuous stream, sidebar/filter, responsive layouts, geometry, renderer, full keyboard map, mouse, dialogs, themes, and context expansion.
4. **Watch and process integration:** reload plans, observation/polling, editor/job control, robust terminal lifecycle, Pi, tmux, and clipboard.
5. **Notes and markup:** normalized agent context, inline human/agent notes, Markdown export, deterministic STML, and markup commands.
6. **Sessions:** same-binary daemon, registration, protocol, projections, complete session CLI, security limits, and multi-session tests.
7. **Parity closure:** performance/windowing, cross-platform hardening, documentation/install updates, exhaustive manifest audit, and end-to-end verification.

Each slice gets a detailed TDD implementation plan and leaves `ramo` usable. Temporary compatibility shims are removed once all consumers use the normalized model.

## Acceptance criteria

The migration is complete only when:

1. one Rust-built `ramo` executable provides every in-scope command and workflow without a JS runtime;
2. every Hunk-compatible command and option has black-box coverage;
3. every Hunk keyboard action except menu-only actions has PTY coverage;
4. Git, jj, sl, direct-file, patch, pager, difftool, and watch inputs are verified;
5. responsive split/stack review, sidebar navigation, filtering, context expansion, themes, notes, mouse, and editor flows are verified;
6. agent context, STML, the daemon, and every session command are verified end to end;
7. legacy `ramo` review export, Pi, tmux, bare-pipe, and input/output flags remain verified;
8. terminal restoration, failure paths, security bounds, and supported platforms are covered at the appropriate level;
9. `docs/parity/hunk.md` contains no in-scope `missing` or merely `implemented` entries;
10. release documentation describes the command surface, configuration, controls, and installation of the single binary.
