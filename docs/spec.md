# Easier — Language and Interpreter Specification (v1)

Date: 2026-10-13 (v1.1: explicit types)

## 1. Goal

Easier turns school-style pseudocode into a real, executable language.

- **Readable**: an exercise written the way it is written in class runs as-is.
- **Fast for an interpreted language**: types are declared and checked before running, and programs run on a register bytecode VM with type-specialized instructions.
- **Clear errors**, found as much as possible *before* the program runs.

Source files use the `.ez` extension. The executable is `easier.exe`. Implementation language: Rust.

## 2. Program structure

A program is a sequence of lines, **one statement per line**, in this order:

1. global declarations (`GLOBAL variables: ...`), optional;
2. main-program declarations (`variables: ...`);
3. `FUNCTION` / `PROCEDURE` definitions and main-program statements, in any order. Main statements run top to bottom. A function can be called before its definition appears in the text.

Comments run from `'` to the end of the line. Blank lines are ignored.

```
GLOBAL variables:
    counter as int

variables:
    n as int
    grades as int[5]

FUNCTION double(x as int) as int
    RETURN x * 2
END FUNCTION

input(n)
output(double(n))
```

## 3. Lexical rules

- **Keywords are UPPERCASE only**: `IF`, `ELSE`, `END IF`, `WHILE`, `FOR`, `FROM`, `TO`, `STEP`, `REPEAT`, `END LOOP`, `FUNCTION`, `END FUNCTION`, `PROCEDURE`, `END PROCEDURE`, `RETURN`, `GLOBAL`, `AND`, `OR`, `NOT`, `DIV`, `MOD`, `TRUE`, `FALSE`.
  The same word in another case (`if`, `If`) is not a keyword. It is treated as a name, and the resulting error suggests the uppercase form.
- **Declarations**: `variables:`, `as`, `of` and the type names `int`, `float`, `string`, `bool` are written in lowercase, exactly like that.
- **Built-in functions**: `input` and `output` are lowercase. `LENGTH`, `APPEND` and `REMOVE` are uppercase.
- **Identifiers**: letters (accented letters included), digits and `_`. They cannot start with a digit. They are case-sensitive.
- **Literals**: integers `42`, reals `3.14`, strings `"text"` (escapes `\"`, `\\`, `\n`), booleans `TRUE` / `FALSE`.
- **Assignment**: `<-` or `←`.
- Source files are UTF-8. A BOM and Windows line endings (CRLF) are accepted.

## 4. Variables and types

### 4.1 Declarations

```
GLOBAL variables:
    total as float

variables:
    price as float
    quantity, discount as int
    name as string
    ok as bool
    grades as int[10]
    grid as float[3, 4]
    names as list of string
```

- `variables:` stands alone on its line. Each following line, **indented** more than the line where the block starts (the one with `variables:` or `GLOBAL variables:`), declares one or more variables (separated by commas) and their type after `as`. The block ends at the first line that is not indented more.
- Every variable used must be declared. A scope may have several `variables:` blocks, all at its top.
- `T[N]` is an array of N elements, where N is an integer literal > 0. `T[N, M]` is an N×M matrix. `list of T` is a dynamic list, initially empty.
- `GLOBAL variables:` (only at the very start of the program) makes those variables visible everywhere, functions included. The main program's own variables are **not** visible inside functions.
- Inside a function or procedure, a `variables:` block declares local variables. A local variable with the same name as a global one hides it.
- Declaring the same name twice in a scope is an error. So is a main variable with the same name as a global, a variable named like a function, or a variable named like a built-in.
- The old form without types (`variables: a, b` on one line) is an error whose hint shows the new form.

### 4.2 Types

| Type | Values |
|---|---|
| `int` | 64-bit integers |
| `float` | 64-bit floating point numbers |
| `string` | text |
| `bool` | `TRUE` / `FALSE` |

Collections: `T[N]` (array), `T[N, M]` (matrix), `list of T` (list), where T is one of the four types above.

- Every variable has the type it is declared with, for the whole program.
- An `int` value can go where a `float` is expected (assignment, argument, element, return): it is converted. Every other mismatch is an error before the program runs:
  `Error at line 9, column 6: "x" is an int, but here it gets a string`.
- In particular a `float` cannot go into an `int` (the hint suggests `DIV` for integer division).
- `input` converts the line it reads to the declared type of the variable.

### 4.3 Values before use

- Reading a simple variable that, on some path through the program, might not have a value yet is an **error before the program runs**. This is a definite-assignment analysis, like Java or C#.
- `GLOBAL` variables can get a value inside any procedure, so for them the check happens **at run time**: reading one that has no value yet stops the program with an error.
- Reading an array or matrix element that was never assigned is a **run-time error**.

## 5. Expressions

Precedence, from highest to lowest:

| Level | Operators |
|---|---|
| 1 | calls `f(...)`, indexing `v[i]`, `m[i, j]` |
| 2 | unary `-`, `NOT` |
| 3 | `*`, `/`, `DIV`, `MOD` |
| 4 | `+`, `-` |
| 5 | `=`, `!=`, `<`, `<=`, `>`, `>=` (cannot be chained: `a < b < c` is an error) |
| 6 | `AND` |
| 7 | `OR` |

- `/` always produces a `float`.
- `DIV` and `MOD` work only on integers. `DIV` truncates toward zero, and `MOD` takes the sign of the first operand (`-7 DIV 2 = -3`, `-7 MOD 2 = -1`).
- `+` between two strings concatenates them. `string + number/boolean` (or the other way round) turns the other operand into text and concatenates.
- `=` and `!=` compare values of the same type; integers and reals can be compared with each other. `<`, `<=`, `>` and `>=` work on numbers and on strings (alphabetical order).
- `AND` and `OR` short-circuit.
- `int` overflow and division by zero are run-time errors.
- Parentheses `( )` group.
- **The arrow inside expressions**: `<-` is always an assignment, except inside an expression, where `x<-1` means `x < -1`.

## 6. Statements

### 6.1 Assignment
```
x <- 5
grades[i] <- 7
grid[i, j] <- 0
```
A whole array, matrix or list cannot be assigned at once. Characters of a string cannot be changed.

### 6.2 Conditionals
```
IF(cond)
    ...
END IF

IF(cond)
    ...
ELSE
    ...
END IF

IF(grade >= 8)
    output("great")
ELSE
IF(grade >= 6)
    output("pass")
ELSE
    output("fail")
END IF
```
- An `ELSE` followed directly by `IF(...)`, on the same line or on the next one, **always** forms an "else if", ignoring blank lines and comments in between. The whole chain is closed by **one** `END IF`. An extra `END IF` gives an error that explains this rule.
- To get a real nested `IF` inside an `ELSE`, it must not be the first statement of the block.
- The condition must be `boolean`.

### 6.3 Loops
```
WHILE(cond)                     ' repeats while cond is TRUE
    ...
END LOOP

FOR(i FROM start TO end)        ' bounds included, step 1
    ...
END LOOP

FOR(i FROM 10 TO 0 STEP -2)
    ...
END LOOP

REPEAT                          ' runs at least once,
    ...                         ' then repeats while cond is TRUE
WHILE(cond)
```
- In `FOR`:
  - `i` must be a declared `int` variable.
  - `start`, `end` and `STEP` are integers, evaluated once on entry. `STEP 0` is a run-time error.
  - With a positive step the loop runs while `i <= end`; with a negative step, while `i >= end`.
  - After the loop, `i` holds the first value that failed the test, as in C. If the loop never ran, it holds `start`.
- `END LOOP` closes `WHILE` and `FOR`. `REPEAT` is closed by `WHILE(cond)`.
- Inside a `REPEAT`, a `WHILE(...)` line **at the same indentation as the `REPEAT` (or less)** closes it. A **more indented** one opens a normal `WHILE ... END LOOP` loop inside the `REPEAT`.

### 6.4 Functions and procedures
```
FUNCTION area(base as float, height as float) as float
    variables:
        result as float
    result <- base * height / 2
    RETURN result
END FUNCTION

PROCEDURE fill(v as int[], value as int)
    ...
END PROCEDURE
```
- **Definition**:
  - Functions and procedures are defined only at top level, never nested. Recursion is allowed.
  - Parameters are local variables and do not go in `variables:`. Each parameter has a type: `name as T`, `name as T[]` (an array of any size), `name as T[,]` (a matrix of any size) or `name as list of T`.
  - A `FUNCTION` declares its return type after the parentheses: `) as T`, where T is one of the four simple types. A `PROCEDURE` has none.
- **Returning**:
  - A `FUNCTION` must execute `RETURN value` on every path. This is checked before the program runs.
  - A `PROCEDURE` may use a bare `RETURN` to leave early.
- **Calling**:
  - A `PROCEDURE` cannot be used inside an expression.
  - A `FUNCTION` called as a statement on its own is allowed; its value is discarded.
- **Types**:
  - Each argument must match its parameter's type; an `int` argument is accepted for a `float` parameter. For arrays, matrices and lists the element type must be exactly the same.
  - Each `RETURN` value must match the declared return type (an `int` is accepted for `float`).
- **Argument passing**: simple values are passed by copy. Arrays, matrices and lists are passed by reference, so changes are visible to the caller.

### 6.5 Input and output
```
input(a)          ' reads one line and converts it to the type of a
input(a, b)       ' reads one line per variable
output("Sum: ", s)
```
- **`output`**:
  - Writes its arguments one after another, without separators, then a newline.
  - Reals are printed in the shortest form that represents them, always with a decimal point (`2.5`, `3.0`).
  - Booleans are printed as `TRUE` / `FALSE`.
- **`input`**:
  - It can also read into an element: `input(grades[i])`.
  - For a `float` it accepts both `3.5` and `3,5`.
  - For a boolean it accepts `TRUE` / `FALSE` in any case.
  - Text that cannot be converted to the required type is a run-time error that says what was expected.

### 6.6 Built-in functions
| Name | Effect |
|---|---|
| `LENGTH(x)` | number of elements of an array/list, or of characters of a string (not usable on matrices) |
| `APPEND(list, value)` | adds a value at the end of the list (procedure) |
| `REMOVE(list, position)` | removes the element at that position, counting from 1 (procedure) |

Indices start at **1** for arrays, matrices, lists and strings. An out-of-range index is a run-time error.

### 6.7 Graphics window

A program can open **one** window and draw in it, game-style: each frame is drawn from scratch and then shown.

```
variables:
    x as float
x <- 100
WINDOW(800, 600, "Demo")
WHILE(OPEN())
    CLEAR("black")
    RECT(x, 250, 50, 50, "red")
    IF(KEY("RIGHT"))
        x <- x + 4
    END IF
    SHOW()
END LOOP
```

| Command | Kind | Effect |
|---|---|---|
| `WINDOW(width as int, height as int, title as string)` | procedure | opens the window (a second call replaces it); the canvas starts black |
| `OPEN()` | function → `bool` | `TRUE` until the user closes the window |
| `SHOW()` | procedure | shows what was drawn, reads keyboard and mouse, and waits so that frames run at about 60 per second |
| `CLEAR(color as string)` | procedure | fills the whole canvas |
| `RECT(x, y, width, height as float, color as string)` | procedure | filled rectangle; (x, y) is its top-left corner |
| `CIRCLE(x, y, radius as float, color as string)` | procedure | filled circle centered on (x, y) |
| `LINE(x1, y1, x2, y2 as float, color as string)` | procedure | 1-pixel line |
| `TEXT(x, y as float, text as string, color as string)` | procedure | text with a built-in 8×8 font drawn at double size (16 pixels per character); accented letters included |
| `KEY(name as string)` | function → `bool` | `TRUE` while the key is held: `"A"`…`"Z"`, `"0"`…`"9"`, `"LEFT"`, `"RIGHT"`, `"UP"`, `"DOWN"`, `"SPACE"`, `"ENTER"`, `"ESCAPE"`, `"SHIFT"`, `"CTRL"` |
| `MOUSE_X()`, `MOUSE_Y()` | function → `int` | mouse position inside the window |
| `CLICK()` | function → `bool` | `TRUE` while the left button is held |

- **Coordinates:** (0, 0) is the top-left corner, x grows to the right and y grows downwards. Coordinates are `float` (an `int` is accepted) and are rounded to pixels; whatever falls outside the canvas is cut off.
- **Colors:** one of the names `"black"`, `"white"`, `"red"`, `"green"`, `"blue"`, `"yellow"`, `"orange"`, `"purple"`, `"pink"`, `"cyan"`, `"gray"`, `"brown"` (case-insensitive), or `"#rrggbb"`.
- **Run-time errors:**
  - an unknown color or key name;
  - a drawing command, `SHOW`, `KEY`, `MOUSE_X`, `MOUSE_Y` or `CLICK` used before `WINDOW` (the hint says to call `WINDOW(...)` first);
  - a non-positive window size.
  `OPEN()` before `WINDOW` returns `FALSE`.
- **End of the program:** when the program ends while the window is still open, the last frame stays on screen until the user closes the window.

### 6.8 Dialog boxes

| Command | Kind | Effect |
|---|---|---|
| `MESSAGE(text as string)` | procedure | Windows message box with an OK button; waits for it |
| `CONFIRM(question as string)` | function → `bool` | Yes/No box: `TRUE` for Yes |
| `ASK(question as string)` | function → `string` | input box; returns the text typed, or `""` if cancelled |

### 6.9 Other built-in functions

| Command | Kind | Effect |
|---|---|---|
| `RGB(r, g, b as int)` | function → `string` | the color `"#rrggbb"`; each value is clamped to 0…255 |
| `RANDOM(min, max as int)` | function → `int` | a random integer from min to max included (run-time error if min > max) |
| `TIME()` | function → `float` | seconds since the program started |

Built-in procedures can only be used as statements, and built-in functions only inside expressions. Arguments follow the same rules as user functions (§6.4). All these names are reserved: they cannot be used for variables or user functions.

### 6.10 3D graphics

The graphics window can also show simple 3D scenes. The 3D commands draw into the same window as the 2D ones and need `WINDOW(...)` first.

```
variables:
    angle as float
WINDOW(800, 600, "3D")
CAMERA(0, 2, -6, 0, -15)
WHILE(OPEN())
    CLEAR("#101020")
    ROTATE(0, angle, 0)
    CUBE(0, 0, 0, 2, "orange")
    ROTATE(0, 0, 0)
    SPHERE(3, 0, 1, 0.8, "cyan")
    BOX(0, -1.5, 0, 20, 0.2, 20, "gray")
    angle <- angle + 1
    SHOW()
END LOOP
```

- **Space:** x grows to the right, y upwards and z forwards (away from the viewer). Units are arbitrary. All 3D arguments are `float` (an `int` is accepted).
- **Camera:** `CAMERA(x, y, z, turn, tilt)` places the eye at (x, y, z).
  - `turn` is the horizontal direction in degrees: 0 looks towards +z, 90 towards +x.
  - `tilt` is the vertical angle in degrees: positive looks up.
  - Before any `CAMERA` call the eye is at (0, 0, 0) looking towards +z.
  - The field of view is 60 degrees vertically. Anything closer than 0.1 to the eye, or behind it, is cut away.
- **Rotation:** `ROTATE(ax, ay, az)` sets the rotation, in degrees around the x, then y, then z axes, applied to every following `CUBE`, `BOX` and `SPHERE` around its own center. `ROTATE(0, 0, 0)` turns it off. It does not affect `TRIANGLE3D`.
- **Shapes:** each is filled with one color and shaded by a fixed light.

| Command | Effect |
|---|---|
| `CUBE(x, y, z, size, color)` | cube centered on (x, y, z) |
| `BOX(x, y, z, width, height, depth, color)` | box centered on (x, y, z); width along x, height along y, depth along z |
| `SPHERE(x, y, z, radius, color)` | sphere |
| `TRIANGLE3D(x1, y1, z1, x2, y2, z2, x3, y3, z3, color)` | one triangle, visible from both sides |

- **Depth:**
  - Every pixel remembers how far its 3D surface is, so nearer surfaces hide farther ones whatever the drawing order.
  - `CLEAR` resets this memory.
  - 2D commands draw on top of whatever is already in the window and do not hide later 3D shapes.
- **Errors:** the same as for the 2D commands (§6.7). The camera and the rotation are reset when `WINDOW` opens a new window.

## 7. Errors

There is one format for all errors, with line, column and a marker under the offending code:
```
Error at line 7, column 5: variable "total" is not declared
    total <- 0
    ^^^^^
Hint: add it to "variables:" at the top of the program
```
- **Before running**: syntax, names, types, value before use, missing `RETURN`.
  - The lexer and the parser stop at the first error.
  - The checker reports every error it finds, up to 20, sorted by position.
  - The program does not start.
- **While running**: index out of range, division by zero, overflow, invalid input, unassigned element or global, `STEP 0`, runaway recursion (more than 100,000 nested calls).
  - The program stops with the same format, pointing at the code being executed.
- **Exit codes**: 0 = ok, 1 = error before running, 2 = error while running, 3 = usage error (missing file etc.).

## 8. Architecture

```
source.ez → Lexer → Parser → Checker → Compiler → VM
            tokens    AST    typed IR   bytecode
```

The project is a Cargo project at the repository root, with one module per phase:

| Module | Job | Depends on |
|---|---|---|
| `errors.rs` | `Pos`, `Error` (position, message, hint) and rendering | — |
| `lexer.rs` | text → tokens with line/column | errors |
| `ast.rs` | syntax tree | errors |
| `parser.rs` | tokens → AST; block matching (`END IF`, `END LOOP`…) | lexer, ast, errors |
| `tir.rs` | typed IR: types, places, typed expressions/statements | ast, errors |
| `checker/` | names and scopes, type checking, definite assignment, return paths → typed IR | ast, tir, errors |
| `bytecode.rs` | instruction set and module layout | tir, errors |
| `compiler.rs` | typed IR → register bytecode | tir, bytecode |
| `vm.rs` | bytecode execution | bytecode, errors |
| `lib.rs` | `compile` and `run` entry points | all |
| `builtins.rs` | table of built-in procedures/functions (names, parameter and return types) and their run-time dispatch | tir, canvas, graphics, dialogs |
| `canvas.rs` | pixel buffer: colors, shapes, text (no window, testable) | — |
| `graphics.rs` | the window, keyboard and mouse (crate `minifb`) | canvas, render3d |
| `render3d.rs` | 3D camera, shapes, clipping, shading and depth buffer on a canvas | canvas |
| `dialogs.rs` | Windows message, Yes/No and input boxes | — |
| `main.rs` | command line | lib |

**Speed choices**
- **Register-based** bytecode, like Lua 5: fewer instructions than a stack machine.
- **Type-specialized instructions** (`AddI`, `AddR`, `CmpI`…): the types are already known, so the VM never checks them during operations.
- Integers and reals are never heap-allocated. Strings, arrays and lists are shared by reference.
- Globals live in fixed-index slots and are never looked up by name at run time.

**Command line**
```
easier program.ez           run
easier --check program.ez   only check for errors, do not run
```

## 9. Tests

- **Unit tests** for every module.
- **Examples**: the `examples/` folder holds pairs of `name.ez` + `name.expected` (expected output) and, where needed, `name.input`. One test runs them all and compares the output. Examples that *must* fail are included too, with their expected error text.
- **Benchmarks**: recursive Fibonacci (fib 30), sieve of Eratosthenes (up to 10 million) and a sum loop (100 million iterations), compared with the same programs in Python.
  **Target**: at least 5 times faster than CPython on all three.

## 10. Out of scope for v1

File I/O, modules and imports, records/structs, math functions, sound, more than one window, editor syntax highlighting, interactive REPL, JIT compilation.
