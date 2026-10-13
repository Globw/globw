use super::*;
use crate::lexer::tokenize;
use crate::parser::parse;

pub(super) fn check_src(src: &str) -> Result<TProgram, Vec<Error>> {
    check(&parse(tokenize(src).unwrap()).unwrap())
}

pub(super) fn main_types(src: &str) -> Vec<(String, Type)> {
    let p = check_src(src).unwrap_or_else(|e| panic!("{:?}", e));
    p.functions[0].locals.iter().map(|s| (s.name.clone(), s.ty)).collect()
}

pub(super) fn first_error(src: &str) -> Error {
    check_src(src).unwrap_err().remove(0)
}

fn t(name: &str, ty: Type) -> (String, Type) {
    (name.to_string(), ty)
}

use BaseType::*;

const DOUBLE: &str = "FUNCTION double(x as int) as int\n    RETURN x * 2\nEND FUNCTION\n";

#[test]
fn declared_types_are_kept() {
    let src = "variables:\n    a as int\n    b as float\n    c as string\n    d as bool\n    v as int[3]\n    l as list of string\na <- 1\n";
    assert_eq!(
        main_types(src),
        vec![
            t("a", Type::Base(Int)),
            t("b", Type::Base(Real)),
            t("c", Type::Base(Str)),
            t("d", Type::Base(Bool)),
            t("v", Type::Array(Some(Int))),
            t("l", Type::List(Some(Str))),
        ]
    );
}

#[test]
fn int_goes_into_float() {
    assert!(check_src("variables:\n    x as float\n    v as float[2]\nx <- 1\nx <- x + 1\nv[1] <- 3\n").is_ok());
}

#[test]
fn float_into_int_is_an_error() {
    let e = first_error("variables:\n    n as int\nn <- 2.5\n");
    assert_eq!(e.message, "\"n\" is an int, but here it gets a float");
    assert_eq!(e.hint.as_deref(), Some("use DIV for integer division"));
    assert_eq!(e.pos.line, 3);
}

#[test]
fn string_into_int_is_an_error() {
    assert_eq!(first_error("variables:\n    n as int\nn <- \"a\"\n").message, "\"n\" is an int, but here it gets a string");
    assert_eq!(
        first_error("variables:\n    l as list of string\nAPPEND(l, 1)\n").message,
        "the elements of \"l\" are strings, but here they get an int"
    );
}

#[test]
fn every_wrong_assignment_is_reported() {
    let errors = check_src("variables:\n    n as int\nn <- \"a\"\nn <- TRUE\n").unwrap_err();
    assert_eq!(errors.len(), 2);
}

#[test]
fn input_plus_input_adds_numbers() {
    let p = check_src("variables:\n    a, b, c as int\ninput(a, b)\nc <- a + b\n").unwrap();
    match &p.functions[0].body[1] {
        TStmt::Assign(_, v) => assert!(matches!(v.kind, TExprKind::Binary(crate::ast::BinOp::Add, Int, ..)), "{:?}", v),
        other => panic!("{:?}", other),
    }
}

#[test]
fn undeclared_variable() {
    let e = first_error("x <- 1\n");
    assert_eq!(e.message, "variable \"x\" is not declared");
    assert_eq!(e.hint.as_deref(), Some("add it to \"variables:\" at the top of the program"));
    assert_eq!((e.pos.line, e.pos.col), (1, 1));
}

#[test]
fn miscased_value_keyword_hint() {
    let e = first_error("variables:\n    b as bool\nb <- true\n");
    assert_eq!(e.hint.as_deref(), Some("keywords are written in UPPERCASE: write TRUE"));
}

#[test]
fn operator_type_errors() {
    let a = "variables:\n    a as int\n    f as float\n    s as string\n    ok as bool\n";
    assert_eq!(first_error(&format!("{}a <- 1 + TRUE\n", a)).message, "cannot use + between an int and a bool");
    assert_eq!(first_error(&format!("{}f <- 7.5 MOD 2\n", a)).message, "DIV and MOD only work with integers");
    assert_eq!(first_error(&format!("{}ok <- \"x\" < 1\n", a)).message, "cannot compare a string with an int");
    assert_eq!(
        first_error(&format!("{}a <- 1\nIF(a)\nEND IF\n", a)).message,
        "the condition must be TRUE or FALSE, here it is an int"
    );
    assert!(check_src(&format!("{}a <- 3\ns <- \"n = \" + a\n", a)).is_ok());
}

#[test]
fn use_before_assignment_in_if_without_else() {
    let e = first_error("variables:\n    x, y as int\nIF(TRUE)\n    x <- 1\nEND IF\ny <- x\n");
    assert_eq!(e.message, "variable \"x\" might not have a value yet");
    assert_eq!(e.pos.line, 6);
}

#[test]
fn use_before_assignment_in_while() {
    let e = first_error("variables:\n    x, y as int\nWHILE(FALSE)\n    x <- 1\nEND LOOP\ny <- x\n");
    assert_eq!(e.message, "variable \"x\" might not have a value yet");
}

#[test]
fn assignment_in_both_branches_or_repeat_is_fine() {
    let d = "variables:\n    x, y as int\n";
    assert!(check_src(&format!("{}IF(TRUE)\n    x <- 1\nELSE\n    x <- 2\nEND IF\ny <- x\n", d)).is_ok());
    assert!(check_src(&format!("{}REPEAT\n    x <- 1\nWHILE(FALSE)\ny <- x\n", d)).is_ok());
}

#[test]
fn for_variable_must_be_int() {
    let e = first_error("variables:\n    i as float\nFOR(i FROM 1 TO 3)\nEND LOOP\n");
    assert_eq!(e.message, "the FOR variable must be an int, \"i\" is a float");
}

#[test]
fn length_rules() {
    assert!(check_src("variables:\n    v as int[3]\n    n as int\nn <- LENGTH(v)\n").is_ok());
    assert_eq!(
        first_error("variables:\n    m as int[2, 2]\n    n as int\nn <- LENGTH(m)\n").message,
        "LENGTH cannot be used on a matrix"
    );
}

#[test]
fn index_errors() {
    assert_eq!(
        first_error("variables:\n    m as int[2, 2]\nm[1] <- 0\n").message,
        "\"m\" is a matrix: it needs 2 indices, like m[i, j]"
    );
    assert_eq!(first_error("variables:\n    v as int[2]\nv <- 1\n").message, "a whole array or list cannot be assigned at once");
    assert_eq!(
        first_error("variables:\n    x as int\nx <- 1\nx[1] <- 2\n").message,
        "\"x\" is not an array: [ ] cannot be used on it"
    );
}

#[test]
fn errors_are_collected_and_sorted() {
    let errors = check_src("a <- 1\nb <- 2\n").unwrap_err();
    assert_eq!(errors.len(), 2);
    assert_eq!((errors[0].pos.line, errors[1].pos.line), (1, 2));
}

#[test]
fn misc_statement_errors() {
    assert_eq!(first_error("RETURN\n").message, "RETURN can only be used inside a FUNCTION or PROCEDURE");
    assert_eq!(first_error("variables:\n    v as int[2]\noutput(v)\n").message, "output prints only simple values");
    assert_eq!(
        first_error("variables:\n    v as int[2]\nAPPEND(v, 1)\n").message,
        "APPEND works only with lists (declared with list of)"
    );
    assert_eq!(first_error("foo(1)\n").message, "function \"foo\" does not exist");
    assert_eq!(first_error("variables:\n    v as list of int\nlength(v)\n").hint.as_deref(), Some("did you mean LENGTH?"));
}

#[test]
fn declaration_errors() {
    assert_eq!(first_error("variables:\n    a, a as int\n").message, "\"a\" is declared twice");
    assert_eq!(first_error("variables:\n    LENGTH as int\n").message, "\"LENGTH\" is a built-in name: choose another one");
    assert_eq!(
        first_error("GLOBAL variables:\n    g as int\nvariables:\n    g as int\n").message,
        "\"g\" is already declared as GLOBAL"
    );
}

#[test]
fn function_call_has_the_declared_return_type() {
    let p = check_src(&format!("variables:\n    r as int\n{}r <- double(4)\n", DOUBLE)).unwrap();
    assert_eq!(p.functions.len(), 2);
    assert_eq!(p.functions[1].name, "double");
    assert_eq!(p.functions[1].ret, Some(Int));
}

#[test]
fn one_instance_per_function() {
    let src = "variables:\n    a, b as float\nFUNCTION half(x as float) as float\n    RETURN x / 2\nEND FUNCTION\na <- half(4)\nb <- half(1.5)\n";
    assert_eq!(check_src(src).unwrap().functions.len(), 2);
}

#[test]
fn argument_types_are_checked() {
    let half = "FUNCTION half(x as float) as float\n    RETURN x / 2\nEND FUNCTION\n";
    assert_eq!(
        first_error(&format!("variables:\n    r as float\n{}r <- half(\"a\")\n", half)).message,
        "argument 1 of \"half\" must be a float, here it is a string"
    );
    let p = check_src(&format!("variables:\n    r as float\n{}r <- half(3)\n", half)).unwrap();
    match &p.functions[0].body[0] {
        TStmt::Assign(_, TExpr { kind: TExprKind::Call(_, args), .. }) => assert_eq!(args[0].kind, TExprKind::Real(3.0)),
        other => panic!("{:?}", other),
    }
}

#[test]
fn collection_element_type_must_match() {
    let p = "PROCEDURE p(a as float[])\nEND PROCEDURE\n";
    assert_eq!(
        first_error(&format!("variables:\n    v as int[3]\n{}p(v)\n", p)).message,
        "argument 1 of \"p\" must be an array of floats, here it is an array of ints"
    );
    assert_eq!(
        first_error(&format!("variables:\n    l as list of float\n{}p(l)\n", p)).message,
        "argument 1 of \"p\" must be an array of floats, here it is a list of floats"
    );
    assert!(check_src(&format!("variables:\n    w as float[5]\n{}p(w)\n", p)).is_ok());
}

#[test]
fn return_type_is_checked() {
    assert_eq!(
        first_error("variables:\n    r as int\nFUNCTION f() as int\n    RETURN \"a\"\nEND FUNCTION\nr <- f()\n").message,
        "this function returns an int, here it returns a string"
    );
    assert!(check_src("variables:\n    r as float\nFUNCTION f() as float\n    RETURN 1\nEND FUNCTION\nr <- f()\n").is_ok());
}

#[test]
fn recursion() {
    let src = "variables:\n    r as int\nFUNCTION fib(n as int) as int\n    IF(n < 2)\n        RETURN n\n    END IF\n    RETURN fib(n - 1) + fib(n - 2)\nEND FUNCTION\nr <- fib(10)\n";
    assert!(check_src(src).is_ok());
}

#[test]
fn call_errors() {
    assert_eq!(
        first_error(&format!("variables:\n    r as int\n{}r <- double(1, 2)\n", DOUBLE)).message,
        "\"double\" takes 1 value, here it gets 2"
    );
    assert_eq!(
        first_error("variables:\n    r as int\nPROCEDURE p()\nEND PROCEDURE\nr <- p()\n").message,
        "PROCEDURE \"p\" does not return a value"
    );
    assert_eq!(
        first_error("variables:\n    r as int\nFUNCTION f(x as int) as int\n    IF(x > 0)\n        RETURN 1\n    END IF\nEND FUNCTION\nr <- f(2)\n")
            .message,
        "function \"f\" can reach END FUNCTION without RETURN"
    );
    assert_eq!(
        first_error("PROCEDURE p()\n    RETURN 1\nEND PROCEDURE\np()\n").message,
        "a PROCEDURE does not return values"
    );
}

#[test]
fn main_variables_are_not_visible_in_functions() {
    let e = first_error("variables:\n    x as int\nPROCEDURE p()\n    x <- 1\nEND PROCEDURE\np()\n");
    assert_eq!(e.message, "variable \"x\" is not declared");
    assert_eq!(
        e.hint.as_deref(),
        Some("\"x\" belongs to the main program: to use it here declare it with GLOBAL variables:")
    );
    assert!(check_src("GLOBAL variables:\n    x as int\nPROCEDURE p()\n    x <- 1\nEND PROCEDURE\np()\n").is_ok());
}

#[test]
fn uncalled_subprograms_are_fully_checked() {
    assert_eq!(
        first_error("PROCEDURE never()\n    y <- 1\nEND PROCEDURE\n").message,
        "variable \"y\" is not declared"
    );
    assert_eq!(
        first_error("FUNCTION unused(x as int) as int\n    RETURN x * \"a\"\nEND FUNCTION\n").message,
        "cannot use * between an int and a string"
    );
}

#[test]
fn builtin_calls_are_typed() {
    let ok = "variables:\n    c as string\n    n as int\n    t as float\n    ok as bool\nc <- RGB(255, 128, 0)\nn <- RANDOM(1, 6)\nt <- TIME()\nWINDOW(400, 300, \"x\")\nRECT(1, 2.5, 10, 10, c)\nok <- KEY(\"A\")\nMESSAGE(\"hi\")\n";
    assert!(check_src(ok).is_ok(), "{:?}", check_src(ok).err());
    assert_eq!(first_error("RECT(1, 2, 3, \"red\")\n").message, "\"RECT\" takes 5 values, here it gets 4");
    assert_eq!(
        first_error("variables:\n    n as int\nn <- SHOW()\n").message,
        "SHOW does not return a value: use it as a statement on its own line"
    );
    assert_eq!(
        first_error("RANDOM(1, 2)\n").message,
        "RANDOM returns a value: use it in an expression, like v <- RANDOM(...)"
    );
    assert_eq!(
        first_error("WINDOW(\"a\", 2, \"t\")\n").message,
        "argument 1 of \"WINDOW\" must be an int, here it is a string"
    );
    assert_eq!(first_error("variables:\n    RECT as int\n").message, "\"RECT\" is a built-in name: choose another one");
}
