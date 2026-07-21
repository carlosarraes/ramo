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

Working-copy reviews include untracked files by default; use `--exclude-untracked` to omit them. Tracked and untracked files over 1,000,000 bytes or 20,000 lines become bounded placeholders so a review cannot consume unbounded memory. Git source endpoints are retained for later context expansion without embedding another runtime.

Use `pager` when a command may produce either a diff or ordinary text:

```bash
git diff --no-color | pdiff pager
PDIFF_TEXT_PAGER="less -R" command-producing-text | pdiff pager
```

Diff-shaped input enters the review UI. Other text is sanitized and sent directly to `PDIFF_TEXT_PAGER`, then `PAGER`, then `less -R`. Pager settings are parsed into a program and literal arguments without a shell; environment assignments are supported, shell operators are not executed, and recursive `pdiff pager` settings fall back safely.

Watch execution, the Hunk-compatible UI replacement, notes, STML, sessions, and final release parity remain staged. See the [parity ledger](docs/parity/hunk.md) for behavior-by-behavior evidence; commands are not considered complete merely because their arguments parse.

## Current controls

These are the retained `pdiff` controls during the UI migration:

| Key | Action |
|---|---|
| `j` / `k`, arrows | Navigate lines |
| `h` / `l` | Switch old/new focus |
| `gg` / `G` | Jump to top/bottom |
| `Ctrl-d` / `Ctrl-u` | Half-page scroll |
| `]` / `[` | Next/previous hunk |
| `H` / `L` | Previous/next file |
| `V` | Visual-line selection |
| `y` | Copy the current line or selection |
| `c` | Create or edit a comment |
| `/`, `n`, `N` | Search and navigate matches |
| `e` | Toggle file list |
| `E` | Toggle expanded comments |
| `F` | Focus one side at full width |
| `Tab` | Toggle current layout |
| `q` | Quit |

The Hunk-compatible keyboard map will replace conflicting bindings during the review-UI slice. Existing actions will move to documented non-conflicting bindings rather than disappear.

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

The installed `/pdiff` command selects a review target and returns exported comments to Pi. Its filesystem-level integration tests and normalized target handling remain tracked in the parity ledger.

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
