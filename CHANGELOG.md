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

**Language.**

- **Conditional values**: `if(cond) a else b` is an expression —
  `max = if(a > b) a else b`. Only the chosen side runs (so
  `if(d == 0) 0 else x div d` never divides by zero); an `int` and a `float`
  side give a `float`; chains like `if(x < 0) "neg" else if(x == 0) "zero" else "pos"`.
- **`match` with ranges and comparisons**: `when 90 to 100`, `when < 0`,
  `when >= 18`, mixed with commas (`when < 0, > 100`); ranges work on text too.
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
