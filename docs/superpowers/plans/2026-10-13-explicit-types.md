# Easier v1.1: Explicit Types — Implementation Plan

> Executed inline (superpowers:executing-plans), TDD per task.

**Goal:** Replace type inference with declared types (`int`, `float`, `string`, `bool`), declared in an indented `variables:` block and on function parameters and return values. Also accept `ELSE IF(...)` on one line, and color variables and types in VS Code.

**Spec:** `docs/spec.md` (v1.1), sections 4.1, 4.2, 6.2 and 6.4.

## Global Constraints
- Same constraints as `2026-10-13-easier-interpreter.md`: no dependencies, English messages asserted by tests, exit codes, commit trailer.
- The lexer does not change. `as`, `of`, `int`, `float`, `string` and `bool` stay `Tok::Name` and are recognized by the parser in context, so they are not reserved outside declarations.
- Error messages name types as `an int`, `a float`, `a string`, `a bool` (plural `ints`, `floats`, `strings`, `bools`).

## Design
- **AST**:
  - `Decl { name, shape, ty: BaseType, pos }`.
  - `Subprogram { params: Vec<Decl>, ret: Option<BaseType>, … }`.
  - A parameter's shape is `Scalar`, `Array(0)` for `T[]`, `Matrix(0, 0)` for `T[,]`, or `List`.
- **Parser**:
  - **Block layout**: `variables:` is followed by a newline, then lines `name {, name} as TYPE`. Each line must be indented more than `variables:`; the block ends at the first line that isn't.
  - **Old form**: `variables: a` gives the error `"variables:" must be followed by a new line with the declarations` with a hint showing the new form.
  - **TYPE grammar**: `int|float|string|bool` [`[N]` | `[N, M]`]; or `list of` `int|float|string|bool`.
  - **Parameters**: `name as TYPE`, where an array or matrix uses `[]` / `[,]` without sizes.
  - **Functions**: a `FUNCTION` requires `) as SIMPLETYPE`.
- **Checker**: keep the existing structure and seed every `Var.ty` from its declaration (`Var.declared = true`).
  - **`widen` on a declared variable** never changes its type. `join(declared, t) == Some(declared)` is fine (int→float). Anything else reports `"x" is an int, but here it gets a float`, or `the elements of "v" are ints, but here they get a string`. When an int variable gets a float the hint is `use DIV for integer division`.
  - **Instances**: the key is the declared parameter types, so there is exactly one instance per subprogram.
  - **Arguments** are checked against the parameter types: scalars must satisfy `join(param, arg) == Some(param)`, collections must have the same shape kind and element type. Errors are `argument 1 of "f" must be a float, here it is a string` and `argument 1 of "f" must be an array of ints, here it is …`.
  - **Returns**: `ret` is seeded from the declaration. `RETURN` checks compatibility with `this function returns an int, here it returns a string`.
  - Hints and defaulting become no-ops, because every type is known. They stay in place: removing them is a separate refactor.

## Tasks

### Task 1: Parser — typed declarations, typed signatures, ELSE IF on one line
Tests (`src/parser.rs`):
- `typed_declarations`: a GLOBAL block and a main block with `price as float`, `q, d as int`, `ok as bool`, `v as int[3]`, `m as float[2, 4]`, `l as list of string` give the right names, shapes and types, in order.
- `typed_signatures`: `FUNCTION area(b as float, h as float) as float` and `PROCEDURE fill(v as int[], m as float[,], l as list of int)` produce the right param shapes and types; `ret` is `Some(Real)` / `None`.
- `else_if_on_one_line`: `ELSE IF(...)` makes 2 branches and an else body, closed by one `END IF`.
- **Errors**:
  - old form `variables: a, b` gives `"variables:" must be followed by a new line with the declarations`;
  - an unknown type gives `unknown type "integer"` with hint `the types are int, float, string, bool`;
  - an unindented declaration line ends the block, so `x as int` on its own reports `expected <- or ( after "x"`;
  - a missing return type gives `a FUNCTION must declare its return type, like ) as int`;
  - an untyped parameter gives `parameter "x" needs a type, like x as int`.
- Update the existing parser tests to the new syntax.

### Task 2: Checker — declared types instead of inference
Tests (`src/checker/tests.rs`). Replace the inference tests with:
- `declared_types_are_kept`;
- `int_goes_into_float`;
- `float_into_int_is_an_error` (with the DIV hint);
- `string_into_int_is_an_error`;
- `input_plus_input_adds_numbers` (`a, b as int`; `c <- a + b` types as int, no concatenation);
- `argument_type_checked`;
- `int_argument_for_float_parameter_is_converted`;
- `collection_element_type_must_match`;
- `return_type_checked`;
- `one_instance_per_function`.

Keep and adapt the definite-assignment, operator, declaration, index, LENGTH, RETURN-placement and uncalled-subprogram tests.

### Task 3: Rewrite programs to the new syntax
Update the Easier programs in the compiler tests, the examples (`*.ez`, plus `*.expected` where error text or line numbers change) and the benchmarks. Add the example `examples/discount.ez`, which is the user's discount program with `ELSE IF` on one line and float prices (input `9,99` and `12`). `cargo test` must be fully green.

### Task 4: VS Code — colors and indentation
- **Grammar**:
  - types `int|float|string|bool` and `list of` → `storage.type.easier`;
  - `as` → `keyword.other.easier`;
  - function call names → `entity.name.function.easier`;
  - remaining identifiers → `variable.other.easier` (shown light blue in Dark+, like Python).
- **Indentation**: `variables:` increases the indent.
- Bump to 0.2.0 and reinstall with `install.ps1`.
- Tests: the existing parse tests still pass, and a node check confirms that every grammar regex compiles.
