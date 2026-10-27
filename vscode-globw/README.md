# Globw for VS Code

Language support for **[Globw](https://github.com/Globw/globw)** — a statically
typed, interpreted language (`.gw` files). Syntax highlighting, a run button and
live error checking, right in the editor.

## Features

- **Syntax highlighting** for `.gw` files (keywords, types, strings, numbers,
  comments, built-in modules).
- **Run** the current program with one click (the ▶ button in the editor title
  bar) or by pressing **F5** — the output opens in an integrated terminal.
- **Live errors**: your file is checked on open and on save, and the errors are
  underlined in place (including errors in the modules it imports).
- A file icon for `.gw` files.

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

## Settings

| Setting      | Description |
|--------------|-------------|
| `globw.path` | Path to the `gw` executable. Leave empty to use a local `target/release/gw` build, or `gw` from your PATH. |

## License

[Apache-2.0](https://github.com/Globw/globw/blob/master/LICENSE) — © 2026 Globw.
