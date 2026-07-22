# Ramo

Ramo is a fast, review-first diff viewer for the terminal. It turns working-tree changes, revision ranges, patches, and direct file comparisons into one keyboard-first review surface.

It is written entirely in Rust and ships as one native `ramo` executable—no Node.js, Bun, TypeScript, browser, or language runtime. Ramo includes Hunk-compatible review workflows while keeping Vim-style selection, Markdown comments, tmux sending, live agent sessions, and optional Pi integration. Hunk's top menu bar and dropdowns are intentionally excluded.

```bash
# Review everything changed on the current branch since it diverged from main
ramo diff main...HEAD

# Review staged changes
ramo diff --staged

# Review any patch-producing command
gh pr diff 123 --color=never | ramo
```

Ramo means “branch” in Portuguese—a small nod to where most reviews begin.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/carlosarraes/ramo/main/install.sh | bash
```

Or install the Rust package directly:

```bash
cargo install --git https://github.com/carlosarraes/ramo --locked
```

On Windows PowerShell:

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/carlosarraes/ramo/main/install.ps1 -OutFile install.ps1
.\install.ps1
```

The release matrix produces one archive containing one executable for Linux, macOS, and Windows on x86-64 and ARM64. `install.sh` selects the Linux/macOS tarball; `install.ps1` selects the Windows zip and installs `ramo.exe` under `%LOCALAPPDATA%\Programs\ramo` by default. Neither installer adds a language runtime.

After a successful Unix install, the script checks for the legacy binary in the same install directory and asks before removing it. It never removes a similarly named program elsewhere on `PATH`. For unattended migration, set `RAMO_REMOVE_LEGACY=yes` or `RAMO_REMOVE_LEGACY=no`.

## Verified review inputs

Review patch output from any command:

```bash
git diff --no-color | ramo
git diff --cached --no-color | ramo
gh pr diff 123 --color=never | ramo
```

Review a saved patch or compare two concrete files without an external diff program:

```bash
ramo patch review.patch
ramo patch - < review.patch
ramo diff before.rs after.rs
```

Legacy patch flags remain supported:

```bash
ramo --input review.patch
ramo --input review.patch --output ramo-review.md
ramo --input review.patch --stdout
```

Native repository reviews are also verified:

```bash
ramo diff
ramo diff --staged
ramo diff main...HEAD -- src
ramo show HEAD~1
ramo stash show 'stash@{0}'
```

`ramo` selects the nearest Git, Jujutsu, or Sapling checkout. Set `vcs = "git"`, `vcs = "jj"`, or `vcs = "sl"` in user or `.ramo/config.toml` configuration when a checkout contains more than one marker. Jujutsu and Sapling support working-copy and show reviews, and reject staged and stash operations with an explicit diagnostic instead of silently changing semantics.

Working-copy reviews include untracked files by default; use `--exclude-untracked` to omit them. Tracked and untracked files over 1,000,000 bytes or 20,000 lines become bounded placeholders so a review cannot consume unbounded memory. Press `z` to expand collapsed unchanged context from bounded native old/new source readers.

Use `pager` when a command may produce either a diff or ordinary text:

```bash
git diff --no-color | ramo pager
RAMO_TEXT_PAGER="less -R" command-producing-text | ramo pager
```

Diff-shaped input enters the review UI. Other text is sanitized and sent directly to `RAMO_TEXT_PAGER`, then `PAGER`, then `less -R`. Pager settings are parsed into a program and literal arguments without a shell; environment assignments are supported, shell operators are not executed, and recursive `ramo pager` settings fall back safely.

See the [parity ledger](docs/parity/hunk.md) for behavior-by-behavior evidence; commands are not considered complete merely because their arguments parse.

## Performance evidence

`cargo bench --bench parity` runs descriptive, dependency-free stress scenarios for a 50,000-changed-line patch, 2,000 files, 20,000 non-ASCII changed lines, repeated navigation/resizes, and 50 native watch reload generations. It deliberately has no arbitrary timing pass/fail threshold. Retained-state tests separately enforce bounded highlight LRUs and stable controller, geometry, context-source, and watch-generation shapes. The latest local release-mode sample is recorded in [docs/performance.md](docs/performance.md).

## Agent context and inline notes

Attach bounded agent findings to any review with `--agent-context`:

```bash
ramo diff --agent-context review-context.json
ramo patch changes.patch --agent-context review-context.json
```

The sidecar is JSON. Its file order leads the review, renamed files match their current or previous path, and file-backed sidecars reload with the diff:

```json
{
  "version": 1,
  "summary": "Authentication review",
  "files": [
    {
      "path": "src/auth.rs",
      "annotations": [
        {
          "id": "auth-retry",
          "newRange": [42, 44],
          "summary": "The final retry still sleeps.",
          "rationale": "Return immediately after the last failed attempt.",
          "source": "agent",
          "author": "Pi",
          "tags": ["correctness"],
          "confidence": "high",
          "markup": "<badge color=warning>RETRY</badge> Check the <b>last attempt</b>."
        }
      ]
    }
  ]
}
```

Ranges are positive, inclusive, 1-based `[start, end]` pairs named `oldRange` and/or `newRange`. Optional note fields are `id`, `rationale`, `markup`, `tags`, `confidence`, `source`, `title`, `author`, `createdAt`, `updatedAt`, and `editable`. The sidecar is limited to 1 MiB, 2,000 files, and 10,000 annotations; each note allows 64 KiB of markup and 64 KiB of combined summary/rationale text. Text and markup are terminal-control sanitized.

Press `a` to reveal or hide AI/agent notes and `{`/`}` to move between annotated hunks. External notes marked as `source: "user"` remain visible; only notes authored interactively in this `ramo` process are exported as Markdown. Press `c` to start an inline human note, Enter for a newline, `Ctrl-S` to save, or Escape to cancel. Clicking a saved human note reopens it for editing; saving it empty removes it.

`--agent-context -` reads the sidecar from stdin only when the review itself does not consume stdin. Patch-stdin and pager-stdin reviews must use a sidecar file.

## Native terminal markup

STML is a small, tolerant terminal markup language rendered directly by Rust inside agent note cards. Preview it without entering the review UI:

```bash
ramo markup render note.stml --width 56 --color auto
printf '<badge color=success>PASS</badge> native' | ramo markup render - --json
ramo markup guide
```

It supports inline emphasis, semantic/named/hex colors, links, badges, keyboard hints, headings, lists, rules, spacers, code blocks, cards, bordered boxes, and responsive rows. Layout uses terminal-cell widths, clips code and wide glyphs safely, and returns bounded degradation notes for malformed or unknown markup. `--color` accepts `auto`, `always`, or `never`; `--theme` selects the preview theme; JSON output is stable `{ "width", "lines", "notes" }`. Parsing is limited to 64 KiB, 2,000 nodes, depth 32, and 20 diagnostics.

## Watch, reload, and editor integration

Use `--watch` with direct files or native repository reviews:

```bash
ramo diff before.rs after.rs --watch
ramo diff --watch
```

Direct files and Git working trees use native filesystem events with a quiet debounce and safety polling. Jujutsu and Sapling use bounded polling. Atomic-save bursts coalesce into one serialized reload; stale generations are rejected, and failures leave the last valid review visible. Press `r` for an immediate reload even when `--watch` is not enabled.

Press `e` to open the selected file at its selected line through `$EDITOR`. `vi`, `vim`, and `nvim` receive `+line`; VS Code and Cursor receive `--goto file:line`; Helix receives `file:line`. Commands are parsed into literal argv without a shell. Terminal editors temporarily return terminal ownership and redraw afterward. On Unix, `Ctrl-z` suspends `ramo`; resuming the job restores the review.

## Live review sessions

Every interactive review registers with a loopback broker served by the same `ramo` executable. A second terminal or an agent can inspect and control the live TUI without a browser, Node.js, Bun, or a separate MCP process:

```bash
ramo session list
ramo session get SESSION_ID
ramo session context SESSION_ID --json
ramo session review SESSION_ID --include-patch --include-notes --json
```

`list` discovers sessions. `get` returns registration metadata, `context` returns the selected file/hunk and note state, and `review` returns the structured file/hunk model. Review exports omit raw patches and notes by default; request them explicitly with `--include-patch` and `--include-notes`. Every session command has human-readable output by default and stable compact JSON with `--json`.

Select a session by its ID or canonical repository root. A repository selector must match exactly one live review:

```bash
ramo session context --repo .
ramo session navigate SESSION_ID --file src/lib.rs --hunk 2
ramo session navigate SESSION_ID --file src/lib.rs --new-line 42
ramo session navigate SESSION_ID --next-comment
```

Hunk numbers are positive and 1-based at the CLI. Navigation also accepts `--old-line`, `--new-line`, and `--prev-comment`. Session paths are a third deterministic selector in the wire protocol; reload exposes it as `--session-path PATH`. Empty, conflicting, missing, and ambiguous selectors fail instead of choosing an arbitrary terminal.

Replace a live review’s source without changing its session ID:

```bash
ramo session reload SESSION_ID -- diff main...HEAD -- src
ramo session reload --repo . -- show HEAD~1
ramo session reload --session-path /dev/pts/7 --source ./nested -- patch review.patch
```

The command after `--` is parsed by the normal typed review CLI. Pager and stdin-backed patch inputs are rejected because they cannot be repeated. Reload is transactional: loading and config resolution must succeed before the visible review or watch plan changes. Selection falls back safely if its target disappears, while human and live comments whose stable file targets remain are preserved.

Live comments use the same native note geometry and STML renderer as in-process agent notes:

```bash
ramo session comment add SESSION_ID --file src/lib.rs --new-line 42 \
  --summary "Check this retry" --rationale "The final attempt still sleeps" \
  --markup '<badge color=warning>RETRY</badge>' --author Pi --focus
ramo session comment list SESSION_ID --type live --json
ramo session comment rm SESSION_ID COMMENT_ID
ramo session comment clear SESSION_ID --file src/lib.rs --yes
```

`comment list --type` accepts `live`, `agent`, `ai`, `user`, or `all`. `comment apply SESSION_ID --stdin` accepts a JSON array (or `{ "comments": [...] }`) of at most 100 comments; `--focus` reveals and selects the first. Clearing requires `--yes`, removes only live comments by default, and touches human notes only with `--include-user` or `--all`.

The broker binds only to loopback and validates HTTP `Host`/`Origin` authority. Configure it with `RAMO_SESSION_HOST` and `RAMO_SESSION_PORT` (default `127.0.0.1:47657`); `HUNK_MCP_HOST` and `HUNK_MCP_PORT` remain compatibility aliases. Non-loopback hosts are rejected. HTTP bodies are limited to 256 KiB, internal frames to 1 MiB, text fields to 64 KiB, and ordinary/reload operations to 5/30-second waits.

Reload filesystem reads are confined to the initial session’s canonical repository root, including symlink resolution. `--source`, direct files, patch files, and `--agent-context` paths outside that root are rejected; `--agent-context -` is not accepted for session reload. Sessions initially opened from stdin or from files outside a repository cannot be remotely reloaded. No session input is evaluated as shell text.

The broker starts on demand, prunes sessions silent for 45 seconds, and exits after 60 idle seconds. Live TUIs reconnect after a broker restart. A stale compatible ramo broker is shut down and replaced; an unrelated service on the configured port is never killed. Normal TUI exit unregisters immediately. `ramo daemon serve` runs the broker in the foreground, and `ramo mcp serve` is a command-compatible alias; the old browser/MCP endpoint is intentionally gone in favor of these native session commands.

`ramo skill path` atomically materializes the embedded `ramo-review` agent skill in the platform data directory and prints its path. The skill instructs agents to use this same native command surface.

## Current controls

The review UI is a continuous file stream with an explicit highlighted cursor. Every file keeps a visible identity header even when the responsive sidebar is hidden. `auto` uses split layout at 160 columns and stack layout below 160; the sidebar appears at 220 columns. There is deliberately no top menu bar or dropdown UI.

| Key | Action |
|---|---|
| `j` / `k`, Up/Down | Move to the previous/next diff row |
| `h` / `l` | Focus the left/right side in split layout |
| Left/Right | Scroll code horizontally; Shift moves faster |
| `Space` / `f`, `b` | Page down/up |
| `d` / `u` | Half-page down/up |
| `g` / `G`, Home/End | Jump to top/bottom |
| `[` / `]` | Previous/next hunk |
| `,` / `.` | Previous/next file |
| `{` / `}` | Previous/next annotated hunk |
| `1` / `2` / `0` | Split/stack/auto layout |
| `s`, `n`, `w`, `m` | Sidebar, line numbers, wrapping, hunk headers |
| `a` | Reveal/hide AI and agent notes |
| `A` | Open the native agent-skill setup; `y`/Enter copies its prompt |
| `z` | Expand/collapse unchanged context |
| `/` | Focus the file filter; `Tab` returns to review; Escape clears and exits |
| `t`, `?` | Theme selector and controls help |
| `V`, `y` | Select lines and copy through OSC 52 |
| `Ctrl-t`, `Ctrl-Shift-t` | Send selection to tmux / choose a new target |
| `c` | Create an inline human review note |
| `e`, `r` | Open in `$EDITOR` / reload now |
| `Ctrl-z` | Suspend and return terminal ownership on Unix |
| `q` | Quit |

Ordinary line movement follows semantic diff rows rather than treating the viewport as the selection. Page and wheel scrolling move the viewport and place the cursor on the selectable row nearest its center. Hunk and file jumps land on their first diff row; `g` and `G` clamp to the first and last diff rows.

The mouse wheel scrolls vertically; Shift-wheel and native horizontal-wheel events scroll code horizontally. Left-click selects sidebar files or collapsed context. The scrollbar and sidebar divider are draggable. Dragged text uses terminal-cell-aware selection, including full-width Unicode characters, and copies through the same OSC 52 path as `V`/`y`.

## View configuration

User preferences live at the platform config path (for example `~/.config/ramo/config.toml` on Linux); repository overrides live in the nearest `.ramo/config.toml`:

```toml
mode = "auto"
theme = "auto"
show_sidebar = true
line_numbers = true
wrap_lines = false
hunk_headers = true
agent_notes = false
copy_decorations = false
transparent_background = false
prompt_save_view_preferences = true
```

Press `t` to preview embedded or custom themes. When interactive view settings change, `q` offers save, discard, never-ask, and cancel choices. Saving edits only changed user-global keys and preserves unrelated TOML comments, command sections, and custom-theme tables. Pager mode never persists view changes.

`copy_decorations = true` includes the rendered line-number/change-marker gutter in full-line copies; the default copies code only. `transparentBackground` remains accepted as Hunk's compatibility alias for `transparent_background`. Deprecated `[custom_theme.syntax]` semantic colors are translated to approximate TextMate scopes and surfaced as a startup notice; exact `[custom_theme.syntax_scopes]` entries override translated values.

After an installed-version change, `ramo` shows a one-time local reminder to refresh any copied agent skill with `ramo skill path`. It also performs an opportunistic, nonblocking `git ls-remote` query for newer GitHub release tags: the first check is delayed 1.2 seconds, the child is killed after five seconds, failures or a missing optional Git executable are ignored, and a long-running review checks again every six hours. Notices are deduplicated, queued for seven seconds each, and suppressed in pager mode. Set `RAMO_DISABLE_UPDATE_NOTICE=1` (or Hunk's compatibility name `HUNK_DISABLE_UPDATE_NOTICE=1`) to disable both update notices. This adds no TLS library or mandatory runtime dependency to the `ramo` executable.

The default `theme = "auto"` reads `COLORFGBG` when available and otherwise selects the dark GitHub default. Startup never sends an active terminal background query, so terminal responses cannot be mistaken for keyboard input. Explicit and custom themes skip auto resolution.

All of this ships in the same Rust executable. Syntax highlighting uses Syntect's pure-Rust regex backend; the dependency graph contains no Oniguruma C implementation. `ramo` does not invoke Node.js, Bun, TypeScript, a browser, or Hunk at runtime.

## Comment output

On quit, `ramo` can write comments to `ramo-review.md`, an explicit `--output` path, or stdout:

```markdown
## Review Comments

### src/auth.rs:L10 → R10
> +    token.len() > 0

Should use proper JWT validation.
```

## Pi integration

```bash
ramo install pi
ramo uninstall pi
```

The installed `/ramo` prompt accepts `staged`, `branch <name>`, or `commit <sha>`, then directs Pi to run this native executable and return its Markdown review comments. Installation writes `~/.pi/agent/prompts/ramo.md`; it installs no TypeScript extension or runtime helper.

## Development

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo build --release
```

The approved architecture and execution plans are in:

- [`docs/superpowers/specs/2026-07-20-hunk-feature-parity-design.md`](docs/superpowers/specs/2026-07-20-hunk-feature-parity-design.md)
- [`docs/superpowers/plans/2026-07-20-foundation-cli-implementation-plan.md`](docs/superpowers/plans/2026-07-20-foundation-cli-implementation-plan.md)
- [`docs/superpowers/plans/2026-07-20-vcs-pager-implementation-plan.md`](docs/superpowers/plans/2026-07-20-vcs-pager-implementation-plan.md)
- [`docs/superpowers/plans/2026-07-21-watch-process-implementation-plan.md`](docs/superpowers/plans/2026-07-21-watch-process-implementation-plan.md)
- [`docs/superpowers/plans/2026-07-21-notes-markup-implementation-plan.md`](docs/superpowers/plans/2026-07-21-notes-markup-implementation-plan.md)
