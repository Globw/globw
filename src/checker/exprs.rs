//! Checking expressions: their types, conversions and the hints they give
//! to variables of still-unknown type.

use crate::ast::{BinOp, Expr, ExprKind, Shape, UnOp};
use crate::errors::{Error, Pos};
use crate::lexer::miscased_value_keyword;
use crate::tir::*;

use super::Checker;

/// The placeholder for an expression whose type is not known (yet, or because of an error).
pub(crate) fn unknown(pos: Pos) -> TExpr {
    TExpr { kind: TExprKind::Int(0), ty: Type::Unknown, pos }
}

fn base(kind: TExprKind, t: BaseType, pos: Pos) -> TExpr {
    TExpr { kind, ty: Type::Base(t), pos }
}

/// Converts an integer expression to real when `target` is real.
pub(crate) fn convert(e: TExpr, target: Option<BaseType>) -> TExpr {
    if target != Some(BaseType::Real) || e.ty != Type::Base(BaseType::Int) {
        return e;
    }
    let pos = e.pos;
    if let TExprKind::Int(n) = &e.kind {
        return base(TExprKind::Real(*n as f64), BaseType::Real, pos);
    }
    base(TExprKind::ToReal(Box::new(e)), BaseType::Real, pos)
}

fn to_str(e: TExpr) -> TExpr {
    if e.ty == Type::Base(BaseType::Str) {
        return e;
    }
    let pos = e.pos;
    base(TExprKind::ToStr(Box::new(e)), BaseType::Str, pos)
}

fn numeric(a: BaseType, b: BaseType) -> Option<BaseType> {
    match (a, b) {
        (BaseType::Int, BaseType::Int) => Some(BaseType::Int),
        (BaseType::Int | BaseType::Real, BaseType::Int | BaseType::Real) => Some(BaseType::Real),
        _ => None,
    }
}

fn symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::IntDiv => "div",
        BinOp::Mod => "mod",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::LtEq => "<=",
        BinOp::Gt => ">",
        BinOp::GtEq => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

impl<'p> Checker<'p> {
    pub(crate) fn expr(&mut self, e: &Expr) -> TExpr {
        let pos = e.pos;
        match &e.kind {
            ExprKind::Int(n) => base(TExprKind::Int(*n), BaseType::Int, pos),
            ExprKind::Real(r) => base(TExprKind::Real(*r), BaseType::Real, pos),
            ExprKind::Str(s) => base(TExprKind::Str(s.as_str().into()), BaseType::Str, pos),
            ExprKind::Bool(b) => base(TExprKind::Bool(*b), BaseType::Bool, pos),
            ExprKind::Var(name) => match self.lookup(name) {
                None => {
                    let err = self.undeclared(name, pos);
                    self.report_structural(err);
                    unknown(pos)
                }
                Some(p) => {
                    self.check_assigned(p, name, pos);
                    TExpr { kind: TExprKind::Load(p), ty: self.var(p).full_type(), pos }
                }
            },
            ExprKind::Index(name, indices) => match self.lookup(name) {
                Some(p) if matches!(self.var(p).full_type(), Type::Dict(..) | Type::ListOf(_)) => {
                    let container = TExpr { kind: TExprKind::Load(p), ty: self.var(p).full_type(), pos };
                    self.subscript(container, indices, pos)
                }
                _ => self.index(name, indices, pos),
            },
            ExprKind::Subscript(base, indices) => {
                let container = self.expr(base);
                self.subscript(container, indices, pos)
            }
            ExprKind::ListLit(items) => self.list_literal(items, pos),
            ExprKind::Named(..) => {
                self.report_structural(Error::new(
                    pos,
                    "a value with a name (like size = 3) can only be passed to a function or procedure of the program",
                ));
                unknown(pos)
            }
            ExprKind::Binary(op, a, b) => {
                let a = self.expr(a);
                let b = self.expr(b);
                self.binary(*op, a, b, pos)
            }
            ExprKind::Unary(op, x) => {
                let x = self.expr(x);
                self.unary(*op, x, pos)
            }
            ExprKind::Call(name, args) => self.call_expr(name, args, pos),
        }
    }

    /// `container[key]` where the container is any expression: a list
    /// (nested or not), a dictionary, an array or a string.
    pub(crate) fn subscript(&mut self, container: TExpr, indices: &[Expr], pos: Pos) -> TExpr {
        if indices.len() != 1 {
            self.report(Error::new(pos, "use one index at a time, like a[i][j]"));
            return unknown(pos);
        }
        let key = self.expr(&indices[0]);
        let element = match container.ty {
            Type::Array(Some(b)) => Some(Type::Base(b)),
            t => list_elem(t),
        };
        match (container.ty, element) {
            (Type::Unknown, _) => unknown(pos),
            (Type::Base(BaseType::Str), _) => {
                let k = self.int_expr(key, "the index");
                base(TExprKind::Char(Box::new(container), Box::new(k)), BaseType::Str, pos)
            }
            (_, Some(elem)) => {
                let k = self.int_expr(key, "the index");
                TExpr { kind: TExprKind::Index(Box::new(container), Box::new(k)), ty: elem, pos }
            }
            (Type::Dict(k, v), _) => {
                if !fits(Type::Base(k), key.ty) {
                    self.report(Error::new(key.pos, format!("the key must be {}, here it is {}", k.name(), key.ty.full_name())));
                }
                TExpr { kind: TExprKind::Index(Box::new(container), Box::new(key)), ty: resolve(v), pos }
            }
            (other, _) => {
                self.report(Error::new(
                    pos,
                    format!("[ ] can be used on lists, dictionaries and strings, not on {}", other.full_name()),
                ));
                unknown(pos)
            }
        }
    }

    /// `[a, b, c]`: every element must have the same type (ints and floats mix as floats).
    fn list_literal(&mut self, items: &[Expr], pos: Pos) -> TExpr {
        let texprs: Vec<TExpr> = items.iter().map(|e| self.expr(e)).collect();
        let empty = list_of(Type::Unknown);
        let mut elem: Option<Type> = None;
        for t in &texprs {
            let next = match (elem, t.ty) {
                (_, Type::Unknown) => elem,
                (None, x) => Some(x),
                (Some(Type::Base(a)), Type::Base(b)) => join(a, b).map(Type::Base),
                (Some(a), b) if a == b => Some(a),
                (Some(a), b) if a == empty && list_elem(b).is_some() => Some(b),
                (Some(a), b) if b == empty && list_elem(a).is_some() => Some(a),
                _ => None,
            };
            if next.is_none() {
                self.report(Error::new(pos, "all the elements of a list must have the same type"));
                return unknown(pos);
            }
            elem = next;
        }
        let elem = elem.unwrap_or(Type::Unknown);
        let converted = texprs.into_iter().map(|t| convert(t, elem.base())).collect();
        TExpr { kind: TExprKind::ListLit(converted), ty: list_of(elem), pos }
    }

    pub(crate) fn undeclared(&self, name: &str, pos: Pos) -> Error {
        let e = Error::new(pos, format!("variable \"{}\" is not declared", name));
        if let Some(k) = miscased_value_keyword(name) {
            return e.with_hint(format!("keywords are written in lowercase: write {}", k));
        }
        if self.sub_by_name.contains_key(name) {
            return e.with_hint(format!("\"{}\" is a function: call it with {}(...)", name, name));
        }
        let in_sub = self.instances[self.current].sub.is_some();
        if in_sub && self.prog.locals.iter().any(|d| d.name == name) {
            return e.with_hint(format!(
                "\"{}\" belongs to the main program: to use it here declare it with global variables:",
                name
            ));
        }
        e.with_hint(if in_sub {
            "add it to \"variables:\" at the top of the function"
        } else {
            "add it to \"variables:\" at the top of the program"
        })
    }

    /// Checks that an expression is an integer (hinting it if unknown).
    pub(crate) fn int_expr(&mut self, t: TExpr, what: &str) -> TExpr {
        self.hint(&t, BaseType::Int);
        if t.ty != Type::Base(BaseType::Int) && t.ty != Type::Unknown {
            self.report(Error::new(t.pos, format!("{} must be an int, here it is {}", what, t.ty.name())));
        }
        t
    }

    pub(crate) fn condition(&mut self, e: &Expr) -> TExpr {
        let t = self.expr(e);
        self.hint(&t, BaseType::Bool);
        if t.ty != Type::Base(BaseType::Bool) && t.ty != Type::Unknown {
            self.report(
                Error::new(t.pos, format!("the condition must be true or false, here it is {}", t.ty.name()))
                    .with_hint("to compare values use ==, !=, <, >, <= or >="),
            );
        }
        t
    }

    fn index(&mut self, name: &str, indices: &[Expr], pos: Pos) -> TExpr {
        let Some(p) = self.lookup(name) else {
            let err = self.undeclared(name, pos);
            self.report_structural(err);
            return unknown(pos);
        };
        let idx: Vec<TExpr> = indices
            .iter()
            .map(|i| {
                let t = self.expr(i);
                self.int_expr(t, "the index")
            })
            .collect();
        let v = self.var(p);
        let (shape, elem) = (v.shape.clone(), v.ty);
        let elem_ty = elem.map_or(Type::Unknown, Type::Base);
        match shape {
            Shape::Array(_) | Shape::List if idx.len() == 1 => TExpr { kind: TExprKind::Elem(p, idx), ty: elem_ty, pos },
            Shape::Matrix(..) if idx.len() == 2 => TExpr { kind: TExprKind::Elem(p, idx), ty: elem_ty, pos },
            Shape::Matrix(..) => {
                self.report(Error::new(pos, format!("\"{}\" is a matrix: it needs 2 indices, like {}[i, j]", name, name)));
                unknown(pos)
            }
            Shape::Array(_) | Shape::List | Shape::Dict => {
                self.report(Error::new(pos, format!("\"{}\" needs exactly 1 index, like {}[i]", name, name)));
                unknown(pos)
            }
            Shape::Scalar => {
                if elem.is_none() {
                    // only strings can be indexed, so an unknown variable must be one
                    self.widen(p, BaseType::Str, pos);
                } else if elem != Some(BaseType::Str) {
                    self.report(Error::new(pos, format!("\"{}\" is not an array: [ ] cannot be used on it", name)));
                    return unknown(pos);
                }
                if idx.len() != 1 {
                    self.report(Error::new(pos, format!("a string needs exactly 1 index, like {}[i]", name)));
                    return unknown(pos);
                }
                self.check_assigned(p, name, pos);
                let s = TExpr { kind: TExprKind::Load(p), ty: Type::Base(BaseType::Str), pos };
                let i = idx.into_iter().next().expect("one index");
                base(TExprKind::Char(Box::new(s), Box::new(i)), BaseType::Str, pos)
            }
        }
    }

    fn binary(&mut self, op: BinOp, a: TExpr, b: TExpr, pos: Pos) -> TExpr {
        use BaseType::*;
        use BinOp::*;
        match op {
            And | Or => {
                self.hint(&a, Bool);
                self.hint(&b, Bool);
            }
            IntDiv | Mod => {
                self.hint(&a, Int);
                self.hint(&b, Int);
            }
            Sub | Mul | Div => {
                // numeric-only operators: an unknown operand is at least an integer
                let t = b.ty.base().or(a.ty.base()).unwrap_or(Int);
                self.hint(&a, t);
                self.hint(&b, t);
            }
            Add if a.ty == Type::Base(Str) || b.ty == Type::Base(Str) => {}
            _ => {
                if let Some(t) = b.ty.base() {
                    self.hint(&a, t);
                }
                if let Some(t) = a.ty.base() {
                    self.hint(&b, t);
                }
            }
        }
        let (ta, tb) = match (a.ty, b.ty) {
            (Type::Base(x), Type::Base(y)) => (x, y),
            (Type::Unknown, _) | (_, Type::Unknown) => return unknown(pos),
            (x, y) => {
                let collection = if x.base().is_none() { x } else { y };
                self.report(
                    Error::new(pos, format!("cannot use {} on {}", symbol(op), collection.name()))
                        .with_hint("use one element, like v[i]"),
                );
                return unknown(pos);
            }
        };
        let typed = |k: BinOp, operands: BaseType, a: TExpr, b: TExpr, result: BaseType| {
            base(TExprKind::Binary(k, operands, Box::new(a), Box::new(b)), result, pos)
        };
        let mismatch = format!("cannot use {} between {} and {}", symbol(op), ta.name(), tb.name());
        match op {
            Add if ta == Str || tb == Str => typed(Add, Str, to_str(a), to_str(b), Str),
            Add | Sub | Mul => match numeric(ta, tb) {
                Some(t) => typed(op, t, convert(a, Some(t)), convert(b, Some(t)), t),
                None => {
                    self.report(Error::new(pos, mismatch));
                    unknown(pos)
                }
            },
            Div => match numeric(ta, tb) {
                Some(_) => typed(Div, Real, convert(a, Some(Real)), convert(b, Some(Real)), Real),
                None => {
                    self.report(Error::new(pos, mismatch));
                    unknown(pos)
                }
            },
            IntDiv | Mod => {
                if ta == Int && tb == Int {
                    typed(op, Int, a, b, Int)
                } else {
                    self.report(Error::new(pos, "div and mod only work with integers"));
                    unknown(pos)
                }
            }
            Eq | NotEq | Lt | LtEq | Gt | GtEq => {
                let ordered = !matches!(op, Eq | NotEq);
                match numeric(ta, tb) {
                    Some(t) => typed(op, t, convert(a, Some(t)), convert(b, Some(t)), Bool),
                    None if ta == tb && (!ordered || ta == Str) => typed(op, ta, a, b, Bool),
                    None => {
                        self.report(Error::new(pos, format!("cannot compare {} with {}", ta.name(), tb.name())));
                        unknown(pos)
                    }
                }
            }
            And | Or => {
                if ta == Bool && tb == Bool {
                    typed(op, Bool, a, b, Bool)
                } else {
                    self.report(Error::new(pos, format!("{} needs conditions (true/false) on both sides", symbol(op))));
                    unknown(pos)
                }
            }
        }
    }

    fn unary(&mut self, op: UnOp, x: TExpr, pos: Pos) -> TExpr {
        match op {
            UnOp::Neg => {
                self.hint(&x, BaseType::Int);
                // fold -5 and -2.5 into literals (only real literals: the
                // `unknown` placeholder is also an Int(0), but of unknown type)
                if matches!(x.ty, Type::Base(_)) {
                    match &x.kind {
                        TExprKind::Int(n) => return base(TExprKind::Int(-n), BaseType::Int, pos),
                        TExprKind::Real(r) => return base(TExprKind::Real(-r), BaseType::Real, pos),
                        _ => {}
                    }
                }
                match x.ty {
                    Type::Base(t @ (BaseType::Int | BaseType::Real)) => base(TExprKind::Neg(t, Box::new(x)), t, pos),
                    Type::Unknown => unknown(pos),
                    other => {
                        self.report(Error::new(pos, format!("cannot use - on {}", other.name())));
                        unknown(pos)
                    }
                }
            }
            UnOp::Not => {
                self.hint(&x, BaseType::Bool);
                match x.ty {
                    Type::Base(BaseType::Bool) => base(TExprKind::Not(Box::new(x)), BaseType::Bool, pos),
                    Type::Unknown => unknown(pos),
                    other => {
                        self.report(Error::new(pos, format!("not needs a condition (true/false), here it is {}", other.name())));
                        unknown(pos)
                    }
                }
            }
        }
    }

    fn call_expr(&mut self, name: &str, args: &[Expr], pos: Pos) -> TExpr {
        match name {
            "length" => {
                if args.len() != 1 {
                    self.report_structural(Error::new(pos, format!("length takes 1 value, here it gets {}", args.len())));
                    return unknown(pos);
                }
                let x = self.expr(&args[0]);
                match x.ty {
                    Type::Unknown => {
                        self.hint(&x, BaseType::Str);
                        unknown(pos)
                    }
                    Type::Array(_) | Type::List(_) | Type::ListOf(_) | Type::Dict(..) | Type::Base(BaseType::Str) => {
                        base(TExprKind::Length(Box::new(x)), BaseType::Int, pos)
                    }
                    Type::Matrix(_) => {
                        self.report(
                            Error::new(pos, "length cannot be used on a matrix")
                                .with_hint("use the sizes written in \"variables:\""),
                        );
                        unknown(pos)
                    }
                    other => {
                        self.report(Error::new(
                            pos,
                            format!("length works with arrays, lists and strings, not with {}", other.name()),
                        ));
                        unknown(pos)
                    }
                }
            }
            "input" | "output" | "append" | "remove" => {
                self.report_structural(Error::new(
                    pos,
                    format!("{} does not return a value: use it as a statement on its own line", name),
                ));
                unknown(pos)
            }
            _ => match crate::builtins::lookup(name) {
                Some(b) => self.builtin_call(b, args, pos, true),
                None => self.call(name, args, pos, true),
            },
        }
    }
}
