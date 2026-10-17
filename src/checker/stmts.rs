//! Checking statements, including definite assignment of local variables.

use crate::ast::{Expr, ExprKind, Shape, Stmt, StmtKind, Target};
use crate::errors::{Error, Pos};
use crate::tir::*;

use super::exprs::convert;
use super::Checker;

fn intersect(states: &[Vec<bool>]) -> Vec<bool> {
    let mut out = states[0].clone();
    for s in &states[1..] {
        for (o, x) in out.iter_mut().zip(s) {
            *o = *o && *x;
        }
    }
    out
}

impl<'p> Checker<'p> {
    pub(crate) fn block(&mut self, body: &[Stmt]) -> Vec<TStmt> {
        body.iter().filter_map(|s| self.stmt(s)).collect()
    }

    fn stmt(&mut self, s: &Stmt) -> Option<TStmt> {
        let is_loop = matches!(
            s.kind,
            StmtKind::While(..) | StmtKind::For { .. } | StmtKind::Repeat(..) | StmtKind::ForEach { .. }
        );
        self.loops += is_loop as usize;
        let out = self.stmt_kind(s);
        self.loops -= is_loop as usize;
        out
    }

    fn stmt_kind(&mut self, s: &Stmt) -> Option<TStmt> {
        let pos = s.pos;
        match &s.kind {
            StmtKind::ForEach { var, var_pos, iter, body } => self.for_each(var, *var_pos, iter, body, pos),
            StmtKind::Break | StmtKind::Continue => {
                let is_break = matches!(s.kind, StmtKind::Break);
                if self.loops == 0 {
                    let word = if is_break { "break" } else { "continue" };
                    self.report_structural(Error::new(pos, format!("{} can only be used inside a loop", word)));
                    return None;
                }
                Some(if is_break { TStmt::Break(pos) } else { TStmt::Continue(pos) })
            }
            StmtKind::Assign(target, value) => {
                let v = self.expr(value);
                if self.is_member_target(target) {
                    return self.assign_member(target, v);
                }
                if self.needs_general_target(target) {
                    return self.assign_element(target, v);
                }
                let t = self.target(target)?;
                Some(self.assign(t, v))
            }
            StmtKind::If { branches, else_body } => {
                let start = self.assigned.clone();
                let mut ends: Vec<Vec<bool>> = Vec::new();
                let mut out = Vec::new();
                for (c, body) in branches {
                    let c = self.condition(c);
                    let b = self.block(body);
                    ends.push(std::mem::replace(&mut self.assigned, start.clone()));
                    out.push((c, b));
                }
                let other = match else_body {
                    Some(b) => {
                        let b = self.block(b);
                        ends.push(std::mem::replace(&mut self.assigned, start.clone()));
                        Some(b)
                    }
                    None => {
                        ends.push(start.clone());
                        None
                    }
                };
                self.assigned = intersect(&ends);
                Some(TStmt::If(out, other))
            }
            StmtKind::While(c, body) => {
                let c = self.condition(c);
                let start = self.assigned.clone();
                let b = self.block(body);
                self.assigned = start;
                Some(TStmt::While(c, b))
            }
            StmtKind::Repeat(body, c) => {
                let b = self.block(body);
                let c = self.condition(c);
                Some(TStmt::Repeat(b, c))
            }
            StmtKind::For { var, var_pos, from, to, step, body } => {
                self.for_stmt(var, *var_pos, from, to, step.as_ref(), body, pos)
            }
            StmtKind::Call(name, args) => self.call_stmt(name, args, pos),
            StmtKind::Return(value) => self.return_stmt(value.as_ref(), pos),
            StmtKind::Try { body, catch_name, catch_pos, catch, always } => {
                self.try_stmt(body, catch_name, *catch_pos, catch, always, pos)
            }
            StmtKind::Fail(e) => {
                let v = self.expr(e);
                if !matches!(v.ty, Type::Base(BaseType::Str) | Type::Unknown) {
                    self.report(Error::new(v.pos, format!("fail needs a text message, here it gets {}", v.ty.full_name())));
                }
                // nothing after fail runs
                self.assigned.iter_mut().for_each(|a| *a = true);
                Some(TStmt::Fail(convert(v, Some(BaseType::Str)), pos))
            }
            StmtKind::SetMember(lvalue, value) => {
                let v = self.expr(value);
                self.assign_lvalue(lvalue, v, pos)
            }
            StmtKind::Eval(e) => match &e.kind {
                ExprKind::MethodCall(obj, method, args) => match self.method_on(obj, method, args, e.pos, false).kind {
                    TExprKind::Call(idx, a) => Some(TStmt::Call(idx, a, e.pos)),
                    TExprKind::VCall { slot, args } => Some(TStmt::VCall(slot, args, e.pos)),
                    _ => None,
                },
                _ => {
                    self.report_structural(Error::new(pos, "only a method call can be a statement on its own"));
                    None
                }
            },
            StmtKind::LoadModule { init, flag } => {
                let s = self.sub_by_name[init.as_str()];
                let inst = self.instance(s);
                let func = self.reach(inst);
                let Some(Place::Global(flag)) = self.lookup(flag) else {
                    panic!("internal error: module flag {} is not a global", flag)
                };
                Some(TStmt::LoadModule { flag, func, pos })
            }
        }
    }

    pub(crate) fn check_assigned(&mut self, p: Place, name: &str, pos: Pos) {
        if let Place::Local(i) = p {
            let i = i as usize;
            if !self.assigned[i] {
                self.assigned[i] = true; // report once
                self.report_structural(
                    Error::new(pos, format!("variable \"{}\" might not have a value yet", name))
                        .with_hint("give it a value (with = or input) before using it"),
                );
            }
        }
    }

    /// `g[1][2] = x`, `d["a"] = x`: element assignments that go through
    /// nested lists or dictionaries.
    fn needs_general_target(&self, t: &Target) -> bool {
        if !t.rest.is_empty() {
            return true;
        }
        match self.lookup(&t.name) {
            Some(p) => !t.indices.is_empty() && matches!(self.var(p).full_type(), Type::Dict(..) | Type::ListOf(_)),
            None => false,
        }
    }

    fn assign_element(&mut self, t: &Target, v: TExpr) -> Option<TStmt> {
        let Some(p) = self.lookup(&t.name) else {
            let err = self.undeclared(&t.name, t.pos);
            self.report_structural(err);
            return None;
        };
        let container = TExpr { kind: TExprKind::Load(p), ty: self.var(p).full_type(), pos: t.pos };
        let groups: Vec<&Vec<Expr>> = std::iter::once(&t.indices).chain(t.rest.iter()).collect();
        self.set_in_container(container, groups, v, t.pos)
    }

    /// Whether the target is an object field, like `p.x` or `self.pos.y`.
    fn is_member_target(&self, t: &Target) -> bool {
        if !t.name.contains('.') || self.lookup(&t.name).is_some() {
            return false;
        }
        let first = t.name.split('.').next().expect("a name has at least one segment");
        first == "self" || self.lookup(first).is_some_and(|p| matches!(self.var(p).full_type(), Type::Object(_)))
    }

    fn assign_member(&mut self, t: &Target, v: TExpr) -> Option<TStmt> {
        let (prefix, field) = t.name.rsplit_once('.').expect("a member target has a dot");
        let object = self.field_chain(prefix, t.pos);
        let Type::Object(cid) = object.ty else {
            return None;
        };
        let Some((slot, fty)) = crate::tir::class_field(cid, field) else {
            self.report(Error::new(
                t.pos,
                format!("class \"{}\" has no field \"{}\"", crate::tir::class_name(cid), field),
            ));
            return None;
        };
        if t.indices.is_empty() && t.rest.is_empty() {
            if !fits(fty, v.ty) {
                self.report(Error::new(
                    v.pos,
                    format!("field \"{}\" is {}, here it gets {}", field, fty.full_name(), v.ty.full_name()),
                ));
            }
            return Some(TStmt::SetField(object, slot, convert(v, fty.base()), t.pos));
        }
        let container = TExpr { kind: TExprKind::GetField(Box::new(object), slot), ty: fty, pos: t.pos };
        let groups: Vec<&Vec<Expr>> = std::iter::once(&t.indices).chain(t.rest.iter()).collect();
        self.set_in_container(container, groups, v, t.pos)
    }

    /// Assignment to a field or element reached through an arbitrary
    /// expression: `people[i].name = x`, `line[i].cells[j] = 0`.
    fn assign_lvalue(&mut self, lvalue: &Expr, v: TExpr, pos: Pos) -> Option<TStmt> {
        match &lvalue.kind {
            ExprKind::Field(obj, field) => {
                let object = self.expr(obj);
                let Type::Object(cid) = object.ty else {
                    if object.ty != Type::Unknown {
                        self.report(Error::new(pos, format!("only objects have fields; {} does not", object.ty.full_name())));
                    }
                    return None;
                };
                let Some((slot, fty)) = crate::tir::class_field(cid, field) else {
                    self.report(Error::new(
                        pos,
                        format!("class \"{}\" has no field \"{}\"", crate::tir::class_name(cid), field),
                    ));
                    return None;
                };
                if !fits(fty, v.ty) {
                    self.report(Error::new(
                        v.pos,
                        format!("field \"{}\" is {}, here it gets {}", field, fty.full_name(), v.ty.full_name()),
                    ));
                }
                Some(TStmt::SetField(object, slot, convert(v, fty.base()), pos))
            }
            ExprKind::Subscript(base, idx) => {
                let container = self.expr(base);
                self.set_in_container(container, vec![idx], v, pos)
            }
            _ => {
                self.report_structural(Error::new(pos, "this cannot be assigned to"));
                None
            }
        }
    }

    /// Sets an element inside a list, array or dictionary reached through
    /// `container` and the chain of index groups.
    fn set_in_container(&mut self, mut container: TExpr, groups: Vec<&Vec<Expr>>, v: TExpr, pos: Pos) -> Option<TStmt> {
        let t = Target { name: String::new(), indices: vec![], rest: vec![], pos };
        let (last, path) = groups.split_last().expect("at least one index group");
        for group in path {
            container = self.subscript(container, group, t.pos);
        }
        if last.len() != 1 {
            self.report(Error::new(t.pos, "use one index at a time, like a[i][j]"));
            return None;
        }
        let key = self.expr(&last[0]);
        let element = match container.ty {
            Type::Array(Some(b)) => Some(Type::Base(b)),
            other => list_elem(other),
        };
        let (elem, key) = match (container.ty, element) {
            (Type::Unknown, _) => return None,
            (_, Some(elem)) => (elem, self.int_expr(key, "the index")),
            (Type::Dict(k, v), _) => {
                if !fits(Type::Base(k), key.ty) {
                    self.report(Error::new(key.pos, format!("the key must be {}, here it is {}", k.name(), key.ty.full_name())));
                }
                (resolve(v), key)
            }
            (Type::Base(BaseType::Str), _) => {
                self.report(Error::new(t.pos, "a character of a string cannot be changed"));
                return None;
            }
            (other, _) => {
                self.report(Error::new(
                    t.pos,
                    format!("[ ] can be used on lists, dictionaries and strings, not on {}", other.full_name()),
                ));
                return None;
            }
        };
        if !fits(elem, v.ty) {
            self.report(Error::new(
                v.pos,
                format!("this element must be {}, here it gets {}", elem.full_name(), v.ty.full_name()),
            ));
        }
        Some(TStmt::SetIndex(container, key, convert(v, elem.base()), t.pos))
    }

    fn mark_assigned(&mut self, t: &TTarget) {
        if let (Place::Local(i), true) = (t.place, t.indices.is_empty()) {
            self.assigned[i as usize] = true;
        }
    }

    fn target(&mut self, t: &Target) -> Option<TTarget> {
        let Some(p) = self.lookup(&t.name) else {
            let err = self.undeclared(&t.name, t.pos);
            self.report_structural(err);
            return None;
        };
        let indices: Vec<TExpr> = t
            .indices
            .iter()
            .map(|i| {
                let e = self.expr(i);
                self.int_expr(e, "the index")
            })
            .collect();
        let v = self.var(p);
        let (shape, ty) = (v.shape.clone(), v.ty);
        let needed = match shape {
            Shape::Scalar | Shape::Set => 0,
            Shape::Array(_) | Shape::List | Shape::Dict => 1,
            Shape::Matrix(..) => 2,
        };
        if indices.len() == needed || (matches!(shape, Shape::List | Shape::Dict | Shape::Set) && indices.is_empty()) {
            return Some(TTarget { place: p, indices, pos: t.pos });
        }
        let err = match (&shape, indices.len()) {
            (Shape::Scalar, _) if ty == Some(BaseType::Str) => {
                Error::new(t.pos, "a character of a string cannot be changed")
            }
            (Shape::Scalar, _) => Error::new(t.pos, format!("\"{}\" is not an array: [ ] cannot be used on it", t.name)),
            (_, 0) => Error::new(t.pos, "a whole array or list cannot be assigned at once")
                .with_hint(format!("assign one element at a time, like {}[i] = ...", t.name)),
            (Shape::Matrix(..), _) => {
                Error::new(t.pos, format!("\"{}\" is a matrix: it needs 2 indices, like {}[i, j]", t.name, t.name))
            }
            _ => Error::new(t.pos, format!("\"{}\" needs exactly 1 index, like {}[i]", t.name, t.name)),
        };
        self.report(err);
        None
    }

    fn assign(&mut self, t: TTarget, v: TExpr) -> TStmt {
        let var = self.var(t.place);
        let whole = var.shape != Shape::Scalar || matches!(var.full_type(), Type::Object(_) | Type::Func(_));
        let _ = Shape::Set;
        if whole && t.indices.is_empty() {
            // a whole list, dictionary or object, like `words = split(s, " ")` or `p = new Point(1, 2)`
            let (expected, name) = (var.full_type(), var.name.clone());
            if !fits(expected, v.ty) {
                self.report(Error::new(
                    v.pos,
                    format!("cannot assign {} to \"{}\", which is {}", v.ty.full_name(), name, expected.full_name()),
                ));
            }
            self.mark_assigned(&t);
            return TStmt::Assign(t, v);
        }
        match v.ty {
            Type::Base(b) => self.widen(t.place, b, v.pos),
            Type::Unknown => {
                if let Some(b) = self.var(t.place).ty {
                    self.hint(&v, b);
                }
            }
            other => {
                let var = self.var(t.place);
                let message = if t.indices.is_empty() {
                    format!("\"{}\" is {}, but here it gets {}", var.name, var.full_type().full_name(), other.full_name())
                } else {
                    format!("this element must be {}, here it gets {}", Type::Base(var.ty.unwrap_or(BaseType::Int)).full_name(), other.full_name())
                };
                self.report(Error::new(v.pos, message));
            }
        }
        self.mark_assigned(&t);
        let ty = self.var(t.place).ty;
        TStmt::Assign(t, convert(v, ty))
    }

    #[allow(clippy::too_many_arguments)]
    fn for_each(&mut self, var: &str, var_pos: Pos, iter: &Expr, body: &[Stmt], pos: Pos) -> Option<TStmt> {
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
                        format!("for ... in works with lists, arrays, dictionaries and strings, not with {}", t.full_name()),
                    ));
                    (Type::Unknown, IterKind::Items)
                }
            },
        };
        let Some(p) = self.lookup(var) else {
            let err = self.undeclared(var, var_pos);
            self.report_structural(err);
            return None;
        };
        let declared = self.var(p).full_type();
        if !fits(declared, elem) {
            let what = if kind == IterKind::Keys { "the keys" } else { "the elements" };
            self.report(Error::new(
                var_pos,
                format!("\"{}\" is {}, but {} are {}", var, declared.full_name(), what, elem.plural()),
            ));
        }
        let to_real = declared == Type::Base(BaseType::Real) && elem == Type::Base(BaseType::Int);
        // the body may not run at all: the variable keeps its state from before the loop
        let before = self.assigned.clone();
        if let Place::Local(i) = p {
            self.assigned[i as usize] = true;
        }
        let body = self.block(body);
        self.assigned = before;
        Some(TStmt::ForEach { var: p, iter: it, kind, to_real, body, pos })
    }

    fn try_stmt(
        &mut self,
        body: &[Stmt],
        catch_name: &str,
        catch_pos: Pos,
        catch: &[Stmt],
        always: &[Stmt],
        pos: Pos,
    ) -> Option<TStmt> {
        let Some(err) = self.lookup(catch_name) else {
            let e = self.undeclared(catch_name, catch_pos);
            self.report_structural(e);
            return None;
        };
        let (shape, ty) = {
            let var = self.var(err);
            (var.shape.clone(), var.ty)
        };
        if shape != Shape::Scalar || !matches!(ty, Some(BaseType::Str) | None) {
            self.report(Error::new(
                catch_pos,
                format!("\"{}\" must be a string: catch stores the error message in it", catch_name),
            ));
        }
        if ty.is_none() {
            self.widen(err, BaseType::Str, catch_pos);
        }
        // like if/else for definite assignment: the body may stop early, so a
        // variable is assigned afterwards only if assigned on both paths
        let before = self.assigned.clone();
        let tbody = self.block(body);
        let after_try = self.assigned.clone();
        self.assigned = before;
        if let Place::Local(i) = err {
            self.assigned[i as usize] = true;
        }
        let tcatch = self.block(catch);
        let after_catch = std::mem::take(&mut self.assigned);
        self.assigned = after_try.iter().zip(&after_catch).map(|(a, b)| *a && *b).collect();
        let talways = self.block(always);
        Some(TStmt::Try { body: tbody, err, catch: tcatch, always: talways, pos })
    }

    fn for_stmt(
        &mut self,
        var: &str,
        var_pos: Pos,
        from: &Expr,
        to: &Expr,
        step: Option<&Expr>,
        body: &[Stmt],
        pos: Pos,
    ) -> Option<TStmt> {
        let from = {
            let e = self.expr(from);
            self.int_expr(e, "the start of for")
        };
        let to = {
            let e = self.expr(to);
            self.int_expr(e, "the end of for")
        };
        let step = step.map(|s| {
            let e = self.expr(s);
            self.int_expr(e, "the step")
        });
        let Some(p) = self.lookup(var) else {
            let err = self.undeclared(var, var_pos);
            self.report_structural(err);
            return None;
        };
        if self.var(p).shape != Shape::Scalar {
            self.report(Error::new(var_pos, "the for variable must be a simple variable"));
            return None;
        }
        if self.var(p).ty.is_none() {
            self.widen(p, BaseType::Int, var_pos);
        }
        if let Some(t) = self.var(p).ty {
            if t != BaseType::Int {
                self.report(Error::new(
                    var_pos,
                    format!("the for variable must be an int, \"{}\" is {}", var, t.name()),
                ));
            }
        }
        if let Place::Local(i) = p {
            self.assigned[i as usize] = true;
        }
        let start = self.assigned.clone();
        let body = self.block(body);
        self.assigned = start;
        Some(TStmt::For { var: p, from, to, step, body, pos })
    }

    fn call_stmt(&mut self, name: &str, args: &[Expr], pos: Pos) -> Option<TStmt> {
        match name {
            "output" => Some(TStmt::Output(args.iter().map(|a| self.expr(a)).collect())),
            "input" => {
                if args.is_empty() {
                    self.report_structural(Error::new(pos, "input needs at least one variable"));
                    return None;
                }
                let mut targets = Vec::new();
                for a in args {
                    let target = match &a.kind {
                        ExprKind::Var(n) => Target { name: n.clone(), indices: vec![], rest: vec![], pos: a.pos },
                        ExprKind::Index(n, ix) => Target { name: n.clone(), indices: ix.clone(), rest: vec![], pos: a.pos },
                        _ => {
                            self.report_structural(Error::new(
                                a.pos,
                                "input needs the name of a variable (or an element like v[i])",
                            ));
                            continue;
                        }
                    };
                    if let Some(p) = self.lookup(&target.name) {
                        if matches!(self.var(p).full_type(), Type::Dict(..) | Type::ListOf(_)) {
                            self.report(
                                Error::new(a.pos, "input cannot read into a dictionary or a list of lists")
                                    .with_hint("read into a variable, then assign it, like input(x) and then d[k] = x"),
                            );
                            continue;
                        }
                    }
                    let Some(t) = self.target(&target) else { continue };
                    let var = self.var(t.place);
                    if var.shape != Shape::Scalar && t.indices.is_empty() {
                        self.report(Error::new(
                            a.pos,
                            format!("input reads one value at a time: write an element, like {}[i]", var.name),
                        ));
                        continue;
                    }
                    self.mark_assigned(&t);
                    let ty = self.var(t.place).ty.unwrap_or(BaseType::Str);
                    targets.push((t, ty));
                }
                Some(TStmt::Input(targets))
            }
            "append" | "remove" => {
                if args.len() != 2 {
                    let what = if name == "append" { "the list and the value" } else { "the list and the position" };
                    self.report_structural(Error::new(pos, format!("{} takes 2 values: {}", name, what)));
                    return None;
                }
                let list = self.expr(&args[0]);
                let second = self.expr(&args[1]);
                let Some(element) = list_elem(list.ty) else {
                    if list.ty != Type::Unknown {
                        self.report(Error::new(list.pos, format!("{} works only with lists (declared with list of)", name)));
                    }
                    return None;
                };
                if name == "remove" {
                    let i = self.int_expr(second, "the position");
                    return Some(TStmt::Remove(list, i, pos));
                }
                let elem = element.base();
                if elem.is_none() || !matches!(list.kind, TExprKind::Load(_)) {
                    // nested lists, or a list reached through an expression like g[1]
                    if !fits(element, second.ty) {
                        self.report(Error::new(
                            second.pos,
                            format!("this list contains {}, here the value is {}", element.plural(), second.ty.full_name()),
                        ));
                    }
                    return Some(TStmt::Append(list, convert(second, elem)));
                }
                match (second.ty, &list.kind) {
                    (Type::Base(b), TExprKind::Load(p)) => self.widen(*p, b, second.pos),
                    (Type::Unknown, _) => {
                        if let Some(b) = elem {
                            self.hint(&second, b);
                        }
                    }
                    (Type::Base(_), _) => {}
                    (other, _) => self.report(Error::new(
                        second.pos,
                        format!("a list can only contain simple values, not {}", other.name()),
                    )),
                }
                let elem = match &list.kind {
                    TExprKind::Load(p) => self.var(*p).ty,
                    _ => elem,
                };
                Some(TStmt::Append(list, convert(second, elem)))
            }
            "length" => {
                self.report_structural(Error::new(
                    pos,
                    "length returns a value: use it in an expression, like n = length(v)",
                ));
                None
            }
            _ if crate::builtins::lookup(name).is_some() => {
                let b = crate::builtins::lookup(name).expect("checked above");
                let call = self.builtin_call(b, args, pos, false);
                match call.kind {
                    TExprKind::Builtin(b, args) => Some(TStmt::Builtin(b, args, pos)),
                    _ => None,
                }
            }
            _ => {
                let call = self.call(name, args, pos, false);
                match call.kind {
                    TExprKind::Call(f, args) => Some(TStmt::Call(f, args, pos)),
                    TExprKind::VCall { slot, args } => Some(TStmt::VCall(slot, args, pos)),
                    TExprKind::CallVal { func, args } => Some(TStmt::CallVal(*func, args, pos)),
                    _ => None,
                }
            }
        }
    }

    fn return_stmt(&mut self, value: Option<&Expr>, pos: Pos) -> Option<TStmt> {
        let prog = self.prog;
        let cur = self.current;
        let is_function = self.instances[cur].sub.map(|s| prog.subprograms[s].is_function);
        let out = match (is_function, value) {
            (None, _) => {
                self.report_structural(Error::new(pos, "return can only be used inside a function or procedure"));
                None
            }
            (Some(false), Some(_)) => {
                self.report_structural(
                    Error::new(pos, "a procedure does not return values").with_hint("use function, or write just return"),
                );
                None
            }
            (Some(true), None) => {
                self.report_structural(Error::new(pos, "in a function, return needs a value"));
                None
            }
            (Some(false), None) => Some(TStmt::Return(None, pos)),
            (Some(true), Some(e)) => {
                let v = self.expr(e);
                let declared = self.instances[cur].ret.unwrap_or(Type::Unknown);
                if !fits(declared, v.ty) {
                    let e = Error::new(
                        v.pos,
                        format!("this function returns {}, here it returns {}", declared.full_name(), v.ty.full_name()),
                    );
                    self.report(super::calls::float_hint(e, declared, v.ty));
                }
                Some(TStmt::Return(Some(convert(v, declared.base())), pos))
            }
        };
        // code after RETURN is unreachable: treat everything as assigned there
        self.assigned.iter_mut().for_each(|a| *a = true);
        out
    }
}
