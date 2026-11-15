# Changelog

## 0.1.6 — unreleased

**Web interfaces.** Build a graphical interface with real HTML and CSS and keep
the logic in Globw.

- New `ui` module: `ui.open(url)` opens the default browser, `ui.app(url)` opens
  a chromeless app window (Chrome, Edge, Chromium or Brave, falling back to the
  default browser), and `ui.serve(dir, port)` serves a whole folder of static
  files and opens it — a website in one line.
- `serve.file(s, dir, path)` answers a request with a file from a folder, with
  the right content type (HTML, CSS, JS, JSON, images, fonts, wasm, audio,
  video); `/` serves `index.html`, missing files are a 404, `..` is refused.
- `serve.reply_as(s, status, type, body)` answers with any content type, e.g.
  JSON for an API.
- `serve.wait` now also returns `route` (the path without the query, decoded)
  and `query` (the query parameters, decoded): `data.text(req, "query.name")`.
- `serve.reply` and the new replies check that the status is a valid HTTP code.
- New demo `examples/demos/webui.gw` (HTML/CSS/JS in `webui/`, a JSON API in
  Globw) and a "Web interfaces" section in the tutorial.

**Debugger.** `gw --dap` speaks the Debug Adapter Protocol, and the VS Code
extension (1.48.0) uses it: breakpoints, step over / into / out, pause, the call
stack, the variables of every frame (lists, dictionaries and objects expand),
hover values, and a stop on any uncaught run-time error with the variables of the
failing line. A `launch.json` can grant permissions and give the `input()` text.

**GUI (`gtq`) toward tkinter.**

- `gtq.textbox` is now a real text field: you type into it, with a blinking
  caret, mouse and Shift selection, word moves, the system clipboard
  (Ctrl+C/X/V), undo/redo (Ctrl+Z/Y), and Tab / Shift+Tab between fields.
  Before, a click opened a separate dialog. New: `gtq.password`,
  `gtq.submitted()` (Enter), `gtq.theme("dark"|"light")` and `gtq.theme_color`.
- `gtq.textarea`: a multi-line text area with the same editing, Enter for new
  lines, Up/Down and Page Up/Down, and a scroll that follows the caret and the
  mouse wheel.
- `gtq.listbox` and `gtq.table`: scrolling lists and tables of strings, with
  click and keyboard selection, letter jumps, the wheel, a scrollbar, sorting by
  a header click (numbers as numbers) that keeps the original row index, and
  `gtq.activated()` for a double-click or Enter.
- Menus: `gtq.menubar` (titles, drop-downs, separators, shortcuts like
  `Save|Ctrl+S` that also work from the keyboard) and `gtq.context_menu` (right
  click). They are drawn on top of everything, and the widgets under an open
  menu ignore the pointer, so a click on a menu item never reaches a button
  under it.
- `screenshot("file.png")` saves the graphics window as a PNG.

**Faster.** Interpreted code runs 19–41% faster than in 0.1.5 (`bench/`, best
of 7: fibb −41%, sumb −39%, sum −38%, mandel −29%, fib −24%, sieve −19%;
nungw-bound benchmarks unchanged). Against CPython 3.14 that is now about 1.7×
on recursive calls, 2.2× on float loops and 3.9× on integer loops.

- The VM loop comes in two copies: a plain one for normal runs and a watched one
  for `--profile`, `--trace` and the debugger, so normal runs no longer check
  for those on every instruction.
- Calls: a callee's registers are set up in one pass, and returning frees only
  the registers that own memory (lists, text, objects).
- Writing a number into a register skips the old value's destructor when that
  value was a number too.
- Type-mismatch panics (which the compiler rules out) are kept out of line; this
  also made speed stable — before, unrelated code changes could move a benchmark
  by up to 40% through different inlining.

**Language.**

- **Conditional values**: `if(cond) a else b` is an expression —
  `max = if(a > b) a else b`. Only the chosen side runs (so
  `if(d == 0) 0 else x div d` never divides by zero); an `int` and a `float`
  side give a `float`; chains like `if(x < 0) "neg" else if(x == 0) "zero" else "pos"`.
- **`match` with ranges and comparisons**: `when 90 to 100`, `when < 0`,
  `when >= 18`, mixed with commas (`when < 0, > 100`); ranges work on text too.
- **Enums**: `enum Light red, yellow, green` (or an `enum … end enum` block)
  declares a type with named values — `l = Light.red`, `l as Light`,
  `Level.values` for all of them. They compare by declaration order (so `match`
  ranges work), print by name (also inside lists and dictionaries), and are
  type-checked: a value of another enum or an `int` does not fit.
- **Tuple unpacking everywhere**: `a, b = b, a` swaps without parentheses,
  `_` skips a part (`a, _ = t`), nested tuples unpack (`a, (b, c) = t`), and
  loops unpack too — `for(name, age in people)` over a list of tuples and
  `for(k, v in d)` over a dictionary's keys and values.
- **Formatted interpolation**: `${value:spec}` inside text — `${price:.2}`,
  `${total:,}`, `${ratio:.1%}`, `${name:>8}`, `${n:05}` — and the matching
  `format(value, spec)` function (align, width, zero padding, thousands,
  decimals, percent).

**Removed.** The `gtq` liquid-glass widgets (`gtq.glass`, `gtq.glass_round`,
`gtq.glass_circle`, `gtq.glass_button`). The dashboard demo uses `gtq.panel`.

## 0.1.5

- Run-time errors inside functions print a **call stack**, with each frame's
  parameters and their values (`in inner(x=0) at line 2`).
- `gw --trace` prints each executed source line, indented by call depth.
- `gw --symbols` lists a file's definitions; the VS Code extension uses it for
  the **outline**, **go to definition** and completion of your own names.
- Tutorial "Learn Globw in 10 minutes"; benchmark methodology in
  `bench/README.md`.

## 0.1.4

- Interactive **REPL** (`gw --repl`), with multi-line blocks and definitions.
- **Formatter** (`gw fmt`) and Format Document in the editor.
- Editor tooling: built-in catalogue (`gw --builtins`), completion, hover,
  structured diagnostics (`gw --check --json`).
- **FFI** to C libraries (`ffi` module), with an optional fully general
  `ffi-full` build.
- `nungw`: resident GPU backend, eigenvectors and thin SVD, row slices.
