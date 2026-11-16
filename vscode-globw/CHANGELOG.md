# Changelog

## 1.48.0

- **Debugger** (needs gw 0.1.6): breakpoints, step over / into / out, pause, the
  call stack and the variables of each frame (lists, dictionaries and objects
  expand), hover to see a variable's value, and a stop on any uncaught run-time
  error. Start it with the debug button in the editor title bar, or from
  *Run and Debug* (a `launch.json` can set permissions and the `input()` text).
- F5 keeps running the program, and inside a debug session it continues.
- Highlighting for `gpu3d` and `gpu3d_active` (3D on the graphics card, gw 0.1.6).

## 1.47.0

- **Outline** of a file's functions, classes, methods and fields (from
  `gw --symbols`), in the breadcrumb bar and the Outline view.
- **Go to Definition** (F12) for functions, procedures, classes and variables.
- **Completion** now also suggests the file's own functions, classes and
  variables, not only the built-ins.

## 1.46.0

- **Format Document** (`Shift+Alt+F`): re-indents a `.gw` file to canonical form
  through `gw fmt`.

## 1.45.0

- **Completion** for every built-in function (from `gw --builtins`) and the
  language keywords.
- **Hover** over a built-in shows its signature.
- Live errors now come through `gw --check --json` (structured diagnostics).

## 1.44.0 and earlier

- Syntax highlighting for `.gw` files.
- **Run** the current program (▶ button / **F5**) in an integrated terminal.
- **Live errors** underlined on open and on save, including errors in imported
  modules.
- A file icon for `.gw` files.
