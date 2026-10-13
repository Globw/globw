//! Typed intermediate representation: produced by the checker, consumed by the compiler.

use std::rc::Rc;

use crate::ast::{BinOp, Shape};
use crate::errors::Pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseType {
    Int,
    Real,
    Str,
    Bool,
}

impl BaseType {
    pub fn name(self) -> &'static str {
        match self {
            BaseType::Int => "an integer",
            BaseType::Real => "a real",
            BaseType::Str => "a string",
            BaseType::Bool => "a boolean",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            BaseType::Int => "integers",
            BaseType::Real => "reals",
            BaseType::Str => "strings",
            BaseType::Bool => "booleans",
        }
    }
}

/// The type of a value. For collections the payload is the element type,
/// `None` while it is still unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Unknown,
    Base(BaseType),
    Array(Option<BaseType>),
    Matrix(Option<BaseType>),
    List(Option<BaseType>),
}

impl Type {
    pub fn base(self) -> Option<BaseType> {
        match self {
            Type::Base(b) => Some(b),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Type::Unknown => "of unknown type",
            Type::Base(b) => b.name(),
            Type::Array(_) => "an array",
            Type::Matrix(_) => "a matrix",
            Type::List(_) => "a list",
        }
    }
}

/// The smallest type that holds both: equal types stay, integer and real
/// widen to real, anything else is incompatible.
pub fn join(a: BaseType, b: BaseType) -> Option<BaseType> {
    match (a, b) {
        (x, y) if x == y => Some(x),
        (BaseType::Int, BaseType::Real) | (BaseType::Real, BaseType::Int) => Some(BaseType::Real),
        _ => None,
    }
}

/// Where a variable lives: a register of the current function or a global slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Place {
    Local(u16),
    Global(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TExpr {
    pub kind: TExprKind,
    pub ty: Type,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TExprKind {
    Int(i64),
    Real(f64),
    Str(Rc<str>),
    Bool(bool),
    /// the value of a variable (for collections: a reference to it)
    Load(Place),
    /// an element of an array/list (1 index) or matrix (2 indices)
    Elem(Place, Vec<TExpr>),
    /// one character of a string: `s[i]`
    Char(Box<TExpr>, Box<TExpr>),
    ToReal(Box<TExpr>),
    ToStr(Box<TExpr>),
    /// the `BaseType` is the type of both operands (after conversions)
    Binary(BinOp, BaseType, Box<TExpr>, Box<TExpr>),
    Neg(BaseType, Box<TExpr>),
    Not(Box<TExpr>),
    /// call of `TProgram::functions[index]`
    Call(usize, Vec<TExpr>),
    Length(Box<TExpr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TTarget {
    pub place: Place,
    pub indices: Vec<TExpr>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TStmt {
    Assign(TTarget, TExpr),
    If(Vec<(TExpr, Vec<TStmt>)>, Option<Vec<TStmt>>),
    While(TExpr, Vec<TStmt>),
    For { var: Place, from: TExpr, to: TExpr, step: Option<TExpr>, body: Vec<TStmt>, pos: Pos },
    Repeat(Vec<TStmt>, TExpr),
    Call(usize, Vec<TExpr>, Pos),
    Output(Vec<TExpr>),
    Input(Vec<(TTarget, BaseType)>),
    Append(TExpr, TExpr),
    Remove(TExpr, TExpr, Pos),
    Return(Option<TExpr>, Pos),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub name: String,
    pub ty: Type,
    pub shape: Shape,
    pub is_param: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TFunction {
    pub name: String,
    pub n_params: usize,
    /// parameters first, then declared locals; slot k lives in register k
    pub locals: Vec<Slot>,
    pub ret: Option<BaseType>,
    pub body: Vec<TStmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TProgram {
    pub globals: Vec<Slot>,
    /// index 0 is the main program
    pub functions: Vec<TFunction>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use BaseType::*;

    #[test]
    fn join_widens_integer_to_real() {
        assert_eq!(join(Int, Int), Some(Int));
        assert_eq!(join(Int, Real), Some(Real));
        assert_eq!(join(Real, Int), Some(Real));
        assert_eq!(join(Str, Str), Some(Str));
        assert_eq!(join(Str, Int), None);
        assert_eq!(join(Bool, Real), None);
    }

    #[test]
    fn names_for_messages() {
        assert_eq!(Int.name(), "an integer");
        assert_eq!(Str.plural(), "strings");
        assert_eq!(Type::Array(None).name(), "an array");
        assert_eq!(Type::Base(Real).name(), "a real");
        assert_eq!(Type::Base(Bool).base(), Some(Bool));
        assert_eq!(Type::List(Some(Int)).base(), None);
    }
}
