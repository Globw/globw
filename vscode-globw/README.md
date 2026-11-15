# Globw for VS Code

Language support for **[Globw](https://github.com/Globw/globw)** — a statically
typed, interpreted language (`.gw` files). Syntax highlighting, a run button,
live error checking, completion, hover and formatting, right in the editor.

## Features

- **Syntax highlighting** for `.gw` files (keywords, types, strings, numbers,
  comments, built-in modules).
- **Run** the current program with one click (the ▶ button in the editor title
  bar) or by pressing **F5** — the output opens in an integrated terminal.
- **Debugger**: click the debug button next to ▶ (or use *Run and Debug*).
  Breakpoints in the margin, step over / into / out, pause, the call stack, and
  the variables of every frame — lists, dictionaries and objects expand. An
  uncaught run-time error stops on the failing line, so you can see why.
- **Live errors**: your file is checked on open and on save, and the errors are
  underlined in place (including errors in the modules it imports).
- **Completion** for every built-in function and the language keywords.
- **Hover** over a built-in to see its signature.
- **Format Document** (`Shift+Alt+F`): re-indents the file to canonical form.
- A file icon for `.gw` files.

> **Tip:** `gw --repl` opens an interactive session where you can try
> expressions, declarations and even multi-line `function`/`for` blocks.

## Requirements

You need the Globw interpreter, the **`gw`** executable:

1. Download it from the [releases](https://github.com/Globw/globw/releases)
   (or build it from source), then either
2. add its folder to your **PATH** (so `gw` works from any terminal), or
3. set **`globw.path`** in the settings to the full path of `gw` (`gw.exe` on
   Windows).

If the extension is opened inside the Globw repository, it also picks up a local
`target/release/gw` build automatically.

## Usage

Open any `.gw` file:

```gw
output("Hello from Globw!")
```

Press **F5** (or click ▶) to run it. Save the file to see errors underlined.

## Debugging

Put breakpoints in the margin and click the debug button in the editor title bar.
To keep options, add a `launch.json` (*Run and Debug → create a launch.json file
→ Globw*):

```json
{
  "type": "globw",
  "request": "launch",
  "name": "Debug Globw program",
  "program": "${file}",
  "stopOnEntry": false,
  "args": ["--allow-read"],
  "input": "Ada\n42\n"
}
```

- `args`: the permissions the program gets, as on the command line
  (`--allow-read`, `--allow-write`, `--allow-net`, `--allow-run`, `--allow-all`).
- `input`: what `input()` reads while debugging, one line per answer (the debug
  console cannot type into the program). Program output appears in the
  *Debug Console*.

## Settings

| Setting      | Description |
|--------------|-------------|
| `globw.path` | Path to the `gw` executable. Leave empty to use a local `target/release/gw` build, or `gw` from your PATH. |

## License

[Apache-2.0](https://github.com/Globw/globw/blob/master/LICENSE) — © 2026 Globw.
