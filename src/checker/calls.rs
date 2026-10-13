//! Calls to user-defined functions and procedures: one instance (specialized
//! copy) per combination of argument types.

use crate::ast::{Decl, Expr, Shape};
use crate::builtins::Builtin;
use crate::errors::{Error, Pos};
use crate::lexer::miscased_statement_keyword;
use crate::tir::*;

use super::exprs::{convert, unknown};
use super::{Checker, Instance, Var, BUILTINS};

/// The type a declaration gives to its variable.
pub(crate) fn declared_type(d: &Decl) -> Type {
    match d.shape {
        Shape::Scalar => Type::Base(d.ty),
        Shape::Array(_) => Type::Array(Some(d.ty)),
        Shape::Matrix(..) => Type::Matrix(Some(d.ty)),
        Shape::List => Type::List(Some(d.ty)),
    }
}

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
            self.report_structural(Error::new(pos, format!("procedure \"{}\" does not return a value", name)));
            return unknown(pos);
        }
        let targs: Vec<TExpr> = args.iter().map(|a| self.expr(a)).collect();
        let inst = self.instance(s);
        let index = self.reach(inst);
        let mut out = Vec::with_capacity(n);
        for (k, (t, d)) in targs.into_iter().zip(&sub.params).enumerate() {
            let expected = declared_type(d);
            let fits = match (t.ty, expected) {
                (Type::Unknown, _) => true,
                (Type::Base(a), Type::Base(p)) => join(p, a) == Some(p),
                (a, p) => a == p,
            };
            if !fits {
                self.report(Error::new(
                    t.pos,
                    format!(
                        "argument {} of \"{}\" must be {}, here it is {}",
                        k + 1,
                        name,
                        expected.full_name(),
                        t.ty.full_name()
                    ),
                ));
            }
            out.push(if d.shape == Shape::Scalar { convert(t, Some(d.ty)) } else { t });
        }
        let ret = self.instances[inst].ret;
        let ty = if in_expr { ret.map_or(Type::Unknown, Type::Base) } else { Type::Unknown };
        TExpr { kind: TExprKind::Call(index, out), ty, pos }
    }

    /// The single instance of subprogram `s`, created on first use with its
    /// parameters and locals typed as declared.
    pub(crate) fn instance(&mut self, s: usize) -> usize {
        let prog = self.prog;
        let sub = &prog.subprograms[s];
        let key: Vec<Type> = sub.params.iter().map(declared_type).collect();
        if let Some(&i) = self.by_key.get(&(s, key.clone())) {
            return i;
        }
        let mut locals: Vec<Var> = sub
            .params
            .iter()
            .map(|d| Var { is_param: true, ..Var::declared(d) })
            .collect();
        locals.extend(sub.locals.iter().map(Var::declared));
        self.instances.push(Instance { sub: Some(s), locals, ret: sub.ret, final_index: None });
        let i = self.instances.len() - 1;
        self.by_key.insert((s, key), i);
        self.changed = true;
        i
    }

    /// A call of a built-in procedure or function: arity, use and argument types.
    pub(crate) fn builtin_call(&mut self, b: Builtin, args: &[Expr], pos: Pos, in_expr: bool) -> TExpr {
        let name = b.name();
        if in_expr && b.ret().is_none() {
            self.report_structural(Error::new(
                pos,
                format!("{} does not return a value: use it as a statement on its own line", name),
            ));
            return unknown(pos);
        }
        if !in_expr && b.ret().is_some() {
            self.report_structural(Error::new(
                pos,
                format!("{} returns a value: use it in an expression, like v = {}(...)", name, name),
            ));
            return unknown(pos);
        }
        let params = b.params();
        if args.len() != params.len() {
            let n = params.len();
            self.report_structural(Error::new(
                pos,
                format!("\"{}\" takes {} value{}, here it gets {}", name, n, if n == 1 { "" } else { "s" }, args.len()),
            ));
            return unknown(pos);
        }
        let mut out = Vec::with_capacity(args.len());
        for (k, (a, &p)) in args.iter().zip(params).enumerate() {
            let t = self.expr(a);
            let fits = match t.ty {
                Type::Unknown => true,
                Type::Base(x) => join(p, x) == Some(p),
                _ => false,
            };
            if !fits {
                self.report(Error::new(
                    t.pos,
                    format!("argument {} of \"{}\" must be {}, here it is {}", k + 1, name, p.name(), t.ty.full_name()),
                ));
            }
            out.push(convert(t, Some(p)));
        }
        TExpr { kind: TExprKind::Builtin(b, out), ty: b.ret().map_or(Type::Unknown, Type::Base), pos }
    }

    pub(crate) fn unknown_function(&self, name: &str, pos: Pos) -> Error {
        let e = Error::new(pos, format!("function \"{}\" does not exist", name));
        if let Some(k) = miscased_statement_keyword(name) {
            return e.with_hint(format!("keywords are written in lowercase: write {}", k));
        }
        if let Some(b) = BUILTINS.iter().find(|b| b.eq_ignore_ascii_case(name)) {
            return e.with_hint(format!("did you mean {}?", b));
        }
        if let Some(b) = crate::builtins::lookup(&name.to_lowercase()) {
            return e.with_hint(format!("did you mean {}?", b.name()));
        }
        e
    }
}
