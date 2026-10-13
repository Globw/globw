//! Builds the syntax tree from tokens and checks that blocks are properly closed.

use crate::ast::*;
use crate::errors::{Error, Pos};
use crate::lexer::{Tok, Token};

pub(crate) struct Parser {
    pub(crate) t: Vec<Token>,
    pub(crate) i: usize,
}

/// Human-readable description of a token, for error messages.
pub fn describe(t: &Tok) -> String {
    match t {
        Tok::Int(n) => format!("the number {}", n),
        Tok::Real(r) => format!("the number {}", r),
        Tok::Str(_) => "a string".to_string(),
        Tok::Name(n) => format!("\"{}\"", n),
        Tok::Newline => "the end of the line".to_string(),
        Tok::Eof => "the end of the file".to_string(),
        other => format!("\"{}\"", text(other)),
    }
}

fn text(t: &Tok) -> &'static str {
    match t {
        Tok::If => "IF",
        Tok::Else => "ELSE",
        Tok::EndIf => "END IF",
        Tok::While => "WHILE",
        Tok::For => "FOR",
        Tok::From => "FROM",
        Tok::To => "TO",
        Tok::Step => "STEP",
        Tok::Repeat => "REPEAT",
        Tok::EndLoop => "END LOOP",
        Tok::Function => "FUNCTION",
        Tok::EndFunction => "END FUNCTION",
        Tok::Procedure => "PROCEDURE",
        Tok::EndProcedure => "END PROCEDURE",
        Tok::Return => "RETURN",
        Tok::Global => "GLOBAL",
        Tok::Variables => "variables:",
        Tok::And => "AND",
        Tok::Or => "OR",
        Tok::Not => "NOT",
        Tok::Div => "DIV",
        Tok::Mod => "MOD",
        Tok::True => "TRUE",
        Tok::False => "FALSE",
        Tok::Arrow => "<-",
        Tok::Eq => "=",
        Tok::NotEq => "!=",
        Tok::Lt => "<",
        Tok::LtEq => "<=",
        Tok::Gt => ">",
        Tok::GtEq => ">=",
        Tok::Plus => "+",
        Tok::Minus => "-",
        Tok::Star => "*",
        Tok::Slash => "/",
        Tok::LParen => "(",
        Tok::RParen => ")",
        Tok::LBracket => "[",
        Tok::RBracket => "]",
        Tok::Comma => ",",
        Tok::Int(_) | Tok::Real(_) | Tok::Str(_) | Tok::Name(_) | Tok::Newline | Tok::Eof => "?",
    }
}

fn binary(op: BinOp, a: Expr, b: Expr, pos: Pos) -> Expr {
    Expr { kind: ExprKind::Binary(op, Box::new(a), Box::new(b)), pos }
}

impl Parser {
    pub(crate) fn tok(&self) -> &Tok {
        &self.t[self.i].tok
    }

    pub(crate) fn pos(&self) -> Pos {
        self.t[self.i].pos
    }

    pub(crate) fn peek_tok(&self, k: usize) -> &Tok {
        &self.t[(self.i + k).min(self.t.len() - 1)].tok
    }

    pub(crate) fn advance(&mut self) -> Token {
        let tok = self.t[self.i].clone();
        if self.i + 1 < self.t.len() {
            self.i += 1;
        }
        tok
    }

    pub(crate) fn is(&self, t: &Tok) -> bool {
        self.tok() == t
    }

    pub(crate) fn accept(&mut self, t: &Tok) -> bool {
        if self.is(t) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(crate) fn fail<T>(&self, message: impl Into<String>) -> Result<T, Error> {
        Err(Error::new(self.pos(), message))
    }

    pub(crate) fn expect(&mut self, t: &Tok, what: &str) -> Result<Token, Error> {
        if self.is(t) {
            Ok(self.advance())
        } else {
            self.fail(format!("expected {}, found {}", what, describe(self.tok())))
        }
    }

    pub(crate) fn expr(&mut self) -> Result<Expr, Error> {
        self.or()
    }

    fn or(&mut self) -> Result<Expr, Error> {
        let mut a = self.and()?;
        while self.is(&Tok::Or) {
            let pos = self.advance().pos;
            let b = self.and()?;
            a = binary(BinOp::Or, a, b, pos);
        }
        Ok(a)
    }

    fn and(&mut self) -> Result<Expr, Error> {
        let mut a = self.comparison()?;
        while self.is(&Tok::And) {
            let pos = self.advance().pos;
            let b = self.comparison()?;
            a = binary(BinOp::And, a, b, pos);
        }
        Ok(a)
    }

    fn comparison(&mut self) -> Result<Expr, Error> {
        let a = self.additive()?;
        let Some(op) = self.comparison_op() else { return Ok(a) };
        let pos = self.advance().pos;
        let b = self.additive()?;
        if self.comparison_op().is_some() {
            return Err(Error::new(self.pos(), "comparisons cannot be chained")
                .with_hint("write for example a < b AND b < c"));
        }
        Ok(binary(op, a, b, pos))
    }

    fn comparison_op(&mut self) -> Option<BinOp> {
        if self.is(&Tok::Arrow) {
            self.split_arrow();
        }
        Some(match self.tok() {
            Tok::Eq => BinOp::Eq,
            Tok::NotEq => BinOp::NotEq,
            Tok::Lt => BinOp::Lt,
            Tok::LtEq => BinOp::LtEq,
            Tok::Gt => BinOp::Gt,
            Tok::GtEq => BinOp::GtEq,
            _ => return None,
        })
    }

    /// Inside an expression `<-` means `<` followed by `-`: `x<-1` is `x < -1`.
    fn split_arrow(&mut self) {
        let p = self.pos();
        self.t[self.i] = Token { tok: Tok::Lt, pos: Pos::new(p.line, p.col, 1) };
        self.t.insert(self.i + 1, Token { tok: Tok::Minus, pos: Pos::new(p.line, p.col + 1, 1) });
    }

    fn additive(&mut self) -> Result<Expr, Error> {
        let mut a = self.multiplicative()?;
        loop {
            let op = match self.tok() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(a),
            };
            let pos = self.advance().pos;
            let b = self.multiplicative()?;
            a = binary(op, a, b, pos);
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, Error> {
        let mut a = self.unary()?;
        loop {
            let op = match self.tok() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Div => BinOp::IntDiv,
                Tok::Mod => BinOp::Mod,
                _ => return Ok(a),
            };
            let pos = self.advance().pos;
            let b = self.unary()?;
            a = binary(op, a, b, pos);
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        let op = match self.tok() {
            Tok::Minus => UnOp::Neg,
            Tok::Not => UnOp::Not,
            _ => return self.primary(),
        };
        let pos = self.advance().pos;
        let x = self.unary()?;
        Ok(Expr { kind: ExprKind::Unary(op, Box::new(x)), pos })
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        let t = self.advance();
        let kind = match t.tok {
            Tok::Int(n) => ExprKind::Int(n),
            Tok::Real(r) => ExprKind::Real(r),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(&Tok::RParen, "\")\"")?;
                return Ok(e);
            }
            Tok::Name(n) => {
                if self.is(&Tok::LParen) {
                    ExprKind::Call(n, self.args()?)
                } else if self.is(&Tok::LBracket) {
                    ExprKind::Index(n, self.indices()?)
                } else {
                    ExprKind::Var(n)
                }
            }
            other => {
                return Err(Error::new(t.pos, format!("expected a value, found {}", describe(&other))))
            }
        };
        Ok(Expr { kind, pos: t.pos })
    }

    pub(crate) fn args(&mut self) -> Result<Vec<Expr>, Error> {
        self.expect(&Tok::LParen, "\"(\"")?;
        let mut v = Vec::new();
        if self.accept(&Tok::RParen) {
            return Ok(v);
        }
        loop {
            v.push(self.expr()?);
            if self.accept(&Tok::RParen) {
                return Ok(v);
            }
            self.expect(&Tok::Comma, "\",\" or \")\"")?;
        }
    }

    pub(crate) fn indices(&mut self) -> Result<Vec<Expr>, Error> {
        self.expect(&Tok::LBracket, "\"[\"")?;
        let mut v = Vec::new();
        loop {
            v.push(self.expr()?);
            if self.accept(&Tok::RBracket) {
                return Ok(v);
            }
            self.expect(&Tok::Comma, "\",\" or \"]\"")?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    /// Prints an expression as a compact s-expression.
    pub(crate) fn sx(e: &Expr) -> String {
        let list = |v: &[Expr]| v.iter().map(sx).collect::<Vec<_>>().join(",");
        match &e.kind {
            ExprKind::Int(n) => n.to_string(),
            ExprKind::Real(r) => format!("{:?}", r),
            ExprKind::Str(s) => format!("{:?}", s),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Var(n) => n.clone(),
            ExprKind::Index(n, ix) => format!("{}[{}]", n, list(ix)),
            ExprKind::Call(n, a) => format!("{}({})", n, list(a)),
            ExprKind::Unary(op, x) => format!("({:?} {})", op, sx(x)),
            ExprKind::Binary(op, a, b) => format!("({:?} {} {})", op, sx(a), sx(b)),
        }
    }

    fn expr_of(src: &str) -> Result<String, Error> {
        let mut p = Parser { t: tokenize(src).unwrap(), i: 0 };
        p.expr().map(|e| sx(&e))
    }

    #[test]
    fn precedence() {
        assert_eq!(expr_of("1 + 2 * 3").unwrap(), "(Add 1 (Mul 2 3))");
        assert_eq!(expr_of("(1 + 2) * 3").unwrap(), "(Mul (Add 1 2) 3)");
        assert_eq!(expr_of("a < b AND NOT c OR d = 1").unwrap(), "(Or (And (Lt a b) (Not c)) (Eq d 1))");
        assert_eq!(expr_of("-a MOD 2 DIV b").unwrap(), "(IntDiv (Mod (Neg a) 2) b)");
        assert_eq!(expr_of("2.5 / x - TRUE").unwrap(), "(Sub (Div 2.5 x) true)");
    }

    #[test]
    fn calls_and_indexing() {
        assert_eq!(expr_of("v[i, j + 1] + f(2, \"s\")").unwrap(), "(Add v[i,(Add j 1)] f(2,\"s\"))");
        assert_eq!(expr_of("g()").unwrap(), "g()");
    }

    #[test]
    fn arrow_inside_expression_is_less_than_minus() {
        assert_eq!(expr_of("x<-1").unwrap(), "(Lt x (Neg 1))");
    }

    #[test]
    fn comparisons_cannot_be_chained() {
        let e = expr_of("a < b < c").unwrap_err();
        assert_eq!(e.message, "comparisons cannot be chained");
        assert_eq!(e.hint.as_deref(), Some("write for example a < b AND b < c"));
    }

    #[test]
    fn missing_value() {
        assert_eq!(expr_of("1 +").unwrap_err().message, "expected a value, found the end of the line");
        assert_eq!(expr_of("f(1").unwrap_err().message, "expected \",\" or \")\", found the end of the line");
    }
}
