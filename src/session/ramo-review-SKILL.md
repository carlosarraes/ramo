---
name: ramo-review
description: Inspect and control a live ramo review through the native ramo session CLI.
---

# ramo live review

Use `ramo session list` to discover live reviews, then select exactly one review by session id or `--repo <path>`.

- Inspect focus with `ramo session context <id>`.
- Export review structure with `ramo session review <id> --include-notes`.
- Add one finding with `ramo session comment add <id> --file <path> --new-line <n> --summary <text>`.
- Apply a bounded JSON batch through `ramo session comment apply <id> --stdin`.
- Preview STML using `ramo markup render - --width <noteMarkupWidth>`.
- Navigate with `ramo session navigate`, and remove only comment ids returned by the selected session.

Never clear comments unless the user explicitly requested it; `ramo session comment clear` requires `--yes` and preserves human notes unless `--include-user` or `--all` is present.
