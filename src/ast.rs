//! The syntax tree produced by the parser.

use crate::errors::Pos;
use crate::tir::{BaseType, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    /// `/`, always a real division
    Div,
    /// `DIV`, integer division
    IntDiv,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Real(f64),
    Str(String),
    Bool(bool),
    Var(String),
    Index(String, Vec<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnOp, Box<Expr>),
    Call(String, Vec<Expr>),
    /// indexing anything that is not a plain variable: `g[1][2]`, `keys(d)[1]`
    Subscript(Box<Expr>, Vec<Expr>),
    /// a list written in brackets: `[1, 2, 3]`
    ListLit(Vec<Expr>),
    /// a value passed by parameter name in a call: `f(size = 3)`
    Named(String, Box<Expr>),
    /// `new Point(3, 4)`: makes an object and runs its constructor
    New(String, Vec<Expr>),
    /// `expr.field` on any expression (for `name.field` the lexer makes one name)
    Field(Box<Expr>, String),
    /// `expr.method(args)` on any expression
    MethodCall(Box<Expr>, String, Vec<Expr>),
    /// `expr is ClassName`: true if the object is of that class or a subclass
    IsType(Box<Expr>, String),
}

/// The left side of an assignment: `x`, `v[i]` or `m[i, j]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub name: String,
    pub indices: Vec<Expr>,
    /// further index groups: `g[1][2] = x` has indices `[1]` and rest `[[2]]`
    pub rest: Vec<Vec<Expr>>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Assign(Target, Expr),
    If { branches: Vec<(Expr, Vec<Stmt>)>, else_body: Option<Vec<Stmt>> },
    While(Expr, Vec<Stmt>),
    For { var: String, var_pos: Pos, from: Expr, to: Expr, step: Option<Expr>, body: Vec<Stmt> },
    Repeat(Vec<Stmt>, Expr),
    /// `for(x in collection)`: lists, arrays, dictionary keys, string characters
    ForEach { var: String, var_pos: Pos, iter: Expr, body: Vec<Stmt> },
    Break,
    Continue,
    Call(String, Vec<Expr>),
    /// a method call used as a statement: `people[i].move(1, 0)`
    Eval(Expr),
    /// assignment to a field or element reached through an expression:
    /// `people[i].name = "x"`, `grid[i].cells[j] = 0`
    SetMember(Expr, Expr),
    Return(Option<Expr>),
    /// runs the start-up code of a module (procedure `init`) unless the
    /// global flag `flag` says it already ran; added by the module loader
    LoadModule { init: String, flag: String },
}

/// `import shapes`, `import shapes as s`, `from shapes import area, side as l`,
/// each optionally preceded by `lazy`.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub module: String,
    pub alias: Option<String>,
    /// for `from ... import`: the names, each with its optional alias
    pub names: Option<Vec<ImportName>>,
    pub lazy: bool,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportName {
    pub name: String,
    pub alias: Option<String>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Shape {
    Scalar,
    Array(usize),
    Matrix(usize, usize),
    List,
    Dict,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub name: String,
    pub shape: Shape,
    /// for collections: the element type
    pub ty: BaseType,
    /// the whole declared type (needed for nested lists and dictionaries)
    pub full: Type,
    /// for parameters: the value used when a call leaves it out
    pub default: Option<Expr>,
    pub pos: Pos,
}

/// A class: fields (in `variables:` blocks) and methods (functions and
/// procedures). The constructor is the method named `new`.
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub name: String,
    /// the parent class, for inheritance (unused in the first version)
    pub parent: Option<(String, Pos)>,
    pub fields: Vec<Decl>,
    pub methods: Vec<Subprogram>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subprogram {
    pub name: String,
    pub is_function: bool,
    /// array and matrix parameters have size 0: any size is accepted
    pub params: Vec<Decl>,
    /// the declared return type of a FUNCTION; None for a PROCEDURE
    pub ret: Option<Type>,
    pub locals: Vec<Decl>,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub imports: Vec<Import>,
    pub classes: Vec<Class>,
    pub globals: Vec<Decl>,
    pub locals: Vec<Decl>,
    pub subprograms: Vec<Subprogram>,
    pub body: Vec<Stmt>,
}
