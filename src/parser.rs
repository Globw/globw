//! Builds the syntax tree from tokens and checks that blocks are properly closed.

use crate::ast::*;
use crate::errors::{Error, Pos};
use crate::lexer::{miscased_statement_keyword, Tok, Token};

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

pub fn parse(tokens: Vec<Token>) -> Result<Program, Error> {
    Parser { t: tokens, i: 0 }.program()
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

    pub(crate) fn end_stmt(&mut self) -> Result<(), Error> {
        if self.accept(&Tok::Newline) || self.is(&Tok::Eof) {
            return Ok(());
        }
        self.fail(format!(
            "unexpected {} after the statement: each statement goes on its own line",
            describe(self.tok())
        ))
    }

    pub(crate) fn skip_newlines(&mut self) {
        while self.accept(&Tok::Newline) {}
    }

    fn program(mut self) -> Result<Program, Error> {
        let mut p = Program::default();
        self.skip_newlines();
        while self.accept(&Tok::Global) {
            if !self.accept(&Tok::Variables) {
                return self.fail("GLOBAL must be followed by \"variables:\"");
            }
            let decls = self.decl_list()?;
            p.globals.extend(decls);
            self.skip_newlines();
        }
        p.locals = self.local_decls()?;
        while !self.is(&Tok::Eof) {
            if self.is(&Tok::Function) || self.is(&Tok::Procedure) {
                let sub = self.subprogram()?;
                p.subprograms.push(sub);
            } else {
                let s = self.stmt()?;
                p.body.push(s);
            }
            self.skip_newlines();
        }
        Ok(p)
    }

    fn local_decls(&mut self) -> Result<Vec<Decl>, Error> {
        let mut v = Vec::new();
        while self.accept(&Tok::Variables) {
            v.extend(self.decl_list()?);
            self.skip_newlines();
        }
        Ok(v)
    }

    fn decl_list(&mut self) -> Result<Vec<Decl>, Error> {
        let mut v = Vec::new();
        loop {
            let pos = self.pos();
            let name = match self.tok().clone() {
                Tok::Name(n) => {
                    self.advance();
                    n
                }
                other => return self.fail(format!("expected the name of a variable, found {}", describe(&other))),
            };
            let shape = if self.accept(&Tok::LBracket) {
                if self.accept(&Tok::RBracket) {
                    Shape::List
                } else {
                    let a = self.dimension()?;
                    let shape = if self.accept(&Tok::Comma) {
                        let b = self.dimension()?;
                        Shape::Matrix(a, b)
                    } else {
                        Shape::Array(a)
                    };
                    self.expect(&Tok::RBracket, "\"]\"")?;
                    shape
                }
            } else {
                Shape::Scalar
            };
            v.push(Decl { name, shape, pos });
            if !self.accept(&Tok::Comma) {
                break;
            }
        }
        self.end_stmt()?;
        Ok(v)
    }

    fn dimension(&mut self) -> Result<usize, Error> {
        match *self.tok() {
            Tok::Int(n) if n > 0 => {
                self.advance();
                Ok(n as usize)
            }
            _ => self.fail("array size must be an integer greater than 0"),
        }
    }

    fn subprogram(&mut self) -> Result<Subprogram, Error> {
        let is_function = self.advance().tok == Tok::Function;
        let pos = self.pos();
        let name = match self.tok().clone() {
            Tok::Name(n) => {
                self.advance();
                n
            }
            other => return self.fail(format!("expected the function name, found {}", describe(&other))),
        };
        self.expect(&Tok::LParen, "\"(\"")?;
        let mut params = Vec::new();
        if !self.accept(&Tok::RParen) {
            loop {
                let p = self.pos();
                match self.tok().clone() {
                    Tok::Name(n) => {
                        self.advance();
                        params.push((n, p));
                    }
                    other => return self.fail(format!("expected a parameter name, found {}", describe(&other))),
                }
                if self.accept(&Tok::RParen) {
                    break;
                }
                self.expect(&Tok::Comma, "\",\" or \")\"")?;
            }
        }
        self.end_stmt()?;
        self.skip_newlines();
        let locals = self.local_decls()?;
        let (end, what, closer) = if is_function {
            (Tok::EndFunction, "FUNCTION", "END FUNCTION")
        } else {
            (Tok::EndProcedure, "PROCEDURE", "END PROCEDURE")
        };
        let body = self.block(&[end], pos, what, closer, None)?;
        self.advance();
        self.end_stmt()?;
        Ok(Subprogram { name, is_function, params, locals, body, pos })
    }

    /// Parses statements until one of `ends` (left unconsumed). For a REPEAT
    /// body, `repeat_col` is the column of REPEAT: a WHILE at that column or
    /// less closes the block.
    fn block(
        &mut self,
        ends: &[Tok],
        opened: Pos,
        what: &str,
        closer: &str,
        repeat_col: Option<u32>,
    ) -> Result<Vec<Stmt>, Error> {
        let mut body = Vec::new();
        self.skip_newlines();
        loop {
            if ends.contains(self.tok()) {
                return Ok(body);
            }
            if let Some(col) = repeat_col {
                if self.is(&Tok::While) && self.pos().col <= col {
                    return Ok(body);
                }
            }
            match self.tok() {
                Tok::Eof => {
                    return Err(Error::new(opened, format!("this {} is never closed", what))
                        .with_hint(format!("add {} at the end of the block", closer)))
                }
                Tok::EndIf | Tok::EndLoop | Tok::EndFunction | Tok::EndProcedure | Tok::Else => {
                    return Err(Error::new(
                        self.pos(),
                        format!(
                            "found {}, but the {} on line {} is not closed yet",
                            describe(self.tok()),
                            what,
                            opened.line
                        ),
                    )
                    .with_hint(format!("close it first with {}", closer)))
                }
                Tok::Function | Tok::Procedure => {
                    return self.fail("FUNCTION and PROCEDURE cannot be defined inside another block")
                }
                _ => {}
            }
            body.push(self.stmt()?);
            self.skip_newlines();
        }
    }

    fn stmt(&mut self) -> Result<Stmt, Error> {
        let pos = self.pos();
        let kind = match self.tok().clone() {
            Tok::If => self.if_stmt()?,
            Tok::While => {
                self.advance();
                let cond = self.condition()?;
                self.end_stmt()?;
                let body = self.block(&[Tok::EndLoop], pos, "WHILE", "END LOOP", None)?;
                self.advance();
                self.end_stmt()?;
                StmtKind::While(cond, body)
            }
            Tok::For => self.for_stmt(pos)?,
            Tok::Repeat => {
                self.advance();
                self.end_stmt()?;
                let body = self.block(&[], pos, "REPEAT", "WHILE(condition)", Some(pos.col))?;
                self.advance();
                let cond = self.condition()?;
                self.end_stmt()?;
                StmtKind::Repeat(body, cond)
            }
            Tok::Return => {
                self.advance();
                let value = if self.is(&Tok::Newline) || self.is(&Tok::Eof) { None } else { Some(self.expr()?) };
                self.end_stmt()?;
                StmtKind::Return(value)
            }
            Tok::Name(name) => self.name_stmt(name, pos)?,
            Tok::Variables | Tok::Global => {
                return self.fail("variable declarations must come at the start of the program or function")
            }
            Tok::EndIf => {
                return Err(Error::new(pos, "END IF without a matching IF")
                    .with_hint("an ELSE followed directly by IF(...) is closed by a single END IF"))
            }
            Tok::EndLoop => return self.fail("END LOOP without a matching WHILE or FOR"),
            other => return self.fail(format!("a statement cannot start with {}", describe(&other))),
        };
        Ok(Stmt { kind, pos })
    }

    fn condition(&mut self) -> Result<Expr, Error> {
        self.expect(&Tok::LParen, "\"(\" with the condition")?;
        let c = self.expr()?;
        self.expect(&Tok::RParen, "\")\"")?;
        Ok(c)
    }

    fn if_stmt(&mut self) -> Result<StmtKind, Error> {
        let opened = self.pos();
        self.advance();
        let ends = [Tok::Else, Tok::EndIf];
        let cond = self.condition()?;
        self.end_stmt()?;
        let body = self.block(&ends, opened, "IF", "END IF", None)?;
        let mut branches = vec![(cond, body)];
        let mut else_body = None;
        while self.accept(&Tok::Else) {
            self.end_stmt()?;
            self.skip_newlines();
            if self.accept(&Tok::If) {
                let cond = self.condition()?;
                self.end_stmt()?;
                let body = self.block(&ends, opened, "IF", "END IF", None)?;
                branches.push((cond, body));
            } else {
                else_body = Some(self.block(&[Tok::EndIf], opened, "IF", "END IF", None)?);
                break;
            }
        }
        self.expect(&Tok::EndIf, "END IF")?;
        self.end_stmt()?;
        Ok(StmtKind::If { branches, else_body })
    }

    fn for_stmt(&mut self, opened: Pos) -> Result<StmtKind, Error> {
        self.advance();
        self.expect(&Tok::LParen, "\"(\"")?;
        let var_pos = self.pos();
        let var = match self.tok().clone() {
            Tok::Name(n) => {
                self.advance();
                n
            }
            other => return self.fail(format!("expected the loop variable name, found {}", describe(&other))),
        };
        self.expect(&Tok::From, "FROM")?;
        let from = self.expr()?;
        self.expect(&Tok::To, "TO")?;
        let to = self.expr()?;
        let step = if self.accept(&Tok::Step) { Some(self.expr()?) } else { None };
        self.expect(&Tok::RParen, "\")\"")?;
        self.end_stmt()?;
        let body = self.block(&[Tok::EndLoop], opened, "FOR", "END LOOP", None)?;
        self.advance();
        self.end_stmt()?;
        Ok(StmtKind::For { var, var_pos, from, to, step, body })
    }

    /// A statement starting with a name: an assignment or a call.
    fn name_stmt(&mut self, name: String, pos: Pos) -> Result<StmtKind, Error> {
        let is_assignment = matches!(self.peek_tok(1), Tok::Arrow | Tok::LBracket);
        if !is_assignment {
            if let Some(kw) = miscased_statement_keyword(&name) {
                return Err(Error::new(pos, format!("\"{}\" is not a keyword", name))
                    .with_hint(format!("keywords are written in UPPERCASE: write {}", kw)));
            }
        }
        self.advance();
        if self.is(&Tok::LParen) {
            let args = self.args()?;
            self.end_stmt()?;
            return Ok(StmtKind::Call(name, args));
        }
        let indices = if self.is(&Tok::LBracket) { self.indices()? } else { Vec::new() };
        if !self.accept(&Tok::Arrow) {
            if self.is(&Tok::Eq) {
                return Err(Error::new(self.pos(), "assignment uses the arrow <-")
                    .with_hint(format!("write {} <- value", name)));
            }
            return self.fail(format!("expected <- or ( after \"{}\"", name));
        }
        let value = self.expr()?;
        self.end_stmt()?;
        Ok(StmtKind::Assign(Target { name, indices, pos }, value))
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

    fn parse_src(src: &str) -> Result<Program, Error> {
        parse(tokenize(src).unwrap())
    }

    fn err(src: &str) -> Error {
        parse_src(src).unwrap_err()
    }

    #[test]
    fn declarations() {
        let p = parse_src("GLOBAL variables: g\nvariables: a, v[3], m[2, 4], l[]\na <- 1\n").unwrap();
        assert_eq!(p.globals[0].name, "g");
        let shapes: Vec<Shape> = p.locals.iter().map(|d| d.shape.clone()).collect();
        assert_eq!(shapes, vec![Shape::Scalar, Shape::Array(3), Shape::Matrix(2, 4), Shape::List]);
        assert_eq!(p.body.len(), 1);
    }

    #[test]
    fn else_if_chain_needs_one_end_if() {
        let src = "IF(v >= 8)\n    output(1)\nELSE\nIF(v >= 6)\n    output(2)\nELSE\n    output(3)\nEND IF\n";
        match &parse_src(src).unwrap().body[0].kind {
            StmtKind::If { branches, else_body } => {
                assert_eq!(branches.len(), 2);
                assert_eq!(else_body.as_ref().unwrap().len(), 1);
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn nested_if_inside_else_is_not_else_if() {
        let src = "IF(a)\n    x <- 1\nELSE\n    x <- 2\n    IF(b)\n        x <- 3\n    END IF\nEND IF\n";
        match &parse_src(src).unwrap().body[0].kind {
            StmtKind::If { branches, else_body } => {
                assert_eq!(branches.len(), 1);
                assert_eq!(else_body.as_ref().unwrap().len(), 2);
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn loops() {
        let src = "WHILE(i < 3)\n    i <- i + 1\nEND LOOP\nFOR(i FROM 10 TO 0 STEP -2)\n    output(i)\nEND LOOP\nREPEAT\n    input(n)\nWHILE(n < 0)\n";
        let p = parse_src(src).unwrap();
        assert!(matches!(p.body[0].kind, StmtKind::While(..)));
        assert!(matches!(&p.body[1].kind, StmtKind::For { step: Some(_), .. }));
        assert!(matches!(p.body[2].kind, StmtKind::Repeat(..)));
    }

    #[test]
    fn while_indented_inside_repeat_is_a_loop() {
        let src = "REPEAT\n    WHILE(a < 3)\n        a <- a + 1\n    END LOOP\nWHILE(a < 10)\n";
        match &parse_src(src).unwrap().body[0].kind {
            StmtKind::Repeat(body, _) => assert!(matches!(body[0].kind, StmtKind::While(..))),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn functions_and_procedures() {
        let src = "FUNCTION add(a, b)\n    variables: r\n    r <- a + b\n    RETURN r\nEND FUNCTION\nPROCEDURE hello()\n    output(\"hi\")\nEND PROCEDURE\nhello()\n";
        let p = parse_src(src).unwrap();
        assert_eq!(p.subprograms.len(), 2);
        let add = &p.subprograms[0];
        assert!(add.is_function);
        assert_eq!(add.params.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        assert_eq!(add.locals[0].name, "r");
        assert_eq!(add.body.len(), 2);
        assert!(!p.subprograms[1].is_function);
        assert!(p.subprograms[1].params.is_empty());
        assert_eq!(p.body.len(), 1);
    }

    #[test]
    fn arrow_inside_condition_means_less_than_minus() {
        match &parse_src("IF(x<-1)\nEND IF\n").unwrap().body[0].kind {
            StmtKind::If { branches, .. } => assert_eq!(sx(&branches[0].0), "(Lt x (Neg 1))"),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn unclosed_if() {
        let e = err("IF(a)\n    x <- 1\n");
        assert_eq!(e.message, "this IF is never closed");
        assert_eq!(e.pos.line, 1);
        assert_eq!(e.hint.as_deref(), Some("add END IF at the end of the block"));
    }

    #[test]
    fn wrong_closer() {
        let e = err("IF(a)\n    x <- 1\nEND LOOP\n");
        assert_eq!(e.message, "found \"END LOOP\", but the IF on line 1 is not closed yet");
        assert_eq!(e.hint.as_deref(), Some("close it first with END IF"));
    }

    #[test]
    fn extra_end_if_after_else_if_chain() {
        let e = err("IF(a)\n    b <- 1\nELSE\nIF(c)\n    b <- 2\nEND IF\nEND IF\n");
        assert_eq!(e.message, "END IF without a matching IF");
        assert_eq!(e.pos.line, 7);
        assert_eq!(e.hint.as_deref(), Some("an ELSE followed directly by IF(...) is closed by a single END IF"));
    }

    #[test]
    fn lowercase_keyword() {
        let e = err("if(x)\nEND IF\n");
        assert_eq!(e.message, "\"if\" is not a keyword");
        assert_eq!(e.hint.as_deref(), Some("keywords are written in UPPERCASE: write IF"));
    }

    #[test]
    fn equals_is_not_assignment() {
        let e = err("x = 5\n");
        assert_eq!(e.message, "assignment uses the arrow <-");
        assert_eq!(e.hint.as_deref(), Some("write x <- value"));
    }

    #[test]
    fn declarations_must_come_first() {
        assert_eq!(
            err("x <- 1\nvariables: y\n").message,
            "variable declarations must come at the start of the program or function"
        );
    }

    #[test]
    fn one_statement_per_line() {
        assert_eq!(
            err("a <- 1 b <- 2\n").message,
            "unexpected \"b\" after the statement: each statement goes on its own line"
        );
    }

    #[test]
    fn no_nested_definitions() {
        assert_eq!(
            err("IF(a)\nFUNCTION f()\nEND FUNCTION\nEND IF\n").message,
            "FUNCTION and PROCEDURE cannot be defined inside another block"
        );
    }
}
