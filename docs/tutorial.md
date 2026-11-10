# Learn Globw in 10 minutes

Globw is a small, statically-typed language that reads like plain steps. You
declare your variables and their types up front; the compiler checks everything
**before** the program runs, so most mistakes are caught instantly with a clear
message. This page takes you from zero to writing real programs.

> Install the `gw` command from the [releases](https://github.com/Globw/globw/releases),
> add it to your PATH, and run a file with `gw program.gw`. To try snippets
> interactively, start the REPL with `gw --repl`.

## 1. Hello, world

A program is a sequence of statements, one per line. The simplest one needs
nothing special:

```gw
output("Hello, world!")
```

```
gw hello.gw
Hello, world!
```

A line starting with `'` is a comment.

## 2. Variables and types

Every variable is declared once, with its type, in a `variables:` block at the
top. After that you use it freely. Assignment is `=`; comparison is `==`.

```gw
variables:
    name as string
    age as int
    height as float

name = "Ada"
age = 36
height = 1.70
output("${name} is ${age}")      ' string interpolation with ${ }
```

The types are `int`, `float`, `string` and `bool`. An `int` is used wherever a
`float` is expected (it converts); every other mismatch is an error reported
before the program runs — no surprises at run time.

## 3. Deciding and repeating

```gw
variables:
    score as int
    i as int

score = 72

if(score >= 90)
    output("A")
else if(score >= 60)
    output("pass")
else
    output("try again")
end if

' a counting loop — lists and loops are 1-based
for(i from 1 to 5)
    output(i, " squared is ", i * i)
end loop
```

There is also `while(condition) … end loop`, and `repeat … while(condition)`
for a loop that always runs at least once. `break` and `continue` work as usual.

## 4. Functions and procedures

A **function** returns a value and declares its return type after the
parentheses. A **procedure** does something but returns nothing. Parameters can
have defaults.

```gw
function squares(n as int, start as int = 1) as list of int
    variables:
        out as list of int
        i as int
    for(i from start to n)
        append(out, i * i)
    end loop
    return out
end function

output(squares(5))        ' [1, 4, 9, 16, 25]
output(squares(5, 3))     ' [9, 16, 25]
```

Each function has its own `variables:` block for its locals. A function must
`return` a value on every path — also checked before the program runs.

## 5. Collections

Lists, dictionaries, arrays and matrices. Lists and arrays are **1-based**.

```gw
variables:
    grades as dict of string to list of int
    names as list of string
    i as int

grades["Anna"] = [8, 7, 9]
grades["Luca"] = [6]
append(grades["Luca"], 7)

names = keys(grades)
for(i from 1 to length(names))
    output(names[i], ": ", join(grades[names[i]], " "))
end loop

output("Marco has ", length(get(grades, "Marco", [])), " grades")   ' get with a default
```

`T[N]` is a fixed array, `T[N, M]` a matrix, `list of T` a growable list,
`set of T` a set, and `dict of K to V` a dictionary. They nest to any depth
(`dict of string to list of int`), and print like Python does.

## 6. Classes and objects

Fields go in a `variables:` block; methods are functions and procedures; the
constructor is the procedure named `new`.

```gw
variables:
    p as Player

class Player
    variables:
        name as string
        score as int
    procedure new(name as string)
        self.name = name
    end procedure
    procedure add(points as int)
        self.score = self.score + points
    end procedure
    function total() as int
        return self.score
    end function
end class

p = new Player("Ada")
p.add(10)
p.add(5)
output(p.name, " scored ", p.total())    ' Ada scored 15
```

## 7. Fast arrays with nungw

`nungw` is Globw's array engine. It works on whole arrays at once and is faster
than NumPy on the CPU — and, when you ask, runs on the GPU.

```gw
import nungw

variables:
    a as nungw of float

a = nungw.of([1.0, 4.0, 9.0, 16.0])
output(nungw.to_list(nungw.sqrt(a)))     ' [1.0, 2.0, 3.0, 4.0]
output(nungw.to_list(a * 2 + 1))         ' [3.0, 9.0, 19.0, 33.0]
```

Operations fuse into one pass, so `sqrt(a) + log(a)` is computed in a single
loop. For heavy work, `nungw.gpu(true)` moves arrays onto the GPU.

## 8. When things go wrong

Most errors are reported before the program runs, pointing at the exact spot:

```
Error at line 4, column 1: variable "cont" is not declared
    cont = 0
    ^^^^
Hint: add it to "variables:" at the top of the program
```

For errors that can only happen at run time (like dividing by zero or reading a
missing file), handle them with `try` (the `catch` name is a declared `string`):

```gw
variables:
    problem as string

try
    output(10 div 0)
catch problem
    output("caught: ", problem)
always
    output("done")
end try
```

## 9. Your toolbox

- `gw program.gw` — run. `gw --repl` — an interactive session where you can try
  expressions, declarations and even multi-line `function`/`for` blocks.
- `gw --check program.gw` — type-check without running.
- `gw fmt program.gw` — re-indent to the canonical style.
- The **VS Code extension** ("Globw") adds highlighting, live errors, completion,
  hover, an outline and go-to-definition.
- `gwget install <name>` — install a library from the catalogue.

## Where next

- The full [language specification](spec.md) documents every type, statement and
  built-in module (GUI, 2D/3D graphics, audio, dataframes, autodiff, symbolic
  math, networking, and more).
- The [`examples/`](../examples) folder has complete, runnable programs.
- Benchmarks and how they are measured: [`bench/README.md`](../bench/README.md).

Welcome to Globw.
