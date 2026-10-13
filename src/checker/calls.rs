//! Calls to user-defined functions and procedures.

use crate::ast::Expr;
use crate::errors::{Error, Pos};
use crate::lexer::miscased_statement_keyword;
use crate::tir::*;

use super::exprs::unknown;
use super::{Checker, BUILTINS};

impl<'p> Checker<'p> {
    /// Task 7 replaces this with real calls; until then every function is unknown.
    pub(crate) fn call(&mut self, name: &str, args: &[Expr], pos: Pos, _in_expr: bool) -> TExpr {
        let err = self.unknown_function(name, pos);
        self.report_structural(err);
        for a in args {
            self.expr(a);
        }
        unknown(pos)
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
