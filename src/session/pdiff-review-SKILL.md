---
name: pdiff-review
description: Inspect and control a live pdiff review through the native pdiff session CLI.
---

# pdiff live review

Use `pdiff session list` to discover live reviews, then select exactly one review by session id or `--repo <path>`.

- Inspect focus with `pdiff session context <id>`.
- Export review structure with `pdiff session review <id> --include-notes`.
- Add one finding with `pdiff session comment add <id> --file <path> --new-line <n> --summary <text>`.
- Apply a bounded JSON batch through `pdiff session comment apply <id> --stdin`.
- Preview STML using `pdiff markup render - --width <noteMarkupWidth>`.
- Navigate with `pdiff session navigate`, and remove only comment ids returned by the selected session.

Never clear comments unless the user explicitly requested it; `pdiff session comment clear` requires `--yes` and preserves human notes unless `--include-user` or `--all` is present.
