//! Turns source text into tokens, each tagged with its position.

use crate::errors::{Error, Pos};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Int(i64),
    Real(f64),
    Str(String),
    Name(String),
    // keywords (uppercase only)
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
    // symbols
    Arrow,
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
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
}

/// Single-word keywords. `END ...` and `variables:` are handled by the scanner.
pub fn keyword(word: &str) -> Option<Tok> {
    Some(match word {
        "IF" => Tok::If,
        "ELSE" => Tok::Else,
        "WHILE" => Tok::While,
        "FOR" => Tok::For,
        "FROM" => Tok::From,
        "TO" => Tok::To,
        "STEP" => Tok::Step,
        "REPEAT" => Tok::Repeat,
        "FUNCTION" => Tok::Function,
        "PROCEDURE" => Tok::Procedure,
        "RETURN" => Tok::Return,
        "GLOBAL" => Tok::Global,
        "AND" => Tok::And,
        "OR" => Tok::Or,
        "NOT" => Tok::Not,
        "DIV" => Tok::Div,
        "MOD" => Tok::Mod,
        "TRUE" => Tok::True,
        "FALSE" => Tok::False,
        _ => return None,
    })
}

fn miscased(name: &str, words: &[&str]) -> Option<String> {
    let upper = name.to_uppercase();
    if upper != name && words.contains(&upper.as_str()) {
        Some(upper)
    } else {
        None
    }
}

/// `if` → `IF`: a keyword that starts a statement, written in the wrong case.
pub fn miscased_statement_keyword(name: &str) -> Option<String> {
    miscased(name, &["IF", "ELSE", "WHILE", "FOR", "REPEAT", "END", "FUNCTION", "PROCEDURE", "RETURN", "GLOBAL"])
}

/// `true` → `TRUE`: a keyword used inside expressions, written in the wrong case.
pub fn miscased_value_keyword(name: &str) -> Option<String> {
    miscased(name, &["TRUE", "FALSE", "NOT", "DIV", "MOD"])
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, Error> {
    let mut s = Scanner {
        chars: source.trim_start_matches('\u{feff}').chars().collect(),
        i: 0,
        line: 1,
        col: 1,
        out: Vec::new(),
    };
    while let Some(c) = s.peek(0) {
        match c {
            ' ' | '\t' | '\r' => {
                s.bump();
            }
            '\n' => {
                let (line, col) = (s.line, s.col);
                s.bump();
                s.newline(line, col);
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
        self.out.push(Token { tok, pos: Pos::new(line, col, len) });
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
        let mut text = String::new();
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
                        _ => {
                            return Err(Error::new(Pos::new(l, k, 2), "invalid escape sequence")
                                .with_hint("after \\ you can use n, \" or \\"))
                        }
                    }
                    self.bump();
                }
                Some(_) => {
                    let c = self.bump();
                    text.push(c);
                }
            }
        }
        let len = self.col - col;
        self.push(Tok::Str(text), line, col, len);
        Ok(())
    }

    fn number(&mut self) -> Result<(), Error> {
        let (line, col) = (self.line, self.col);
        let mut text = String::new();
        while self.peek(0).map_or(false, |c| c.is_ascii_digit()) {
            text.push(self.bump());
        }
        let is_real = self.peek(0) == Some('.') && self.peek(1).map_or(false, |c| c.is_ascii_digit());
        if is_real {
            text.push(self.bump());
            while self.peek(0).map_or(false, |c| c.is_ascii_digit()) {
                text.push(self.bump());
            }
        }
        let pos = Pos::new(line, col, text.chars().count() as u32);
        let tok = if is_real {
            Tok::Real(text.parse().expect("digits with one dot always parse as f64"))
        } else {
            Tok::Int(text.parse().map_err(|_| Error::new(pos, "integer too large"))?)
        };
        self.out.push(Token { tok, pos });
        Ok(())
    }

    fn word(&mut self) -> Result<(), Error> {
        let (line, col) = (self.line, self.col);
        let w = self.read_word();
        if w == "END" {
            self.skip_blanks();
            let tok = match self.read_word().as_str() {
                "IF" => Tok::EndIf,
                "LOOP" => Tok::EndLoop,
                "FUNCTION" => Tok::EndFunction,
                "PROCEDURE" => Tok::EndProcedure,
                _ => {
                    return Err(Error::new(
                        Pos::new(line, col, 3),
                        "END must be followed by IF, LOOP, FUNCTION or PROCEDURE",
                    )
                    .with_hint("for example: END IF or END LOOP"))
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
        let len = w.chars().count() as u32;
        let tok = keyword(&w).unwrap_or(Tok::Name(w));
        self.push(tok, line, col, len);
        Ok(())
    }

    fn symbol(&mut self) -> Result<(), Error> {
        let (line, col) = (self.line, self.col);
        let c = self.bump();
        let next = self.peek(0);
        let (tok, len) = match (c, next) {
            ('<', Some('-')) => {
                self.bump();
                (Tok::Arrow, 2)
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
            ('←', _) => (Tok::Arrow, 1),
            ('<', _) => (Tok::Lt, 1),
            ('>', _) => (Tok::Gt, 1),
            ('=', _) => (Tok::Eq, 1),
            ('+', _) => (Tok::Plus, 1),
            ('-', _) => (Tok::Minus, 1),
            ('*', _) => (Tok::Star, 1),
            ('/', _) => (Tok::Slash, 1),
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            ('[', _) => (Tok::LBracket, 1),
            (']', _) => (Tok::RBracket, 1),
            (',', _) => (Tok::Comma, 1),
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
        assert_eq!(toks("IF(x)\nEND IF"), vec![If, LParen, name("x"), RParen, Newline, EndIf, Newline, Eof]);
    }

    #[test]
    fn all_end_forms() {
        assert_eq!(
            toks("END LOOP\nEND FUNCTION\nEND   PROCEDURE"),
            vec![EndLoop, Newline, EndFunction, Newline, EndProcedure, Newline, Eof]
        );
    }

    #[test]
    fn lowercase_words_are_names() {
        assert_eq!(toks("if while")[..2], [name("if"), name("while")]);
    }

    #[test]
    fn variables_declaration() {
        assert_eq!(toks("variables: a, b")[..4], [Variables, name("a"), Comma, name("b")]);
        assert_eq!(toks("variables <- 1")[0], name("variables"));
    }

    #[test]
    fn numbers() {
        assert_eq!(toks("42 3.5")[..2], [Int(42), Real(3.5)]);
    }

    #[test]
    fn operators() {
        assert_eq!(
            toks("<- ← <= >= != < > = + - * / ( ) [ ] ,")[..17],
            [Arrow, Arrow, LtEq, GtEq, NotEq, Lt, Gt, Eq, Plus, Minus, Star, Slash, LParen, RParen, LBracket, RBracket, Comma]
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
            toks("\u{feff}a <- 1\r\nb\r\n"),
            vec![name("a"), Arrow, Int(1), Newline, name("b"), Newline, Eof]
        );
    }

    #[test]
    fn accented_identifiers_and_strings() {
        assert_eq!(toks("città <- \"perché\"")[..3], [name("città"), Arrow, Str("perché".to_string())]);
        assert_eq!(tokenize("città <- 1").unwrap()[1].pos, Pos::new(1, 7, 2));
    }

    #[test]
    fn positions() {
        let t = tokenize("x <- 1\n  y").unwrap();
        assert_eq!(t[0].pos, Pos::new(1, 1, 1));
        assert_eq!(t[1].pos, Pos::new(1, 3, 2));
        assert_eq!(t[4].pos, Pos::new(2, 3, 1));
    }

    #[test]
    fn errors() {
        let msg = |s: &str| tokenize(s).unwrap_err().message;
        assert_eq!(msg("\"abc"), "unterminated string");
        assert_eq!(msg("END x"), "END must be followed by IF, LOOP, FUNCTION or PROCEDURE");
        assert_eq!(msg("a # b"), "invalid character '#'");
        assert_eq!(msg("a <> b"), "invalid operator <>");
        assert_eq!(msg("99999999999999999999"), "integer too large");
        assert_eq!(tokenize("a <> b").unwrap_err().hint.as_deref(), Some("\"not equal\" is written !="));
    }

    #[test]
    fn miscased_keywords() {
        assert_eq!(miscased_statement_keyword("if"), Some("IF".to_string()));
        assert_eq!(miscased_statement_keyword("End"), Some("END".to_string()));
        assert_eq!(miscased_statement_keyword("IF"), None);
        assert_eq!(miscased_statement_keyword("x"), None);
        assert_eq!(miscased_value_keyword("true"), Some("TRUE".to_string()));
        assert_eq!(miscased_value_keyword("a"), None);
    }
}
