//! Calls to user-defined functions and procedures, and to built-ins.

use crate::ast::{Decl, Expr, ExprKind, Shape};
use crate::builtins::{Builtin, CheckError};
use crate::errors::{Error, Pos};
use crate::lexer::miscased_statement_keyword;
use crate::tir::*;

use super::exprs::{convert, unknown};
use super::{Checker, Instance, Var, BUILTINS};

/// Adds the round/div hint when a float is given where an int is expected.
pub(crate) fn float_hint(e: Error, expected: Type, actual: Type) -> Error {
    if expected == Type::Base(BaseType::Int) && actual == Type::Base(BaseType::Real) {
        e.with_hint(super::FLOAT_TO_INT_HINT)
    } else {
        e
    }
}

/// The type a declaration gives to its variable.
pub(crate) fn declared_type(d: &Decl) -> Type {
    d.full
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
        let Some(args) = self.arrange(name, &sub.params, args, pos) else {
            return unknown(pos);
        };
        let args = &args[..];
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
            if !fits(expected, t.ty) {
                let e = Error::new(
                    t.pos,
                    format!(
                        "argument {} of \"{}\" must be {}, here it is {}",
                        k + 1,
                        name,
                        expected.full_name(),
                        t.ty.full_name()
                    ),
                );
                self.report(float_hint(e, expected, t.ty));
            }
            out.push(if d.shape == Shape::Scalar { convert(t, Some(d.ty)) } else { t });
        }
        let ret = self.instances[inst].ret;
        let ty = if in_expr { ret.unwrap_or(Type::Unknown) } else { Type::Unknown };
        TExpr { kind: TExprKind::Call(index, out), ty, pos }
    }

    /// The values of a call in parameter order: positional ones first, then
    /// named ones (`size = 3`), then the defaults of the parameters left out.
    fn arrange(&mut self, name: &str, params: &[Decl], args: &[Expr], pos: Pos) -> Option<Vec<Expr>> {
        let n = params.len();
        let required = params.iter().filter(|d| d.default.is_none()).count();
        let positional = args.iter().take_while(|a| !matches!(a.kind, ExprKind::Named(..))).count();
        let named = args.len() > positional;
        let takes = if required == n { n.to_string() } else { format!("{} to {}", required, n) };
        if positional > n || (!named && positional < required) {
            self.report_structural(Error::new(
                pos,
                format!("\"{}\" takes {} value{}, here it gets {}", name, takes, if n == 1 { "" } else { "s" }, positional),
            ));
            return None;
        }
        let mut out: Vec<Option<Expr>> = vec![None; n];
        for (slot, a) in out.iter_mut().zip(&args[..positional]) {
            *slot = Some(a.clone());
        }
        let mut ok = true;
        for a in &args[positional..] {
            let ExprKind::Named(p, value) = &a.kind else { continue };
            match params.iter().position(|d| &d.name == p) {
                None => {
                    self.report_structural(Error::new(a.pos, format!("\"{}\" has no parameter called \"{}\"", name, p)));
                    ok = false;
                }
                Some(i) if out[i].is_some() => {
                    self.report_structural(Error::new(a.pos, format!("parameter \"{}\" gets two values", p)));
                    ok = false;
                }
                Some(i) => out[i] = Some((**value).clone()),
            }
        }
        for (slot, d) in out.iter_mut().zip(params) {
            if slot.is_none() {
                match &d.default {
                    Some(e) => *slot = Some(Expr { kind: e.kind.clone(), pos }),
                    None => {
                        self.report_structural(Error::new(pos, format!("parameter \"{}\" of \"{}\" gets no value", d.name, name)));
                        ok = false;
                    }
                }
            }
        }
        if ok {
            Some(out.into_iter().map(|e| e.expect("filled above")).collect())
        } else {
            None
        }
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
        if in_expr && !b.is_function() {
            self.report_structural(Error::new(
                pos,
                format!("{} does not return a value: use it as a statement on its own line", name),
            ));
            return unknown(pos);
        }
        if !in_expr && b.is_function() {
            self.report_structural(Error::new(
                pos,
                format!("{} returns a value: use it in an expression, like v = {}(...)", name, name),
            ));
            return unknown(pos);
        }
        let targs: Vec<TExpr> = args.iter().map(|a| self.expr(a)).collect();
        let types: Vec<Type> = targs.iter().map(|t| t.ty).collect();
        match b.check(&types) {
            Ok(checked) => {
                let out = targs.into_iter().zip(checked.convert).map(|(t, c)| convert(t, c)).collect();
                TExpr { kind: TExprKind::Builtin(b, out), ty: checked.ret, pos }
            }
            Err(CheckError::Arity(n)) => {
                self.report_structural(Error::new(
                    pos,
                    format!("\"{}\" takes {} value{}, here it gets {}", name, n, if n == 1 { "" } else { "s" }, args.len()),
                ));
                unknown(pos)
            }
            Err(CheckError::Arg { index, expected }) => {
                let t = &targs[index];
                let e = Error::new(
                    t.pos,
                    format!("argument {} of \"{}\" must be {}, here it is {}", index + 1, name, expected.full_name(), t.ty.full_name()),
                );
                self.report(float_hint(e, expected, t.ty));
                unknown(pos)
            }
            Err(CheckError::Other(message)) => {
                self.report(Error::new(pos, message));
                unknown(pos)
            }
        }
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
