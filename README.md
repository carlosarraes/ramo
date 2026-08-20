# Ramo

Ramo is a fast, review-first diff viewer for the terminal. It turns working-tree changes, revision ranges, patches, and direct file comparisons into one keyboard-first review surface.

It is written entirely in Rust and needs no Node.js, Bun, TypeScript, browser, or language runtime. The normal terminal workflow remains one native `ramo` executable. Release archives also include the optional native `ramo-server` companion for private local-AI Review Maps and phone access. Ramo includes Hunk-compatible review workflows while keeping Vim-style selection, Markdown comments, tmux sending, live agent sessions, and optional Pi integration. Hunk's top menu bar and dropdowns are intentionally excluded.

```bash
# Review everything changed on the current branch since it diverged from main
ramo diff main...HEAD

# Review staged changes
ramo diff --staged

# Review GitHub PR #123 and publish new comments as one GitHub review
ramo pr 123
```

Ramo means “branch” in Portuguese—a small nod to where most reviews begin.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/carlosarraes/ramo/main/install.sh | bash
```

Or install the Rust package directly:

```bash
cargo install --git https://github.com/carlosarraes/ramo --package ramo --locked
# Optional local-AI/mobile companion:
cargo install --git https://github.com/carlosarraes/ramo --package ramo-server --locked
```

On Windows PowerShell:

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/carlosarraes/ramo/main/install.ps1 -OutFile install.ps1
.\install.ps1
```

The release matrix produces one archive containing `ramo` and the optional `ramo-server` companion for Linux, macOS, and Windows on x86-64 and ARM64. `install.sh` selects the Linux/macOS tarball; `install.ps1` selects the Windows zip and installs both `.exe` files under `%LOCALAPPDATA%\Programs\ramo` by default. Neither installer adds a language runtime or enables the background service automatically.

After a successful Unix install, the script checks for the legacy binary in the same install directory and asks before removing it. It never removes a similarly named program elsewhere on `PATH`. For unattended migration, set `RAMO_REMOVE_LEGACY=yes` or `RAMO_REMOVE_LEGACY=no`.

### Private local-AI Review Maps

Ramo can build a review-first map of a GitHub PR: exact file/addition/deletion facts appear immediately and entirely locally, while an AI model adds bounded summaries, risk cues, logical groups, and recommended review order.

**From 0.1.0 the enrichment runs through the `pi` CLI against a remote provider, and it is off by default.** Enabling `[map] enabled = true` means every changed file's patch is sent to the configured provider when you open a pull request. The exact map — paths, counts, groups, order — is computed on your machine and still renders instantly with no network at all, so a disabled map costs you only the summaries.

The loopback Ollama path has not been deleted: `ramo-server` still speaks it, and it remains the option for private source that must not leave the machine. The companion binds only to loopback either way; Android reaches it through authenticated Tailscale Serve, and keeps working unchanged because only the server's backend changed.

```bash
ramo server setup --dry-run
ramo server setup
ramo server status
ramo server pair
```

Setup is explicit and currently automated on Linux. It checks `gh`, a running Ollama service, Tailscale/MagicDNS, and `systemd --user` before writing anything. See [Private local Review Maps](docs/server.md) for the privacy boundary, pairing, cache commands, troubleshooting, and typed failures.

`ramo pr N` opens the exact Review Map first. Use `j`/`k` or arrows to move, `h`/`l` to fold or expand, Enter to open one file, `/` to filter, and `M` to switch between map and code without rebuilding the review. Lowercase `m` keeps its existing hunk-header binding. Local diffs remain code-first; press `M` for their deterministic map. Pager mode never starts background analysis. To open pull requests directly on the code view instead, set `start_on_map = false` in the configuration or pass `--no-start-on-map`; the map stays one `M` away.

The terminal enrichment client accepts only loopback HTTP and a paired-client token file. Exchange a five-minute pairing code locally, then point Ramo at the saved credential:

Ramo reads its configuration from the platform config directory, which is **not** `~/.config` on
macOS. Resolve it once and reuse it:

```bash
# Linux: ~/.config/ramo · macOS: ~/Library/Application Support/ramo
case "$(uname -s)" in
  Darwin) RAMO_CONFIG="$HOME/Library/Application Support/ramo" ;;
  *)      RAMO_CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/ramo" ;;
esac
mkdir -p "$RAMO_CONFIG"

ramo-server pair
# Replace PAIRING_CODE below with the printed code.
curl -fsS http://127.0.0.1:47831/v1/pair/exchange \
  -H 'Content-Type: application/json' \
  -d '{"code":"PAIRING_CODE","label":"Ramo terminal"}' \
  > "$RAMO_CONFIG/review-map-client.json"
chmod 600 "$RAMO_CONFIG/review-map-client.json"
```

`ramo server setup` installs a **systemd** user unit and is Linux-only. On macOS run the service
directly with `ramo-server serve` and pair as above; pairing works without the setup step.

Add this to `~/.config/ramo/config.toml` using the resulting absolute path:

```toml
[map]
enabled = true
server = "http://127.0.0.1:47831"
token_file = "/home/you/.config/ramo/review-map-client.json"
# backend = "ollama"   # keep enrichment on this machine instead of the pi provider
```

`backend`, `provider`, `model`, and `effort` are read by `ramo-server`, which owns the model call.
Restart the service after changing them.

Without a token, server, or Ollama, the exact tree remains usable and the failure notice can be dismissed. Existing comments and publication state survive every map/code transition.

### Android PR reviews

Ramo also has a standalone, personal Android client for focused GitHub reviews: a two-tab inbox, map-first PR navigation, one-file unified diffs, Tokyo Night syntax colors, encrypted drafts, Viewed synchronization, and Comment/Approve/Request changes publication. Its GitHub review path uses a narrow fine-grained token and works without the laptop. Optional AI summaries pair to `ramo-server` over tailnet HTTPS; the phone sends only PR identity and expected head SHA, while raw code stays between GitHub and the laptop's local Ollama model.

See [Ramo for Android](docs/android.md) for token setup, build/install commands, notification behavior, security details, and v1 limitations.

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

## GitHub pull request reviews

With the authenticated [GitHub CLI](https://cli.github.com/) installed, run
Ramo inside the target repository:

```bash
gh auth login
ramo pr 123
ramo pr 123 --with-comments
```

Ramo loads a frozen PR snapshot without checking out the branch or changing
the working tree. It starts on the Review Map; Enter opens the selected file
and `M` returns to the same map position. New inline notes are kept local while you review. Press `q`
to open the publication confirmation, then choose Comment only, Approve, or
Request changes. Press `o` before choosing a verdict to edit the generated
overall comment. At the first prompt, `n` or Escape returns to the review with
all notes intact; `d` explicitly discards them and quits.

By default the overall comment reads `Review submitted from Ramo with N inline
comments.` Set `review_message` in your configuration to replace it with a fixed
body for every verdict — `review_message = "approved"` — or with an empty string
to publish no overall comment at all. Editing the body with `o` always wins over
the configured value.

Immediately before publishing, Ramo checks that the PR head commit still
matches the loaded snapshot. If it changed, nothing is submitted and the notes
remain open. GitHub receives one review containing the overall body and every
new Ramo inline comment. Reviews of your own PR offer Comment only because
GitHub rejects self-approval and self-requested changes.

Add `--with-comments` to fetch one snapshot of unresolved, non-outdated GitHub
inline and file-level review threads. They appear as read-only conversation
cards from every author. Ramo maps comments to the frozen diff when possible;
anything that cannot be mapped remains visible under `Unplaced GitHub comments`.
Publishing still sends only newly created Ramo comments—imported feedback is
never copied into the outgoing review.

Press `z` to lazily fetch unchanged context from the captured base or head commit
through the authenticated GitHub CLI. Ramo does not fetch source blobs when the
review opens, and it caches each result for later expand/collapse actions. If a
snapshot file is missing, inaccessible, too large, or otherwise unavailable,
Ramo shows a dismissible message and keeps the review intact.

Imported threads are a read-only snapshot: Ramo does not reply to or resolve
them, refresh them, watch/reload the PR, or open snapshot files in the local
editor. GitLab and Bitbucket are not supported yet. The view-only generic patch
workflow remains available:

```bash
gh pr diff 123 --color=never | ramo
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

Press `a` to reveal or hide AI/agent notes and `{`/`}` to move between annotated hunks. External notes marked as `source: "user"` remain visible; only notes authored interactively in this `ramo` process are exported as Markdown. Press `c` to start an inline human note, Enter or `Ctrl-S` to save, Shift-Enter for a newline, or Escape to cancel. Clicking a saved human note reopens it for editing; saving it empty removes it.

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

The review UI is a continuous file stream with an explicit highlighted cursor. Every file keeps a visible identity header even when the responsive sidebar is hidden. A sticky header shows the review identity, file count, and colored total additions/deletions; the footer shows monotonic changed-line review progress. Unified layout is the default. `--mode split` keeps the side-by-side view, while `--mode auto` uses split layout at 160 columns and unified layout below 160; the sidebar appears at 220 columns. The deprecated `stack` spelling remains accepted as a compatibility alias for `unified`. There is deliberately no dropdown UI.

| Key | Action |
|---|---|
| `j` / `k`, Up/Down | Move to the previous/next diff row |
| `h` / `l` | Focus the left/right side in split layout |
| Left/Right | Scroll code horizontally; Shift moves faster |
| `Space` / `f`, `b` | Page down/up |
| `d` / `u`, `Ctrl-d` / `Ctrl-u` | Half-page down/up |
| `g` / `G`, Home/End | Jump to top/bottom |
| `[` / `]` | Previous/next hunk |
| `,` / `.` | Previous/next file |
| `{` / `}` | Previous/next annotated hunk |
| `1` / `2` / `0` | Split/unified/auto layout |
| `s`, `n`, `w`, `m` | Sidebar, line numbers, wrapping, hunk headers |
| `i` | Reveal/hide AI and agent notes |
| `a` | Ask an AI about the change under the cursor; press it again inside the same lines to follow up (off by default; see Ask AI about the diff) |
| `o` | Jump to the next ready AI answer |
| `M` | Open the Review Map: the change grouped, ordered, and summarised. `M` again returns to the code |
| `P` | In `ramo pr`, read the pull request description; `j`/`k`, `d`/`u`, `g`/`G` scroll and `P`, `q`, or Escape returns |
| `L` | In `ramo pr`, read the Linear ticket the PR refers to. The identifier is inferred from the branch, title, or a Linear URL in the description; same scroll keys, `L` returns |
| `C` | Open chat. Type a question and press Enter; press `C` again on an empty prompt to return to the diff while the reply arrives. Read-only: the model can read the repository but cannot change anything |
| `M`/`L`/`P`/`C` | Switch straight between the map, the ticket, the PR body, and chat from any of them. Inside chat these switch only while the prompt is empty, so a half-written question still types |
| `Ctrl-Q` | Back to the code from whatever is open |
| `A` | Open the native agent-skill setup; `y`/Enter copies its prompt |
| `z` | Expand/collapse unchanged context |
| `T` | Compact or restore recognized test files |
| `v` | Mark the current file viewed: collapses it, counts toward reviewed progress, and advances to the next file |
| `Enter` / click | Expand one compacted file; on a viewed file this also clears its viewed mark |
| `/` | Focus the file filter; `Tab` returns to review; Escape clears and exits |
| `t`, `?` | Theme selector and controls help |
| `Ctrl-A`/`Ctrl-E`, `Ctrl-U`/`Ctrl-K`, `Ctrl-W`, `Alt-B`/`Alt-F` | Readline editing in every text input: start/end of line, kill to start/end, delete previous word, move by word. `Ctrl-U` kills to the start of the line as it does in bash |
| `V`, `y` | Select lines and copy through OSC 52 |
| `Ctrl-t`, `Ctrl-Shift-t` | Send the current line/selection to tmux / choose a new target |
| `c` | Create an inline human review note; Enter saves, Shift-Enter adds a line, `Ctrl-s` also saves |
| `e`, `r` | Open in `$EDITOR` / reload now |
| `Ctrl-z` | Suspend and return terminal ownership on Unix |
| `q` | Quit; in `ramo pr`, confirm publication and choose a verdict |

Ordinary line movement follows semantic diff rows rather than treating the viewport as the selection. Page and wheel scrolling move the viewport and place the cursor on the selectable row nearest its center. Hunk and file jumps land on their first diff row; `g` and `G` clamp to the first and last diff rows.

Review progress counts changed lines, not screen rows. It only moves forward while you scroll, jump, filter, or compact files, so returning to earlier code never reduces the percentage. Compacting a test file marks its changed lines as reviewed; pressing Enter or clicking its summary row expands that file without changing the rest.

### Ask AI about the diff

Press `a` on any hunk to ask a question about it. The request runs in the background, so you keep reviewing while it works. When the answer arrives the footer shows an `AI n · o` badge; press `o` to jump straight to the answer, which renders as a card anchored where you asked. Up to three questions can be in flight at once. `Enter` sends, `Shift+Enter` adds a line, `Esc` cancels.

Press `a` again anywhere inside a question's lines — or on its answer card, after `o` — to ask a follow-up. The card is titled `Ask AI · follow-up` and the earlier questions and answers travel with the new one, so "why not?" works without restating anything. Asking on any other line starts a fresh conversation. A follow-up is refused while the previous answer is still pending, since there would be nothing to build on.

**This is the one part of Ramo that sends your code off this machine, and it is off by default.** Enabling it means your question, the file path, and the anchored diff hunk are sent to the configured remote provider through the `pi` CLI. Ramo never sends the rest of the repository, other files, your environment, or credentials, and it never reads or handles API keys — `pi` owns `~/.pi/agent/auth.json`. Ramo runs `pi` with `--no-tools --no-session`, so nothing executes on your machine and no transcript is stored. Follow-ups keep that guarantee: each call is still a fresh, stateless `pi -p`, and the conversation is replayed from Ramo's own in-memory cards rather than from a session file on disk. ### Chat about the pull request

Press `C` to open chat. It carries the pull request, the Linear ticket when one was opened, the
file you are reading, **and the review you are writing** — your inline notes, the answers Ask has
given you, and your overall comment. Only what changed since your last question is resent, so a
long review does not resend itself every turn. pi's `read` tool lets the model follow code beyond
the diff. It is read-only by construction: no write, no edit, no shell.

Chat is full screen by default. For the older pane beside the diff:

```toml
[chat]
layout = "side"
```

**Chat keeps a transcript, unlike Ask.** A conversation needs memory, so ramo names a pi session
per review and pi stores it under `~/.pi/agent/sessions/`. That is what lets the model remember
both the thread and the files it has already read; it also means the exchange and the code it read
persist after you quit. Delete that directory to remove them.

**The conversation survives closing ramo.** Reopening the same pull request restores the thread and
resumes pi's session, so a follow-up still builds on what came before. Ramo keeps the transcript
under its own state directory, capped at the 50 most recently used conversations. pi scopes its
sessions by directory, so the same PR reviewed from two worktrees is two conversations. If pi's
session is gone — pruned, upgraded away, or deleted by you — ramo says so and replays the thread
into the next question rather than pretending the model still remembers it.

Because chat now carries your notes and Ask answers, enabling it sends those to the provider too,
alongside the code. Chat is off by default:

```toml
[chat]
enabled = true
model = "gpt-5.6-luna"
```

The Review Map is a separate path with its own switch, `[map] enabled`, which is also off by default from 0.1.0.

Turn it on deliberately:

```toml
[ask]
enabled = true
provider = "openai-codex"
model = "gpt-5.6-luna"
effort = "max"
timeout_secs = 180
```

`--no-ask` disables it for a single run; `--ask` enables it for one run without editing the config. Setting `ask_provider`/`ask_model` alone does nothing: only `ask_enabled = true` grants consent. Asking is unavailable in pager mode. If the provider rejects the model, the failure card names the model and the `pi --list-models` command that lists valid ids, so a stale `ask_model` is obvious rather than silent.

The first `Ctrl-t` opens a visible tmux pane picker; `j`/`k` chooses a target, Enter sends, and Escape cancels. Ramo remembers the target for later sends, while `Ctrl-Shift-t` always asks again. Inside a draft note, `Ctrl-t` sends the selected range, bounded code context, and comment, then saves the note only after tmux accepts the payload.

The mouse wheel scrolls vertically; Shift-wheel and native horizontal-wheel events scroll code horizontally. Left-click selects sidebar files or collapsed context. The scrollbar and sidebar divider are draggable. Dragged text uses terminal-cell-aware selection, including full-width Unicode characters, and copies through the same OSC 52 path as `V`/`y`.

## View configuration

User preferences live at the platform config path (for example `~/.config/ramo/config.toml` on Linux); repository overrides live in the nearest `.ramo/config.toml`:

```toml
[general]
prompt_save_view_preferences = true

[view]
mode = "unified"
show_sidebar = true
line_numbers = true
wrap_lines = false
hunk_headers = true
agent_notes = false
copy_decorations = false
transparent_background = false

[theme]
name = "auto"

[review]
message = "approved"        # optional; replaces the generated PR review body
tests_last = true
test_file_patterns = ["qa/**", "**/*_snapshot.*"]

[ask]
enabled = false             # opt in; see Ask AI about the diff

[map]
enabled = false             # opt in; sends whole patches to a remote provider
```

Configuration is organized into sections. A pre-0.1.0 flat config is **migrated automatically on
first run**: the user config is rewritten into sections, the original is kept beside it as
`config.toml.bak`, and a startup notice lists exactly what moved. A repository `.ramo/config.toml`
is version-controlled and shared with your team, so it is read in either shape and never rewritten.
Both spellings keep working, so an un-migrated file never blocks startup.

Press `t` to preview embedded or custom themes. When interactive view settings change, `q` offers save, discard, never-ask, and cancel choices. Saving edits only changed user-global keys and preserves unrelated TOML comments, command sections, and custom-theme tables. Pager mode never persists view changes.

`copy_decorations = true` includes the rendered line-number/change-marker gutter in full-line copies; the default copies code only. `transparentBackground` remains accepted as Hunk's compatibility alias for `transparent_background`. Deprecated `[custom_theme.syntax]` semantic colors are translated to approximate TextMate scopes and surfaced as a startup notice; exact `[custom_theme.syntax_scopes]` entries override translated values.

Ramo recognizes common test paths and names such as `tests/**`, `test/**`, `__tests__/**`, `test_*`, `*_test.*`, `*.test.*`, and `*.spec.*`. `test_file_patterns` adds project-specific glob patterns; patterns from user and repository configuration accumulate.

`tests_last = true` (the default) orders recognized test files after authored files while keeping each group in diff order, so reviews open on production code. Set `tests_last = false` or pass `--no-tests-last` to keep the order the diff provided.

After an installed-version change, `ramo` shows a one-time local reminder to refresh any copied agent skill with `ramo skill path`. It also performs an opportunistic, nonblocking `git ls-remote` query for newer GitHub release tags: the first check is delayed 1.2 seconds, the child is killed after five seconds, failures or a missing optional Git executable are ignored, and a long-running review checks again every six hours. Notices are deduplicated, queued for seven seconds each, and suppressed in pager mode. Set `RAMO_DISABLE_UPDATE_NOTICE=1` (or Hunk's compatibility name `HUNK_DISABLE_UPDATE_NOTICE=1`) to disable both update notices. This adds no TLS library or mandatory runtime dependency to the `ramo` executable.

The default `theme = "auto"` reads `COLORFGBG` when available, uses GitHub Light for a light terminal, and otherwise selects Tokyo Night, whose dark blue palette is close to tmux-recife. Startup never sends an active terminal background query, so terminal responses cannot be mistaken for keyboard input. Explicit and custom themes skip auto resolution.

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
