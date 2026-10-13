# Easier v1.2: Graphics Window and Dialogs — Implementation Plan

> Executed inline (superpowers:executing-plans), TDD where a test can run without a screen.

**Goal:** Built-in procedures and functions for a game-style drawing window (B) and Windows dialog boxes (A), plus `RGB`, `RANDOM` and `TIME`.

**Spec:** `docs/spec.md` §6.7, §6.8, §6.9.

## Global Constraints
- **New dependencies:** `minifb` (window, keyboard, mouse) and `font8x8` (bitmap font). Nothing else.
- **Builtin names** (uppercase) are reserved: the checker's declaration checks reject them as variable or function names.
- **Errors and hints** are English, exactly as written below.

## Design
- **`builtins.rs`:** the `Builtin` enum and `pub fn lookup(name) -> Option<Builtin>`. Each builtin knows its parameter types (`&[BaseType]`) and its return type (`Option<BaseType>`).
- **Typed IR:** `TExprKind::Builtin(Builtin, Vec<TExpr>)` and `TStmt::Builtin(Builtin, Vec<TExpr>, Pos)`.
- **Checker:**
  - a procedure in an expression → `X does not return a value: use it as a statement on its own line`;
  - a function used as a statement → `X returns a value: use it in an expression, like v <- X(...)`;
  - wrong number of arguments → `"X" takes N value(s), here it gets M`;
  - argument type checks as for user functions, with ints converted to floats.
- **Bytecode:** `Op::Builtin { id: u16, first: Reg, argc: u16, dst: Reg }`.
- **VM:** `builtins::call(b, &[Value], &mut Env) -> Result<Value, String>`. `Env` holds the optional `Graphics`, the start `Instant` and the random generator state (xorshift64, seeded from the clock). At the end of `run`, `Env::finish()` keeps an open window on screen until it is closed.
- **`canvas.rs`:**
  - `Canvas { w, h, pixels: Vec<u32> }` with `clear`, `rect`, `circle`, `line` (Bresenham) and `text` (font8x8 basic + latin, scale 2);
  - `parse_color(&str) -> Result<u32, String>` returns `unknown color "x"` with the hint text listing the color names.
- **`graphics.rs`:** `Graphics { window, canvas, … }` and `key_from_name(&str) -> Option<minifb::Key>`. `SHOW` calls `update_with_buffer` with a target of 60 fps.
- **`dialogs.rs`:** `MESSAGE` and `CONFIRM` call `MessageBoxW` through `extern "system"`. `ASK` runs PowerShell's `Microsoft.VisualBasic.Interaction.InputBox`, with the question passed in an environment variable so that it is never pasted into the command line.

## Tasks
1. **Canvas.** Tests:
   - color parsing: names, case, `#rrggbb`, unknown color;
   - clear;
   - rect clipped at the edges;
   - circle center and edge pixels;
   - line endpoints;
   - text sets pixels, and an accented letter draws something.
2. **Builtins through checker, compiler and VM** (`RGB`, `RANDOM`, `TIME`, the drawing calls with no window). Tests:
   - checker: types, arity, procedure/function misuse, reserved names;
   - end to end: `RGB(255, 128, 0)` → `#ff8000`; `RANDOM(1, 3)` stays in range over 200 draws; `RANDOM(5, 1)` gives a run-time error; `TIME() >= 0`; `RECT(...)` before `WINDOW` gives `there is no window: call WINDOW(...) first`; `OPEN()` before `WINDOW` is `FALSE`.
3. **Window, keyboard and mouse** with minifb. Tests: `key_from_name` mapping and unknown key names. Then a manual run of a demo, checking that the window opens, draws and closes, with the last frame kept at program end.
4. **Dialogs.** Manual run of `MESSAGE`, `CONFIRM` and `ASK`, plus a build check on Windows.
5. **Editor support and examples:**
   - the VS Code grammar colors the new builtins (`support.function.builtin`), with a grammar test;
   - `examples/windows/` gets a drawing demo, which is not part of the automated examples test;
   - a Pong game goes in `Desktop\easier\pong.ez`.
