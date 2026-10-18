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
    /// a class name written in a declaration, before it is linked to a class
    /// (resolved to `Object` by the class loader)
    Named(u32),
    /// an object of the class with this id
    Object(u16),
    /// a function value with the given signature (made with `func_type`)
    Func(u32),
    /// the type of the `none` literal (fits any object or function type)
    None,
    /// a set of simple values (int or string)
    Set(BaseType),
    /// a tuple (a fixed group of possibly different types), made with `tuple_type`
    Tuple(u32),
    /// a fast contiguous array of floats (the `grid` engine); at run time an int handle
    Grid,
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

thread_local! {
    static NAMED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    static CLASSES: std::cell::RefCell<Vec<ClassInfo>> = const { std::cell::RefCell::new(Vec::new()) };
    static METHOD_SLOTS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    static METHOD_TARGETS: std::cell::RefCell<Vec<i32>> = const { std::cell::RefCell::new(Vec::new()) };
    static FUNCS: std::cell::RefCell<Vec<(Vec<Type>, Option<Type>)>> = const { std::cell::RefCell::new(Vec::new()) };
    static TUPLES: std::cell::RefCell<Vec<Vec<Type>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The tuple type with the given element types; equal tuples get the same id.
pub fn tuple_type(elems: Vec<Type>) -> Type {
    TUPLES.with(|t| {
        let mut t = t.borrow_mut();
        let i = t.iter().position(|e| *e == elems).unwrap_or_else(|| {
            t.push(elems);
            t.len() - 1
        });
        Type::Tuple(i as u32)
    })
}

pub fn tuple_parts(t: Type) -> Option<Vec<Type>> {
    match t {
        Type::Tuple(id) => Some(TUPLES.with(|t| t.borrow()[id as usize].clone())),
        _ => None,
    }
}

/// The type of a function value with the given parameter types and return
/// type (`None` for a procedure); equal signatures get the same id.
pub fn func_type(params: Vec<Type>, ret: Option<Type>) -> Type {
    FUNCS.with(|t| {
        let mut t = t.borrow_mut();
        let i = t.iter().position(|(p, r)| *p == params && *r == ret).unwrap_or_else(|| {
            t.push((params, ret));
            t.len() - 1
        });
        Type::Func(i as u32)
    })
}

pub fn func_parts(t: Type) -> Option<(Vec<Type>, Option<Type>)> {
    match t {
        Type::Func(id) => Some(FUNCS.with(|t| t.borrow()[id as usize].clone())),
        _ => None,
    }
}

/// Records, for each subprogram index, the index of its compiled function
/// (or -1). The vtable uses it to turn method subprograms into call targets.
pub fn set_method_targets(targets: Vec<i32>) {
    METHOD_TARGETS.with(|t| *t.borrow_mut() = targets);
}

fn method_target(sub: usize) -> i32 {
    METHOD_TARGETS.with(|t| t.borrow().get(sub).copied().unwrap_or(-1))
}

/// The dispatch slot for a method name, assigning a new one if needed. All
/// methods with the same name share a slot, so dispatch is an array lookup.
pub fn method_slot(name: &str) -> usize {
    METHOD_SLOTS.with(|t| {
        let mut t = t.borrow_mut();
        t.iter().position(|n| n == name).unwrap_or_else(|| {
            t.push(name.to_string());
            t.len() - 1
        })
    })
}

pub fn method_slot_count() -> usize {
    METHOD_SLOTS.with(|t| t.borrow().len())
}

/// A class as the checker and compiler see it.
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    /// the parent class, if any (its fields come first, at the same slots)
    pub parent: Option<u16>,
    /// fields in slot order (inherited first), each with its (resolved) type
    pub fields: Vec<(String, Type)>,
    /// method name -> subprogram index, inherited methods included, overrides resolved
    pub methods: Vec<(String, usize)>,
}

/// Interns a class name written in a declaration, giving it a stable id.
pub fn intern_name(name: &str) -> u32 {
    NAMED.with(|t| {
        let mut t = t.borrow_mut();
        let i = t.iter().position(|n| n == name).unwrap_or_else(|| {
            t.push(name.to_string());
            t.len() - 1
        });
        i as u32
    })
}

pub fn named_name(id: u32) -> String {
    NAMED.with(|t| t.borrow()[id as usize].clone())
}

/// Replaces the class table (called once by the class loader, before checking).
pub fn set_classes(list: Vec<ClassInfo>) {
    CLASSES.with(|t| *t.borrow_mut() = list);
}

pub fn class_count() -> usize {
    CLASSES.with(|t| t.borrow().len())
}

/// The dispatch row of a class: for each method slot, the subprogram index of
/// its implementation, or -1 if the class has no method in that slot.
pub fn class_vtable(id: u16) -> Vec<i32> {
    let n = method_slot_count();
    let mut row = vec![-1i32; n];
    let methods: Vec<(String, usize)> = CLASSES.with(|t| t.borrow()[id as usize].methods.clone());
    for (name, f) in methods {
        row[method_slot(&name)] = method_target(f);
    }
    row
}

pub fn class_name(id: u16) -> String {
    CLASSES.with(|t| t.borrow()[id as usize].name.clone())
}

pub fn class_id(name: &str) -> Option<u16> {
    CLASSES.with(|t| t.borrow().iter().position(|c| c.name == name).map(|i| i as u16))
}

pub fn class_parent(id: u16) -> Option<u16> {
    CLASSES.with(|t| t.borrow()[id as usize].parent)
}

/// Whether class `sub` is `sup` or inherits from it.
pub fn is_subclass(sub: u16, sup: u16) -> bool {
    let mut cur = Some(sub);
    while let Some(c) = cur {
        if c == sup {
            return true;
        }
        cur = class_parent(c);
    }
    false
}

/// The (slot, type) of a field of a class, or None if it has no such field.
pub fn class_field(id: u16, field: &str) -> Option<(usize, Type)> {
    CLASSES.with(|t| {
        let c = &t.borrow()[id as usize];
        c.fields.iter().position(|(n, _)| n == field).map(|i| (i, c.fields[i].1))
    })
}

pub fn class_field_count(id: u16) -> usize {
    CLASSES.with(|t| t.borrow()[id as usize].fields.len())
}

pub fn class_field_names(id: u16) -> Vec<String> {
    CLASSES.with(|t| t.borrow()[id as usize].fields.iter().map(|(n, _)| n.clone()).collect())
}

/// For each field, how a fresh object must initialize it: 1 = empty list,
/// 2 = empty dictionary, 0 = no value yet.
pub fn class_field_kinds(id: u16) -> Vec<u8> {
    CLASSES.with(|t| {
        t.borrow()[id as usize]
            .fields
            .iter()
            .map(|(_, ty)| match ty {
                Type::List(_) | Type::ListOf(_) => 1,
                Type::Dict(..) => 2,
                Type::Base(BaseType::Int) => 3,
                Type::Base(BaseType::Real) => 4,
                Type::Base(BaseType::Bool) => 5,
                Type::Base(BaseType::Str) => 6,
                _ => 0,
            })
            .collect()
    })
}

/// The subprogram index of a method of a class, or None.
pub fn class_method(id: u16, method: &str) -> Option<usize> {
    CLASSES.with(|t| {
        let c = &t.borrow()[id as usize];
        c.methods.iter().find(|(n, _)| n == method).map(|(_, i)| *i)
    })
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
            Type::Named(_) | Type::Object(_) => "an object",
            Type::Func(_) => "a function",
            Type::None => "none",
            Type::Set(_) => "a set",
            Type::Tuple(_) => "a tuple",
            Type::Grid => "a grid",
        }
    }

    /// Like `name`, but collections also say their element type: "an array of ints".
    pub fn full_name(self) -> String {
        match self {
            Type::Array(Some(b)) => format!("an array of {}", b.plural()),
            Type::Matrix(Some(b)) => format!("a matrix of {}", b.plural()),
            Type::List(Some(_)) | Type::ListOf(_) => format!("a list of {}", list_elem(self).map_or("values".into(), Type::plural)),
            Type::Dict(k, v) => format!("a dictionary from {} to {}", k.plural(), resolve(v).plural()),
            Type::Set(e) => format!("a set of {}", e.plural()),
            Type::Tuple(_) => {
                let parts = tuple_parts(self).expect("a tuple");
                format!("a tuple of ({})", parts.iter().map(|p| p.full_name()).collect::<Vec<_>>().join(", "))
            }
            Type::Object(id) => format!("a {}", class_name(id)),
            Type::Named(id) => format!("a {}", named_name(id)),
            Type::Func(_) => {
                let (params, ret) = func_parts(self).expect("a function type");
                let ps = params.iter().map(|p| p.full_name()).collect::<Vec<_>>().join(", ");
                match ret {
                    Some(r) => format!("a function({}) as {}", ps, r.full_name()),
                    None => format!("a function({})", ps),
                }
            }
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
            Type::Object(id) => format!("{} objects", class_name(id)),
            Type::Grid => "grids".to_string(),
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
        (Type::Object(e), Type::Object(a)) => is_subclass(a, e),
        (Type::Func(_), Type::Func(_)) => expected == actual,
        (Type::Object(_) | Type::Func(_), Type::None) => true,
        (Type::Tuple(_), Type::Tuple(_)) => match (tuple_parts(expected), tuple_parts(actual)) {
            (Some(e), Some(a)) => e.len() == a.len() && e.iter().zip(&a).all(|(x, y)| fits(*x, *y)),
            _ => false,
        },
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
    /// makes an object of a class and runs its constructor (if any)
    New { class: u16, ctor: Option<usize>, args: Vec<TExpr> },
    /// the value of a field of an object: `object.field`
    GetField(Box<TExpr>, usize),
    /// a virtual method call dispatched on the runtime class of `args[0]` (self)
    VCall { slot: usize, args: Vec<TExpr> },
    /// `object is Class`: true when the object's class is that class or a subclass
    IsType(Box<TExpr>, u16),
    /// the `none` value (an absent reference)
    NoneVal,
    /// true when the expression is `none`
    IsNone(Box<TExpr>),
    /// a tuple built from its elements
    TupleLit(Vec<TExpr>),
    /// one element of a tuple (0-based slot), fixed at compile time
    TupleGet(Box<TExpr>, usize),
    /// a function value: the compiled function index
    FuncVal(usize),
    /// a call through a function value
    CallVal { func: Box<TExpr>, args: Vec<TExpr> },
    /// a higher-order list operation (map/filter/reduce) with a function value
    Hof { op: HofOp, list: Box<TExpr>, func: Box<TExpr>, init: Option<Box<TExpr>> },
}

/// The higher-order list operations expanded inline by the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HofOp {
    Transform,
    Keep,
    Reduce,
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
    /// a virtual method call used as a statement (procedure method)
    VCall(usize, Vec<TExpr>, Pos),
    /// a call through a function value, used as a statement (procedure value)
    CallVal(TExpr, Vec<TExpr>, Pos),
    Output(Vec<TExpr>),
    Input(Vec<(TTarget, BaseType)>),
    Append(TExpr, TExpr),
    Remove(TExpr, TExpr, Pos),
    Return(Option<TExpr>, Pos),
    /// a built-in procedure (WINDOW, SHOW, RECT, MESSAGE...)
    Builtin(Builtin, Vec<TExpr>, Pos),
    /// `container[key] = value` for lists and dictionaries reached through any expression
    SetIndex(TExpr, TExpr, TExpr, Pos),
    /// `object.field = value`
    SetField(TExpr, usize, TExpr, Pos),
    /// `try` body, catch block (with the error message stored in `err`), and
    /// an optional `always` block
    Try { body: Vec<TStmt>, err: Place, catch: Vec<TStmt>, always: Vec<TStmt>, pos: Pos },
    /// `fail message`: raises a run-time error
    Fail(TExpr, Pos),
    /// `a, b = tuple`: evaluate the tuple once, store each element in a place
    Unpack(Vec<Place>, TExpr, Pos),
    /// runs function `func` (a module's start-up code) unless global `flag` is true
    LoadModule { flag: u16, func: usize, pos: Pos },
    /// `for(x in c)`; `to_real` converts int elements for a float variable
    ForEach { var: Place, iter: TExpr, kind: IterKind, to_real: bool, body: Vec<TStmt>, pos: Pos },
    Break(Pos),
    Continue(Pos),
}

/// What a `for ... in` goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterKind {
    /// the elements of a list or an array
    Items,
    /// the keys of a dictionary
    Keys,
    /// the elements of a set
    Set,
    /// the characters of a string
    Chars,
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
    pub ret: Option<Type>,
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
