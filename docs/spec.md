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

- **Keywords are lowercase only**: `if`, `else`, `end if`, `while`, `for`, `from`, `to`, `step`, `repeat`, `end loop`, `function`, `end function`, `procedure`, `end procedure`, `return`, `global`, `break`, `continue`, `try`, `catch`, `always`, `fail`, `gives`, `and`, `or`, `not`, `div`, `mod`, `true`, `false`. `import`, `lazy` (at the start of a line) and `in` (in `for`) are keywords only where they are used that way.
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

Collections: `T[N]` (array), `T[N, M]` (matrix), `list of T` (list), `set of T` (set), `dict of K to V` (dictionary) and `tuple of (T1, T2, ...)` (tuple).

- Arrays and matrices contain simple values (one of the four types above).
- The elements of a list and the values of a dictionary can be of any type, collections included: `list of list of int`, `dict of string to list of string`, `dict of int to dict of string to float`… Collections can nest to any depth.
- Dictionary keys (K) are `int` or `string`.

- Every variable has the type it is declared with, for the whole program.
- An `int` value can go where a `float` is expected (assignment, argument, element, return): it is converted. Every other mismatch is an error before the program runs:
  `Error at line 9, column 6: "x" is an int, but here it gets a string`.
- In particular a `float` cannot go into an `int` (the hint suggests `div` for integer division).
- `input` converts the line it reads to the declared type of the variable.

### 4.3 Nothing: `none`

An object or function variable can hold `none`, meaning "no value / absent".

```
variables:
    friend as Person
friend = none
if(friend == none)
    output("no friend")
end if
```

- `none` is written where an object or function value is expected; test it with `== none` and `!= none`.
- An object field of object type starts as `none` until it is set, so a chain of objects (a linked list, a tree) ends at `none`.
- Using `none` as if it were an object (a field or method) is a run-time error: `this object has no value (it is none)`. A `try` can catch it.
- Numbers, booleans, strings, lists and dictionaries are never `none`: they always have a value (lists and dictionaries start empty).

### 4.4 Values before use

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

**Sets** (a `set of int` or `set of string`: unique values, no order kept for lookup but iterated in insertion order)

| Command | Result | Effect |
|---|---|---|
| `add(s, value)` | procedure | adds the value (nothing happens if already there) |
| `discard(s, value)` | procedure | removes the value (nothing happens if absent) |
| `contains(s, value)` | `bool` | the value is in the set |
| `length(s)` | `int` | how many values |
| `to_list(s)` | `list of T` | the values as a list, in insertion order |

`for(x in s)` goes through the values. A set is printed as `{1, 2, 3}`.

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

**Built-in modules.** Some modules are provided by Globw itself, backed by Rust
libraries and reinterpreted with Globw names. They are imported the same way,
with `import <name>`, but there is no `.gw` file to find:

```
import text          ' regular expressions
import hash          ' digests
import web           ' web requests
import data          ' JSON
import clock         ' dates and times
import encode        ' base64
```

- A built-in module is imported **plainly**: `import text` only, not `import text as t` nor `from text import ...`.
- Its functions are used with the module prefix, like any module: `text.matches(s, pattern)`.
- The built-in module names (`text`, `hash`, `web`, `data`, `clock`, `encode`) are not reserved: a file that does not import them can still use those words as its own names.

| Module | Function | Result | What it does |
|---|---|---|---|
| `text` | `text.matches(s, pattern)` | `bool` | whether `s` contains a match of the regular expression `pattern` |
| `text` | `text.find(s, pattern)` | `string` | the first match, or `""` if there is none |
| `text` | `text.all(s, pattern)` | `list of string` | every match, in order |
| `text` | `text.replace_all(s, pattern, with)` | `string` | `s` with every match replaced by `with` |
| `text` | `text.split(s, pattern)` | `list of string` | `s` split on every match of `pattern` |
| `text` | `text.count(s, pattern)` | `int` | how many matches of `pattern` are in `s` |
| `text` | `text.group(s, pattern, n)` | `string` | the n-th capture group of the first match (0 = the whole match), `""` if none |
| `hash` | `hash.sha256(s)` | `string` | the SHA-256 digest of `s`, as lowercase hexadecimal |
| `hash` | `hash.sha512(s)` | `string` | the SHA-512 digest of `s`, as lowercase hexadecimal |
| `web` | `web.get(url)` | `string` | fetches `url` over HTTP(S) and returns the reply body as text |
| `web` | `web.post(url, body)` | `string` | sends `body` with a POST request and returns the reply as text |
| `web` | `web.status(url)` | `int` | the HTTP status code of a GET to `url` (e.g. 200, 404) |
| `web` | `web.download(url, path)` | — | saves the bytes at `url` to the file `path` |
| `data` | `data.text(json, path)` | `string` | the value at `path` as text (a JSON string, number or bool) |
| `data` | `data.number(json, path)` | `float` | the value at `path`, which must be a JSON number |
| `data` | `data.flag(json, path)` | `bool` | the value at `path`, which must be a JSON true/false |
| `data` | `data.has(json, path)` | `bool` | whether `path` exists in the JSON |
| `data` | `data.size(json, path)` | `int` | how many elements the array or object at `path` has |
| `data` | `data.keys(json, path)` | `list of string` | the keys of the object at `path` |
| `data` | `data.to_json(value)` | `string` | the JSON text of a Globw value (number, string, bool, `none`, list, set or dictionary; nested too) |
| `data` | `data.pretty(json)` | `string` | the JSON re-formatted on several indented lines |
| `data` | `data.type(json, path)` | `string` | the kind at `path`: `"number"`, `"text"`, `"bool"`, `"list"`, `"object"` or `"none"` |
| `clock` | `clock.stamp()` | `int` | the current time as a Unix timestamp (seconds since 1970-01-01 UTC) |
| `clock` | `clock.format(stamp, pattern)` | `string` | a Unix timestamp written as **local** time with a date pattern |
| `clock` | `clock.parse(text, pattern)` | `int` | reads a local date-and-time written like `pattern` and returns its Unix timestamp |
| `encode` | `encode.base64(s)` | `string` | the Base64 of the text's bytes |
| `encode` | `encode.from_base64(s)` | `string` | the text decoded from Base64 (error if it is not valid Base64 or not text) |

- A bad regular expression, an unreachable address, a server error or a timeout (30 s for `web.get`) becomes an ordinary run-time error, so it can be handled with `try`/`catch`.
- In a `data` **path**, object members are written `name` or `.name` and array positions `[n]`, **1-based**: `"users[1].name"`. An empty path `""` is the whole document. Asking for the wrong kind of value (e.g. `data.number` on text), a missing path or invalid JSON is a catchable run-time error. `data.to_json` fails on values that have no JSON form (functions, objects, matrices). (For now each `data.*` call parses the JSON again; a pre-parsed handle is planned.)
- A `clock` **pattern** uses the usual date fields: `%Y` year, `%m` month, `%d` day, `%H` hour, `%M` minute, `%S` second (e.g. `"%Y-%m-%d %H:%M:%S"`). Timestamps are plain `int`s, so you can add or subtract seconds to do date arithmetic. An invalid pattern, or a text that does not match it, is a catchable run-time error.

There are many more built-in modules, each imported the same way. Below, one row per module with its functions.

| Module | Functions | Purpose |
|---|---|---|
| `shell` | `run(cmd)` | run a system command, return its output (its failure is a catchable error) |
| `math` | `gcd(a,b)`, `lcm(a,b)`, `factorial(n)`, `is_prime(n)`, `sign(x)`, `hypot(a,b)`, `clamp(x,lo,hi)`, `cbrt(x)` | extra maths |
| `bits` | `and`, `or`, `xor`, `not`, `left(n,by)`, `right(n,by)`, `ones(n)` | bitwise operations on ints |
| `base` | `to(n, base)`, `from(text, base)` | whole numbers in any base from 2 to 36 |
| `roman` | `to(n)`, `from(text)` | Roman numerals (1–3999) |
| `temp` | `c_to_f`, `f_to_c`, `c_to_k`, `k_to_c` | temperature conversions |
| `angle` | `to_radians`, `to_degrees`, `normalize` | angles (degrees) |
| `percent` | `of(p, x)`, `change(from, to)` | percentages |
| `geom` | `circle_area(r)`, `rect_area(w,h)`, `triangle_area(b,h)`, `sphere_volume(r)`, `cube_volume(s)` | areas and volumes |
| `length` | `meters_to_feet`, `feet_to_meters`, `km_to_miles`, `miles_to_km` | length conversions |
| `weight` | `kg_to_lb`, `lb_to_kg` | weight conversions |
| `size` | `human(bytes)` | a byte count as `"1.5 MB"` |
| `duration` | `human(seconds)` | a number of seconds as `"1h 2m 5s"` |
| `geo` | `distance(lat1,lon1,lat2,lon2)` | distance in km between two coordinates |
| `tax` | `add(amount, rate)`, `remove(amount, rate)` | add or remove a percentage tax |
| `compass` | `direction(degrees)` | a bearing as `"N"`, `"NE"`, … |
| `ordinal` | `of(n)` | `1` → `"1st"` |
| `fib` | `at(n)` | the nth Fibonacci number |
| `prime` | `next(n)`, `nth(n)` | the next prime after `n`, or the nth prime |
| `str` | `repeat(s,n)`, `reverse(s)`, `pad_left(s,w,fill)`, `pad_right(s,w,fill)`, `center(s,w,fill)` | more string tools |
| `hex` | `encode(s)`, `decode(s)` | text to/from hexadecimal |
| `url` | `encode(s)`, `decode(s)` | percent-encoding for URLs |
| `case` | `title(s)`, `snake(s)`, `camel(s)`, `kebab(s)` | naming styles |
| `slug` | `make(s)` | a URL-friendly slug |
| `ascii` | `is_digit(s)`, `is_alpha(s)`, `is_space(s)`, `is_upper(s)`, `is_lower(s)` | character-class checks of a whole string |
| `morse` | `encode(s)`, `decode(s)` | Morse code |
| `caesar` | `encode(s, shift)`, `decode(s, shift)` | Caesar cipher |
| `rot13` | `apply(s)` | ROT13 |
| `leet` | `encode(s)` | leetspeak |
| `nato` | `encode(s)` | the NATO phonetic alphabet |
| `color` | `hex(r,g,b)`, `gray(level)`, `invert(color)` | `#rrggbb` colors |
| `valid` | `email(s)`, `integer(s)`, `number(s)` | simple format checks |
| `luhn` | `valid(s)` | the Luhn checksum (card numbers) |
| `isbn` | `valid(s)` | an ISBN-10 or ISBN-13 check digit |
| `uuid` | `make()` | a random UUID (v4) |
| `crc` | `value(s)` | the CRC-32 checksum of the text |
| `encode` | `base64(s)`, `from_base64(s)` | Base64 |
| `env` | `get(name)`, `has(name)`, `set(name, value)` | environment variables |
| `path` | `join(a,b)`, `name(p)`, `stem(p)`, `extension(p)`, `parent(p)`, `is_absolute(p)` | file-path parts (text only) |
| `os` | `name()`, `arch()`, `family()` | the operating system |
| `dir` | `list(path)`, `make(path)`, `current()`, `exists(path)` | folders |
| `file` | `size(p)`, `copy(from,to)`, `rename(from,to)`, `modified(p)`, `delete(p)` | file operations |
| `tmp` | `path()` | the system temporary folder |
| `args` | `count()`, `get(n)` | the arguments the program was started with |
| `csv` | `fields(line)`, `line(list)` | one CSV line to or from a list of strings |
| `stats` | `mean(list)`, `median(list)`, `stdev(list)` | statistics of a list of numbers |

Most of these are plain functions whose errors (a bad number, a missing file, a value of the wrong kind) are catchable with `try`/`catch`. `shell`, `env.set`, `dir.make`, `file.copy`/`rename`/`delete` change the computer or the environment, by design: Globw does not forbid a capability just because it is powerful.

**And many more.** Globw ships a large standard library; the modules below round it out (import each with `import <name>`; the function names follow the same `module.function(...)` shape and read for themselves). Grouped by theme:

- **Unit conversions:** `area`, `volume`, `speed`, `pressure`, `energy`, `power`, `fuel`, `force`, `torque`, `datarate`, `frequency`, `cooking`, `nautical`, `astro`, `stone`, `density`, `db`, `tempo`, `grad`, `timeof`, `pace`.
- **Maths:** `hyper`, `ahyper`, `logs`, `trigx`, `choose`, `seq`, `modular`, `lerp`, `rounding`, `vec2`, `vec3`, `complex`, `parity`, `divisible`, `digits`, `collatz`, `perfect`, `armstrong`, `palindrome`, `range`, `point`, `point3`, `slope`, `root`, `pow`, `isqrt`, `factor`, `totient`, `compare`, `clampint`, `cycle`, `digitroot`, `midpoint`.
- **Text:** `count`, `truncate`, `indent`, `strip`, `mask`, `initials`, `acronym`, `pig`, `atbash`, `vigenere`, `rot47`, `bin`, `base32`, `html`, `shout`, `words`, `unicode`, `normalize`, `anagram`, `fizzbuzz`, `quote`, `rle`, `wrap`, `spongebob`, `clap`, `caps`, `swapcase`, `consonants`.
- **Science & geometry:** `bmi`, `ohm`, `physics`, `wave`, `gravity`, `bearing`, `triangle`, `circle`, `rectangle`, `sphere`, `cylinder`, `cone`, `trapezoid`, `ellipse`, `pentagon`, `hexagon`, `polygon`, `pyramid`, `prism`, `torus`, `cuboid`, `annulus`, `quadratic`, `motion`, `wind`, `heat`, `dewpoint`, `bmr`, `heartrate`.
- **Finance:** `interest`, `loan`, `tip`, `discount`, `roi`, `money`, `savings`, `depreciation`, `cents`, `markup`, `apr`.
- **Dates:** `weekday`, `month`, `leap`, `age`, `zodiac`, `season`.
- **Hashes & checks:** `fnv`, `djb2`, `sdbm`, `adler`, `ipv4`, `mac`, `hexcolor`, `weburl`, `grade`, `logic`, `yesno`, `scientific`, `fraction`.
- **Randomness & ids:** `dice`, `coin`, `pick`, `token`, `password`, `nanoid`, `otp`, `lorem`, `randcolor`.
- **Collections:** `counter` — `counter.items(list)`, `counter.chars(s)`, `counter.words(s)` tally into a dictionary (like Python's `Counter`).
- **Charts:** `chart` — `chart.line(list, path)`, `chart.bars(list, path)`, `chart.scatter(xs, ys, path)` write an SVG plot to a file (like matplotlib's `savefig`). Paths use `/` (a Globw string cannot hold a lone `\`).
- **Tables (CSV, light pandas):** `table` — `table.headers(csv)`, `table.column(csv, name)`, `table.numbers(csv, name)`, `table.rows(csv)`, `table.mean(csv, name)`, `table.sum(csv, name)`. The first CSV line is the header.
- **Iterators (like itertools, over a string's characters):** `iter` — `iter.combinations(s, k)`, `iter.permutations(s, k)`, `iter.product(a, b)` return a `list of string`.
- **Random over lists:** `pick.shuffle(list)` (in place), `pick.choice(list)` (one element), `pick.sample(list, k)` (a new list of k distinct elements).
- **Timestamp maths:** `clock.add(stamp, seconds)`, `clock.diff(a, b)`.

And a further 150 small modules (one function family each), among them: more unit conversions (`acceleration`, `lux`, `radiation`, `turn`, `flow`, `typography`, `oven`, `glucose`); activation and special functions (`sigmoid`, `relu`, `softsign`, `gauss`, `erf`, `gammaf`, `sinc`, `smoothstep`); string tools (`nospaces`, `digitsonly`, `disemvowel`, `screaming`, `dotcase`, `pathcase`, `sentencecase`, `dedup`, `rotstr`, `zfill`, `soundex`, `hamming`, `levenshtein`, `syllables`); physics (`kinematics`, `projectile`, `pendulum`, `spring`, `lens`, `resistors`, `capacitors`, `doppler`, `escape`, `orbit`, `lorentz`, `freefall`, `gforce`); finance (`annuity`, `breakeven`, `profit`, `commission`, `wage`, `inflation`, `rule72`, `splitbill`); health (`idealweight`, `tdee`, `water`, `dosage`); geometry (`tetrahedron`, `octahedron`, `parallelogram`, `rhombus`, `sector`, `ellipsoid`, `ngon`, `frustum`); number theory and sequences (`lucas`, `tribonacci`, `padovan`, `pentagonal`, `hexagonal`, `star`, `pronic`, `polygonal`, `primorial`, `primecount`, `prevprime`, `coprime`, `amicable`, `happy`, `aliquot`, `collatzmax`); bits (`nibble`, `byteops`, `endian`, `nextpow2`, `ispow2`, `align`, `graycode`, `revbits`); and checks/formatting (`approx`, `bankers`, `plural`, `abbreviate`, `gpa`, `zscore`, `ean`, `cardtype`, `a1z26`, `luminance`, `contrast`, `note`, `prob`, `odds`, `ph`, `humidex`, `sound`). Each follows the same `module.function(...)` shape.

**The array engine (`nungw`).** For heavy number crunching, `nungw of float` is a
first-class type: a one-dimensional array of floats kept in contiguous memory and
worked on across every CPU core. `import nungw` to use it. The usual operators
work element-wise, and `g[i]` reads an element (1-based). (The name is Globw's own
take on the idea a tool like NumPy covers.)

```
import nungw
variables:
    a, c as nungw of float
a = nungw.range(10000000)            ' 0, 1, 2, ... as a fast float array
c = nungw.sqrt(a) * 1.5 - a / 3.0    ' operators work element-wise (fused)
output(c[1], " ", nungw.sum(c))      ' index (1-based) and a reduction
```

- **Operators:** `a + b`, `a - b`, `a * b`, `a / b` between two nungws, and between a
  nungw and a number (`a * 2.0`, `100.0 - a`), all element-wise, giving a new nungw.
- **Indexing:** `g[i]` reads the i-th float (1-based). `nungw.set(g, i, v)` writes one.

| Function | Purpose |
|---|---|
| `nungw.range(n)`, `nungw.zeros(n)`, `nungw.fill(n, v)`, `nungw.of(list)` | build one |
| `nungw.size(g)`, `nungw.get(g, i)`, `nungw.set(g, i, v)`, `nungw.to_list(g)` | inspect, read, change, read back |
| `nungw.sqrt/square/abs(g)` | element-wise, giving a new array (same as the operators) |
| `nungw.sum/mean/max/min(g)`, `nungw.dot(a, b)`, `nungw.sum_squares(g)`, `nungw.norm(g)` | reductions to a single number |
| `nungw.matmul(a, rows, inner, b, cols)` | matrix product: `a` is `rows`×`inner`, `b` is `inner`×`cols` (row-major), result `rows`×`cols` |

A nungw is a value: a temporary made by an operator frees itself when it is no
longer used, so loops do not leak. The reductions `sum_squares`, `norm` and `dot`
are **fused** — one pass over memory, no temporary array.

**Fusion.** An element-wise expression assigned to a nungw, like
`c = nungw.sqrt(a) * 1.5 - a / 3.0`, is compiled into **one** kernel that runs in a
single parallel pass with no temporary arrays — not one pass per operator.

**Speed.** On a 10-million element array, against NumPy (16 cores):
- `nungw.sum_squares(a)` runs in ~0.36 ms, vs NumPy `np.sum(a**2)` ~14 ms and `np.einsum` ~2 ms.
- the fused expression `nungw.sqrt(a) * 1.5 - a / 3.0` runs in ~13 ms, vs ~41 ms for the same NumPy expression (~3x).

Globw wins because the arrays are contiguous unboxed floats worked on across every
core, while NumPy's element-wise operations are single-threaded (and build
temporaries). The fused expression is still interpreted per element; compiling
each kernel to machine code (a JIT) would take it close to the memory-bandwidth
limit.

For **matrix multiplication** (`nungw.matmul`), Globw uses an optimized BLAS when
one is available — exactly as NumPy does — and otherwise a cache-blocked,
multi-threaded pure-Rust kernel. Point the `GLOBW_BLAS` environment variable at an
OpenBLAS (or compatible) shared library, or put `openblas.dll` next to the program,
and `nungw.matmul` calls it.

On a 2000×2000 product (f64): with the same OpenBLAS that NumPy ships, Globw
runs in ~25 ms against NumPy's ~22 ms — **a tie**. Without a BLAS, the built-in
kernel does it in ~39 ms. So on linear algebra Globw matches NumPy when it has the
same library, and stays within ~2× on its own.

**Databases (`db`).** `import db` gives a real SQL database, backed by SQLite
(the same engine behind Python's `sqlite3`). A connection is an `int` handle.

```
import db
import data
variables:
    c, n as int
    rows as string
c = db.open("shop.db")               ' a file, or ":memory:" for a temporary one
n = db.run(c, "create table if not exists items (name text, price real)")
n = db.run(c, "insert into items values ('pen', 1.5), ('book', 9.0)")
rows = db.query(c, "select name, price from items order by price")
output(data.text(rows, "[1].name"), " costs ", data.number(rows, "[1].price"))
db.close(c)
```

- `db.open(path)` opens (creating it if needed) a SQLite file — or `":memory:"` — and returns a connection handle. Data in a file **persists** between runs.
- `db.run(c, sql)` runs a statement (`create`/`insert`/`update`/`delete`) and returns the number of rows changed.
- `db.value(c, sql)` runs a query and returns the first column of the first row as text (`""` if there are no rows) — handy for counts and single values.
- `db.query(c, sql)` runs a query and returns the whole result as a **JSON** array of row objects, which you read with the `data` module (`data.size`, `data.text(rows, "[1].name")`, …).
- `db.close(c)` closes the connection.
- SQL errors, a bad query or a closed handle become catchable run-time errors. (The SQL is sent as written; build it carefully.)

**Web server (`serve`).** `import serve` runs an HTTP server. It is synchronous:
the program takes one request at a time and answers it, so there are no callbacks.

```
import serve
import data
variables:
    s as int
    req, path as string
s = serve.start(8080)                 ' listen on 127.0.0.1:8080
while(true)
    req = serve.wait(s)               ' blocks until a request arrives
    path = data.text(req, "path")
    if(path == "/hello")
        serve.reply(s, 200, "<h1>Hello from Globw!</h1>")
    else
        serve.reply(s, 404, "not found")
    end if
end loop
```

- `serve.start(port)` starts listening on `127.0.0.1:port` and returns a server handle.
- `serve.wait(s)` blocks until a request arrives and returns it as **JSON** — `{"method":…, "path":…, "body":…}` — read with the `data` module.
- `serve.reply(s, status, body)` answers the current request (HTML by default).
- `serve.stop(s)` stops the server.
- One request is "current" between `wait` and `reply`; reply before the next `wait`.

**Background processes (`proc`).** `import proc` runs other programs at the same
time — concurrency at the level of the operating system (Globw's own code stays
single-threaded, so this does not share memory; it runs separate programs in
parallel and collects their output).

```
import proc
variables:
    a, b as int
a = proc.start("curl -s https://example.com")   ' both start at once
b = proc.start("curl -s https://example.org")
output(proc.wait(a))                              ' then collect each result
output(proc.wait(b))
```

- `proc.start(command)` starts a command in the background and returns a handle; it returns immediately.
- `proc.done(h)` is `true` once the command has finished (a non-blocking check).
- `proc.wait(h)` blocks until it finishes and returns its output; a non-zero exit is a catchable error carrying the error output.
- `proc.kill(h)` stops it.

Three 2-second commands started together finish in about 2 seconds, not 6 — they really run at the same time.

**GUI widgets (`gtq`, inspired by Qt).** `import gtq` adds buttons, checkboxes and
sliders to the graphics window. It is **immediate mode**: inside the usual
`while(open()) … show() end loop`, you draw each widget every frame and it tells
you what the user did.

```
import gtq
variables:
    on as bool
    vol as float
window(400, 250, "My app")
while(open())
    clear("#1e293b")
    gtq.panel(20, 18, 360, 52, "#334155")
    gtq.label(34, 36, "A little GUI", "#e2e8f0")
    if(gtq.button(34, 95, 150, 38, "Click me"))
        output("clicked")
    end if
    on = gtq.checkbox(210, 104, "On", on)      ' returns the new state
    vol = gtq.slider(34, 185, 330, vol, 0, 100) ' returns the new value
    show()
end loop
```

- `gtq.panel(x, y, w, h, color)` draws a filled rectangle (a background).
- `gtq.label(x, y, text, color)` draws text.
- `gtq.button(x, y, w, h, label)` draws a button and returns `true` on the frame it is clicked.
- `gtq.checkbox(x, y, label, checked)` draws a checkbox and returns its new checked state (give it back the value each frame).
- `gtq.slider(x, y, w, value, min, max)` draws a slider and returns its new value while dragged.
- `gtq.radio(x, y, label, selected)` — a radio button; returns `true` on the frame it is clicked (give it `selected` to draw the filled state).
- `gtq.toggle(x, y, label, on)` — a switch; returns its new on/off state.
- `gtq.progress(x, y, w, value, max)` — a progress bar.
- `gtq.separator(x, y, w)` — a horizontal divider line.
- `gtq.groupbox(x, y, w, h, title)` — a titled frame.
- `gtq.spinbox(x, y, value, step, min, max)` — a number field with − / + buttons; returns the new value.
- `gtq.selector(x, y, w, "a,b,c", index)` — a combo that cycles through the comma-separated options on click; returns the new index.
- `gtq.textbox(x, y, w, h, current)` — a text field; clicking it opens a dialog to edit; returns the new text.
- `gtq.vslider(x, y, h, value, min, max)` — a vertical slider.
- `gtq.hovered(x, y, w, h)` / `gtq.clicked(x, y, w, h)` — generic hit tests for a region (true on hover / on the click frame).

(`gtq` covers the common Qt Widgets. Qt's other areas are served by other modules: SQL by `db`, networking by `web`/`serve`, processes by `proc`, charts by `chart` — and Globw does not replicate Qt's full class library.)

**`gtq` batch G — 150 more GUI functions.** Four families:

- *Colour helpers* (pure, return a `"#rrggbb"` string or a number): `lighten`,
  `darken`, `mix`, `grayscale`, `invert`, `complement`, `tint`, `shade`,
  `contrast_color`, `from_rgb`, `from_hsl`, `hue_shift`, `saturate`, `desaturate`,
  `with_red`/`with_green`/`with_blue`, `brighten`, `dim`, `mute`,
  `red_of`/`green_of`/`blue_of`, `luminance_of`, `is_dark` (25).
- *Layout helpers* (pure, return a coordinate): `row_x`, `col_y`, `grid_x`,
  `grid_y`, `center_x`, `center_y`, `right_x`, `bottom_y`, `fit_w`, `fit_h`,
  `aspect_w`, `aspect_h`, `lerp_pos`, `remap`, `snap`, `golden`, `pad_in`,
  `inset_w`, `distribute`, `span_end`, `scale_at`, `wrap_index`, `clamp_pos`,
  `cell_x`, `cell_y` (25).
- *Vector icons* `gtq.ic_<name>(x, y, size, color)` — 49 named icons (check, close,
  plus, minus, arrows, chevrons, menu, gear, home, search, heart, star, user,
  bell, mail, trash, pencil, save, folder, file, play, pause, stop, next, prev,
  volume, mute, wifi, battery, lock, unlock, eye, eye_off, calendar, clock, pin,
  tag, flag, bookmark, download, upload, refresh, …) plus the parameterised
  `gtq.icon(x, y, size, "name", color)` (50).
- *More widgets*: display — `card`, `card_header`, `badge`, `chip`, `pill`,
  `avatar`, `status_dot`, `vdivider`, `tooltip`, `toast`, `gauge`, `meter`,
  `spinner`, `titlebar`, `statusbar`, `toolbar`, `breadcrumb`, `kbd`,
  `progress_ring`, `skeleton`, `image_placeholder`, `ribbon`, `note`, `bar`;
  interactive — `icon_button`, `link`, `switch`, `stepper`, `segmented`, `tabs`,
  `numberbox`, `list_item`, `menu_item`, `accordion`, `rating`, `color_swatch`,
  `chip_close`, `fab`, `pagination`, `button_primary`, `button_ghost`, `knob`,
  `scrollbar_v`, `dropdown`, `checkbox_card`, `toggle_icon`, `text_button`,
  `slider_mini`, `stepper_f`, `close_button` (50).

**More built-in functions (batch H, 150 functions).**

- `ease` — Robert Penner easing curves for animation, `t` in 0..1:
  `in_quad`/`out_quad`/`in_out_quad` and the same `in`/`out`/`in_out` trio for
  `cubic`, `quart`, `quint`, `sine`, `expo`, `circ`, `back`, `elastic`, `bounce` (30).
- `trig2` — `haversin`, `versin`, `coversin`, `exsec`, `excosec`, `chord` (angle in degrees).
- `special` — `stirling(n)`, `logit(p)`, `softplus(x)`, `gudermannian(x)`.
- `polar` — `to_x(r,angle)`, `to_y(r,angle)`, `to_r(x,y)`, `to_angle(x,y)` (degrees).
- `transform` — `rotate_x(x,y,deg)`, `rotate_y(x,y,deg)` (rotate a point about the origin).
- `temp2` — Rankine and Réaumur conversions (`c_to_rankine`, `rankine_to_c`, `f_to_rankine`, `rankine_to_f`, `c_to_reaumur`, `reaumur_to_c`).
- `storage` — binary size steps: `bytes_to_kb`, `kb_to_mb`, `mb_to_gb`, `gb_to_tb`, `tb_to_pb`, `bits_to_bytes`.
- `area2` — `acres_to_hectares`, `hectares_to_acres`, `sqkm_to_sqmi`, `sqmi_to_sqkm`.
- `kitchen` — `tsp_to_tbsp`, `tbsp_to_cup`, `cup_to_floz`, `floz_to_ml`, `ml_to_floz`, `oz_to_grams`, `grams_to_oz`, `lb_to_oz`.
- `chem` — `molarity`, `moles`, `mass`, `dilution`, `poh`, `percent_yield`, `gas_pressure`, `density`, `atoms`, `moles_from_atoms`.
- `elec` — `power`, `energy`, `capacitor_charge`, `inductor_energy`, `rc_time`, `reactance_c`, `reactance_l`, `resonance`.
- `finance2` — `cagr`, `pv`, `fv`, `effective_rate`, `real_return`, `debt_ratio`, `current_ratio`, `roe`, `eps`, `pe_ratio`.
- `health2` — `bsa`, `waist_hip`, `target_hr`, `calories_walking`, `calories_running`, `vo2max`.
- `music` — `semitones_between`, `transpose`, `bpm_to_hz`, `hz_to_bpm`, `cents_to_ratio`, `ratio_to_cents`.
- `space` — `gravity_force`, `schwarzschild`, `orbital_velocity`, `surface_gravity`.
- `optics` — `magnification`, `focal_from_radius`, `power`, `snell`.
- `prob2` — `binomial_pmf`, `poisson_pmf`, `normal_cdf`, `normal_pdf`.
- `rpg` — `damage`, `xp_for_level`, `level_from_xp`, `crit`, `hit_chance`.
- `physics2` — `energy_mass`, `photon_energy`, `de_broglie`, `coulomb`.
- `stats2` — `coefficient_variation`, `standard_error`, `margin_error`.
- `map` — `mercator_y`, `tile_x`, `tile_y` (web-map tile coordinates).
- `date2` — `day_of_year`, `weekday_num` (Zeller, 0 = Saturday), `days_in_year`.
- `wave2` — `angular`, `period`, `frequency`.
- `light` — `speed`, `time_to_travel`, `distance_in_lightyears`, `years_to_km`.
- `sound2` — `speed`, `distance_from_delay`, `time_for_distance`.

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
- **Inheritance:** `class Child is Parent` makes `Child` inherit the fields and methods of `Parent` (single inheritance). A `Child` object can be used wherever a `Parent` is expected (variable, parameter, list element). `Child` can **override** a method by redefining it with the same parameters and return type; the version that runs is chosen by the object's real class (dynamic dispatch), even through an inherited method that calls `self.method(...)`.
- **`parent`** inside a method calls the parent class's version: `parent.new(...)` in a constructor, `parent.area()` in an overridden method. The constructor `new` may take different parameters from the parent's.
- **`object is Class`** is `true` when the object's class is `Class` or a subclass of it.
- **Errors:** an unknown class, a field or method a class does not have, the wrong number of constructor or method values, assigning the wrong type to a field, a field or variable named `self`, a duplicate class, field or method, or a constructor written as a function, an unknown parent, an inheritance cycle, or an override whose parameters or return type differ.

### 6.14 Error handling

A run-time error normally stops the program. A `try` block catches it instead.

```
variables:
    problem as string

try
    output(100 div n)
catch problem                ' "problem" receives the error message
    output("Something went wrong: ", problem)
always                       ' optional: runs on every path
    output("Done.")
end try

fail "the balance cannot be negative"   ' raise your own error
```

- **`try ... catch name ... [always ...] end try`**: the `try` body runs; on a run-time error control jumps to the **`catch`** block, with the message stored in `name` (a declared `string` variable). The **`always`** block, if present, runs afterwards on both paths (no error and caught error).
- Caught errors include every run-time error: division by zero, a value that cannot be read by `input`, a missing file, an index or key out of range, and `fail`.
- **`fail message`** raises a run-time error on purpose, with a text message.
- An error raised inside a called function, and not caught there, unwinds to the nearest `try` in the caller.
- If no `try` catches it, the program stops with the error, as before.
- Leaving a `try` with `return`, `break` or `continue` removes its handler correctly.
- Note: if the `catch` block itself raises an error, the `always` block is skipped and the error propagates outward.

### 6.15 Functions as values

A function can be stored in a variable, passed to another function, and called
through the variable. There are also short anonymous functions (lambdas).

```
variables:
    op as function(int, int) as int     ' a function type

function add(a as int, b as int) as int
    return a + b
end function

op = add                                ' a named function used as a value
output(op(3, 4))                         ' called through the variable

' a lambda: one line with "gives", or a block closed by end function
double = function(x as int) as int gives x * 2
noisy = function(x as int) as int
    output("seen ", x)
    return x
end function
```

- **Function type:** `function(T1, T2) as R` (or `function(T1, T2)` for a value
  that returns nothing). It can be the type of a variable, parameter, field,
  list element or dictionary value.
- A **named function** used where a value is expected becomes a function value.
  Calling it through a variable passes all its parameters (no defaults or names).
- A **lambda** is `function(params) as R gives expression` (one line) or the
  block form `function(params) as R ... end function`. A lambda is a value.
- **Limit (for now):** a lambda does not capture the variables around it — it
  uses only its own parameters, globals and other functions. (Planned next.)

**List operations** that take a function:

| Command | Result | Effect |
|---|---|---|
| `transform(list, f)` | `list of R` | a new list with `f` applied to each element (map) |
| `keep(list, f)` | same list type | a new list with the elements where `f` is true (filter) |
| `reduce(list, f, start)` | the accumulator type | folds the list to one value: `f(acc, element)` |

```
squares = transform(nums, function(x as int) as int gives x * x)
odds    = keep(nums, function(x as int) as bool gives x mod 2 == 1)
total   = reduce(nums, function(acc as int, x as int) as int gives acc + x, 0)
```

### 6.16 Tuples

A tuple groups a fixed number of values that may have different types, without
defining a class. Its main use is returning several values at once.

```
variables:
    point as tuple of (int, int)
    lo, hi as int

point = (3, 7)                 ' a tuple literal (two or more values)
output(point.1, " ", point.2)  ' elements by 1-based position

function range(l as list of int) as tuple of (int, int)
    return (minimum(l), maximum(l))
end function

lo, hi = range([5, 2, 9])      ' unpacking: several variables from one tuple
lo, hi = (hi, lo)              ' the right side is evaluated first, so this swaps
```

- **Type:** `tuple of (T1, T2, ...)`, two or more element types (any type).
- **Literal:** `(a, b, ...)` with at least one comma. `(x)` alone is just grouping.
- **Access:** `t.1`, `t.2`, ... by 1-based position (checked at compile time).
- **Unpacking:** `a, b = t` assigns the elements to the variables, which must be
  declared and match the element types; the counts must be equal.
- A tuple is printed like a list (`[3, 7]`).

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
globw --check program.gw
       globw --profile program.gw  (counts how often each opcode runs)   only check for errors, do not run
```

## 9. Tests

- **Unit tests** for every module.
- **Examples**: the `examples/` folder holds pairs of `name.gw` + `name.expected` (expected output) and, where needed, `name.input`. One test runs them all and compares the output. Examples that *must* fail are included too, with their expected error text.
- **Benchmarks**: recursive Fibonacci (fib 30), sieve of Eratosthenes (up to 10 million) and a sum loop (100 million iterations), compared with the same programs in Python.
  **Target**: at least 5 times faster than CPython on all three.

## 10. Out of scope for v1

Packages (modules in subfolders), records/structs, sets, sound, more than one window, editor syntax highlighting, interactive REPL, JIT compilation.
