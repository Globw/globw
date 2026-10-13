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
        let pos = s.pos;
        match &s.kind {
            StmtKind::Assign(target, value) => {
                let v = self.expr(value);
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
            Shape::Scalar => 0,
            Shape::Array(_) | Shape::List => 1,
            Shape::Matrix(..) => 2,
        };
        if indices.len() == needed {
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
        match v.ty {
            Type::Base(b) => self.widen(t.place, b, v.pos),
            Type::Unknown => {
                if let Some(b) = self.var(t.place).ty {
                    self.hint(&v, b);
                }
            }
            other => self.report(
                Error::new(v.pos, format!("cannot assign {} to a variable", other.name()))
                    .with_hint("assign one element at a time, like v[i] = ..."),
            ),
        }
        self.mark_assigned(&t);
        let ty = self.var(t.place).ty;
        TStmt::Assign(t, convert(v, ty))
    }

    #[allow(clippy::too_many_arguments)]
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
            "output" => {
                let mut out = Vec::new();
                for a in args {
                    let t = self.expr(a);
                    if !matches!(t.ty, Type::Base(_) | Type::Unknown) {
                        self.report(
                            Error::new(t.pos, "output prints only simple values")
                                .with_hint("to print an array use a for loop"),
                        );
                    }
                    out.push(t);
                }
                Some(TStmt::Output(out))
            }
            "input" => {
                if args.is_empty() {
                    self.report_structural(Error::new(pos, "input needs at least one variable"));
                    return None;
                }
                let mut targets = Vec::new();
                for a in args {
                    let target = match &a.kind {
                        ExprKind::Var(n) => Target { name: n.clone(), indices: vec![], pos: a.pos },
                        ExprKind::Index(n, ix) => Target { name: n.clone(), indices: ix.clone(), pos: a.pos },
                        _ => {
                            self.report_structural(Error::new(
                                a.pos,
                                "input needs the name of a variable (or an element like v[i])",
                            ));
                            continue;
                        }
                    };
                    let Some(t) = self.target(&target) else { continue };
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
                let Type::List(elem) = list.ty else {
                    if list.ty != Type::Unknown {
                        self.report(Error::new(list.pos, format!("{} works only with lists (declared with list of)", name)));
                    }
                    return None;
                };
                if name == "remove" {
                    let i = self.int_expr(second, "the position");
                    return Some(TStmt::Remove(list, i, pos));
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
                match v.ty {
                    Type::Base(b) => self.widen_return(b, v.pos),
                    Type::Unknown => {}
                    other => self.report(Error::new(
                        v.pos,
                        format!("a function can only return a simple value, not {}", other.name()),
                    )),
                }
                let ret = self.instances[cur].ret;
                Some(TStmt::Return(Some(convert(v, ret)), pos))
            }
        };
        // code after RETURN is unreachable: treat everything as assigned there
        self.assigned.iter_mut().for_each(|a| *a = true);
        out
    }

    fn widen_return(&mut self, t: BaseType, pos: Pos) {
        if let Some(declared) = self.instances[self.current].ret {
            if join(declared, t) != Some(declared) {
                self.report(Error::new(
                    pos,
                    format!("this function returns {}, here it returns {}", declared.name(), t.name()),
                ));
            }
        }
    }
}
