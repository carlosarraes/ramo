---
description: Review changes with the native pdiff executable
argument-hint: "[staged|branch <name>|commit <sha>]"
---
Use the native `pdiff` executable to review the requested target. Do not create or invoke a JavaScript or TypeScript wrapper.

Choose the target from `$ARGUMENTS`:

- no argument or `staged`: the review command is `pdiff diff --staged`
- `branch <name>`: the review command is `pdiff diff <name>...HEAD`
- `commit <sha>`: the review command is `pdiff show <sha>`

Run the chosen command interactively and request Markdown output by putting the global output option before the command, for example `pdiff --output .pdiff-review.md diff --staged`. After pdiff exits, read `.pdiff-review.md` if it exists, return its review comments to this conversation, and remove only that generated file. If pdiff reports no changes or no comments, say so directly.
