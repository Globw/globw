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

#[test]
fn infers_types_from_assignments() {
    let src = "variables: a, b, c, d\na <- 1\nb <- 2.5\nc <- \"x\"\nd <- a < 2\n";
    assert_eq!(
        main_types(src),
        vec![t("a", Type::Base(Int)), t("b", Type::Base(Real)), t("c", Type::Base(Str)), t("d", Type::Base(Bool))]
    );
}

#[test]
fn integer_widens_to_real() {
    assert_eq!(main_types("variables: x\nx <- 0\nx <- x + 0.5\n"), vec![t("x", Type::Base(Real))]);
}

#[test]
fn input_type_comes_from_later_use() {
    let src = "variables: n\ninput(n)\nIF(n > 0)\n    output(n)\nEND IF\n";
    assert_eq!(main_types(src), vec![t("n", Type::Base(Int))]);
}

#[test]
fn input_only_defaults_to_string() {
    assert_eq!(main_types("variables: s\ninput(s)\noutput(s)\n"), vec![t("s", Type::Base(Str))]);
}

#[test]
fn types_flow_backwards_through_assignments() {
    let src = "variables: a, b\ninput(a)\nb <- a\nb <- b * 2\n";
    assert_eq!(main_types(src), vec![t("a", Type::Base(Int)), t("b", Type::Base(Int))]);
}

#[test]
fn arithmetic_makes_unknown_operands_numbers() {
    assert_eq!(main_types("variables: a, b\ninput(a)\nb <- a * a\n")[0], t("a", Type::Base(Int)));
}

#[test]
fn element_types() {
    let src = "variables: v[3], l[]\nv[1] <- 2\nAPPEND(l, \"x\")\n";
    assert_eq!(main_types(src), vec![t("v", Type::Array(Some(Int))), t("l", Type::List(Some(Str)))]);
}

#[test]
fn string_concatenation() {
    let src = "variables: s, n\nn <- 3\ns <- \"n = \" + n\n";
    assert_eq!(main_types(src)[0], t("s", Type::Base(Str)));
}

#[test]
fn conflict_is_reported() {
    let e = first_error("variables: x\nx <- 1\nx <- \"a\"\n");
    assert_eq!(e.message, "\"x\" is an integer (line 2), but here it gets a string");
    assert_eq!(e.pos.line, 3);
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
    let e = first_error("variables: b\nb <- true\n");
    assert_eq!(e.hint.as_deref(), Some("keywords are written in UPPERCASE: write TRUE"));
}

#[test]
fn operator_type_errors() {
    assert_eq!(first_error("variables: a\na <- 1 + TRUE\n").message, "cannot use + between an integer and a boolean");
    assert_eq!(first_error("variables: a\na <- 7.5 MOD 2\n").message, "DIV and MOD only work with integers");
    assert_eq!(first_error("variables: a\na <- \"x\" < 1\n").message, "cannot compare a string with an integer");
    assert_eq!(
        first_error("variables: a\na <- 1\nIF(a)\nEND IF\n").message,
        "the condition must be TRUE or FALSE, here it is an integer"
    );
}

#[test]
fn use_before_assignment_in_if_without_else() {
    let e = first_error("variables: x, y\nIF(TRUE)\n    x <- 1\nEND IF\ny <- x\n");
    assert_eq!(e.message, "variable \"x\" might not have a value yet");
    assert_eq!(e.pos.line, 5);
}

#[test]
fn use_before_assignment_in_while() {
    let e = first_error("variables: x, y\nWHILE(FALSE)\n    x <- 1\nEND LOOP\ny <- x\n");
    assert_eq!(e.message, "variable \"x\" might not have a value yet");
}

#[test]
fn assignment_in_both_branches_or_repeat_is_fine() {
    assert!(check_src("variables: x, y\nIF(TRUE)\n    x <- 1\nELSE\n    x <- 2\nEND IF\ny <- x\n").is_ok());
    assert!(check_src("variables: x, y\nREPEAT\n    x <- 1\nWHILE(FALSE)\ny <- x\n").is_ok());
}

#[test]
fn for_variable_must_be_integer() {
    let e = first_error("variables: i\ni <- 0.5\nFOR(i FROM 1 TO 3)\nEND LOOP\n");
    assert_eq!(e.message, "the FOR variable must be an integer, \"i\" is a real");
}

#[test]
fn length_rules() {
    assert_eq!(main_types("variables: v[3], n\nn <- LENGTH(v)\n")[1], t("n", Type::Base(Int)));
    assert_eq!(first_error("variables: m[2, 2], n\nn <- LENGTH(m)\n").message, "LENGTH cannot be used on a matrix");
}

#[test]
fn index_errors() {
    assert_eq!(
        first_error("variables: m[2, 2]\nm[1] <- 0\n").message,
        "\"m\" is a matrix: it needs 2 indices, like m[i, j]"
    );
    assert_eq!(first_error("variables: v[2]\nv <- 1\n").message, "a whole array or list cannot be assigned at once");
    assert_eq!(first_error("variables: x\nx <- 1\nx[1] <- 2\n").message, "\"x\" is not an array: [ ] cannot be used on it");
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
    assert_eq!(first_error("variables: v[2]\noutput(v)\n").message, "output prints only simple values");
    assert_eq!(first_error("variables: v[2]\nAPPEND(v, 1)\n").message, "APPEND works only with lists (declared with [])");
    assert_eq!(first_error("foo(1)\n").message, "function \"foo\" does not exist");
    assert_eq!(first_error("variables: v[]\nlength(v)\n").hint.as_deref(), Some("did you mean LENGTH?"));
}

#[test]
fn declaration_errors() {
    assert_eq!(first_error("variables: a, a\n").message, "\"a\" is declared twice");
    assert_eq!(first_error("variables: LENGTH\n").message, "\"LENGTH\" is a built-in name: choose another one");
    assert_eq!(first_error("GLOBAL variables: g\nvariables: g\n").message, "\"g\" is already declared as GLOBAL");
}
