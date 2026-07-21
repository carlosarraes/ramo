# Ramo Rename Design

## Goal

Rename the project from its legacy Pi-oriented identity to **Ramo** without changing its review behavior, and publish the existing Git history as the public `carlosarraes/ramo` repository.

## Product identity

Ramo is a friendly, standalone brand for the Rust-native terminal review tool. The Pi integration remains supported, but it is presented as one optional integration rather than the project's identity.

The rename is intentionally clean rather than transitional:

- the Cargo package, library crate, and executable are `ramo`;
- user configuration moves to `.ramo/config.toml` and the platform `ramo` config directory;
- project environment variables use the `RAMO_` prefix;
- archives, installers, session identity, temporary names, skill names, and generated Pi prompt names use `ramo`;
- user-facing commands and diagnostics use `ramo`;
- tracked documentation and tests use Ramo branding, except for the narrowly scoped legacy-binary migration check.

Hunk-named compatibility variables remain supported because they are part of the compatibility contract. Old project environment variables and the legacy executable are not retained as supported interfaces.

The Unix installer checks only for the legacy executable beside the newly installed `ramo` binary. It shows the exact path and asks before removal. It never deletes a same-named program elsewhere on `PATH`; without a terminal it leaves the file in place and prints an explicit cleanup instruction. `RAMO_REMOVE_LEGACY=yes` and `RAMO_REMOVE_LEGACY=no` provide deterministic non-interactive choices.

## Documentation

Rewrite the README around Ramo's current product: a single native executable for terminal diff review, with Git/Jujutsu/Sapling inputs, keyboard-first review, notes, live sessions, and optional Pi integration. Installation examples and all commands must point to `carlosarraes/ramo` and invoke `ramo`.

## Publication

Create `github.com/carlosarraes/ramo` as a public repository. Reuse the current Git repository and push `main` plus all existing tags so commit history is preserved. Replace the stale local `upstream` URL with the new repository URL.

## Verification

- old-brand matches are limited to the intentional installer migration and its tests;
- `ramo --version` and `ramo diff --help` succeed;
- format, strict Clippy, full tests, installer dry-runs, and a locked release build pass;
- the GitHub repository is public, `upstream/main` matches local `main`, all tags are present, and the local tree is clean.
