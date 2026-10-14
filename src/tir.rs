//! Typed intermediate representation: produced by the checker, consumed by the compiler.

use std::rc::Rc;

use crate::ast::{BinOp, Shape};
use crate::builtins::Builtin;
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
            BaseType::Int => "an int",
            BaseType::Real => "a float",
            BaseType::Str => "a string",
            BaseType::Bool => "a bool",
        }
    }

    pub fn plural(self) -> &'static str {
        match self {
            BaseType::Int => "ints",
            BaseType::Real => "floats",
            BaseType::Str => "strings",
            BaseType::Bool => "bools",
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
    /// a list of simple values
    List(Option<BaseType>),
    /// a list whose elements are lists or dictionaries (made with `list_of`)
    ListOf(TypeId),
    /// a dictionary from simple keys to values of any type (made with `dict_of`)
    Dict(BaseType, TypeId),
}

/// A type stored in the type table, so that `Type` stays small and `Copy`
/// even when types nest (`list of list of int`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(u32);

thread_local! {
    static TYPES: std::cell::RefCell<Vec<Type>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The id of `t` in the type table; equal types always get the same id.
pub fn intern(t: Type) -> TypeId {
    TYPES.with(|table| {
        let mut table = table.borrow_mut();
        let i = table.iter().position(|&x| x == t).unwrap_or_else(|| {
            table.push(t);
            table.len() - 1
        });
        TypeId(i as u32)
    })
}

pub fn resolve(id: TypeId) -> Type {
    TYPES.with(|table| table.borrow()[id.0 as usize])
}

/// The list type with elements of type `elem` (simple elements keep the `List` form).
pub fn list_of(elem: Type) -> Type {
    match elem {
        Type::Base(b) => Type::List(Some(b)),
        other => Type::ListOf(intern(other)),
    }
}

/// The element type of a list.
pub fn list_elem(t: Type) -> Option<Type> {
    match t {
        Type::List(Some(b)) => Some(Type::Base(b)),
        Type::ListOf(id) => Some(resolve(id)),
        _ => None,
    }
}

pub fn dict_of(key: BaseType, value: Type) -> Type {
    Type::Dict(key, intern(value))
}

/// Key and value types of a dictionary.
pub fn dict_parts(t: Type) -> Option<(BaseType, Type)> {
    match t {
        Type::Dict(k, v) => Some((k, resolve(v))),
        _ => None,
    }
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
            Type::List(_) | Type::ListOf(_) => "a list",
            Type::Dict(..) => "a dictionary",
        }
    }

    /// Like `name`, but collections also say their element type: "an array of ints".
    pub fn full_name(self) -> String {
        match self {
            Type::Array(Some(b)) => format!("an array of {}", b.plural()),
            Type::Matrix(Some(b)) => format!("a matrix of {}", b.plural()),
            Type::List(Some(_)) | Type::ListOf(_) => format!("a list of {}", list_elem(self).map_or("values".into(), Type::plural)),
            Type::Dict(k, v) => format!("a dictionary from {} to {}", k.plural(), resolve(v).plural()),
            other => other.name().to_string(),
        }
    }

    /// "ints", "lists of strings"... for messages about elements.
    pub fn plural(self) -> String {
        match self {
            Type::Base(b) => b.plural().to_string(),
            Type::List(Some(_)) | Type::ListOf(_) => format!("lists of {}", list_elem(self).map_or("values".into(), Type::plural)),
            Type::Dict(k, v) => format!("dictionaries from {} to {}", k.plural(), resolve(v).plural()),
            Type::Array(_) => "arrays".to_string(),
            Type::Matrix(_) => "matrices".to_string(),
            _ => "values".to_string(),
        }
    }
}

/// Whether a value of type `actual` can be stored where `expected` is
/// declared: equal types, an int where a float goes, or an empty list `[]`
/// where any list goes. Unknown types (after an error) always fit.
pub fn fits(expected: Type, actual: Type) -> bool {
    match (expected, actual) {
        (_, Type::Unknown) | (Type::Unknown, _) => true,
        (Type::Base(e), Type::Base(a)) => join(e, a) == Some(e),
        (e, a) if list_elem(e).is_some() && list_elem(a) == Some(Type::Unknown) => true,
        (e, a) => e == a,
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
    /// a built-in function (graphics, dialogs, RGB, RANDOM, TIME)
    Builtin(Builtin, Vec<TExpr>),
    /// an element of a list or a dictionary value: `container[key]`
    Index(Box<TExpr>, Box<TExpr>),
    /// a list written in brackets
    ListLit(Vec<TExpr>),
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
    /// a built-in procedure (WINDOW, SHOW, RECT, MESSAGE...)
    Builtin(Builtin, Vec<TExpr>, Pos),
    /// `container[key] = value` for lists and dictionaries reached through any expression
    SetIndex(TExpr, TExpr, TExpr, Pos),
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
        assert_eq!(Int.name(), "an int");
        assert_eq!(Type::Array(Some(Real)).full_name(), "an array of floats");
        assert_eq!(Str.plural(), "strings");
        assert_eq!(Type::Array(None).name(), "an array");
        assert_eq!(Type::Base(Real).name(), "a float");
        assert_eq!(Type::Base(Bool).base(), Some(Bool));
        assert_eq!(Type::List(Some(Int)).base(), None);
    }

    #[test]
    fn nested_types() {
        let ints = list_of(Type::Base(Int));
        assert_eq!(ints, Type::List(Some(Int)), "lists of simple values keep their simple form");
        let grid = list_of(ints);
        assert_eq!(grid, list_of(list_of(Type::Base(Int))), "the same structure gives the same type");
        assert_eq!(grid.full_name(), "a list of lists of ints");
        assert_eq!(list_elem(grid), Some(ints));
        let d = dict_of(Str, grid);
        assert_eq!(d.full_name(), "a dictionary from strings to lists of lists of ints");
        assert_eq!(d.name(), "a dictionary");
        assert_eq!(dict_parts(d), Some((Str, grid)));
    }
}
