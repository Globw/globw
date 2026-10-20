//! Turns source text into tokens, each tagged with its position.

use crate::errors::{Error, Pos};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Int(i64),
    Real(f64),
    Str(String),
    Name(String),
    // keywords (lowercase only)
    If,
    Else,
    EndIf,
    While,
    For,
    From,
    To,
    Step,
    Repeat,
    EndLoop,
    Function,
    EndFunction,
    Procedure,
    EndProcedure,
    Return,
    Global,
    Variables,
    And,
    Or,
    Not,
    Div,
    Mod,
    True,
    False,
    None,
    Break,
    Continue,
    Class,
    EndClass,
    New,
    Is,
    Try,
    Catch,
    Always,
    EndTry,
    Fail,
    Const,
    // symbols
    Assign,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
    /// indentation of the token's line, with tabs stopping every 4 columns
    pub indent: u32,
}

/// Single-word keywords. `END ...` and `variables:` are handled by the scanner.
pub fn keyword(word: &str) -> Option<Tok> {
    Some(match word {
        "if" => Tok::If,
        "else" => Tok::Else,
        "while" => Tok::While,
        "for" => Tok::For,
        "from" => Tok::From,
        "to" => Tok::To,
        "step" => Tok::Step,
        "repeat" => Tok::Repeat,
        "function" => Tok::Function,
        "procedure" => Tok::Procedure,
        "return" => Tok::Return,
        "global" => Tok::Global,
        "and" => Tok::And,
        "or" => Tok::Or,
        "not" => Tok::Not,
        "div" => Tok::Div,
        "mod" => Tok::Mod,
        "true" => Tok::True,
        "false" => Tok::False,
        "none" => Tok::None,
        "break" => Tok::Break,
        "continue" => Tok::Continue,
        "class" => Tok::Class,
        "new" => Tok::New,
        "is" => Tok::Is,
        "try" => Tok::Try,
        "catch" => Tok::Catch,
        "always" => Tok::Always,
        "fail" => Tok::Fail,
        "const" => Tok::Const,
        _ => return None,
    })
}

fn miscased(name: &str, words: &[&str]) -> Option<String> {
    let lower = name.to_lowercase();
    if lower != name && words.contains(&lower.as_str()) {
        Some(lower)
    } else {
        None
    }
}

/// `IF` → `if`: a keyword that starts a statement, written in the wrong case.
pub fn miscased_statement_keyword(name: &str) -> Option<String> {
    miscased(
        name,
        &[
            "if", "else", "while", "for", "repeat", "end", "function", "procedure", "return", "global", "break",
            "continue", "class", "try", "catch", "always", "fail",
        ],
    )
}

/// `TRUE` → `true`: a keyword used inside expressions, written in the wrong case.
pub fn miscased_value_keyword(name: &str) -> Option<String> {
    miscased(name, &["true", "false", "not", "div", "mod"])
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, Error> {
    let mut s = Scanner {
        chars: source.trim_start_matches('\u{feff}').chars().collect(),
        i: 0,
        line: 1,
        col: 1,
        out: Vec::new(),
        indent: 0,
        at_line_start: true,
    };
    while let Some(c) = s.peek(0) {
        match c {
            ' ' | '\t' | '\r' => {
                if s.at_line_start {
                    s.indent = match c {
                        '\t' => (s.indent / 4 + 1) * 4,
                        ' ' => s.indent + 1,
                        _ => s.indent,
                    };
                }
                s.bump();
            }
            '\n' => {
                let (line, col) = (s.line, s.col);
                s.bump();
                s.newline(line, col);
                s.indent = 0;
                s.at_line_start = true;
            }
            '\'' => {
                while s.peek(0).map_or(false, |c| c != '\n') {
                    s.bump();
                }
            }
            '"' => s.string()?,
            '0'..='9' => s.number()?,
            c if c.is_alphabetic() || c == '_' => s.word()?,
            _ => s.symbol()?,
        }
    }
    let (line, col) = (s.line, s.col);
    s.newline(line, col);
    s.push(Tok::Eof, line, col, 1);
    Ok(s.out)
}

struct Scanner {
    chars: Vec<char>,
    i: usize,
    line: u32,
    col: u32,
    out: Vec<Token>,
    indent: u32,
    at_line_start: bool,
}

impl Scanner {
    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.i + k).copied()
    }

    fn bump(&mut self) -> char {
        let c = self.chars[self.i];
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn push(&mut self, tok: Tok, line: u32, col: u32, len: u32) {
        self.out.push(Token { tok, pos: Pos::new(line, col, len), indent: self.indent });
        self.at_line_start = false;
    }

    /// Emits a newline token unless the previous token already is one.
    fn newline(&mut self, line: u32, col: u32) {
        if !matches!(self.out.last(), None | Some(Token { tok: Tok::Newline, .. })) {
            self.push(Tok::Newline, line, col, 1);
        }
    }

    fn skip_blanks(&mut self) {
        while matches!(self.peek(0), Some(' ') | Some('\t')) {
            self.bump();
        }
    }

    fn read_word(&mut self) -> String {
        let mut w = String::new();
        while let Some(c) = self.peek(0) {
            if c.is_alphanumeric() || c == '_' {
                w.push(self.bump());
            } else {
                break;
            }
        }
        w
    }

    fn string(&mut self) -> Result<(), Error> {
        let (line, col) = (self.line, self.col);
        self.bump();
        // An interpolated string "a ${expr} b" becomes ("a " + (expr) + " b"); plain
        // `{` stays literal (so regex and JSON text are unaffected), only `${` starts
        // an inserted value, and `\$` is a literal dollar sign.
        enum Part {
            Lit(String),
            Expr(String),
        }
        let mut parts: Vec<Part> = Vec::new();
        let mut text = String::new();
        let mut interpolated = false;
        loop {
            match self.peek(0) {
                None | Some('\n') => {
                    return Err(Error::new(Pos::new(line, col, 1), "unterminated string")
                        .with_hint("close the text with \""))
                }
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    let (l, k) = (self.line, self.col);
                    self.bump();
                    match self.peek(0) {
                        Some('n') => text.push('\n'),
                        Some('"') => text.push('"'),
                        Some('\\') => text.push('\\'),
                        Some('$') => text.push('$'),
                        _ => {
                            return Err(Error::new(Pos::new(l, k, 2), "invalid escape sequence")
                                .with_hint("after \\ you can use n, \", \\ or $"))
                        }
                    }
                    self.bump();
                }
                Some('$') if self.peek(1) == Some('{') => {
                    self.bump();
                    self.bump();
                    interpolated = true;
                    parts.push(Part::Lit(std::mem::take(&mut text)));
                    let mut src = String::new();
                    let mut depth = 1;
                    loop {
                        match self.peek(0) {
                            None | Some('\n') => {
                                return Err(Error::new(Pos::new(line, col, 1), "unterminated ${ } in the text")
                                    .with_hint("close the inserted value with }"))
                            }
                            Some('}') => {
                                self.bump();
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                src.push('}');
                            }
                            Some('{') => {
                                self.bump();
                                depth += 1;
                                src.push('{');
                            }
                            Some('"') => {
                                // copy a nested string literal verbatim
                                src.push(self.bump());
                                loop {
                                    match self.peek(0) {
                                        None | Some('\n') => {
                                            return Err(Error::new(Pos::new(line, col, 1), "unterminated string inside ${ }"))
                                        }
                                        Some('\\') => {
                                            src.push(self.bump());
                                            if self.peek(0).is_some() {
                                                src.push(self.bump());
                                            }
                                        }
                                        Some('"') => {
                                            src.push(self.bump());
                                            break;
                                        }
                                        Some(_) => {
                                            src.push(self.bump());
                                        }
                                    }
                                }
                            }
                            Some(_) => {
                                src.push(self.bump());
                            }
                        }
                    }
                    parts.push(Part::Expr(src));
                }
                Some(_) => {
                    let c = self.bump();
                    text.push(c);
                }
            }
        }
        if !interpolated {
            let len = self.col - col;
            self.push(Tok::Str(text), line, col, len);
            return Ok(());
        }
        parts.push(Part::Lit(text));
        // emit  ( lit0 + (expr0) + lit1 + ... )
        self.push(Tok::LParen, line, col, 1);
        let mut first = true;
        for part in parts {
            if !first {
                self.push(Tok::Plus, line, col, 1);
            }
            first = false;
            match part {
                Part::Lit(s) => self.push(Tok::Str(s), line, col, 1),
                Part::Expr(src) => {
                    self.push(Tok::LParen, line, col, 1);
                    let toks = tokenize(&src).map_err(|_| {
                        Error::new(Pos::new(line, col, 1), format!("invalid expression inside ${{ }}: {}", src.trim()))
                            .with_hint("inside ${ } write a value, like a variable or a + b")
                    })?;
                    for t in toks {
                        if matches!(t.tok, Tok::Newline | Tok::Eof) {
                            continue;
                        }
                        self.push(t.tok, line, col, 1);
                    }
                    self.push(Tok::RParen, line, col, 1);
                }
            }
        }
        self.push(Tok::RParen, line, col, 1);
        Ok(())
    }

    fn number(&mut self) -> Result<(), Error> {
        let (line, col) = (self.line, self.col);
        let mut text = String::new();
        while self.peek(0).map_or(false, |c| c.is_ascii_digit()) {
            text.push(self.bump());
        }
        let has_dot = self.peek(0) == Some('.') && self.peek(1).map_or(false, |c| c.is_ascii_digit());
        if has_dot {
            text.push(self.bump());
            while self.peek(0).map_or(false, |c| c.is_ascii_digit()) {
                text.push(self.bump());
            }
        }
        // optional scientific exponent, e.g. 6.022e23 or 1e-10
        let has_exp = matches!(self.peek(0), Some('e') | Some('E'))
            && (self.peek(1).map_or(false, |c| c.is_ascii_digit())
                || (matches!(self.peek(1), Some('+') | Some('-')) && self.peek(2).map_or(false, |c| c.is_ascii_digit())));
        if has_exp {
            text.push(self.bump()); // the e / E
            if matches!(self.peek(0), Some('+') | Some('-')) {
                text.push(self.bump());
            }
            while self.peek(0).map_or(false, |c| c.is_ascii_digit()) {
                text.push(self.bump());
            }
        }
        let is_real = has_dot || has_exp;
        let pos = Pos::new(line, col, text.chars().count() as u32);
        let tok = if is_real {
            Tok::Real(text.parse().expect("a numeric literal always parses as f64"))
        } else {
            Tok::Int(text.parse().map_err(|_| Error::new(pos, "integer too large"))?)
        };
        self.out.push(Token { tok, pos, indent: self.indent });
        self.at_line_start = false;
        Ok(())
    }

    fn word(&mut self) -> Result<(), Error> {
        let (line, col) = (self.line, self.col);
        let w = self.read_word();
        if w == "end" {
            self.skip_blanks();
            let tok = match self.read_word().as_str() {
                "if" => Tok::EndIf,
                "loop" => Tok::EndLoop,
                "function" => Tok::EndFunction,
                "procedure" => Tok::EndProcedure,
                "class" => Tok::EndClass,
                "try" => Tok::EndTry,
                _ => {
                    return Err(Error::new(
                        Pos::new(line, col, 3),
                        "end must be followed by if, loop, function, procedure, class or try",
                    )
                    .with_hint("for example: end if or end loop"))
                }
            };
            let len = self.col - col;
            self.push(tok, line, col, len);
            return Ok(());
        }
        if w == "variables" {
            let saved = (self.i, self.line, self.col);
            self.skip_blanks();
            if self.peek(0) == Some(':') {
                self.bump();
                let len = self.col - col;
                self.push(Tok::Variables, line, col, len);
                return Ok(());
            }
            (self.i, self.line, self.col) = saved;
        }
        let tok = match keyword(&w) {
            Some(k) => k,
            None => {
                // a qualified name: module.name
                let mut w = w;
                while self.peek(0) == Some('.') && self.peek(1).map_or(false, |c| c.is_alphabetic() || c == '_') {
                    w.push(self.bump());
                    w.push_str(&self.read_word());
                }
                Tok::Name(w)
            }
        };
        let len = self.col - col;
        self.push(tok, line, col, len);
        Ok(())
    }

    fn symbol(&mut self) -> Result<(), Error> {
        let (line, col) = (self.line, self.col);
        let c = self.bump();
        let next = self.peek(0);
        let (tok, len) = match (c, next) {
            ('=', Some('=')) => {
                self.bump();
                (Tok::Eq, 2)
            }
            ('<', Some('=')) => {
                self.bump();
                (Tok::LtEq, 2)
            }
            ('<', Some('>')) => {
                return Err(Error::new(Pos::new(line, col, 2), "invalid operator <>")
                    .with_hint("\"not equal\" is written !="))
            }
            ('>', Some('=')) => {
                self.bump();
                (Tok::GtEq, 2)
            }
            ('!', Some('=')) => {
                self.bump();
                (Tok::NotEq, 2)
            }
            ('+', Some('=')) => {
                self.bump();
                (Tok::PlusEq, 2)
            }
            ('-', Some('=')) => {
                self.bump();
                (Tok::MinusEq, 2)
            }
            ('*', Some('=')) => {
                self.bump();
                (Tok::StarEq, 2)
            }
            ('/', Some('=')) => {
                self.bump();
                (Tok::SlashEq, 2)
            }
            ('<', _) => (Tok::Lt, 1),
            ('>', _) => (Tok::Gt, 1),
            ('=', _) => (Tok::Assign, 1),
            ('+', _) => (Tok::Plus, 1),
            ('-', _) => (Tok::Minus, 1),
            ('*', _) => (Tok::Star, 1),
            ('/', _) => (Tok::Slash, 1),
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            ('[', _) => (Tok::LBracket, 1),
            (']', _) => (Tok::RBracket, 1),
            (',', _) => (Tok::Comma, 1),
            ('.', _) => (Tok::Dot, 1),
            _ => return Err(Error::new(Pos::new(line, col, 1), format!("invalid character '{}'", c))),
        };
        self.push(tok, line, col, len);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Tok::*;

    fn toks(src: &str) -> Vec<Tok> {
        tokenize(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn name(s: &str) -> Tok {
        Name(s.to_string())
    }

    #[test]
    fn keywords_and_names() {
        assert_eq!(toks("if(x)\nend if"), vec![If, LParen, name("x"), RParen, Newline, EndIf, Newline, Eof]);
    }

    #[test]
    fn all_end_forms() {
        assert_eq!(
            toks("end loop\nend function\nend   procedure"),
            vec![EndLoop, Newline, EndFunction, Newline, EndProcedure, Newline, Eof]
        );
    }

    #[test]
    fn uppercase_words_are_names() {
        assert_eq!(toks("IF WHILE")[..2], [name("IF"), name("WHILE")]);
    }

    #[test]
    fn variables_declaration() {
        assert_eq!(toks("variables: a, b")[..4], [Variables, name("a"), Comma, name("b")]);
        assert_eq!(toks("variables = 1")[0], name("variables"));
    }

    #[test]
    fn numbers() {
        assert_eq!(toks("42 3.5")[..2], [Int(42), Real(3.5)]);
        assert_eq!(toks("6e2 1.5e-3 1E10 2e+3")[..4], [Real(600.0), Real(0.0015), Real(1e10), Real(2000.0)]);
        // a bare `e` after a number is not an exponent: 4e is 4 then the name e
        assert_eq!(toks("4e")[..2], [Int(4), name("e")]);
    }

    #[test]
    fn compound_assignment_operators() {
        assert_eq!(toks("+= -= *= /=")[..4], [PlusEq, MinusEq, StarEq, SlashEq]);
    }

    #[test]
    fn string_interpolation() {
        // "a ${x} b" lexes as ( "a " + ( x ) + " b" )
        assert_eq!(
            toks("\"a ${x} b\"")[..9],
            [LParen, Str("a ".to_string()), Plus, LParen, name("x"), RParen, Plus, Str(" b".to_string()), RParen]
        );
        // plain braces (regex, JSON) are left alone: one string token
        assert_eq!(toks("\"[0-9]{4}\"")[..1], [Str("[0-9]{4}".to_string())]);
    }

    #[test]
    fn operators() {
        assert_eq!(
            toks("= = <= >= != < > == + - * / ( ) [ ] ,")[..17],
            [Assign, Assign, LtEq, GtEq, NotEq, Lt, Gt, Eq, Plus, Minus, Star, Slash, LParen, RParen, LBracket, RBracket, Comma]
        );
    }

    #[test]
    fn strings_with_escapes() {
        assert_eq!(toks(r#""a\"b\n\\""#)[0], Str("a\"b\n\\".to_string()));
    }

    #[test]
    fn comments_and_blank_lines_collapse() {
        assert_eq!(toks("a ' note\n\n\nb"), vec![name("a"), Newline, name("b"), Newline, Eof]);
    }

    #[test]
    fn crlf_and_bom_are_accepted() {
        assert_eq!(
            toks("\u{feff}a = 1\r\nb\r\n"),
            vec![name("a"), Assign, Int(1), Newline, name("b"), Newline, Eof]
        );
    }

    #[test]
    fn accented_identifiers_and_strings() {
        assert_eq!(toks("città = \"perché\"")[..3], [name("città"), Assign, Str("perché".to_string())]);
        assert_eq!(tokenize("città = 1").unwrap()[1].pos, Pos::new(1, 7, 1));
    }

    #[test]
    fn positions() {
        let t = tokenize("x = 1\n  y").unwrap();
        assert_eq!(t[0].pos, Pos::new(1, 1, 1));
        assert_eq!(t[1].pos, Pos::new(1, 3, 1));
        assert_eq!(t[4].pos, Pos::new(2, 3, 1));
    }

    #[test]
    fn errors() {
        let msg = |s: &str| tokenize(s).unwrap_err().message;
        assert_eq!(msg("\"abc"), "unterminated string");
        assert_eq!(msg("end x"), "end must be followed by if, loop, function, procedure, class or try");
        assert_eq!(msg("a # b"), "invalid character '#'");
        assert_eq!(msg("a <> b"), "invalid operator <>");
        assert_eq!(msg("99999999999999999999"), "integer too large");
        assert_eq!(tokenize("a <> b").unwrap_err().hint.as_deref(), Some("\"not equal\" is written !="));
    }

    #[test]
    fn miscased_keywords() {
        assert_eq!(miscased_statement_keyword("IF"), Some("if".to_string()));
        assert_eq!(miscased_statement_keyword("End"), Some("end".to_string()));
        assert_eq!(miscased_statement_keyword("if"), Option::None);
        assert_eq!(miscased_statement_keyword("x"), Option::None);
        assert_eq!(miscased_value_keyword("TRUE"), Some("true".to_string()));
        assert_eq!(miscased_value_keyword("a"), Option::None);
    }
}
