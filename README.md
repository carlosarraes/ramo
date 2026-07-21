# pdiff

`pdiff` is a review-first terminal diff viewer written entirely in Rust. It is distributed as one native executable with no Node.js, Bun, TypeScript, or browser runtime.

The project is migrating Hunk's review workflows into Rust while retaining `pdiff`'s Vim selection, Markdown comments, tmux sending, and Pi integration. Hunk's top menu bar and dropdown menus are intentionally excluded; actions remain keyboard-first.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/carlosarraes/pdiff/main/install.sh | bash
```

Or install the Rust package directly:

```bash
cargo install --git https://github.com/carlosarraes/pdiff --locked
```

## Verified review inputs

Review patch output from any command:

```bash
git diff --no-color | pdiff
git diff --cached --no-color | pdiff
gh pr diff 123 --color=never | pdiff
```

Review a saved patch or compare two concrete files without an external diff program:

```bash
pdiff patch review.patch
pdiff patch - < review.patch
pdiff diff before.rs after.rs
```

Legacy patch flags remain supported:

```bash
pdiff --input review.patch
pdiff --input review.patch --output pdiff-review.md
pdiff --input review.patch --stdout
```

Native repository reviews are also verified:

```bash
pdiff diff
pdiff diff --staged
pdiff diff main...HEAD -- src
pdiff show HEAD~1
pdiff stash show 'stash@{0}'
```

`pdiff` selects the nearest Git, Jujutsu, or Sapling checkout. Set `vcs = "git"`, `vcs = "jj"`, or `vcs = "sl"` in user or `.pdiff/config.toml` configuration when a checkout contains more than one marker. Jujutsu and Sapling support working-copy and show reviews, and reject staged and stash operations with an explicit diagnostic instead of silently changing semantics.

Working-copy reviews include untracked files by default; use `--exclude-untracked` to omit them. Tracked and untracked files over 1,000,000 bytes or 20,000 lines become bounded placeholders so a review cannot consume unbounded memory. Press `z` to expand collapsed unchanged context from bounded native old/new source readers.

Use `pager` when a command may produce either a diff or ordinary text:

```bash
git diff --no-color | pdiff pager
PDIFF_TEXT_PAGER="less -R" command-producing-text | pdiff pager
```

Diff-shaped input enters the review UI. Other text is sanitized and sent directly to `PDIFF_TEXT_PAGER`, then `PAGER`, then `less -R`. Pager settings are parsed into a program and literal arguments without a shell; environment assignments are supported, shell operators are not executed, and recursive `pdiff pager` settings fall back safely.

Normalized agent notes, STML, live sessions, and final cross-platform release closure remain staged. See the [parity ledger](docs/parity/hunk.md) for behavior-by-behavior evidence; commands are not considered complete merely because their arguments parse.

## Watch, reload, and editor integration

Use `--watch` with direct files or native repository reviews:

```bash
pdiff diff before.rs after.rs --watch
pdiff diff --watch
```

Direct files and Git working trees use native filesystem events with a quiet debounce and safety polling. Jujutsu and Sapling use bounded polling. Atomic-save bursts coalesce into one serialized reload; stale generations are rejected, and failures leave the last valid review visible. Press `r` for an immediate reload even when `--watch` is not enabled.

Press `e` to open the selected file at its selected line through `$EDITOR`. `vi`, `vim`, and `nvim` receive `+line`; VS Code and Cursor receive `--goto file:line`; Helix receives `file:line`. Commands are parsed into literal argv without a shell. Terminal editors temporarily return terminal ownership and redraw afterward. On Unix, `Ctrl-z` suspends `pdiff`; resuming the job restores the review.

## Current controls

The review UI is a continuous file stream. `auto` uses split layout at 160 columns and stack layout below 160; the responsive sidebar appears at 220 columns. There is deliberately no top menu bar or dropdown UI.

| Key | Action |
|---|---|
| `j` / `k`, arrows | Scroll one row |
| `Space` / `f`, `b` | Page down/up |
| `d` / `u` | Half-page down/up |
| `g` / `G`, Home/End | Jump to top/bottom |
| `[` / `]` | Previous/next hunk |
| `,` / `.` | Previous/next file |
| `{` / `}` | Previous/next annotated hunk |
| `1` / `2` / `0` | Split/stack/auto layout |
| `s`, `l`, `w`, `m` | Sidebar, line numbers, wrapping, hunk headers |
| `z` | Expand/collapse unchanged context |
| `/` | Focus the file filter; `Tab` returns to review |
| `t`, `?` | Theme selector and controls help |
| `V`, `y` | Select lines and copy through OSC 52 |
| `Ctrl-t`, `Ctrl-Shift-t` | Send selection to tmux / choose a new target |
| `c` | Create a review note |
| `e`, `r` | Open in `$EDITOR` / reload now |
| `Ctrl-z` | Suspend and return terminal ownership on Unix |
| `q` | Quit |

The mouse wheel scrolls vertically; Shift-wheel and native horizontal-wheel events scroll code horizontally. Left-click selects sidebar files or collapsed context. The scrollbar and sidebar divider are draggable. Dragged text uses terminal-cell-aware selection, including full-width Unicode characters, and copies through the same OSC 52 path as `V`/`y`.

## View configuration

User preferences live at the platform config path (for example `~/.config/pdiff/config.toml` on Linux); repository overrides live in the nearest `.pdiff/config.toml`:

```toml
mode = "auto"
theme = "github-dark-default"
show_sidebar = true
line_numbers = true
wrap_lines = false
hunk_headers = true
agent_notes = false
transparent_background = false
prompt_save_view_preferences = true
```

Press `t` to preview embedded or custom themes. When interactive view settings change, `q` offers save, discard, never-ask, and cancel choices. Saving edits only changed user-global keys and preserves unrelated TOML comments, command sections, and custom-theme tables. Pager mode never persists view changes.

All of this ships in the same Rust executable. `pdiff` does not invoke Node.js, Bun, TypeScript, a browser, or Hunk at runtime.

## Comment output

On quit, `pdiff` can write comments to `pdiff-review.md`, an explicit `--output` path, or stdout:

```markdown
## Review Comments

### src/auth.rs:10-12(new)
> +    token.len() > 0

Should use proper JWT validation.
```

## Pi integration

```bash
pdiff install pi
pdiff uninstall pi
```

The installed `/pdiff` prompt accepts `staged`, `branch <name>`, or `commit <sha>`, then directs Pi to run this native executable and return its Markdown review comments. Installation writes `~/.pi/agent/prompts/pdiff.md`; it installs no TypeScript extension or runtime helper.

## Development

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo build --release
```

The approved architecture and execution plan are in:

- [`docs/superpowers/specs/2026-07-20-hunk-feature-parity-design.md`](docs/superpowers/specs/2026-07-20-hunk-feature-parity-design.md)
- [`docs/superpowers/plans/2026-07-20-foundation-cli-implementation-plan.md`](docs/superpowers/plans/2026-07-20-foundation-cli-implementation-plan.md)
- [`docs/superpowers/plans/2026-07-20-vcs-pager-implementation-plan.md`](docs/superpowers/plans/2026-07-20-vcs-pager-implementation-plan.md)
- [`docs/superpowers/plans/2026-07-21-watch-process-implementation-plan.md`](docs/superpowers/plans/2026-07-21-watch-process-implementation-plan.md)
