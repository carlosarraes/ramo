# Ramo Rename Design

## Goal

Rename the project from `pdiff`/Pi Diff to **Ramo** without changing its review behavior, and publish the existing Git history as the public `carlosarraes/ramo` repository.

## Product identity

Ramo is a friendly, standalone brand for the Rust-native terminal review tool. The Pi integration remains supported, but it is presented as one optional integration rather than the project's identity.

The rename is intentionally clean rather than transitional:

- the Cargo package, library crate, and executable are `ramo`;
- user configuration moves to `.ramo/config.toml` and the platform `ramo` config directory;
- project environment variables use the `RAMO_` prefix;
- archives, installers, session identity, temporary names, skill names, and generated Pi prompt names use `ramo`;
- user-facing commands and diagnostics use `ramo`;
- tracked documentation and tests contain no `pdiff` or `pi-diff` branding.

Hunk-named compatibility variables remain supported because they are part of the compatibility contract. Old `PDIFF_` variables and the `pdiff` executable are not retained.

## Documentation

Rewrite the README around Ramo's current product: a single native executable for terminal diff review, with Git/Jujutsu/Sapling inputs, keyboard-first review, notes, live sessions, and optional Pi integration. Installation examples and all commands must point to `carlosarraes/ramo` and invoke `ramo`.

## Publication

Create `github.com/carlosarraes/ramo` as a public repository. Reuse the current Git repository and push `main` plus all existing tags so commit history is preserved. Replace the stale local `upstream` URL with the new repository URL.

## Verification

- no tracked working-tree reference to `pdiff`, `pi-diff`, `PDIFF_`, or the old GitHub repository remains;
- `ramo --version` and `ramo diff --help` succeed;
- format, strict Clippy, full tests, installer dry-runs, and a locked release build pass;
- the GitHub repository is public, `upstream/main` matches local `main`, all tags are present, and the local tree is clean.
