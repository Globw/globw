//! Calls to user-defined functions and procedures: one instance (specialized
//! copy) per combination of argument types.

use crate::ast::{Expr, Shape};
use crate::errors::{Error, Pos};
use crate::lexer::miscased_statement_keyword;
use crate::tir::*;

use super::exprs::{convert, unknown};
use super::{Checker, Instance, Var, BUILTINS};

impl<'p> Checker<'p> {
    pub(crate) fn call(&mut self, name: &str, args: &[Expr], pos: Pos, in_expr: bool) -> TExpr {
        let prog = self.prog;
        let Some(&s) = self.sub_by_name.get(name) else {
            let err = self.unknown_function(name, pos);
            self.report_structural(err);
            for a in args {
                self.expr(a);
            }
            return unknown(pos);
        };
        let sub = &prog.subprograms[s];
        let n = sub.params.len();
        if args.len() != n {
            self.report_structural(Error::new(
                pos,
                format!("\"{}\" takes {} value{}, here it gets {}", name, n, if n == 1 { "" } else { "s" }, args.len()),
            ));
            return unknown(pos);
        }
        if in_expr && !sub.is_function {
            self.report_structural(Error::new(pos, format!("PROCEDURE \"{}\" does not return a value", name)));
            return unknown(pos);
        }
        let targs: Vec<TExpr> = args.iter().map(|a| self.expr(a)).collect();
        let key: Vec<Type> = targs.iter().map(|t| t.ty).collect();
        let inst = self.instance(s, key);
        let index = self.reach(inst);
        let params: Vec<(Option<BaseType>, Shape)> =
            self.instances[inst].locals[..n].iter().map(|v| (v.ty, v.shape.clone())).collect();
        let mut out = Vec::with_capacity(n);
        for (t, (ty, shape)) in targs.into_iter().zip(params) {
            if let Some(b) = ty {
                if shape == Shape::Scalar {
                    self.hint(&t, b);
                } else if let TExprKind::Load(p) = &t.kind {
                    if self.var(*p).shape != Shape::Scalar {
                        self.widen(*p, b, t.pos);
                    }
                }
            }
            out.push(if shape == Shape::Scalar { convert(t, ty) } else { t });
        }
        let ret = self.instances[inst].ret;
        if in_expr && ret.is_none() {
            self.report(Error::new(pos, format!("cannot work out what type \"{}\" returns", name)));
        }
        let ty = if in_expr { ret.map_or(Type::Unknown, Type::Base) } else { Type::Unknown };
        TExpr { kind: TExprKind::Call(index, out), ty, pos }
    }

    /// Finds or creates the instance of subprogram `s` for these argument types.
    pub(crate) fn instance(&mut self, s: usize, key: Vec<Type>) -> usize {
        if let Some(&i) = self.by_key.get(&(s, key.clone())) {
            return i;
        }
        let prog = self.prog;
        let sub = &prog.subprograms[s];
        let mut locals = Vec::new();
        for ((name, pos), t) in sub.params.iter().zip(&key) {
            let (shape, ty) = match *t {
                Type::Unknown => (Shape::Scalar, None),
                Type::Base(b) => (Shape::Scalar, Some(b)),
                Type::Array(e) => (Shape::Array(0), e),
                Type::Matrix(e) => (Shape::Matrix(0, 0), e),
                Type::List(e) => (Shape::List, e),
            };
            locals.push(Var { name: name.clone(), shape, ty, origin: Some(*pos), conflict: None, is_param: true });
        }
        locals.extend(sub.locals.iter().map(Var::declared));
        self.instances.push(Instance { sub: Some(s), locals, ret: None, ret_origin: None, final_index: None });
        let i = self.instances.len() - 1;
        self.by_key.insert((s, key), i);
        self.changed = true;
        i
    }

    pub(crate) fn unknown_function(&self, name: &str, pos: Pos) -> Error {
        let e = Error::new(pos, format!("function \"{}\" does not exist", name));
        if let Some(k) = miscased_statement_keyword(name) {
            return e.with_hint(format!("keywords are written in UPPERCASE: write {}", k));
        }
        if let Some(b) = BUILTINS.iter().find(|b| b.eq_ignore_ascii_case(name)) {
            return e.with_hint(format!("did you mean {}?", b));
        }
        e
    }
}
