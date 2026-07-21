# ramo terminal markup (STML)

Render a file with `ramo markup render note.stml`, or pipe markup to
`ramo markup render -`. Use `--width`, `--color`, and `--json` for deterministic
automation output.

Inline emphasis and semantic color:

```stml
<h1>Review summary</h1>
<b>Important</b>, <i>contextual</i>, <u>underlined</u>, and <color fg=success>passing</color>.
```

Badges and keyboard hints:

```stml
<badge color=success>PASS</badge> Press <kbd>Enter</kbd> to continue.
```

Cards:

```stml
<card title="Finding"><b>Null handling</b>
The fallback remains deterministic.</card>
```

Columns stack automatically when the terminal is too narrow:

```stml
<row gap=1><box border title=Before>old value</box><box border title=After>new value</box></row>
```

Lists, rules, and spacing:

```stml
<ol><item>Inspect the change</item><item>Run focused tests</item></ol>
<hr><spacer size=1/><muted>No JavaScript runtime is required.</muted>
```

Code is clipped to the requested terminal width:

```stml
<code title=rust>fn main() {
    println!("single native binary");
}</code>
```
