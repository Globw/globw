//! The syntax tree produced by the parser.

use crate::errors::Pos;

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
}

/// The left side of an assignment: `x`, `v[i]` or `m[i, j]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub name: String,
    pub indices: Vec<Expr>,
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
    Call(String, Vec<Expr>),
    Return(Option<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Shape {
    Scalar,
    Array(usize),
    Matrix(usize, usize),
    List,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub name: String,
    pub shape: Shape,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subprogram {
    pub name: String,
    pub is_function: bool,
    pub params: Vec<(String, Pos)>,
    pub locals: Vec<Decl>,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Program {
    pub globals: Vec<Decl>,
    pub locals: Vec<Decl>,
    pub subprograms: Vec<Subprogram>,
    pub body: Vec<Stmt>,
}
