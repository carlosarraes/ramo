---
description: Review changes with the native ramo executable
argument-hint: "[staged|branch <name>|commit <sha>]"
---
Use the native `ramo` executable to review the requested target. Do not create or invoke a JavaScript or TypeScript wrapper.

Choose the target from `$ARGUMENTS`:

- no argument or `staged`: the review command is `ramo diff --staged`
- `branch <name>`: the review command is `ramo diff <name>...HEAD`
- `commit <sha>`: the review command is `ramo show <sha>`

Run the chosen command interactively and request Markdown output by putting the global output option before the command, for example `ramo --output .ramo-review.md diff --staged`. After ramo exits, read `.ramo-review.md` if it exists, return its review comments to this conversation, and remove only that generated file. If ramo reports no changes or no comments, say so directly.
