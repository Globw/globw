//! Checking expressions: their types, conversions and the hints they give
//! to variables of still-unknown type.

use crate::ast::{BinOp, Expr, ExprKind, Shape, UnOp};
use crate::builtins::Builtin;
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

/// Folds `+`, `-`, `*` between two numeric literals into a single literal at compile
/// time, so a constant expression like `(5 + 2) * (5 - 4)` costs nothing at run time
/// (it collapses to `7`). Integer overflow is left to the run time, so the result is
/// always identical to evaluating the operation normally.
fn fold_const(op: BinOp, a: &TExpr, b: &TExpr, pos: Pos) -> Option<TExpr> {
    use BinOp::*;
    let int = |k: &TExprKind| if let TExprKind::Int(n) = k { Some(*n) } else { None };
    let real = |k: &TExprKind| match k {
        TExprKind::Int(n) => Some(*n as f64),
        TExprKind::Real(r) => Some(*r),
        _ => None,
    };
    if let (Some(x), Some(y)) = (int(&a.kind), int(&b.kind)) {
        let r = match op {
            Add => x.checked_add(y),
            Sub => x.checked_sub(y),
            Mul => x.checked_mul(y),
            _ => return None,
        }?;
        return Some(base(TExprKind::Int(r), BaseType::Int, pos));
    }
    let (x, y) = (real(&a.kind)?, real(&b.kind)?);
    let r = match op {
        Add => x + y,
        Sub => x - y,
        Mul => x * y,
        _ => return None,
    };
    Some(base(TExprKind::Real(r), BaseType::Real, pos))
}

// ---- grid expression fusion -------------------------------------------------
// A tree of element-wise grid builtins (a+b, a*k, sqrt(a), ...) is collapsed
// into a single `grid.eval` call carrying an RPN program, so the whole
// expression runs in one parallel pass with no temporary grids.

#[derive(Default)]
struct Fuser {
    leaves: Vec<TExpr>,
    scalars: Vec<TExpr>,
    prog: Vec<String>,
}

impl Fuser {
    fn leaf(&mut self, e: TExpr) {
        // the same grid used twice (sqrt(a) and a/3) becomes one leaf, read once
        let k = self.leaves.iter().position(|x| *x == e).unwrap_or_else(|| {
            self.leaves.push(e);
            self.leaves.len() - 1
        });
        self.prog.push(format!("l{}", k));
    }
    fn scalar(&mut self, e: TExpr) {
        self.prog.push(format!("s{}", self.scalars.len()));
        self.scalars.push(e);
    }
    fn walk(&mut self, e: TExpr) {
        use Builtin::*;
        let TExprKind::Builtin(b, mut args) = e.kind else {
            self.leaf(e);
            return;
        };
        match b {
            GridAdd | GridSub | GridMul | GridDiv => {
                let rhs = args.pop().expect("binary");
                let lhs = args.pop().expect("binary");
                self.walk(lhs);
                self.walk(rhs);
                self.prog.push(matches!(b, GridAdd).then(|| "+").or_else(|| matches!(b, GridSub).then(|| "-")).or_else(|| matches!(b, GridMul).then(|| "*")).unwrap_or("/").to_string());
            }
            GridScale | GridOffset | GridSubScalar | GridDivScalar => {
                let s = args.pop().expect("scalar");
                let g = args.pop().expect("grid");
                self.walk(g);
                self.scalar(s);
                self.prog.push(match b {
                    GridScale => "*",
                    GridOffset => "+",
                    GridSubScalar => "-",
                    _ => "/",
                }.to_string());
            }
            GridScalarSub | GridScalarDiv => {
                let s = args.pop().expect("scalar");
                let g = args.pop().expect("grid");
                self.scalar(s);
                self.walk(g);
                self.prog.push(if b == GridScalarSub { "-" } else { "/" }.to_string());
            }
            GridSqrt | GridSquare | GridAbs => {
                let g = args.pop().expect("grid");
                self.walk(g);
                self.prog.push(match b {
                    GridSqrt => "q",
                    GridSquare => "sq",
                    _ => "ab",
                }.to_string());
            }
            _ => self.leaf(TExpr { kind: TExprKind::Builtin(b, args), ty: e.ty, pos: e.pos }),
        }
    }
}

fn is_fuseable_root(e: &TExpr) -> bool {
    use Builtin::*;
    matches!(&e.kind, TExprKind::Builtin(b, _) if matches!(b,
        GridAdd | GridSub | GridMul | GridDiv | GridScale | GridOffset | GridSubScalar
        | GridScalarSub | GridDivScalar | GridScalarDiv | GridSqrt | GridSquare | GridAbs))
}

/// Collapses an element-wise grid expression into one fused `grid.eval` call.
pub(crate) fn fuse(e: TExpr) -> TExpr {
    if e.ty != Type::Grid || !is_fuseable_root(&e) {
        return e;
    }
    let pos = e.pos;
    let mut f = Fuser::default();
    f.walk(e);
    let mut args = Vec::with_capacity(1 + f.leaves.len() + f.scalars.len());
    args.push(TExpr { kind: TExprKind::Str(f.prog.join(" ").into()), ty: Type::Base(BaseType::Str), pos });
    args.extend(f.leaves);
    args.extend(f.scalars);
    TExpr { kind: TExprKind::Builtin(Builtin::GridEval, args), ty: Type::Grid, pos }
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
                Some(p) => {
                    self.check_assigned(p, name, pos);
                    TExpr { kind: TExprKind::Load(p), ty: self.var(p).full_type(), pos }
                }
                None if self.sub_by_name.contains_key(name) => self.func_value(name, pos),
                None if name.contains('.') => self.field_chain(name, pos),
                None => {
                    let err = self.undeclared(name, pos);
                    self.report_structural(err);
                    unknown(pos)
                }
            },
            ExprKind::Index(name, indices) => match self.lookup(name) {
                Some(p) if matches!(self.var(p).full_type(), Type::Dict(..) | Type::ListOf(_)) => {
                    let container = TExpr { kind: TExprKind::Load(p), ty: self.var(p).full_type(), pos };
                    self.subscript(container, indices, pos)
                }
                None if name.contains('.') => {
                    let container = self.field_chain(name, pos);
                    self.subscript(container, indices, pos)
                }
                _ => self.index(name, indices, pos),
            },
            ExprKind::Subscript(base, indices) => {
                let container = self.expr(base);
                self.subscript(container, indices, pos)
            }
            ExprKind::ListLit(items) => self.list_literal(items, pos),
            ExprKind::Comprehension { item, var, var_pos, iter, cond } => {
                self.comprehension(item, var, *var_pos, iter, cond.as_deref(), pos)
            }
            ExprKind::New(class, args) => self.construct(class, args, pos),
            ExprKind::Field(obj, field) => {
                let object = self.expr(obj);
                self.get_field(object, field, pos)
            }
            ExprKind::MethodCall(obj, method, args) => self.method_on(obj, method, args, pos, true),
            ExprKind::FuncRef(name) => self.func_value(name, pos),
            ExprKind::NoneLit => TExpr { kind: TExprKind::NoneVal, ty: Type::None, pos },
            ExprKind::TupleLit(items) => {
                let ts: Vec<TExpr> = items.iter().map(|e| self.expr(e)).collect();
                if ts.iter().any(|t| t.ty == Type::Unknown) {
                    return unknown(pos);
                }
                let ty = crate::tir::tuple_type(ts.iter().map(|t| t.ty).collect());
                TExpr { kind: TExprKind::TupleLit(ts), ty, pos }
            }
            ExprKind::TupleGet(e, n) => {
                let t = self.expr(e);
                match crate::tir::tuple_parts(t.ty) {
                    Some(parts) if *n >= 1 && *n <= parts.len() => {
                        TExpr { kind: TExprKind::TupleGet(Box::new(t), *n), ty: parts[*n - 1], pos }
                    }
                    Some(parts) => {
                        self.report(Error::new(pos, format!("this tuple has {} parts, there is no .{}", parts.len(), n)));
                        unknown(pos)
                    }
                    None if t.ty == Type::Unknown => unknown(pos),
                    None => {
                        self.report(Error::new(pos, format!(".{} works only on a tuple, here it is {}", n, t.ty.full_name())));
                        unknown(pos)
                    }
                }
            }
            ExprKind::IsType(obj, class) => {
                let object = self.expr(obj);
                if !matches!(object.ty, Type::Object(_) | Type::Unknown) {
                    self.report(Error::new(pos, format!("\"is\" tests an object; {} is not one", object.ty.full_name())));
                }
                match crate::tir::class_id(class) {
                    Some(cid) => base(TExprKind::IsType(Box::new(object), cid), BaseType::Bool, pos),
                    None => {
                        self.report_structural(Error::new(pos, format!("unknown class \"{}\"", class)));
                        base(TExprKind::Bool(false), BaseType::Bool, pos)
                    }
                }
            }
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

    /// `[item for var in iter if cond]`: like a `for ... in` loop, but it builds a
    /// list of the `item` values. The variable must be declared, as in a for-each.
    fn comprehension(&mut self, item: &Expr, var: &str, var_pos: Pos, iter: &Expr, cond: Option<&Expr>, pos: Pos) -> TExpr {
        let it = self.expr(iter);
        let (elem, kind) = match it.ty {
            Type::Unknown => (Type::Unknown, IterKind::Items),
            Type::Base(BaseType::Str) => (Type::Base(BaseType::Str), IterKind::Chars),
            Type::Array(Some(b)) => (Type::Base(b), IterKind::Items),
            Type::Dict(k, _) => (Type::Base(k), IterKind::Keys),
            Type::Set(e) => (Type::Base(e), IterKind::Set),
            t => match list_elem(t) {
                Some(e) => (e, IterKind::Items),
                None => {
                    self.report(Error::new(
                        it.pos,
                        format!("a comprehension iterates over lists, arrays, dictionaries and strings, not over {}", t.full_name()),
                    ));
                    (Type::Unknown, IterKind::Items)
                }
            },
        };
        let Some(p) = self.lookup(var) else {
            let err = self.undeclared(var, var_pos);
            self.report_structural(err);
            return unknown(pos);
        };
        let declared = self.var(p).full_type();
        if !fits(declared, elem) {
            let what = if kind == IterKind::Keys { "the keys" } else { "the elements" };
            self.report(Error::new(var_pos, format!("\"{}\" is {}, but {} are {}", var, declared.full_name(), what, elem.plural())));
        }
        let to_real = declared == Type::Base(BaseType::Real) && elem == Type::Base(BaseType::Int);
        let before = self.assigned.clone();
        if let Place::Local(i) = p {
            self.assigned[i as usize] = true;
        }
        let item_t = self.expr(item);
        let cond_t = cond.map(|c| {
            let ct = self.expr(c);
            if !matches!(ct.ty, Type::Base(BaseType::Bool) | Type::Unknown) {
                self.report(Error::new(ct.pos, format!("the condition in a comprehension must be true or false, here it is {}", ct.ty.full_name())));
            }
            Box::new(ct)
        });
        self.assigned = before;
        let result_ty = list_of(item_t.ty);
        TExpr {
            kind: TExprKind::Comprehension { item: Box::new(item_t), var: p, iter: Box::new(it), kind, to_real, cond: cond_t },
            ty: result_ty,
            pos,
        }
    }

    /// Reads `base.field.field...` where `base` is a variable and the rest are
    /// object fields.
    pub(crate) fn field_chain(&mut self, name: &str, pos: Pos) -> TExpr {
        let mut segs = name.split('.');
        let base = segs.next().expect("a name has at least one segment");
        let Some(p) = self.lookup(base) else {
            let err = self.undeclared(base, pos);
            self.report_structural(err);
            return unknown(pos);
        };
        self.check_assigned(p, base, pos);
        let mut cur = TExpr { kind: TExprKind::Load(p), ty: self.var(p).full_type(), pos };
        for field in segs {
            cur = self.get_field(cur, field, pos);
        }
        cur
    }

    /// Reads one field of an object expression.
    pub(crate) fn get_field(&mut self, object: TExpr, field: &str, pos: Pos) -> TExpr {
        match object.ty {
            Type::Unknown => unknown(pos),
            Type::Object(cid) => match crate::tir::class_field(cid, field) {
                Some((slot, ty)) => TExpr { kind: TExprKind::GetField(Box::new(object), slot), ty, pos },
                None => {
                    self.report(Error::new(
                        pos,
                        format!("class \"{}\" has no field \"{}\"", crate::tir::class_name(cid), field),
                    ));
                    unknown(pos)
                }
            },
            other => {
                self.report(Error::new(pos, format!("only objects have fields; {} does not", other.full_name())));
                unknown(pos)
            }
        }
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
        if self.var(p).full_type() == Type::Grid {
            self.check_assigned(p, name, pos);
            if idx.len() != 1 {
                self.report(Error::new(pos, format!("a nungw needs exactly 1 index, like {}[i]", name)));
                return unknown(pos);
            }
            let g = TExpr { kind: TExprKind::Load(p), ty: Type::Grid, pos };
            let i = idx.into_iter().next().expect("one index");
            return TExpr { kind: TExprKind::Builtin(Builtin::GridGet, vec![g, i]), ty: Type::Base(BaseType::Real), pos };
        }
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
            Shape::Set => {
                self.report(Error::new(pos, format!("a set is not indexed; use contains({}, x) or for(x in {})", name, name)));
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
        // comparing an object or function value to `none`
        if matches!(op, Eq | NotEq) && (a.ty == Type::None || b.ty == Type::None) {
            let other = if a.ty == Type::None { b } else { a };
            if !matches!(other.ty, Type::Object(_) | Type::Func(_) | Type::None | Type::Unknown) {
                self.report(Error::new(pos, format!("{} cannot be none", other.ty.full_name())));
                return unknown(pos);
            }
            let check = base(TExprKind::IsNone(Box::new(other)), Bool, pos);
            return if op == NotEq { base(TExprKind::Not(Box::new(check)), Bool, pos) } else { check };
        }
        // operators on grids become calls to the grid engine
        if a.ty == Type::Grid || b.ty == Type::Grid {
            return self.grid_binary(op, a, b, pos);
        }
        // fold a constant sub-expression (5+2)*(5-4) into a literal at compile time
        if let Some(folded) = fold_const(op, &a, &b, pos) {
            return folded;
        }
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

    /// Lowers an operator with a grid operand to a call to the `grid` engine.
    fn grid_binary(&mut self, op: BinOp, a: TExpr, b: TExpr, pos: Pos) -> TExpr {
        use BinOp::*;
        let grid = |bi: Builtin, args: Vec<TExpr>| TExpr { kind: TExprKind::Builtin(bi, args), ty: Type::Grid, pos };
        if a.ty == Type::Grid && b.ty == Type::Grid {
            return match op {
                Add => grid(Builtin::GridAdd, vec![a, b]),
                Sub => grid(Builtin::GridSub, vec![a, b]),
                Mul => grid(Builtin::GridMul, vec![a, b]),
                Div => grid(Builtin::GridDiv, vec![a, b]),
                _ => {
                    self.report(Error::new(pos, format!("cannot use {} between two nungws", symbol(op))));
                    unknown(pos)
                }
            };
        }
        // one side is a grid, the other a number
        let (g, scalar, grid_left) = if a.ty == Type::Grid { (a, b, true) } else { (b, a, false) };
        if scalar.ty.base().map_or(true, |t| !matches!(t, BaseType::Int | BaseType::Real)) {
            self.report(Error::new(pos, format!("cannot use {} between a nungw and {}", symbol(op), scalar.ty.name())));
            return unknown(pos);
        }
        let s = convert(scalar, Some(BaseType::Real));
        match (op, grid_left) {
            (Add, _) => grid(Builtin::GridOffset, vec![g, s]),
            (Mul, _) => grid(Builtin::GridScale, vec![g, s]),
            (Sub, true) => grid(Builtin::GridSubScalar, vec![g, s]),
            (Sub, false) => grid(Builtin::GridScalarSub, vec![g, s]),
            (Div, true) => grid(Builtin::GridDivScalar, vec![g, s]),
            (Div, false) => grid(Builtin::GridScalarDiv, vec![g, s]),
            _ => {
                self.report(Error::new(pos, format!("cannot use {} between a nungw and a number", symbol(op))));
                unknown(pos)
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
                    Type::Array(_) | Type::List(_) | Type::ListOf(_) | Type::Dict(..) | Type::Set(_) | Type::Base(BaseType::Str) => {
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
            "transform" | "keep" | "reduce" => self.hof(name, args, pos),
            _ => match crate::builtins::lookup(name) {
                Some(b) => self.builtin_call(b, args, pos, true),
                None => self.call(name, args, pos, true),
            },
        }
    }
}
