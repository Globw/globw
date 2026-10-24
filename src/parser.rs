//! Builds the syntax tree from tokens and checks that blocks are properly closed.

use crate::ast::*;
use crate::errors::{Error, Pos};
use crate::lexer::{miscased_statement_keyword, Tok, Token};
use crate::tir::Type;

pub(crate) struct Parser {
    pub(crate) t: Vec<Token>,
    pub(crate) i: usize,
    /// how deeply blocks and expressions are nested right now
    pub(crate) depth: u32,
    /// operators in the current statement's expressions
    pub(crate) ops: u32,
    /// anonymous functions (lambdas) lifted to top-level subprograms
    pub(crate) lambdas: Vec<Subprogram>,
    /// compile-time constants: `const NAME = value` substitutes the value inline
    pub(crate) consts: std::collections::HashMap<String, ExprKind>,
}

/// Deeper nesting would overflow the stack of the later phases.
const MAX_DEPTH: u32 = 80;
const MAX_OPS: u32 = 1000;
const MAX_ELEMENTS: usize = 10_000_000;
const TOO_BIG: &str = "an array can have at most 10000000 elements";

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
        Tok::If => "if",
        Tok::Else => "else",
        Tok::EndIf => "end if",
        Tok::While => "while",
        Tok::For => "for",
        Tok::From => "from",
        Tok::To => "to",
        Tok::Step => "step",
        Tok::Repeat => "repeat",
        Tok::Break => "break",
        Tok::Continue => "continue",
        Tok::Class => "class",
        Tok::EndClass => "end class",
        Tok::New => "new",
        Tok::Is => "is",
        Tok::None => "none",
        Tok::Try => "try",
        Tok::Catch => "catch",
        Tok::Always => "always",
        Tok::EndTry => "end try",
        Tok::Fail => "fail",
        Tok::Dot => "\".\"",
        Tok::EndLoop => "end loop",
        Tok::Function => "function",
        Tok::EndFunction => "end function",
        Tok::Procedure => "procedure",
        Tok::EndProcedure => "end procedure",
        Tok::Return => "return",
        Tok::Global => "global",
        Tok::Variables => "variables:",
        Tok::And => "and",
        Tok::Or => "or",
        Tok::Not => "not",
        Tok::Div => "div",
        Tok::Mod => "mod",
        Tok::True => "true",
        Tok::False => "false",
        Tok::Assign => "=",
        Tok::Eq => "==",
        Tok::NotEq => "!=",
        Tok::Lt => "<",
        Tok::LtEq => "<=",
        Tok::Gt => ">",
        Tok::GtEq => ">=",
        Tok::Const => "const",
        Tok::Match => "match",
        Tok::When => "when",
        Tok::EndMatch => "end match",
        Tok::Plus => "+",
        Tok::Minus => "-",
        Tok::Star => "*",
        Tok::Slash => "/",
        Tok::PlusEq => "+=",
        Tok::MinusEq => "-=",
        Tok::StarEq => "*=",
        Tok::SlashEq => "/=",
        Tok::LParen => "(",
        Tok::RParen => ")",
        Tok::LBracket => "[",
        Tok::RBracket => "]",
        Tok::Comma => ",",
        Tok::Int(_) | Tok::Real(_) | Tok::Str(_) | Tok::Name(_) | Tok::Newline | Tok::Eof => "?",
    }
}



/// Whether a word is one of the four built-in simple types.
fn is_base_type_word(w: &str) -> bool {
    matches!(w, "int" | "float" | "string" | "bool")
}

/// Whether `e` is written directly (a literal, a negative number, or a list
/// of those), so that it can be a default value.
fn is_constant(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_) | ExprKind::Real(_) | ExprKind::Str(_) | ExprKind::Bool(_) => true,
        ExprKind::Unary(UnOp::Neg, x) => matches!(x.kind, ExprKind::Int(_) | ExprKind::Real(_)),
        ExprKind::ListLit(items) => items.iter().all(is_constant),
        _ => false,
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<Program, Error> {
    Parser { t: tokens, i: 0, depth: 0, ops: 0, lambdas: Vec::new(), consts: std::collections::HashMap::new() }.program()
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
        while self.at_import() {
            p.imports.extend(self.import()?);
            self.skip_newlines();
        }
        while self.is(&Tok::Global) {
            self.advance();
            if !self.is(&Tok::Variables) {
                return self.fail("global must be followed by \"variables:\"");
            }
            let decls = self.decl_block()?;
            p.globals.extend(decls);
            self.skip_newlines();
        }
        p.locals = self.local_decls()?;
        while !self.is(&Tok::Eof) {
            if self.is(&Tok::Class) {
                p.classes.push(self.class_def()?);
            } else if self.is(&Tok::Function) || self.is(&Tok::Procedure) {
                let sub = self.subprogram()?;
                p.subprograms.push(sub);
            } else {
                let s = self.stmt()?;
                p.body.push(s);
            }
            self.skip_newlines();
        }
        p.subprograms.append(&mut self.lambdas);
        Ok(p)
    }

    /// Whether an import starts here: `import x`, `from x import`, `lazy ...`.
    fn at_import(&self) -> bool {
        let named = |t: &Tok, w: &str| matches!(t, Tok::Name(n) if n == w);
        let start = if named(self.tok(), "lazy") { 1 } else { 0 };
        let first = self.peek_tok(start);
        (named(first, "import") || *first == Tok::From) && matches!(self.peek_tok(start + 1), Tok::Name(_))
    }

    fn module_name(&mut self) -> Result<String, Error> {
        match self.tok().clone() {
            Tok::Name(n) if n.contains('.') => self.fail(format!("\"{}\": a module name cannot contain dots", n)),
            Tok::Name(n) => {
                self.advance();
                Ok(n)
            }
            _ => self.fail("expected the name of a module, like import shapes"),
        }
    }

    fn alias(&mut self) -> Result<Option<String>, Error> {
        if !self.is_name("as") {
            return Ok(None);
        }
        self.advance();
        match self.tok().clone() {
            Tok::Name(n) if !n.contains('.') => {
                self.advance();
                Ok(Some(n))
            }
            _ => self.fail("expected a name after \"as\""),
        }
    }

    /// One import line; `import a, b as c` gives one import per module.
    fn import(&mut self) -> Result<Vec<Import>, Error> {
        let pos = self.pos();
        let lazy = self.is_name("lazy");
        if lazy {
            self.advance();
        }
        let mut out = Vec::new();
        if self.accept(&Tok::From) {
            let module = self.module_name()?;
            if !self.is_name("import") {
                return self.fail(format!("expected \"import\", like from {} import name", module));
            }
            self.advance();
            let mut names = Vec::new();
            loop {
                let npos = self.pos();
                let name = match self.tok().clone() {
                    Tok::Name(n) if !n.contains('.') => n,
                    _ => return self.fail("expected the name of a function or variable of the module"),
                };
                self.advance();
                names.push(ImportName { name, alias: self.alias()?, pos: npos });
                if !self.accept(&Tok::Comma) {
                    break;
                }
            }
            out.push(Import { module, alias: None, names: Some(names), lazy, pos });
        } else {
            self.advance(); // import
            loop {
                let mpos = self.pos();
                let module = self.module_name()?;
                out.push(Import { module, alias: self.alias()?, names: None, lazy, pos: mpos });
                if !self.accept(&Tok::Comma) {
                    break;
                }
            }
        }
        self.end_stmt()?;
        Ok(out)
    }

    fn local_decls(&mut self) -> Result<Vec<Decl>, Error> {
        let mut v = Vec::new();
        while self.is(&Tok::Variables) {
            v.extend(self.decl_block()?);
            self.skip_newlines();
        }
        Ok(v)
    }

    fn is_name(&self, word: &str) -> bool {
        matches!(self.tok(), Tok::Name(n) if n == word)
    }

    /// `variables:` on its own line, then lines `a, b as TYPE` indented more
    /// than the line where the block starts. An indented line that is not a
    /// declaration (like `x = 1`) ends the block and is read as a statement.
    fn decl_block(&mut self) -> Result<Vec<Decl>, Error> {
        let block_indent = self.advance().indent;
        if !self.is(&Tok::Newline) {
            return Err(Error::new(self.pos(), "\"variables:\" must be followed by a new line with the declarations")
                .with_hint("put each declaration on its own indented line below, like \"    a as int\""));
        }
        self.skip_newlines();
        let mut v = Vec::new();
        while matches!(self.tok(), Tok::Name(_))
            && self.t[self.i].indent > block_indent
            && (matches!(self.peek_tok(1), Tok::Comma) || matches!(self.peek_tok(1), Tok::Name(n) if n == "as"))
        {
            v.extend(self.decl_line()?);
            self.skip_newlines();
        }
        if v.is_empty() {
            return self.fail("the declarations under \"variables:\" must be indented");
        }
        Ok(v)
    }

    fn decl_line(&mut self) -> Result<Vec<Decl>, Error> {
        let mut names = Vec::new();
        loop {
            let pos = self.pos();
            match self.tok().clone() {
                Tok::Name(n) => {
                    self.advance();
                    names.push((n, pos));
                }
                other => return self.fail(format!("expected the name of a variable, found {}", describe(&other))),
            }
            if !self.accept(&Tok::Comma) {
                break;
            }
        }
        if !self.is_name("as") {
            return self.fail(format!("expected \"as\" and a type after \"{}\"", names[names.len() - 1].0))
                .map_err(|e: Error| e.with_hint("for example: x as int"));
        }
        self.advance();
        let (shape, ty, full) = self.type_spec(false)?;
        self.end_stmt()?;
        Ok(names.into_iter().map(|(name, pos)| Decl { name, shape: shape.clone(), ty, full, default: None, pos }).collect())
    }

    fn base_type(&mut self) -> Result<crate::tir::BaseType, Error> {
        use crate::tir::BaseType;
        let ty = match self.tok() {
            Tok::Name(n) if n == "int" => BaseType::Int,
            Tok::Name(n) if n == "float" => BaseType::Real,
            Tok::Name(n) if n == "string" => BaseType::Str,
            Tok::Name(n) if n == "bool" => BaseType::Bool,
            Tok::Name(n) => {
                return Err(Error::new(self.pos(), format!("unknown type \"{}\"", n))
                    .with_hint("the types are int, float, string, bool"))
            }
            other => return self.fail(format!("expected a type, found {}", describe(other))),
        };
        self.advance();
        Ok(ty)
    }

    /// A type after `as`. Parameters write arrays as `int[]` and matrices as
    /// `int[,]` (any size); declarations give the sizes.
    /// A type after `as`: its shape, its simple (element) type and the whole
    /// type. Parameters write arrays as `int[]` and matrices as `int[,]`.
    fn type_spec(&mut self, param: bool) -> Result<(Shape, crate::tir::BaseType, Type), Error> {
        use crate::tir::BaseType;
        if self.is(&Tok::Function) {
            return Ok((Shape::Scalar, BaseType::Int, self.func_type_spec()?));
        }
        if self.is_name("tuple") {
            return Ok((Shape::Scalar, BaseType::Int, self.tuple_type_spec()?));
        }
        if self.is_name("nungw") {
            self.advance();
            if !self.is_name("of") {
                return self.fail("expected \"of\", like nungw of float");
            }
            self.advance();
            let p = self.pos();
            if self.base_type()? != BaseType::Real {
                return Err(Error::new(p, "a nungw holds floats: write nungw of float"));
            }
            return Ok((Shape::Scalar, BaseType::Int, Type::Grid));
        }
        if self.is_name("list") || self.is_name("dict") || self.is_name("set") {
            let full = self.collection_type()?;
            let shape = match full {
                Type::Dict(..) => Shape::Dict,
                Type::Set(_) => Shape::Set,
                _ => Shape::List,
            };
            let base = full.base().or(match full {
                Type::List(Some(b)) => Some(b),
                _ => None,
            });
            return Ok((shape, base.unwrap_or(BaseType::Int), full));
        }
        if let Tok::Name(n) = self.tok() {
            if !is_base_type_word(n) {
                let id = crate::tir::intern_name(n);
                self.advance();
                return Ok((Shape::Scalar, BaseType::Int, Type::Named(id)));
            }
        }
        let ty = self.base_type()?;
        if !self.accept(&Tok::LBracket) {
            return Ok((Shape::Scalar, ty, Type::Base(ty)));
        }
        if param {
            let shape = if self.accept(&Tok::Comma) { Shape::Matrix(0, 0) } else { Shape::Array(0) };
            if !self.is(&Tok::RBracket) {
                return self.fail("a parameter takes arrays of any size: write int[] or int[,]");
            }
            self.advance();
            let full = if shape == Shape::Array(0) { Type::Array(Some(ty)) } else { Type::Matrix(Some(ty)) };
            return Ok((shape, ty, full));
        }
        if self.is(&Tok::RBracket) {
            return self.fail("an array needs its size, like int[10]");
        }
        let a = self.dimension()?;
        let (shape, full) = if self.accept(&Tok::Comma) {
            let b = self.dimension()?;
            if a * b > MAX_ELEMENTS {
                return self.fail(TOO_BIG);
            }
            (Shape::Matrix(a, b), Type::Matrix(Some(ty)))
        } else {
            (Shape::Array(a), Type::Array(Some(ty)))
        };
        self.expect(&Tok::RBracket, "\"]\"")?;
        Ok((shape, ty, full))
    }

    /// `list of T` or `dict of K to V`, where T and V may be collections too.
    fn collection_type(&mut self) -> Result<Type, Error> {
        use crate::tir::{dict_of, list_of, BaseType, Type};
        let kind = if self.is_name("list") {
            0
        } else if self.is_name("set") {
            1
        } else {
            2
        };
        self.advance();
        if !self.is_name("of") {
            let hint = ["list of int", "set of string", "dict of string to int"][kind];
            return self.fail(format!("expected \"of\", like {}", hint));
        }
        self.advance();
        if kind == 0 {
            return Ok(list_of(self.element_type()?));
        }
        if kind == 1 {
            let p = self.pos();
            let e = self.base_type()?;
            if e != BaseType::Int && e != BaseType::Str {
                return Err(Error::new(p, "a set holds int or string values"));
            }
            return Ok(Type::Set(e));
        }
        let key_pos = self.pos();
        let key = self.base_type()?;
        if key != BaseType::Int && key != BaseType::Str {
            return Err(Error::new(key_pos, "dictionary keys must be int or string"));
        }
        if !self.accept(&Tok::To) {
            return self.fail("expected \"to\" after the key type, like dict of string to int");
        }
        Ok(dict_of(key, self.element_type()?))
    }

    /// The type of the elements of a list or the values of a dictionary.
    fn element_type(&mut self) -> Result<Type, Error> {
        if self.is(&Tok::Function) {
            self.func_type_spec()
        } else if self.is_name("tuple") {
            self.tuple_type_spec()
        } else if self.is_name("list") || self.is_name("dict") || self.is_name("set") {
            self.collection_type()
        } else if let Tok::Name(n) = self.tok() {
            if is_base_type_word(n) {
                Ok(Type::Base(self.base_type()?))
            } else {
                let id = crate::tir::intern_name(n);
                self.advance();
                Ok(Type::Named(id))
            }
        } else {
            Ok(Type::Base(self.base_type()?))
        }
    }

    fn dimension(&mut self) -> Result<usize, Error> {
        match *self.tok() {
            Tok::Int(n) if n > MAX_ELEMENTS as i64 => self.fail(TOO_BIG),
            Tok::Int(n) if n > 0 => {
                self.advance();
                Ok(n as usize)
            }
            _ => self.fail("array size must be an integer greater than 0"),
        }
    }

    fn class_def(&mut self) -> Result<Class, Error> {
        let pos = self.pos();
        self.advance(); // class
        let name = match self.tok().clone() {
            Tok::Name(n) => {
                self.advance();
                n
            }
            other => return self.fail(format!("expected the class name, found {}", describe(&other))),
        };
        let parent = if self.accept(&Tok::Is) {
            let ppos = self.pos();
            match self.tok().clone() {
                Tok::Name(n) if !n.contains('.') => {
                    self.advance();
                    Some((n, ppos))
                }
                other => return self.fail(format!("expected the parent class name after \"is\", found {}", describe(&other))),
            }
        } else {
            None
        };
        self.end_stmt()?;
        self.skip_newlines();
        let fields = self.local_decls()?;
        let mut methods = Vec::new();
        while self.is(&Tok::Function) || self.is(&Tok::Procedure) {
            methods.push(self.subprogram()?);
            self.skip_newlines();
        }
        if !self.is(&Tok::EndClass) {
            return self.fail(format!("expected a method or \"end class\" to close class \"{}\"", name));
        }
        self.advance();
        self.end_stmt()?;
        Ok(Class { name, parent, fields, methods, pos })
    }

    fn param_list(&mut self) -> Result<Vec<Decl>, Error> {
        self.expect(&Tok::LParen, "\"(\"")?;
        let mut params = Vec::new();
        if self.accept(&Tok::RParen) {
            return Ok(params);
        }
        loop {
            let p = self.pos();
            let name = match self.tok().clone() {
                Tok::Name(n) => {
                    self.advance();
                    n
                }
                other => return self.fail(format!("expected a parameter name, found {}", describe(&other))),
            };
            if !self.is_name("as") {
                return Err(Error::new(p, format!("parameter \"{}\" needs a type, like {} as int", name, name)));
            }
            self.advance();
            let (shape, ty, full) = self.type_spec(true)?;
            let default = if self.accept(&Tok::Assign) {
                let e = self.expr()?;
                if !is_constant(&e) {
                    return Err(Error::new(e.pos, "a default value must be written directly, like 0, \"text\" or []"));
                }
                Some(e)
            } else {
                if params.iter().any(|d: &Decl| d.default.is_some()) {
                    return Err(Error::new(p, format!("\"{}\" needs a default value: it comes after parameters that have one", name))
                        .with_hint(format!("write {} as ... = value, or move it before them", name)));
                }
                None
            };
            params.push(Decl { name, shape, ty, full, default, pos: p });
            if self.accept(&Tok::RParen) {
                return Ok(params);
            }
            self.expect(&Tok::Comma, "\",\" or \")\"")?;
        }
    }

    /// A function used as a value: `function(x as int) as int gives x * x`, or a
    /// block form closed by `end function` / `end procedure`. It is lifted to a
    /// top-level subprogram and the expression becomes a reference to it.
    fn lambda(&mut self, pos: Pos) -> Result<Expr, Error> {
        // the `function` token was already consumed by atom()
        let params = self.param_list()?;
        let ret = if self.is_name("as") {
            self.advance();
            Some(self.element_type()?)
        } else {
            None
        };
        let (locals, body) = if self.is_name("gives") {
            self.advance();
            let e = self.expr()?;
            (Vec::new(), vec![Stmt { kind: StmtKind::Return(Some(e)), pos }])
        } else {
            self.end_stmt()?;
            self.skip_newlines();
            let locals = self.local_decls()?;
            let end = if ret.is_some() { Tok::EndFunction } else { Tok::EndProcedure };
            let (what, closer) =
                if ret.is_some() { ("function", "end function") } else { ("procedure", "end procedure") };
            let body = self.block(&[end], pos, what, closer, None)?;
            self.advance();
            (locals, body)
        };
        let name = format!("lambda{}{}", crate::classes::SEP, self.lambdas.len());
        self.lambdas.push(Subprogram { name: name.clone(), is_function: ret.is_some(), params, ret, locals, body, pos });
        Ok(Expr { kind: ExprKind::FuncRef(name), pos })
    }

    /// A tuple type: `tuple of (int, string)`.
    fn tuple_type_spec(&mut self) -> Result<Type, Error> {
        self.advance(); // tuple
        if !self.is_name("of") {
            return self.fail("expected \"of\", like tuple of (int, string)");
        }
        self.advance();
        self.expect(&Tok::LParen, "\"(\"")?;
        let mut types = Vec::new();
        loop {
            types.push(self.element_type()?);
            if self.accept(&Tok::RParen) {
                break;
            }
            self.expect(&Tok::Comma, "\",\" or \")\"")?;
        }
        if types.len() < 2 {
            return self.fail("a tuple needs at least two types, like tuple of (int, string)");
        }
        Ok(crate::tir::tuple_type(types))
    }

    /// A function type: `function(int, string) as float`, or without `as` for
    /// a procedure value.
    fn func_type_spec(&mut self) -> Result<Type, Error> {
        self.advance(); // function
        self.expect(&Tok::LParen, "\"(\"")?;
        let mut params = Vec::new();
        if !self.accept(&Tok::RParen) {
            loop {
                params.push(self.element_type()?);
                if self.accept(&Tok::RParen) {
                    break;
                }
                self.expect(&Tok::Comma, "\",\" or \")\"")?;
            }
        }
        let ret = if self.is_name("as") {
            self.advance();
            Some(self.element_type()?)
        } else {
            None
        };
        Ok(crate::tir::func_type(params, ret))
    }

    fn subprogram(&mut self) -> Result<Subprogram, Error> {
        let is_function = self.advance().tok == Tok::Function;
        let pos = self.pos();
        let name = match self.tok().clone() {
            Tok::Name(n) => {
                self.advance();
                n
            }
            Tok::New => {
                self.advance();
                "new".to_string()
            }
            other => return self.fail(format!("expected the function name, found {}", describe(&other))),
        };
        let params = self.param_list()?;
        let ret = if is_function {
            if !self.is_name("as") {
                return self.fail("a function must declare its return type, like ) as int");
            }
            self.advance();
            let ty = self.element_type()?;
            if self.is(&Tok::LBracket) {
                return self.fail("a function cannot return an array: return a list, like ) as list of int");
            }
            Some(ty)
        } else {
            if self.is_name("as") {
                return self.fail("a procedure does not return a value, so it has no return type");
            }
            None
        };
        self.end_stmt()?;
        self.skip_newlines();
        let locals = self.local_decls()?;
        let (end, what, closer) = if is_function {
            (Tok::EndFunction, "function", "end function")
        } else {
            (Tok::EndProcedure, "procedure", "end procedure")
        };
        let body = self.block(&[end], pos, what, closer, None)?;
        self.advance();
        self.end_stmt()?;
        Ok(Subprogram { name, is_function, params, ret, locals, body, pos })
    }

    /// Parses statements until one of `ends` (left unconsumed). For a repeat
    /// body, `repeat_indent` is the indentation of its line: a while indented
    /// that much or less closes the block.
    fn block(
        &mut self,
        ends: &[Tok],
        opened: Pos,
        what: &str,
        closer: &str,
        repeat_indent: Option<u32>,
    ) -> Result<Vec<Stmt>, Error> {
        self.enter()?;
        let body = self.block_body(ends, opened, what, closer, repeat_indent);
        self.depth -= 1;
        body
    }

    fn block_body(
        &mut self,
        ends: &[Tok],
        opened: Pos,
        what: &str,
        closer: &str,
        repeat_indent: Option<u32>,
    ) -> Result<Vec<Stmt>, Error> {
        let mut body = Vec::new();
        self.skip_newlines();
        loop {
            if ends.contains(self.tok()) {
                return Ok(body);
            }
            if let Some(indent) = repeat_indent {
                if self.is(&Tok::While) && self.t[self.i].indent <= indent {
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
                    return self.fail("function and procedure cannot be defined inside another block")
                }
                _ => {}
            }
            body.push(self.stmt()?);
            self.skip_newlines();
        }
    }

    fn stmt(&mut self) -> Result<Stmt, Error> {
        self.ops = 0;
        let pos = self.pos();
        if self.at_import() {
            return Err(Error::new(pos, "imports go at the very start of the file, before everything else")
                .with_hint("move this line to the top"));
        }
        let kind = match self.tok().clone() {
            Tok::If => self.if_stmt()?,
            Tok::While => {
                self.advance();
                let cond = self.condition()?;
                self.end_stmt()?;
                let body = self.block(&[Tok::EndLoop], pos, "while", "end loop", None)?;
                self.advance();
                self.end_stmt()?;
                StmtKind::While(cond, body)
            }
            Tok::For => self.for_stmt(pos)?,
            Tok::Repeat => {
                self.advance();
                self.end_stmt()?;
                let body = self.block(&[], pos, "repeat", "while(condition)", Some(self.t[self.i - 1].indent))?;
                self.advance();
                let cond = self.condition()?;
                self.end_stmt()?;
                StmtKind::Repeat(body, cond)
            }
            Tok::Break | Tok::Continue => {
                let kind = if self.advance().tok == Tok::Break { StmtKind::Break } else { StmtKind::Continue };
                self.end_stmt()?;
                kind
            }
            Tok::Return => {
                self.advance();
                let value = if self.is(&Tok::Newline) || self.is(&Tok::Eof) { None } else { Some(self.expr()?) };
                self.end_stmt()?;
                StmtKind::Return(value)
            }
            Tok::Try => self.try_stmt(pos)?,
            Tok::Fail => {
                self.advance();
                let message = self.expr()?;
                self.end_stmt()?;
                StmtKind::Fail(message)
            }
            Tok::Const => self.const_stmt()?,
            Tok::Match => self.match_stmt(pos)?,
            Tok::Name(name) => self.name_stmt(name, pos)?,
            Tok::Variables | Tok::Global => {
                return self.fail("variable declarations must come at the start of the program or function")
            }
            Tok::EndIf => {
                return Err(Error::new(pos, "end if without a matching if")
                    .with_hint("an else followed directly by if(...) is closed by a single end if"))
            }
            Tok::EndLoop => return self.fail("end loop without a matching while or for"),
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
        let body = self.block(&ends, opened, "if", "end if", None)?;
        let mut branches = vec![(cond, body)];
        let mut else_body = None;
        while self.accept(&Tok::Else) {
            // "else if(...)" may be written on one line, or on two with the
            // `if` at the same indentation as `else`; a more indented `if`
            // is the first statement of the else block
            let else_indent = self.t[self.i - 1].indent;
            let mut chained = self.is(&Tok::If);
            if !chained {
                self.end_stmt()?;
                self.skip_newlines();
                chained = self.is(&Tok::If) && self.t[self.i].indent <= else_indent;
            }
            if chained && self.accept(&Tok::If) {
                let cond = self.condition()?;
                self.end_stmt()?;
                let body = self.block(&ends, opened, "if", "end if", None)?;
                branches.push((cond, body));
            } else {
                else_body = Some(self.block(&[Tok::EndIf], opened, "if", "end if", None)?);
                break;
            }
        }
        self.expect(&Tok::EndIf, "end if")?;
        self.end_stmt()?;
        Ok(StmtKind::If { branches, else_body })
    }

    fn try_stmt(&mut self, opened: Pos) -> Result<StmtKind, Error> {
        self.advance(); // try
        self.end_stmt()?;
        let body = self.block(&[Tok::Catch], opened, "try", "catch", None)?;
        if !self.is(&Tok::Catch) {
            return self.fail("a try needs a catch: try ... catch name ... end try");
        }
        self.advance(); // catch
        let catch_pos = self.pos();
        let catch_name = match self.tok().clone() {
            Tok::Name(n) if !n.contains('.') => {
                self.advance();
                n
            }
            other => return self.fail(format!("expected a variable name after \"catch\", found {}", describe(&other))),
        };
        self.end_stmt()?;
        let catch = self.block(&[Tok::Always, Tok::EndTry], opened, "try", "end try", None)?;
        let always = if self.is(&Tok::Always) {
            self.advance();
            self.end_stmt()?;
            self.block(&[Tok::EndTry], opened, "try", "end try", None)?
        } else {
            Vec::new()
        };
        self.advance(); // end try
        self.end_stmt()?;
        Ok(StmtKind::Try { body, catch_name, catch_pos, catch, always })
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
        if self.is_name("in") {
            self.advance();
            let iter = self.expr()?;
            self.expect(&Tok::RParen, "\")\"")?;
            self.end_stmt()?;
            let body = self.block(&[Tok::EndLoop], opened, "for", "end loop", None)?;
            self.advance();
            self.end_stmt()?;
            return Ok(StmtKind::ForEach { var, var_pos, iter, body });
        }
        if !self.is(&Tok::From) {
            return self.fail("expected \"from\" or \"in\": for(i from 1 to n) or for(x in list)");
        }
        self.expect(&Tok::From, "from")?;
        let from = self.expr()?;
        self.expect(&Tok::To, "to")?;
        let to = self.expr()?;
        let step = if self.accept(&Tok::Step) { Some(self.expr()?) } else { None };
        self.expect(&Tok::RParen, "\")\"")?;
        self.end_stmt()?;
        let body = self.block(&[Tok::EndLoop], opened, "for", "end loop", None)?;
        self.advance();
        self.end_stmt()?;
        Ok(StmtKind::For { var, var_pos, from, to, step, body })
    }

    /// `a, b = tuple`: unpacks a tuple into several variables.
    fn unpack_stmt(&mut self, first: String, pos: Pos) -> Result<StmtKind, Error> {
        self.advance(); // first name
        let mut names = vec![(first, pos)];
        while self.accept(&Tok::Comma) {
            let p = self.pos();
            match self.tok().clone() {
                Tok::Name(n) if !n.contains('.') => {
                    self.advance();
                    names.push((n, p));
                }
                other => return self.fail(format!("expected a variable name, found {}", describe(&other))),
            }
        }
        if !self.accept(&Tok::Assign) {
            return self.fail("expected = to unpack a tuple, like a, b = point");
        }
        let value = self.expr()?;
        self.end_stmt()?;
        Ok(StmtKind::Unpack(names, value))
    }

    /// `match subject ... when v: ... when a, b: ... else: ... end match`.
    /// Desugars to an if / else-if chain: each `when` arm becomes a branch whose
    /// condition is `subject == value` (OR-ed for several values). The subject is
    /// compared in each branch, so match it on a variable or a simple value.
    fn match_stmt(&mut self, pos: Pos) -> Result<StmtKind, Error> {
        self.advance(); // match
        let subject = self.expr()?;
        self.end_stmt()?;
        self.enter()?;
        let eq = |a: Expr, b: Expr| Expr { kind: ExprKind::Binary(BinOp::Eq, Box::new(a), Box::new(b)), pos };
        let mut branches: Vec<(Expr, Vec<Stmt>)> = Vec::new();
        let mut else_body: Option<Vec<Stmt>> = None;
        self.skip_newlines();
        loop {
            if self.is(&Tok::EndMatch) {
                break;
            }
            if self.is(&Tok::Eof) {
                self.depth -= 1;
                return Err(Error::new(pos, "this match is never closed").with_hint("add end match at the end"));
            }
            if self.is(&Tok::When) {
                if else_body.is_some() {
                    self.depth -= 1;
                    return self.fail("when cannot come after else in a match");
                }
                self.advance();
                let mut values = vec![self.expr()?];
                while self.accept(&Tok::Comma) {
                    values.push(self.expr()?);
                }
                self.end_stmt()?;
                let body = self.match_arm_body()?;
                let mut cond = eq(subject.clone(), values[0].clone());
                for v in values.into_iter().skip(1) {
                    cond = Expr {
                        kind: ExprKind::Binary(BinOp::Or, Box::new(cond), Box::new(eq(subject.clone(), v))),
                        pos,
                    };
                }
                branches.push((cond, body));
            } else if self.is(&Tok::Else) {
                self.advance();
                self.end_stmt()?;
                else_body = Some(self.match_arm_body()?);
            } else {
                self.depth -= 1;
                return self.fail("inside match, expected when, else or end match");
            }
        }
        self.depth -= 1;
        self.advance(); // end match
        self.end_stmt()?;
        Ok(StmtKind::If { branches, else_body })
    }

    /// The statements of one `when`/`else` arm, up to the next arm or `end match`.
    fn match_arm_body(&mut self) -> Result<Vec<Stmt>, Error> {
        let mut body = Vec::new();
        self.skip_newlines();
        while !matches!(self.tok(), Tok::When | Tok::Else | Tok::EndMatch | Tok::Eof) {
            body.push(self.stmt()?);
            self.skip_newlines();
        }
        Ok(body)
    }

    /// `const NAME = value`: a compile-time constant. The value must be a fixed
    /// literal (a number, text or true/false); it is substituted wherever the name
    /// is used, and the name cannot be assigned to.
    fn const_stmt(&mut self) -> Result<StmtKind, Error> {
        self.advance(); // const
        let (name, npos) = match self.tok() {
            Tok::Name(n) => {
                let n = n.clone();
                let p = self.pos();
                self.advance();
                (n, p)
            }
            _ => return self.fail("expected a name after const"),
        };
        if self.consts.contains_key(&name) {
            return Err(Error::new(npos, format!("the constant \"{}\" is already defined", name)));
        }
        if !self.accept(&Tok::Assign) {
            return self.fail(format!("expected = after the constant name \"{}\"", name));
        }
        let value = self.expr()?;
        self.end_stmt()?;
        let is_literal = matches!(
            value.kind,
            ExprKind::Int(_) | ExprKind::Real(_) | ExprKind::Str(_) | ExprKind::Bool(_)
        ) || matches!(&value.kind, ExprKind::Unary(_, inner)
            if matches!(inner.kind, ExprKind::Int(_) | ExprKind::Real(_)));
        if !is_literal {
            return Err(Error::new(value.pos, "a constant must be a fixed value: a number, text, or true/false")
                .with_hint("the value is substituted where the name is used, so it has to be known in advance"));
        }
        self.consts.insert(name, value.kind);
        Ok(StmtKind::Nop)
    }

    /// A statement starting with a name: an assignment or a call.
    fn name_stmt(&mut self, name: String, pos: Pos) -> Result<StmtKind, Error> {
        if self.consts.contains_key(&name)
            && matches!(
                self.peek_tok(1),
                Tok::Assign | Tok::PlusEq | Tok::MinusEq | Tok::StarEq | Tok::SlashEq
            )
        {
            return Err(Error::new(pos, format!("\"{}\" is a constant, it cannot be reassigned", name))
                .with_hint("to have a value that changes, declare it in \"variables:\" instead"));
        }
        if matches!(self.peek_tok(1), Tok::Comma) {
            return self.unpack_stmt(name, pos);
        }
        let is_assignment = matches!(
            self.peek_tok(1),
            Tok::Assign | Tok::LBracket | Tok::PlusEq | Tok::MinusEq | Tok::StarEq | Tok::SlashEq
        );
        if !is_assignment {
            if let Some(kw) = miscased_statement_keyword(&name) {
                return Err(Error::new(pos, format!("\"{}\" is not a keyword", name))
                    .with_hint(format!("keywords are written in lowercase: write {}", kw)));
            }
        }
        self.advance();
        if self.is(&Tok::LParen) {
            let args = self.args()?;
            self.end_stmt()?;
            return Ok(StmtKind::Call(name, args));
        }
        let indices = if self.is(&Tok::LBracket) { self.indices()? } else { Vec::new() };
        let mut rest = Vec::new();
        while self.is(&Tok::LBracket) {
            rest.push(self.indices()?);
        }
        if self.is(&Tok::Dot) {
            // a method call on an indexed receiver, like people[i].move(1, 0)
            let mut base = Expr { kind: ExprKind::Var(name.clone()), pos };
            for group in std::iter::once(indices).chain(rest) {
                base = Expr { kind: ExprKind::Subscript(Box::new(base), group), pos };
            }
            let e = self.postfix(base)?;
            match e.kind {
                ExprKind::MethodCall(..) => {
                    self.end_stmt()?;
                    return Ok(StmtKind::Eval(e));
                }
                ExprKind::Field(..) | ExprKind::Subscript(..) => {
                    if let Some(op) = self.compound_op() {
                        self.advance();
                        let rhs = self.expr()?;
                        self.end_stmt()?;
                        let value = Expr { kind: ExprKind::Binary(op, Box::new(e.clone()), Box::new(rhs)), pos };
                        return Ok(StmtKind::SetMember(e, value));
                    }
                    if !self.accept(&Tok::Assign) {
                        if self.is(&Tok::Eq) {
                            return Err(Error::new(self.pos(), "== compares two values, it does not assign")
                                .with_hint("to assign write the field = value"));
                        }
                        return self.fail("expected = to assign this field, or a method call");
                    }
                    let value = self.expr()?;
                    self.end_stmt()?;
                    return Ok(StmtKind::SetMember(e, value));
                }
                _ => return self.fail("only a method call or a field assignment can be a statement here"),
            }
        }
        if let Some(op) = self.compound_op() {
            self.advance();
            let rhs = self.expr()?;
            self.end_stmt()?;
            // x += e  becomes  x = x + e  (same for an indexed target)
            let mut read = Expr { kind: ExprKind::Var(name.clone()), pos };
            if !indices.is_empty() {
                read = Expr { kind: ExprKind::Subscript(Box::new(read), indices.clone()), pos };
            }
            for group in &rest {
                read = Expr { kind: ExprKind::Subscript(Box::new(read), group.clone()), pos };
            }
            let value = Expr { kind: ExprKind::Binary(op, Box::new(read), Box::new(rhs)), pos };
            return Ok(StmtKind::Assign(Target { name, indices, rest, pos }, value));
        }
        if !self.accept(&Tok::Assign) {
            if self.is(&Tok::Eq) {
                return Err(Error::new(self.pos(), "== compares two values, it does not assign")
                    .with_hint(format!("to assign write {} = value", name)));
            }
            return self.fail(format!("expected = or ( after \"{}\"", name));
        }
        let value = self.expr()?;
        self.end_stmt()?;
        Ok(StmtKind::Assign(Target { name, indices, rest, pos }, value))
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.fail(format!("the program is nested too deeply (more than {} levels)", MAX_DEPTH));
        }
        Ok(())
    }

    fn binary(&mut self, op: BinOp, a: Expr, b: Expr, pos: Pos) -> Result<Expr, Error> {
        self.ops += 1;
        if self.ops > MAX_OPS {
            return self.fail(format!("this expression is too long (more than {} operations)", MAX_OPS));
        }
        Ok(Expr { kind: ExprKind::Binary(op, Box::new(a), Box::new(b)), pos })
    }

    pub(crate) fn expr(&mut self) -> Result<Expr, Error> {
        self.enter()?;
        let e = self.or();
        self.depth -= 1;
        e
    }

    fn or(&mut self) -> Result<Expr, Error> {
        let mut a = self.and()?;
        while self.is(&Tok::Or) {
            let pos = self.advance().pos;
            let b = self.and()?;
            a = self.binary(BinOp::Or, a, b, pos)?;
        }
        Ok(a)
    }

    fn and(&mut self) -> Result<Expr, Error> {
        let mut a = self.comparison()?;
        while self.is(&Tok::And) {
            let pos = self.advance().pos;
            let b = self.comparison()?;
            a = self.binary(BinOp::And, a, b, pos)?;
        }
        Ok(a)
    }

    fn comparison(&mut self) -> Result<Expr, Error> {
        let a = self.additive()?;
        if self.is(&Tok::Is) {
            let pos = self.advance().pos;
            let class = match self.tok().clone() {
                Tok::Name(n) if !n.contains('.') => {
                    self.advance();
                    n
                }
                other => return self.fail(format!("expected a class name after \"is\", found {}", describe(&other))),
            };
            return Ok(Expr { kind: ExprKind::IsType(Box::new(a), class), pos });
        }
        if self.is(&Tok::Assign) {
            return Err(Error::new(self.pos(), "a single = assigns a value")
                .with_hint("to compare two values use =="));
        }
        let Some(op) = self.comparison_op() else { return Ok(a) };
        let pos = self.advance().pos;
        let b = self.additive()?;
        if self.comparison_op().is_some() {
            return Err(Error::new(self.pos(), "comparisons cannot be chained")
                .with_hint("write for example a < b and b < c"));
        }
        self.binary(op, a, b, pos)
    }

    fn comparison_op(&mut self) -> Option<BinOp> {
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

    /// The operator of a compound assignment (`+=`, `-=`, `*=`, `/=`), if the current
    /// token is one. Does not consume it.
    fn compound_op(&mut self) -> Option<BinOp> {
        Some(match self.tok() {
            Tok::PlusEq => BinOp::Add,
            Tok::MinusEq => BinOp::Sub,
            Tok::StarEq => BinOp::Mul,
            Tok::SlashEq => BinOp::Div,
            _ => return None,
        })
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
            a = self.binary(op, a, b, pos)?;
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
            a = self.binary(op, a, b, pos)?;
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        let op = match self.tok() {
            Tok::Minus => UnOp::Neg,
            Tok::Not => UnOp::Not,
            _ => return self.primary(),
        };
        let pos = self.advance().pos;
        self.enter()?;
        let x = self.unary();
        self.depth -= 1;
        Ok(Expr { kind: ExprKind::Unary(op, Box::new(x?)), pos })
    }

    /// A value followed by any number of `[...]` index groups.
    fn primary(&mut self) -> Result<Expr, Error> {
        let e = self.atom()?;
        self.postfix(e)
    }

    /// Continues parsing `[...]`, `.field` and `.method(...)` after `e`.
    fn postfix(&mut self, mut e: Expr) -> Result<Expr, Error> {
        loop {
            if self.is(&Tok::LBracket) {
                let pos = self.pos();
                let idx = self.indices()?;
                e = Expr { kind: ExprKind::Subscript(Box::new(e), idx), pos };
            } else if self.is(&Tok::Dot) {
                let pos = self.advance().pos;
                if let Tok::Int(n) = self.tok().clone() {
                    // tuple element by 1-based position: t.1
                    self.advance();
                    e = Expr { kind: ExprKind::TupleGet(Box::new(e), n as usize), pos };
                    continue;
                }
                // the lexer merges `a.b.c` into one name, so one dot token may
                // carry a whole chain of fields ending in a field or a method
                let name = match self.tok().clone() {
                    Tok::Name(n) => {
                        self.advance();
                        n
                    }
                    other => return self.fail(format!("expected a field or method name after \".\", found {}", describe(&other))),
                };
                let mut segs: Vec<&str> = name.split('.').collect();
                let last = segs.pop().expect("a name has at least one segment").to_string();
                for seg in segs {
                    e = Expr { kind: ExprKind::Field(Box::new(e), seg.to_string()), pos };
                }
                e = if self.is(&Tok::LParen) {
                    Expr { kind: ExprKind::MethodCall(Box::new(e), last, self.args()?), pos }
                } else {
                    Expr { kind: ExprKind::Field(Box::new(e), last), pos }
                };
            } else {
                return Ok(e);
            }
        }
    }

    fn atom(&mut self) -> Result<Expr, Error> {
        let t = self.advance();
        let kind = match t.tok {
            Tok::Int(n) => ExprKind::Int(n),
            Tok::Real(r) => ExprKind::Real(r),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::None => ExprKind::NoneLit,
            Tok::LParen => {
                let first = self.expr()?;
                if self.accept(&Tok::Comma) {
                    // a tuple literal: (a, b, ...)
                    let mut items = vec![first];
                    loop {
                        items.push(self.expr()?);
                        if self.accept(&Tok::RParen) {
                            break;
                        }
                        self.expect(&Tok::Comma, "\",\" or \")\"")?;
                    }
                    return Ok(Expr { kind: ExprKind::TupleLit(items), pos: t.pos });
                }
                self.expect(&Tok::RParen, "\")\"")?;
                return Ok(first);
            }
            Tok::LBracket => {
                if self.accept(&Tok::RBracket) {
                    ExprKind::ListLit(Vec::new())
                } else {
                    let first = self.expr()?;
                    if self.is(&Tok::For) {
                        // a comprehension: [ item for var in iter (if cond)? ]
                        self.advance();
                        let var_pos = self.pos();
                        let var = match self.tok().clone() {
                            Tok::Name(n) => {
                                self.advance();
                                n
                            }
                            other => return self.fail(format!("expected the loop variable name, found {}", describe(&other))),
                        };
                        if !self.is_name("in") {
                            return self.fail("expected \"in\" in the comprehension (like [x for x in list])");
                        }
                        self.advance();
                        let iter = self.expr()?;
                        let cond = if self.is(&Tok::If) {
                            self.advance();
                            Some(Box::new(self.expr()?))
                        } else {
                            None
                        };
                        self.expect(&Tok::RBracket, "\"]\" to close the comprehension")?;
                        ExprKind::Comprehension { item: Box::new(first), var, var_pos, iter: Box::new(iter), cond }
                    } else {
                        let mut items = vec![first];
                        if !self.accept(&Tok::RBracket) {
                            self.expect(&Tok::Comma, "\",\" or \"]\"")?;
                            loop {
                                items.push(self.expr()?);
                                if self.accept(&Tok::RBracket) {
                                    break;
                                }
                                self.expect(&Tok::Comma, "\",\" or \"]\"")?;
                            }
                        }
                        ExprKind::ListLit(items)
                    }
                }
            }
            Tok::Function => return self.lambda(t.pos),
            Tok::New => {
                let class = match self.tok().clone() {
                    Tok::Name(n) => {
                        self.advance();
                        n
                    }
                    other => return self.fail(format!("expected a class name after \"new\", found {}", describe(&other))),
                };
                ExprKind::New(class, self.args()?)
            }
            Tok::Name(n) => {
                if self.is(&Tok::LParen) {
                    ExprKind::Call(n, self.args()?)
                } else if self.is(&Tok::LBracket) {
                    ExprKind::Index(n, self.indices()?)
                } else if let Some(k) = self.consts.get(&n) {
                    k.clone()
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
            match (self.tok().clone(), self.peek_tok(1)) {
                (Tok::Name(n), Tok::Assign) if !n.contains('.') => {
                    let pos = self.pos();
                    self.advance();
                    self.advance();
                    let value = self.expr()?;
                    v.push(Expr { kind: ExprKind::Named(n, Box::new(value)), pos });
                }
                _ => {
                    if v.iter().any(|e: &Expr| matches!(e.kind, ExprKind::Named(..))) {
                        return self.fail("after a named value, all the others must be named too");
                    }
                    v.push(self.expr()?)
                }
            }
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
            ExprKind::Subscript(b, ix) => format!("{}[{}]", sx(b), list(ix)),
            ExprKind::ListLit(items) => format!("[{}]", list(items)),
            ExprKind::Comprehension { item, var, iter, cond, .. } => match cond {
                Some(c) => format!("[{} for {} in {} if {}]", sx(item), var, sx(iter), sx(c)),
                None => format!("[{} for {} in {}]", sx(item), var, sx(iter)),
            },
            ExprKind::Call(n, a) => format!("{}({})", n, list(a)),
            ExprKind::Unary(op, x) => format!("({:?} {})", op, sx(x)),
            ExprKind::Binary(op, a, b) => format!("({:?} {} {})", op, sx(a), sx(b)),
            ExprKind::Named(n, v) => format!("{}={}", n, sx(v)),
            ExprKind::New(c, a) => format!("new {}({})", c, list(a)),
            ExprKind::Field(b, f) => format!("{}.{}", sx(b), f),
            ExprKind::MethodCall(b, m, a) => format!("{}.{}({})", sx(b), m, list(a)),
            ExprKind::IsType(b, c) => format!("({} is {})", sx(b), c),
            ExprKind::FuncRef(n) => format!("@{}", n),
            ExprKind::NoneLit => "none".to_string(),
            ExprKind::TupleLit(items) => format!("({})", list(items)),
            ExprKind::TupleGet(b, n) => format!("{}.{}", sx(b), n),
        }
    }

    fn expr_of(src: &str) -> Result<String, Error> {
        let mut p =
            Parser { t: tokenize(src).unwrap(), i: 0, depth: 0, ops: 0, lambdas: Vec::new(), consts: std::collections::HashMap::new() };
        p.expr().map(|e| sx(&e))
    }

    #[test]
    fn precedence() {
        assert_eq!(expr_of("1 + 2 * 3").unwrap(), "(Add 1 (Mul 2 3))");
        assert_eq!(expr_of("(1 + 2) * 3").unwrap(), "(Mul (Add 1 2) 3)");
        assert_eq!(expr_of("a < b and not c or d == 1").unwrap(), "(Or (And (Lt a b) (Not c)) (Eq d 1))");
        assert_eq!(expr_of("-a mod 2 div b").unwrap(), "(IntDiv (Mod (Neg a) 2) b)");
        assert_eq!(expr_of("2.5 / x - true").unwrap(), "(Sub (Div 2.5 x) true)");
    }

    #[test]
    fn calls_and_indexing() {
        assert_eq!(expr_of("v[i, j + 1] + f(2, \"s\")").unwrap(), "(Add v[i,(Add j 1)] f(2,\"s\"))");
        assert_eq!(expr_of("g()").unwrap(), "g()");
    }

    #[test]
    fn less_than_minus_needs_no_spaces() {
        assert_eq!(expr_of("x<-1").unwrap(), "(Lt x (Neg 1))");
    }

    #[test]
    fn comparisons_cannot_be_chained() {
        let e = expr_of("a < b < c").unwrap_err();
        assert_eq!(e.message, "comparisons cannot be chained");
        assert_eq!(e.hint.as_deref(), Some("write for example a < b and b < c"));
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
    fn typed_declarations() {
        use crate::tir::BaseType::*;
        let src = "global variables:\n    g as float\nvariables:\n    price as float\n    q, d as int\n    ok as bool\n    v as int[3]\n    m as float[2, 4]\n    l as list of string\na = 1\n";
        let p = parse_src(src).unwrap();
        assert_eq!((p.globals[0].name.as_str(), p.globals[0].ty), ("g", Real));
        let got: Vec<(&str, Shape, crate::tir::BaseType)> =
            p.locals.iter().map(|d| (d.name.as_str(), d.shape.clone(), d.ty)).collect();
        assert_eq!(
            got,
            vec![
                ("price", Shape::Scalar, Real),
                ("q", Shape::Scalar, Int),
                ("d", Shape::Scalar, Int),
                ("ok", Shape::Scalar, Bool),
                ("v", Shape::Array(3), Int),
                ("m", Shape::Matrix(2, 4), Real),
                ("l", Shape::List, Str),
            ]
        );
        assert_eq!(p.body.len(), 1);
    }

    #[test]
    fn typed_signatures() {
        use crate::tir::BaseType::*;
        let src = "function area(b as float, h as float) as float\n    return b * h\nend function\nprocedure fill(v as int[], m as float[,], l as list of int)\nend procedure\n";
        let p = parse_src(src).unwrap();
        let params = |i: usize| -> Vec<(String, Shape, crate::tir::BaseType)> {
            p.subprograms[i].params.iter().map(|d| (d.name.clone(), d.shape.clone(), d.ty)).collect()
        };
        assert_eq!(params(0), vec![("b".into(), Shape::Scalar, Real), ("h".into(), Shape::Scalar, Real)]);
        assert_eq!(p.subprograms[0].ret, Some(crate::tir::Type::Base(Real)));
        assert_eq!(
            params(1),
            vec![("v".into(), Shape::Array(0), Int), ("m".into(), Shape::Matrix(0, 0), Real), ("l".into(), Shape::List, Int)]
        );
        assert_eq!(p.subprograms[1].ret, None);
    }

    #[test]
    fn else_if_on_one_line() {
        let src = "if(a)\n    x = 1\nelse if(b)\n    x = 2\nelse\n    x = 3\nend if\n";
        match &parse_src(src).unwrap().body[0].kind {
            StmtKind::If { branches, else_body } => {
                assert_eq!(branches.len(), 2);
                assert_eq!(else_body.as_ref().unwrap().len(), 1);
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn declaration_errors() {
        let e = err("variables: a, b\n");
        assert_eq!(e.message, "\"variables:\" must be followed by a new line with the declarations");
        assert_eq!(e.hint.as_deref(), Some("put each declaration on its own indented line below, like \"    a as int\""));
        // an unknown type now parses as a class name and is reported by the class loader
        assert!(parse_src("variables:\n    x as integer\n").is_ok());
        assert_eq!(err("variables:\nx as int\n").message, "the declarations under \"variables:\" must be indented");
        assert_eq!(
            err("variables:\n    v as int[]\n").message,
            "an array needs its size, like int[10]"
        );
        assert_eq!(
            err("function f(x as int)\n    return x\nend function\n").message,
            "a function must declare its return type, like ) as int"
        );
        assert_eq!(err("procedure p(x)\nend procedure\n").message, "parameter \"x\" needs a type, like x as int");
    }

    #[test]
    fn else_if_chain_needs_one_end_if() {
        let src = "if(v >= 8)\n    output(1)\nelse\nif(v >= 6)\n    output(2)\nelse\n    output(3)\nend if\n";
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
        let src = "if(a)\n    x = 1\nelse\n    x = 2\n    if(b)\n        x = 3\n    end if\nend if\n";
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
        let src = "while(i < 3)\n    i = i + 1\nend loop\nfor(i from 10 to 0 step -2)\n    output(i)\nend loop\nrepeat\n    input(n)\nwhile(n < 0)\n";
        let p = parse_src(src).unwrap();
        assert!(matches!(p.body[0].kind, StmtKind::While(..)));
        assert!(matches!(&p.body[1].kind, StmtKind::For { step: Some(_), .. }));
        assert!(matches!(p.body[2].kind, StmtKind::Repeat(..)));
    }

    #[test]
    fn while_indented_inside_repeat_is_a_loop() {
        let src = "repeat\n    while(a < 3)\n        a = a + 1\n    end loop\nwhile(a < 10)\n";
        match &parse_src(src).unwrap().body[0].kind {
            StmtKind::Repeat(body, _) => assert!(matches!(body[0].kind, StmtKind::While(..))),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn functions_and_procedures() {
        let src = "function add(a as int, b as int) as int\n    variables:\n        r as int\n    r = a + b\n    return r\nend function\nprocedure hello()\n    output(\"hi\")\nend procedure\nhello()\n";
        let p = parse_src(src).unwrap();
        assert_eq!(p.subprograms.len(), 2);
        let add = &p.subprograms[0];
        assert!(add.is_function);
        assert_eq!(add.params.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        assert_eq!(add.locals[0].name, "r");
        assert_eq!(add.body.len(), 2);
        assert!(!p.subprograms[1].is_function);
        assert!(p.subprograms[1].params.is_empty());
        assert_eq!(p.body.len(), 1);
    }

    #[test]
    fn less_than_minus_inside_a_condition() {
        match &parse_src("if(x<-1)\nend if\n").unwrap().body[0].kind {
            StmtKind::If { branches, .. } => assert_eq!(sx(&branches[0].0), "(Lt x (Neg 1))"),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn unclosed_if() {
        let e = err("if(a)\n    x = 1\n");
        assert_eq!(e.message, "this if is never closed");
        assert_eq!(e.pos.line, 1);
        assert_eq!(e.hint.as_deref(), Some("add end if at the end of the block"));
    }

    #[test]
    fn wrong_closer() {
        let e = err("if(a)\n    x = 1\nend loop\n");
        assert_eq!(e.message, "found \"end loop\", but the if on line 1 is not closed yet");
        assert_eq!(e.hint.as_deref(), Some("close it first with end if"));
    }

    #[test]
    fn extra_end_if_after_else_if_chain() {
        let e = err("if(a)\n    b = 1\nelse\nif(c)\n    b = 2\nend if\nend if\n");
        assert_eq!(e.message, "end if without a matching if");
        assert_eq!(e.pos.line, 7);
        assert_eq!(e.hint.as_deref(), Some("an else followed directly by if(...) is closed by a single end if"));
    }

    #[test]
    fn lowercase_keyword() {
        let e = err("IF(x)\nend if\n");
        assert_eq!(e.message, "\"IF\" is not a keyword");
        assert_eq!(e.hint.as_deref(), Some("keywords are written in lowercase: write if"));
    }

    #[test]
    fn equals_is_not_assignment() {
        let e = err("x == 5\n");
        assert_eq!(e.message, "== compares two values, it does not assign");
        assert_eq!(e.hint.as_deref(), Some("to assign write x = value"));
    }

    #[test]
    fn declarations_must_come_first() {
        assert_eq!(
            err("x = 1\nvariables:\n    y as int\n").message,
            "variable declarations must come at the start of the program or function"
        );
    }

    #[test]
    fn one_statement_per_line() {
        assert_eq!(
            err("a = 1 b = 2\n").message,
            "unexpected \"b\" after the statement: each statement goes on its own line"
        );
    }

    #[test]
    fn no_nested_definitions() {
        assert_eq!(
            err("if(a)\nfunction f()\nend function\nend if\n").message,
            "function and procedure cannot be defined inside another block"
        );
    }

    #[test]
    fn statements_after_declarations_may_be_indented() {
        let p = parse_src("variables:\n    r as int\n    r = 1\n    output(r)\n").unwrap();
        assert_eq!(p.locals.len(), 1);
        assert_eq!(p.body.len(), 2);
    }

    #[test]
    fn function_return_types() {
        assert_eq!(
            err("function f() as int[]
    return 1
end function
").message,
            "a function cannot return an array: return a list, like ) as list of int"
        );
        let p = parse_src("function f() as dict of string to list of int
    return 1
end function
").unwrap();
        assert_eq!(p.subprograms[0].ret.unwrap().full_name(), "a dictionary from strings to lists of ints");
    }

    #[test]
    fn tabs_count_as_four_columns() {
        let p = parse_src("\trepeat\n\t\tx = 1\n    while(x > 1)\n").unwrap();
        match &p.body[0].kind {
            StmtKind::Repeat(body, _) => assert_eq!(body.len(), 1),
            other => panic!("{:?}", other),
        }
        assert_eq!(parse_src("  variables:\n\tx as int\n").unwrap().locals.len(), 1);
    }

    #[test]
    fn nesting_and_length_have_limits() {
        let deep = "if(true)\n".repeat(150) + &"end if\n".repeat(150);
        assert_eq!(err(&deep).message, "the program is nested too deeply (more than 80 levels)");
        let parens = format!("x = {}1{}\n", "(".repeat(3000), ")".repeat(3000));
        assert_eq!(err(&parens).message, "the program is nested too deeply (more than 80 levels)");
        let long = format!("x = {}1\n", "1 + ".repeat(5000));
        assert_eq!(err(&long).message, "this expression is too long (more than 1000 operations)");
    }

    #[test]
    fn huge_arrays_are_rejected() {
        let msg = "an array can have at most 10000000 elements";
        assert_eq!(err("variables:\n    v as int[4294967297]\n").message, msg);
        assert_eq!(err("variables:\n    m as int[100000, 100000]\n").message, msg);
        assert!(parse_src("variables:\n    v as bool[10000000]\n").is_ok());
    }

    #[test]
    fn nested_declarations() {
        use crate::tir::{dict_of, list_of, Type};
        let p = parse_src("variables:\n    g as list of list of int\n    d as dict of string to list of int\n    e as dict of int to string\n").unwrap();
        let ints = list_of(Type::Base(crate::tir::BaseType::Int));
        assert_eq!(p.locals[0].full, list_of(ints));
        assert_eq!(p.locals[1].full, dict_of(crate::tir::BaseType::Str, ints));
        assert_eq!(p.locals[1].shape, Shape::Dict);
        assert_eq!(p.locals[2].full, dict_of(crate::tir::BaseType::Int, Type::Base(crate::tir::BaseType::Str)));
        assert_eq!(err("variables:\n    d as dict of float to int\n").message, "dictionary keys must be int or string");
    }

    #[test]
    fn list_literals_and_chained_indexing() {
        assert_eq!(expr_of("[1, 2, [3]]").unwrap(), "[1,2,[3]]");
        assert_eq!(expr_of("[]").unwrap(), "[]");
        assert_eq!(expr_of("g[1][2] + d[\"a\"]").unwrap(), "(Add g[1][2] d[\"a\"])");
        assert_eq!(expr_of("keys(d)[1]").unwrap(), "keys(d)[1]");
        match &parse_src("g[1][2] = 3\n").unwrap().body[0].kind {
            StmtKind::Assign(t, _) => {
                assert_eq!(t.indices.len(), 1);
                assert_eq!(t.rest.len(), 1);
            }
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn imports() {
        let p = parse_src("lazy import a, b as c
from d import e, f as g
global variables:
    x as int
output(c.h(1))
").unwrap();
        let summary: Vec<(String, Option<String>, bool)> =
            p.imports.iter().map(|i| (i.module.clone(), i.alias.clone(), i.lazy)).collect();
        assert_eq!(
            summary,
            vec![("a".into(), None, true), ("b".into(), Some("c".into()), true), ("d".into(), None, false)]
        );
        let names = p.imports[2].names.as_ref().unwrap();
        assert_eq!((names[1].name.as_str(), names[1].alias.as_deref()), ("f", Some("g")));
        assert_eq!(expr_of("c.h(1) + m.v").unwrap(), "(Add c.h(1) m.v)");
        assert_eq!(err("x = 1
import a
").message, "imports go at the very start of the file, before everything else");
    }

    #[test]
    fn else_then_if_on_the_next_line() {
        let chained = parse_src("if(true)
    x = 1
else
if(false)
    x = 2
end if
").unwrap();
        match &chained.body[0].kind {
            StmtKind::If { branches, else_body } => assert_eq!((branches.len(), else_body.is_none()), (2, true)),
            other => panic!("{:?}", other),
        }
        let nested = parse_src("if(true)
    x = 1
else
    if(false)
        x = 2
    end if
    x = 3
end if
").unwrap();
        match &nested.body[0].kind {
            StmtKind::If { branches, else_body } => {
                assert_eq!(branches.len(), 1);
                assert_eq!(else_body.as_ref().unwrap().len(), 2);
            }
            other => panic!("{:?}", other),
        }
    }
}
