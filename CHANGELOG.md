# Changelog

## 0.1.7 — 2026-11-27

**A JIT.** `import jit` and the program's hot code becomes machine code
(Cranelift), with the same results and the same errors as the interpreter. A
program without it is interpreted, as before: the JIT is for when speed matters.

- Functions made only of numbers, booleans and calls to such functions become
  native functions that call each other directly.
- Any loop (`for`, `while`, `repeat`) that turns 50 times is compiled, also in the
  main program: numbers, array, list and matrix elements, `length`, `append`,
  global variables and calls to compiled functions run as machine code. When a loop
  meets something it does not handle, it hands its registers back and the
  interpreter goes on from that very instruction.
- An overflow, a division by zero or an index out of range in compiled code gives
  the interpreter's error at the same line, and `try` catches it as before.
- Array, list and matrix elements are read and written in place, as a few
  machine instructions (bounds check, kind check, access); the memory layout of a
  value is measured and checked at start-up, and the helpers take over if it
  ever differs.
- fib(35) 0.53 → 0.09 s, a 200-million-step loop 1.05 → 0.22 s, Mandelbrot
  0.94 → 0.11 s: faster than Java.
- On with `import jit` (in any file of the program); the debugger, `--trace`,
  `--profile` and the web playground use the interpreter. `GLOBW_JIT=1` turns it
  on without the import, `GLOBW_NO_JIT=1` keeps it off.
- Hot functions are compiled whole, the main program too, and can start at any
  instruction: what compiled code cannot run (a call to an interpreted function, a
  return) it hands to the interpreter, which goes back into compiled code right
  after. Text (constants, `+`, comparisons, characters), `output` and calls to the
  library run inside compiled code; an error a library function raises there is
  reported by the interpreter at the same line, without calling it again.
- Compiled functions call each other directly, the interpreter's frames built as
  it would build them: when a callee stops, the interpreter goes on inside it,
  with the same backtrace. Only functions with a loop are compiled whole; a short
  one runs faster in the interpreter. Windows gives `gw` a 64 MB stack.
- **Loops on every core, by themselves.** A `for` loop that adds up integers
  (`total = total + ...`, `count = count + 1`, through any `+` and `-`) and
  otherwise only computes, reads compact arrays and branches, runs in pieces on
  all the cores, with no change to the program. The result is the sequential
  one exactly: every piece tracks its lowest and highest running sums, so an
  overflow the sequential loop would meet is found; any error (overflow, division
  by zero, index out of range) runs the loop again sequentially and reports the
  interpreter's error at the same line. Sums of decimals stay sequential (their
  last digits depend on the order). Mandelbrot 900 x 600: 0.10 -> 0.055 s; a
  200-million-step sum: 0.16 -> 0.060 s. Measured fairly (`bench/README.md`),
  Globw with its JIT is on par with Go: a few percent to a third slower on one
  core, and on every core even with Go parallelised by hand, ahead on the sum.
- Loops that fill compact arrays (`a[i] = ...`, at the counter's own position)
  run on every core too, also together with sums. Only when it changes nothing
  that can be seen: the loop does not read the arrays it writes, no array
  written is one read (checked when it runs), and no `try` is around nor any
  other task, so an error mid-loop ends the program exactly as before.
- Innermost `for` loops that index compact arrays with their counter get a
  second copy without bounds checks, chosen once at the loop head when every
  turn left stays inside the arrays (step 1, counter from 1, end within the
  length): array writes 10-25% faster on one core.
- A compact array whose elements all have a value is marked full (seen when a
  compiled loop starts); compiled code then skips the "has a value" checks. The
  bits stay exact, so every view of the array stays valid.
- Check-free loop copies are taken only when the arrays they read are full,
  and have no exits at all: no bounds checks, no value checks, no overflow
  checks a counter cannot reach.
- `if(c) a = a + k end if` runs without a branch; a comparison or a bool read
  followed by a jump on it branches at once; and / or chains are threaded.
- The JIT keeps registers in machine registers: per-register kinds, constants,
  compact arrays specialised on the kind seen, their views made once at entry,
  and exits that write back only what the interpreter reads.
- `gw --bytecode` prints a program's compiled instructions.

**Language**

- Arrays and matrices are as big as the memory allows (they were limited to 10
  million elements): `flags as bool[5000000000]` is 5 GB. One that does not fit
  is a clean run-time error at its declaration, not a crash.
- `b = a` copies a whole array or matrix into another of the same element type
  and size; changing `b` afterwards leaves `a` as it was.
- No compiler warnings in the native or the web build.

**Faster start.** On Windows the graphics DLLs are loaded only when the GPU is
used: every program starts in about 45 ms instead of 105 ms.

**Compact arrays.** `int[n]`, `float[n]` and `bool[n]` keep one machine value per
element (8 bytes, a bool 1 byte) instead of a 24-byte value, plus one bit saying the
element has a value. Reading and writing them is faster in the interpreter and is a
plain memory access in compiled loops; built-ins see them as lists, and what they
change (a `sort`) is taken back. The sieve up to 10 million: 0.77 → 0.14 s with the
JIT (Java: 0.18 s), 0.48 s in the interpreter alone.

**Large arrays.** Other arrays and matrices are created from memory the system hands
out already zeroed, instead of writing every element.

## 0.1.6 — 2026-11-16

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
- System dialogs: `gtq.open_file`, `gtq.save_file`, `gtq.choose_folder` and
  `gtq.choose_color`, with simple filters like `"Images: *.png, *.jpg"`.
- The light theme now covers every `gtq` widget (the classic ones translate
  their palette), and button labels are centred for any font.
- Automatic layout: `gtq.column` / `gtq.row` stack widgets down a column, and
  `gtq.form_text` / `gtq.form_password` add a labelled field in one line.
- More windows: `gtq.window` opens one and returns its number, `gtq.use`
  switches the drawing to it, `gtq.is_open` / `gtq.close`; `show()` shows them
  all, each with its own focus, menus and theme.
- Demo: `examples/demos/contacts.gw`, a small address book.
- `screenshot("file.png")` saves the graphics window as a PNG.

**3D on the GPU and 3D models.**

- The built-in 3D (`cube`, `box`, `sphere`, `triangle3d`) is drawn on the
  graphics card when a window opens and one answers, by the processor otherwise,
  with the same picture. A scene of 1,600 spheres and 40 full-window walls at
  1600 × 900 runs at 38 frames a second instead of 4–6. `gpu3d(false)` keeps the
  3D on the processor; `gpu3d_active()` tells which one draws.
- The release binaries now include the GPU backend (wgpu): `gw` grows from
  about 8.5 MB to about 13 MB. `nungw` still uses the GPU only after
  `nungw.gpu(true)`.
- New `model` module: `model.load` reads `.glb` / `.gltf` (the whole scene, with
  materials, vertex colors and PNG/JPEG textures), `.obj` (with the colors and
  textures of its `.mtl`) and `.stl`; `model.new` and `model.triangle` build a
  model in the program; `model.draw(m, x, y, z, scale)` draws it anywhere, at
  any size, turned by `rotate`. Also `model.color`, `model.center`,
  `model.triangles`, `model.width`, `model.height`, `model.depth`.
  `model.load` needs `--allow-read`.

**Fixed.** `screenshot` wrote its file without `--allow-write`: like every other
built-in that writes files, it now needs it.

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
