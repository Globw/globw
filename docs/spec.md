# Globw — Language and Interpreter Specification

Date: 2026-10-13 (v1.1: explicit types)

## 1. Goal

Globw turns school-style pseudocode into a real, executable language.

- **Readable**: an exercise written the way it is written in class runs as-is.
- **Fast for an interpreted language**: types are declared and checked before running, and programs run on a register bytecode VM with type-specialized instructions.
- **Clear errors**, found as much as possible *before* the program runs.

Source files use the `.gw` extension. The executable is `globw.exe`. Implementation language: Rust.

## 2. Program structure

A program is a sequence of lines, **one statement per line**, in this order:

1. imports (`import ...`, see 6.12), optional;
2. global declarations (`global variables: ...`), optional;
3. main-program declarations (`variables: ...`);
4. `function` / `procedure` definitions and main-program statements, in any order. Main statements run top to bottom. A function can be called before its definition appears in the text.

Comments run from `'` to the end of the line. Blank lines are ignored.

```
global variables:
    counter as int

variables:
    n as int
    grades as int[5]

function double(x as int) as int
    return x * 2
end function

input(n)
output(double(n))
```

## 3. Lexical rules

- **Keywords are lowercase only**: `if`, `else`, `end if`, `while`, `for`, `from`, `to`, `step`, `repeat`, `end loop`, `function`, `end function`, `procedure`, `end procedure`, `return`, `global`, `break`, `continue`, `and`, `or`, `not`, `div`, `mod`, `true`, `false`. `import`, `lazy` (at the start of a line) and `in` (in `for`) are keywords only where they are used that way.
  The same word in another case (`IF`, `If`) is not a keyword. It is treated as a name, and the resulting error suggests the lowercase form.
- **Declarations**: `variables:`, `as`, `of` and the type names `int`, `float`, `string`, `bool` are written in lowercase, exactly like that.
- **Built-in procedures and functions** are lowercase too (`input`, `output`, `length`, `window`, `rect`…). All keywords and built-in names are reserved: they cannot name variables or functions.
- **Identifiers**: letters (accented letters included), digits and `_`. They cannot start with a digit. They are case-sensitive.
- **Literals**: integers `42`, reals `3.14`, strings `"text"` (escapes `\"`, `\\`, `\n`), booleans `true` / `false`.
- **Assignment** is `=`; equality is compared with `==`. A single `=` inside a condition is an error that suggests `==`, and `==` used as a statement is an error that suggests `=`.
- Source files are UTF-8. A BOM and Windows line endings (CRLF) are accepted.

## 4. Variables and types

### 4.1 Declarations

```
global variables:
    total as float

variables:
    price as float
    quantity, discount as int
    name as string
    ok as bool
    grades as int[10]
    grid as float[3, 4]
    names as list of string
    grid2 as list of list of int
    phone as dict of string to string
```

- `variables:` stands alone on its line. Each following line, **indented** more than the line where the block starts (the one with `variables:` or `global variables:`), declares one or more variables (separated by commas) and their type after `as`. The block ends at the first line that is not indented more, or that is not a declaration (an indented `x = 1` is simply the first statement). For indentation a tab counts as moving to the next multiple of 4 columns.
- Every variable used must be declared. A scope may have several `variables:` blocks, all at its top.
- `T[N]` is an array of N elements, where N is an integer literal > 0. An array or matrix has at most 10,000,000 elements. `T[N, M]` is an N×M matrix. `list of T` is a dynamic list, initially empty. `dict of K to V` is a dictionary, initially empty.
- `global variables:` (only at the very start of the program) makes those variables visible everywhere, functions included. The main program's own variables are **not** visible inside functions.
- Inside a function or procedure, a `variables:` block declares local variables. A local variable with the same name as a global one hides it.
- Declaring the same name twice in a scope is an error. So is a main variable with the same name as a global, a variable named like a function, or a variable named like a built-in.
- The old form without types (`variables: a, b` on one line) is an error whose hint shows the new form.

### 4.2 Types

| Type | Values |
|---|---|
| `int` | 64-bit integers |
| `float` | 64-bit floating point numbers |
| `string` | text |
| `bool` | `true` / `false` |

Collections: `T[N]` (array), `T[N, M]` (matrix), `list of T` (list) and `dict of K to V` (dictionary).

- Arrays and matrices contain simple values (one of the four types above).
- The elements of a list and the values of a dictionary can be of any type, collections included: `list of list of int`, `dict of string to list of string`, `dict of int to dict of string to float`… Collections can nest to any depth.
- Dictionary keys (K) are `int` or `string`.

- Every variable has the type it is declared with, for the whole program.
- An `int` value can go where a `float` is expected (assignment, argument, element, return): it is converted. Every other mismatch is an error before the program runs:
  `Error at line 9, column 6: "x" is an int, but here it gets a string`.
- In particular a `float` cannot go into an `int` (the hint suggests `div` for integer division).
- `input` converts the line it reads to the declared type of the variable.

### 4.3 Values before use

- Reading a simple variable that, on some path through the program, might not have a value yet is an **error before the program runs**. This is a definite-assignment analysis, like Java or C#.
- `global` variables can get a value inside any procedure, so for them the check happens **at run time**: reading one that has no value yet stops the program with an error.
- Reading an array or matrix element that was never assigned is a **run-time error**.

## 5. Expressions

Precedence, from highest to lowest:

| Level | Operators |
|---|---|
| 1 | calls `f(...)`, indexing `v[i]`, `m[i, j]`, chained indexing `g[i][j]`, `keys(d)[1]` |
| 2 | unary `-`, `not` |
| 3 | `*`, `/`, `div`, `mod` |
| 4 | `+`, `-` |
| 5 | `==`, `!=`, `<`, `<=`, `>`, `>=` (cannot be chained: `a < b < c` is an error) |
| 6 | `and` |
| 7 | `or` |

- `/` always produces a `float`.
- `div` and `mod` work only on integers. `div` truncates toward zero, and `mod` takes the sign of the first operand (`-7 div 2` is -3, `-7 mod 2` is -1).
- `+` between two strings concatenates them. `string + number/boolean` (or the other way round) turns the other operand into text and concatenates.
- `==` and `!=` compare values of the same type; integers and reals can be compared with each other. `<`, `<=`, `>` and `>=` work on numbers and on strings (alphabetical order).
- `and` and `or` short-circuit.
- `int` overflow and division by zero are run-time errors.
- Parentheses `( )` group.
- A **list literal** `[a, b, c]` builds a new list. All its elements must have the same type (ints and floats mix as floats); `[]` is the empty list and fits any list. Literals nest: `[[1, 2], [3]]` is a list of lists of ints.
- `d[k]` is the value of key `k` in dictionary `d`; a key that is not in the dictionary is a run-time error (`key "Zoe" is not in the dictionary`).

## 6. Statements

### 6.1 Assignment
```
x = 5
grades[i] = 7
grid[i, j] = 0
```
A whole array or matrix cannot be assigned at once. A whole list or dictionary can, by reference, from a value of the same type: `words = split(s, " ")`, `row = [1, 2, 3]`. Characters of a string cannot be changed.

```
grid2[1][2] = 9          ' element 2 of the first inner list
phone["Anna"] = "123"    ' adds the key, or replaces its value
book["Anna"]["mail"] = "a@b.it"
```
Assigning to a dictionary key adds it if it is missing. Collections are shared by reference: after `append(grid2, row)`, changing `row` changes `grid2[length(grid2)]` too.

### 6.2 Conditionals
```
if(cond)
    ...
end if

if(cond)
    ...
else
    ...
end if

if(grade >= 8)
    output("great")
else
if(grade >= 6)
    output("pass")
else
    output("fail")
end if
```
- An `else` followed by `if(...)` on the same line, or on the next line **at the same indentation as the `else` (or less)**, forms an "else if", ignoring blank lines and comments in between. The whole chain is closed by **one** `end if`. An extra `end if` gives an error that explains this rule.
- An `if` on the line after `else` but **more indented** than it is a real nested `if`, the first statement of the else block, with its own `end if`.
- The condition must be `boolean`.

### 6.3 Loops
```
while(cond)                     ' repeats while cond is true
    ...
end loop

for(i from start to end)        ' bounds included, step 1
    ...
end loop

for(i from 10 to 0 step -2)
    ...
end loop

repeat                          ' runs at least once,
    ...                         ' then repeats while cond is true
while(cond)

for(x in collection)            ' each element of a list or array,
    ...                         ' each key of a dictionary,
end loop                        ' each character of a string

break                           ' leaves the innermost loop
continue                        ' goes on with its next round
```
- In `for`:
  - `i` must be a declared `int` variable.
  - `start`, `end` and `step` are integers, evaluated once on entry. `step 0` is a run-time error.
  - With a positive step the loop runs while `i <= end`; with a negative step, while `i >= end`.
  - After the loop, `i` holds the first value that failed the test, as in C. If the loop never ran, it holds `start`.
- In `for(x in c)`:
  - `x` must be a declared variable whose type fits the elements (the keys, for a dictionary; one-character strings, for a string). An `int` element goes into a `float` variable.
  - Lists are visited by position while the position is within their current length, as in Python: elements appended during the loop are visited too. Dictionaries are visited in the order their keys were added, through a copy of the keys taken when the loop starts.
  - If the collection is empty the body never runs, and `x` keeps its previous state.
- `break` and `continue` work in every loop. In a `repeat`, `continue` goes to the `while(cond)` test.
- `end loop` closes `while` and `for`. `repeat` is closed by `while(cond)`.
- Inside a `repeat`, a `while(...)` line **at the same indentation as the `repeat` (or less)** closes it. A **more indented** one opens a normal `while ... end loop` loop inside the `repeat`.

### 6.4 Functions and procedures
```
function area(base as float, height as float) as float
    variables:
        result as float
    result = base * height / 2
    return result
end function

procedure fill(v as int[], value as int)
    ...
end procedure
```
- **Definition**:
  - Functions and procedures are defined only at top level, never nested. Recursion is allowed.
  - Parameters are local variables and do not go in `variables:`. Each parameter has a type: `name as T`, `name as T[]` (an array of any size), `name as T[,]` (a matrix of any size), `name as list of T` or `name as dict of K to V` (nesting included). Collections are passed by reference.
  - A `function` declares its return type after the parentheses: `) as T`, where T is a simple type, a list or a dictionary (`) as list of int`, `) as dict of string to list of int`). Arrays and matrices cannot be returned. A `procedure` has no return type.
  - A parameter can have a **default value**, written directly: `size as int = 10`, `name as string = "?"`, `items as list of int = []`. Parameters with a default come after those without. Every call gets a new copy of a default list.
- **Returning**:
  - A `function` must execute `return value` on every path. This is checked before the program runs.
  - A `procedure` may use a bare `return` to leave early.
- **Calling**:
  - Values can be passed by position or **by name**, as in Python: `area(height = 3, base = 4)`, `greet("Anna", mark = "?")`. Named values come after the positional ones. Parameters left out take their default value. Built-in functions take positional values only.
  - A `procedure` cannot be used inside an expression.
  - A `function` called as a statement on its own is allowed; its value is discarded.
- **Types**:
  - Each argument must match its parameter's type; an `int` argument is accepted for a `float` parameter. For arrays, matrices and lists the element type must be exactly the same.
  - Each `return` value must match the declared return type (an `int` is accepted for `float`).
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
  - Booleans are printed as `true` / `false`.
  - Lists, arrays, matrices and dictionaries are printed as Python does: `[1, 2, 3]`, `['a', 'b']`, `[[1, 2], [3]]`, `{'Anna': 8, 'Luca': 6}`. Strings inside a collection are quoted; an array element with no value yet is printed `?`. `to_text(c)` gives the same text.
- **`input`**:
  - It can also read into an element: `input(grades[i])`.
  - For a `float` it accepts both `3.5` and `3,5`, but not `nan` or infinite values.
  - For a boolean it accepts `true` / `false` in any case.
  - Text that cannot be converted to the required type is a run-time error that says what was expected.

### 6.6 Built-in functions
| Name | Effect |
|---|---|
| `length(x)` | number of elements of an array/list, or of characters of a string (not usable on matrices) |
| `append(list, value)` | adds a value at the end of the list (procedure) |
| `remove(list, position)` | removes the element at that position, counting from 1 (procedure) |

Indices start at **1** for arrays, matrices, lists and strings. An out-of-range index is a run-time error.

### 6.7 Graphics window

A program can open **one** window and draw in it, game-style: each frame is drawn from scratch and then shown.

```
variables:
    x as float
x = 100
window(800, 600, "Demo")
while(open())
    clear("black")
    rect(x, 250, 50, 50, "red")
    if(key("RIGHT"))
        x = x + 4
    end if
    show()
end loop
```

| Command | Kind | Effect |
|---|---|---|
| `window(width as int, height as int, title as string)` | procedure | opens the window (a second call replaces it); the canvas starts black |
| `open()` | function → `bool` | `true` until the user closes the window |
| `show()` | procedure | shows what was drawn, reads keyboard and mouse, and waits so that frames run at about 60 per second |
| `clear(color as string)` | procedure | fills the whole canvas |
| `rect(x, y, width, height as float, color as string)` | procedure | filled rectangle; (x, y) is its top-left corner |
| `circle(x, y, radius as float, color as string)` | procedure | filled circle centered on (x, y) |
| `line(x1, y1, x2, y2 as float, color as string)` | procedure | 1-pixel line |
| `text(x, y as float, text as string, color as string)` | procedure | text in the current font; (x, y) is the top-left corner of the line |
| `font(name as string, size as int)` | procedure | chooses the font for the next `text` calls; works with or without a window (see below) |
| `text_width(text as string)` | function → `int` | width in pixels of the text in the current font |
| `key(name as string)` | function → `bool` | `true` while the key is held: `"A"`…`"Z"`, `"0"`…`"9"`, `"LEFT"`, `"RIGHT"`, `"UP"`, `"DOWN"`, `"SPACE"`, `"ENTER"`, `"ESCAPE"`, `"SHIFT"`, `"CTRL"` |
| `mouse_x()`, `mouse_y()` | function → `int` | mouse position inside the window |
| `click()` | function → `bool` | `true` while the left button is held |

- **Coordinates:** (0, 0) is the top-left corner, x grows to the right and y grows downwards. Coordinates are `float` (an `int` is accepted) and are rounded to pixels; whatever falls outside the canvas is cut off.
- **Colors:** one of the names `"black"`, `"white"`, `"red"`, `"green"`, `"blue"`, `"yellow"`, `"orange"`, `"purple"`, `"pink"`, `"cyan"`, `"gray"`, `"brown"`, `"lime"`, `"navy"`, `"teal"`, `"magenta"`, `"gold"`, `"silver"`, `"maroon"`, `"olive"`, `"darkgray"`, `"lightgray"` (case-insensitive), or `"#rrggbb"`.
- **Run-time errors:**
  - an unknown color or key name;
  - a drawing command, `show`, `key`, `mouse_x`, `mouse_y` or `click` used before `window` (the hint says to call `window(...)` first);
  - a non-positive window size.
  `open()` before `window` returns `false`.
- **Fonts:** `size` is the height in pixels, from 1 to 500. The `name` of `font` can be:
  - the family name of a font installed in Windows, in any case, like `"Arial"`, `"Segoe UI"` or `"Comic Sans MS"`;
  - a font file ending in `.ttf`, `.otf` or `.ttc`, like `"fonts/MyFont.ttf"`; a relative path starts from the folder of the program;
  - `"pixel"`: the built-in 8×8 bitmap font, drawn at a multiple of 8 pixels.

  Vector fonts are drawn anti-aliased. Before any `font` call the font is `"Segoe UI"` at 20 pixels (or `"pixel"` at 16 if Segoe UI is missing). An unknown font or an unreadable file is a run-time error.
- **End of the program:** when the program ends while the window is still open, the last frame stays on screen until the user closes the window.

### 6.8 Dialog boxes

| Command | Kind | Effect |
|---|---|---|
| `message(text as string)` | procedure | Windows message box with an OK button; waits for it |
| `confirm(question as string)` | function → `bool` | Yes/No box: `true` for Yes |
| `ask(question as string)` | function → `string` | input box; returns the text typed, or `""` if cancelled |

### 6.9 Other built-in functions

| Command | Kind | Effect |
|---|---|---|
| `rgb(r, g, b as int)` | function → `string` | the color `"#rrggbb"`; each value is clamped to 0…255 |
| `random(min, max as int)` | function → `int` | a random integer from min to max included (run-time error if min > max) |
| `time()` | function → `float` | seconds since the program started |
| `sin(degrees as float)`, `cos(degrees as float)` | function → `float` | sine and cosine of an angle in degrees |
| `sqrt(x as float)` | function → `float` | square root (run-time error if x < 0) |
| `round(x as float)` | function → `int` | nearest integer, halves away from zero (`round(2.5)` is 3, `round(-2.5)` is -3) |

Built-in procedures can only be used as statements, and built-in functions only inside expressions. Arguments follow the same rules as user functions (§6.4). All these names are reserved: they cannot be used for variables or user functions.

### 6.10 3D graphics

The graphics window can also show simple 3D scenes. The 3D commands draw into the same window as the 2D ones and need `window(...)` first.

```
variables:
    angle as float
window(800, 600, "3D")
camera(0, 2, -6, 0, -15)
while(open())
    clear("#101020")
    rotate(0, angle, 0)
    cube(0, 0, 0, 2, "orange")
    rotate(0, 0, 0)
    sphere(3, 0, 1, 0.8, "cyan")
    box(0, -1.5, 0, 20, 0.2, 20, "gray")
    angle = angle + 1
    show()
end loop
```

- **Space:** x grows to the right, y upwards and z forwards (away from the viewer). Units are arbitrary. All 3D arguments are `float` (an `int` is accepted).
- **Camera:** `camera(x, y, z, turn, tilt)` places the eye at (x, y, z).
  - `turn` is the horizontal direction in degrees: 0 looks towards +z, 90 towards +x.
  - `tilt` is the vertical angle in degrees: positive looks up.
  - Before any `camera` call the eye is at (0, 0, 0) looking towards +z.
  - The field of view is 60 degrees vertically. Anything closer than 0.1 to the eye, or behind it, is cut away.
- **Rotation:** `rotate(ax, ay, az)` sets the rotation, in degrees around the x, then y, then z axes, applied to every following `cube`, `box` and `sphere` around its own center. `rotate(0, 0, 0)` turns it off. It does not affect `triangle3d`.
- **Shapes:** each is filled with one color and shaded by a fixed light.

| Command | Effect |
|---|---|
| `cube(x, y, z, size, color)` | cube centered on (x, y, z) |
| `box(x, y, z, width, height, depth, color)` | box centered on (x, y, z); width along x, height along y, depth along z |
| `sphere(x, y, z, radius, color)` | sphere |
| `triangle3d(x1, y1, z1, x2, y2, z2, x3, y3, z3, color)` | one triangle, visible from both sides |

- **Depth:**
  - Every pixel remembers how far its 3D surface is, so nearer surfaces hide farther ones whatever the drawing order.
  - `clear` resets this memory.
  - 2D commands draw on top of whatever is already in the window and do not hide later 3D shapes.
- **Errors:** the same as for the 2D commands (§6.7). The camera and the rotation are reset when `window` opens a new window.

### 6.11 Standard library

Python-like functions, named in plain English in Globw's style. "Number" means `int` or `float`; "collection" means an array or a list (not a matrix). Positions count from 1. A collection argument is changed in place by the procedures that modify it.

**Conversions**

| Command | Result | Effect |
|---|---|---|
| `to_int(x)` | `int` | x is a string (`"42"`, spaces ignored), a float (truncated toward zero), an int or a bool (1/0); a string that is not a whole number is a run-time error |
| `to_float(x)` | `float` | x is a string (`"3.5"` or `"3,5"`) or a number |
| `to_text(x)` | `string` | any simple value, written as `output` writes it |
| `char(code as int)` | `string` | the character with that Unicode code (`char(65)` is `"A"`) |
| `code(c as string)` | `int` | the Unicode code of the first character |
| `to_hex(n as int)`, `to_binary(n as int)` | `string` | `to_hex(255)` is `"ff"`, `to_binary(5)` is `"101"` |

**Math** (angles in degrees, as for `sin` and `cos`)

| Command | Result | Effect |
|---|---|---|
| `absolute(x)` | same as x | absolute value of a number |
| `minimum(a, b)`, `maximum(a, b)` | int if both are ints, else float | the smaller / larger of two numbers |
| `minimum(c)`, `maximum(c)` | element type | smallest / largest element of a collection of numbers or strings (run-time error if empty) |
| `power(base, exponent)` | `float` | base raised to exponent |
| `floor(x)`, `ceiling(x)`, `truncate(x)` | `int` | round down, up, toward zero |
| `round_to(x, digits as int)` | `float` | x rounded to that many decimal digits |
| `exponential(x)` | `float` | e to the power x |
| `logarithm(x)`, `logarithm(x, base)` | `float` | natural logarithm, or in the given base |
| `tan(a)`, `arcsin(x)`, `arccos(x)`, `arctan(x)`, `arctan2(y, x)` | `float` | tangent and inverse functions |
| `pi()` | `float` | 3.14159… |
| `random_float()` | `float` | a random number from 0 (included) to 1 (excluded) |

**Strings**

| Command | Result | Effect |
|---|---|---|
| `uppercase(s)`, `lowercase(s)`, `trim(s)` | `string` | change case; remove spaces at both ends |
| `find(s, part)` | `int` | position of the first `part` in `s`, 0 if absent |
| `count(s, part)` | `int` | how many times `part` appears (not overlapping) |
| `replace(s, old, new)` | `string` | every `old` replaced by `new` |
| `split(s, separator)` | `list of string` | the pieces between separators; an empty separator splits into characters |
| `join(c, separator)` | `string` | the elements of a collection, as text, separated by `separator` |
| `starts_with(s, part)`, `ends_with(s, part)` | `bool` | |
| `substring(s, from, to)` | `string` | characters `from` to `to` included; positions outside the string are cut to it |
| `contains(s, part)` | `bool` | `part` appears in `s` |

**Collections**

| Command | Result | Effect |
|---|---|---|
| `sum(c)` | element type | sum of a collection of numbers |
| `contains(c, value)` | `bool` | the value is an element |
| `index_of(c, value)` | `int` | position of the first element equal to value, 0 if absent |
| `sort(c)` | procedure | sorts in increasing order (numbers, strings or bools) |
| `reverse(c)` | procedure | reverses the order |
| `insert(list, position, value)` | procedure | inserts before `position` (1 … length + 1) |
| `take_last(list)` | element type | removes and returns the last element (run-time error if empty) |
| `clear_list(list)` | procedure | removes every element |

`sum`, `contains`, `index_of`, `sort`, `join`, `minimum` and `maximum` work on collections of simple values; `length`, `append`, `remove`, `reverse`, `insert`, `take_last` and `clear_list` work on any list, nested ones included.

**Dictionaries**

| Command | Result | Effect |
|---|---|---|
| `length(d)` | `int` | number of keys |
| `has_key(d, key)` | `bool` | the key is in the dictionary |
| `get(d, key, default)` | value type | the value of the key, or `default` if it is missing |
| `remove_key(d, key)` | procedure | removes the key (run-time error if it is missing) |
| `keys(d)` | `list of K` | the keys, in the order they were first added |
| `values(d)` | `list of V` | the values, in the same order |

`input` cannot read into a dictionary or a list of lists: read into a variable, then assign it.

A list returned by a function (like `split` or `read_lines`) can be assigned to a list variable of the same type: `words = split(line_text, " ")`. Lists are assigned by reference.

**Files** (relative paths start from the folder of the program; text is UTF-8)

| Command | Result | Effect |
|---|---|---|
| `read_file(path)` | `string` | the whole file |
| `read_lines(path)` | `list of string` | one element per line, without line endings |
| `write_file(path, text)` | procedure | creates or replaces the file |
| `append_file(path, text)` | procedure | adds at the end, creating the file if needed |
| `file_exists(path)` | `bool` | |

A file that cannot be read or written is a run-time error that names it.

**System**

| Command | Result | Effect |
|---|---|---|
| `wait(milliseconds as int)` | procedure | pauses the program |
| `today()` | `string` | today's date, like `"2026-10-14"` |
| `now()` | `string` | the time, like `"16:05:09"` |
| `stop()` | procedure | ends the program at once (the window, if any, closes) |

All these names are reserved like the other built-ins. Python features that need types Globw does not have (sets, tuples), functions as values (`map`, `filter`, `lambda`) or classes are not part of Globw.

### 6.12 Modules and imports

Any `.gw` file can be imported by another one as a **module**. Imports go at the very start of the file:

```
import shapes                     ' shapes.gw, in the folder of the program
import statistics as st           ' with another name
from tools import mean, middle as mid
lazy import charts                ' loaded only when first used
lazy from sounds import beep

output(shapes.area(3, 4), " ", st.median(v), " ", mean(v))
shapes.calls = 0
```

- `import m` makes the functions, procedures and `global variables` of `m.gw` available as `m.name`. `import m as x` uses `x.name` instead. `from m import a, b as c` makes them available without the prefix. Several modules can go on one line: `import a, b as c`.
- A module is looked for next to the file that imports it, then in the folder of the main program. Module names cannot contain dots.
- A module is a normal program: its statements are its **start-up code**, which runs **once**, the first time the module is loaded. Its `variables:` (not global) are visible only to that start-up code; its `global variables` are visible to its own functions and, with the prefix, to the files that import it.
- A normal `import` loads the module where the import is written, in order. A **lazy import** (as in Python 3.15) loads it just before the first statement that uses it; a lazy module that is never used never runs. Whatever the kind of import, every file and module is **checked before the program starts**: an error in a lazy module is reported at once, not when it is first used.
- Circular imports are allowed, as in Python: a module that is already loading is not loaded again, so it may be used before its start-up code has finished.
- A module does not see the names of the program that imports it: pass values as parameters.
- Errors in a module name its file: `Error in shapes.gw at line 4, column 12: ...`.
- Errors: a module that does not exist, a name the module does not define, `m.name` without `import m`, a variable or function named like an imported module or name, an import after other statements, a module importing itself or the main program.

### 6.13 Classes

A class groups data (fields) and the operations on it (methods). Objects are made with `new`.

```
class Point
    variables:
        x, y as float

    procedure new(x as float, y as float)      ' the constructor
        self.x = x
        self.y = y
    end procedure

    function distance(other as Point) as float
        return sqrt((self.x - other.x) * (self.x - other.x) + (self.y - other.y) * (self.y - other.y))
    end function

    procedure move(dx as float, dy as float)
        self.x = self.x + dx
        self.y = self.y + dy
    end procedure
end class

variables:
    p as Point
    line as list of Point

p = new Point(3, 4)
output(p.x, " ", p.distance(new Point(0, 0)))
p.move(1, -1)
append(line, new Point(0, 0))
output(line[1].x)
```

- **Definition:** `class Name ... end class`, at the top level like functions, after the main `variables:` block. Fields go in one or more `variables:` blocks; methods are `function`/`procedure` definitions. A class can be used before it is defined. (For now classes are defined only in the main program, not in imported modules.)
- **Type:** a class name is a type, so a variable, parameter, field or return value can be an object: `p as Point`, `) as Point`, `list of Point`, `dict of string to Point`. Fields cannot be fixed arrays or matrices (use a list).
- **`self`** is the current object inside a method; `self.field` and `self.method(...)` reach its own fields and methods. `self` is reserved.
- **Constructor:** the method `new`, a procedure, run by `new Class(args)`. Without a constructor, `new Class()` takes no values. Every object starts with its number fields at 0, bool fields `false`, string fields `""`, list and dictionary fields empty, and object fields with no value yet.
- **Objects are references,** like lists and dictionaries: `b = a` makes `b` another name for the same object, and passing an object to a function lets it change the object. They can go in lists, dictionaries and other objects.
- **Fields and methods:** `object.field` reads or assigns a field; `object.method(args)` calls a method. These work on any object expression, like `line[i].x` or `line[i].move(1, 0)`.
- **Printing:** `output(p)` prints an object as `Point(x=3.0, y=4.0)`.
- **Errors:** an unknown class, a field or method a class does not have, the wrong number of constructor or method values, assigning the wrong type to a field, a field or variable named `self`, a duplicate class, field or method, or a constructor written as a function.

## 7. Errors

There is one format for all errors, with line, column and a marker under the offending code:
```
Error at line 7, column 5: variable "total" is not declared
    total = 0
    ^^^^^
Hint: add it to "variables:" at the top of the program
```
- **Before running**: syntax, names, types, value before use, missing `return`, and programs nested more than 100 levels deep or with a statement of more than 1000 operations.
  - The lexer and the parser stop at the first error.
  - The checker reports every error it finds, up to 20, sorted by position.
  - The program does not start.
- **While running**: index out of range, division by zero, overflow, invalid input, unassigned element or global, `step 0`, runaway recursion (more than 100,000 nested calls).
  - The program stops with the same format, pointing at the code being executed.
- **Exit codes**: 0 = ok, 1 = error before running, 2 = error while running, 3 = usage error (missing file etc.).

## 8. Architecture

```
source.gw → Lexer → Parser → Checker → Compiler → VM
            tokens    AST    typed IR   bytecode
```

The project is a Cargo project at the repository root, with one module per phase:

| Module | Job | Depends on |
|---|---|---|
| `errors.rs` | `Pos`, `Error` (position, message, hint) and rendering | — |
| `lexer.rs` | text → tokens with line/column | errors |
| `ast.rs` | syntax tree | errors |
| `parser.rs` | tokens → AST; block matching (`end if`, `end loop`…) | lexer, ast, errors |
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
globw program.gw           run
globw --check program.gw   only check for errors, do not run
```

## 9. Tests

- **Unit tests** for every module.
- **Examples**: the `examples/` folder holds pairs of `name.gw` + `name.expected` (expected output) and, where needed, `name.input`. One test runs them all and compares the output. Examples that *must* fail are included too, with their expected error text.
- **Benchmarks**: recursive Fibonacci (fib 30), sieve of Eratosthenes (up to 10 million) and a sum loop (100 million iterations), compared with the same programs in Python.
  **Target**: at least 5 times faster than CPython on all three.

## 10. Out of scope for v1

Packages (modules in subfolders), records/structs, sets, sound, more than one window, editor syntax highlighting, interactive REPL, JIT compilation.
